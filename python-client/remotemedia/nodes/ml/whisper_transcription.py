import asyncio
import numpy as np
import logging
from typing import Optional, Any, AsyncGenerator, Dict, List, Tuple
from dataclasses import dataclass
import time
import re

from remotemedia.core.node import Node
from remotemedia.core.exceptions import NodeError

# Try to import model registry for optimization
try:
    from remotemedia.core import get_or_load
    MODEL_REGISTRY_AVAILABLE = True
except ImportError:
    MODEL_REGISTRY_AVAILABLE = False
    get_or_load = None

# Configure basic logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

try:
    import torch
    from transformers import AutoModelForSpeechSeq2Seq, AutoProcessor, pipeline
except ImportError:
    logger.warning("ML libraries not found. WhisperTranscriptionNode will not be available.")
    torch = None
    pipeline = None


@dataclass
class WordTiming:
    """Represents timing information for a word in transcription."""
    word: str
    start: float
    end: float
    confidence: float = 1.0

@dataclass
class TranscriptionDelta:
    """Represents a transcription update with word-level changes."""
    text: str
    words: List[WordTiming]
    is_partial: bool
    confidence: float
    audio_duration: float

@dataclass
class WordUpdate:
    """Represents a single word update in streaming transcription."""
    word: str
    start: float
    end: float
    confidence: float
    is_final: bool
    segment_id: int
    word_index: int



class WhisperTranscriptionNode(Node):
    """
    A node that performs real-time audio transcription using a Whisper model with delta updates.
    Supports word-level timing and progressive accuracy improvement through growing context windows.
    """

    def __init__(self,
                 model_id: str = "openai/whisper-large-v3-turbo",
                 device: Optional[str] = None,
                 torch_dtype: str = "float16",
                 chunk_length_s: int = 30,
                 initial_buffer_duration_s: int = 3,
                 max_buffer_duration_s: int = 15,
                 buffer_growth_factor: float = 1.5,
                 overlap_duration_s: float = 1.0,
                 use_registry: bool = True,
                 **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.is_streaming = True
        self.model_id = model_id
        self._requested_device = device
        self._requested_torch_dtype = torch_dtype
        self.chunk_length_s = chunk_length_s
        self.sample_rate = 16000  # Whisper models expect 16kHz audio
        self.use_registry = use_registry and MODEL_REGISTRY_AVAILABLE
        
        # Dynamic buffer management
        self.initial_buffer_duration_s = initial_buffer_duration_s
        self.max_buffer_duration_s = max_buffer_duration_s
        self.buffer_growth_factor = buffer_growth_factor
        self.overlap_duration_s = overlap_duration_s
        self.current_buffer_duration_s = initial_buffer_duration_s
        
        self.audio_buffer = np.array([], dtype=np.float32)
        self.full_audio_accumulator = np.array([], dtype=np.float32)  # Limited to last sentence + current
        self.processed_audio_length = 0
        self.last_transcription = ""
        self.word_history: List[WordTiming] = []
        self.transcription_count = 0
        self.segment_id = 0
        self.transcribed_words_by_time = {}  # timestamp_key -> WordTiming for correction detection
        self.buffer_start_time = 0.0  # Absolute time position of the first sample in full_audio_accumulator

        self.transcription_pipeline = None
        self.device = None
        self.torch_dtype = None

    async def initialize(self) -> None:
        """
        Load the model and processor. This runs on the execution environment (local or remote).
        """
        try:
            import torch
            from transformers import AutoModelForSpeechSeq2Seq, AutoProcessor, pipeline
        except ImportError:
            raise NodeError("Required ML libraries (torch, transformers) are not installed on the execution environment.")

        if self._requested_device:
            self.device = self._requested_device
        elif torch.cuda.is_available():
            self.device = "cuda:0"
        elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            self.device = "mps"
        else:
            self.device = "cpu"

        try:
            resolved_torch_dtype = getattr(torch, self._requested_torch_dtype)
        except AttributeError:
            raise NodeError(f"Invalid torch_dtype '{self._requested_torch_dtype}'")
        self.torch_dtype = resolved_torch_dtype if torch.cuda.is_available() else torch.float32

        logger.info(f"WhisperNode configured for model '{self.model_id}' on device '{self.device}'")
        logger.info(f"Initializing Whisper model '{self.model_id}' (registry: {self.use_registry})...")
        
        try:
            if self.use_registry and MODEL_REGISTRY_AVAILABLE:
                # Use model registry for efficient sharing
                model_key = f"{self.model_id}@{self.device}"
                
                def load_whisper_model():
                    model = AutoModelForSpeechSeq2Seq.from_pretrained(
                        self.model_id,
                        torch_dtype=self.torch_dtype,
                        low_cpu_mem_usage=True,
                        use_safetensors=True
                    )
                    model.to(self.device)
                    return model
                
                def load_whisper_processor():
                    return AutoProcessor.from_pretrained(self.model_id)
                
                # Load via registry (shared across nodes)
                model = await asyncio.to_thread(
                    get_or_load,
                    model_key,
                    load_whisper_model
                )
                
                processor = await asyncio.to_thread(
                    get_or_load,
                    f"{model_key}_processor",
                    load_whisper_processor
                )
                
                logger.info("Loaded Whisper model via registry (shared)")
            else:
                # Load directly without registry (baseline)
                model = await asyncio.to_thread(
                    AutoModelForSpeechSeq2Seq.from_pretrained,
                    self.model_id,
                    torch_dtype=self.torch_dtype,
                    low_cpu_mem_usage=True,
                    use_safetensors=True
                )
                model.to(self.device)
                processor = await asyncio.to_thread(AutoProcessor.from_pretrained, self.model_id)
                
                logger.info("Loaded Whisper model directly (no registry)")

            self.transcription_pipeline = pipeline(
                "automatic-speech-recognition",
                model=model,
                tokenizer=processor.tokenizer,
                feature_extractor=processor.feature_extractor,
                torch_dtype=self.torch_dtype,
                device=self.device,
                chunk_length_s=self.chunk_length_s,
                return_timestamps="word",  # Enable word-level timestamps
            )
            logger.info("Whisper model initialized successfully.")
        except Exception as e:
            raise NodeError(f"Failed to initialize Whisper model: {e}")

    async def process(self, data_stream: AsyncGenerator[Any, None]) -> AsyncGenerator[WordUpdate, None]:
        """
        Process an incoming audio stream with streaming word-level transcription.
        Expects tuples of (numpy_array, sample_rate).
        Yields WordUpdate objects as individual words become available.
        """
        if not self.transcription_pipeline:
            raise NodeError("Transcription pipeline is not initialized.")

        async for data in data_stream:
            if not isinstance(data, tuple) or len(data) != 2:
                logger.warning(f"WhisperNode received data of unexpected type or length {type(data)}, skipping.")
                continue
            
            audio_chunk, _ = data

            if not isinstance(audio_chunk, np.ndarray):
                logger.warning(f"Received non-numpy audio_chunk of type {type(audio_chunk)}, skipping.")
                continue

            # Append to both buffers - audio_buffer for processing, full_accumulator for complete context
            audio_chunk_flat = audio_chunk.flatten().astype(np.float32)
            self.audio_buffer = np.concatenate([self.audio_buffer, audio_chunk_flat])
            self.full_audio_accumulator = np.concatenate([self.full_audio_accumulator, audio_chunk_flat])

            # Calculate current buffer size based on dynamic duration
            current_buffer_size = int(self.current_buffer_duration_s * self.sample_rate)
            
            # If buffer has enough audio, transcribe with growing context and stream words
            if len(self.audio_buffer) >= current_buffer_size:
                async for word_update in self._transcribe_and_stream_words():
                    yield word_update
                    
                    # Grow buffer size for next iteration (up to max)
                    if self.current_buffer_duration_s < self.max_buffer_duration_s:
                        old_duration = self.current_buffer_duration_s
                        self.current_buffer_duration_s = min(
                            self.current_buffer_duration_s * self.buffer_growth_factor,
                            self.max_buffer_duration_s
                        )
                        if old_duration < self.current_buffer_duration_s:
                            logger.info(f"Growing buffer to {self.current_buffer_duration_s:.1f}s for improved accuracy")
        
        # Stream ended - process any remaining buffered audio
        if len(self.full_audio_accumulator) > 0:
            logger.info(f"Stream ended - processing final {len(self.full_audio_accumulator) / self.sample_rate:.2f}s of buffered audio")
            
            # Process all remaining audio in one final transcription
            try:
                audio_to_process = self.full_audio_accumulator.copy()
                audio_duration = len(audio_to_process) / self.sample_rate
                
                logger.info(f"Final transcription of {audio_duration:.2f}s of remaining audio...")
                
                # Perform transcription with word-level timestamps
                result = await asyncio.to_thread(self.transcription_pipeline, audio_to_process)
                transcribed_text = result["text"].strip()
                
                if transcribed_text:
                    # Extract word-level timing information
                    words = self._extract_word_timings(result)
                    
                    logger.info(f"Final transcription yielded {len(words)} words")
                    
                    # Stream final words
                    for word_index, word in enumerate(words):
                        # Check for corrections
                        time_key = round(word.start, 1)
                        existing_word = self.transcribed_words_by_time.get(time_key)
                        is_correction = existing_word is not None and existing_word.word != word.word
                        
                        if is_correction:
                            logger.info(f"FINAL CORRECTION at {time_key}s: '{existing_word.word}' → '{word.word}'")
                        
                        self.transcribed_words_by_time[time_key] = word
                        
                        word_update = WordUpdate(
                            word=word.word,
                            start=word.start,
                            end=word.end,
                            confidence=word.confidence,
                            is_final=True,  # This is truly final
                            segment_id=self.transcription_count,
                            word_index=word_index
                        )
                        yield word_update
                        
            except Exception as e:
                logger.error(f"Error during final transcription: {e}")
    
    async def _transcribe_and_stream_words(self) -> AsyncGenerator[WordUpdate, None]:
        """Transcribe accumulated audio and stream word updates with correction detection."""
        try:
            # Use full accumulated audio up to max buffer size for complete context
            max_buffer_size = int(self.max_buffer_duration_s * self.sample_rate)
            audio_to_process = self.full_audio_accumulator[:min(len(self.full_audio_accumulator), max_buffer_size)].copy()
            audio_duration = len(audio_to_process) / self.sample_rate
            
            logger.info(f"Transcribing {audio_duration:.2f}s of accumulated audio (iteration #{self.transcription_count + 1})...")
            
            # Perform transcription with word-level timestamps
            start_time = time.time()
            result = await asyncio.to_thread(self.transcription_pipeline, audio_to_process)
            inference_time = time.time() - start_time
            
            transcribed_text = result["text"].strip()
            if not transcribed_text:
                return
                
            # Extract word-level timing information
            words = self._extract_word_timings(result)
            
            logger.info(f"Transcribed {len(words)} words from {audio_duration:.2f}s audio (inference: {inference_time:.2f}s)")
            
            # Stream words, detecting corrections by comparing timestamps with previous transcriptions
            for word_index, word in enumerate(words):
                # Create timestamp key for correction detection (rounded to 100ms precision)
                time_key = round(word.start, 1)
                
                # Check if we've seen this timestamp before
                existing_word = self.transcribed_words_by_time.get(time_key)
                is_correction = existing_word is not None and existing_word.word != word.word
                
                if is_correction:
                    logger.info(f"CORRECTION detected at {time_key}s: '{existing_word.word}' → '{word.word}'")
                
                # Update our timestamp tracking
                self.transcribed_words_by_time[time_key] = word
                
                word_update = WordUpdate(
                    word=word.word,
                    start=word.start,
                    end=word.end,
                    confidence=word.confidence,
                    is_final=False,  # Always streaming, never truly final
                    segment_id=self.transcription_count,  # Use transcription count as segment_id
                    word_index=word_index
                )
                yield word_update
            
            # Update state
            self.last_transcription = transcribed_text
            self.word_history = words
            self.transcription_count += 1
            
            # Check for sentence boundaries and trim accumulator to prevent indefinite growth
            sentence_end_time = self._detect_sentence_boundaries(transcribed_text, words)
            if sentence_end_time:
                self._trim_accumulator_to_sentence(sentence_end_time)
            
            # Only trim the processing buffer to trigger next growth cycle
            overlap_size = int(self.overlap_duration_s * self.sample_rate)
            current_buffer_size = int(self.current_buffer_duration_s * self.sample_rate)
            move_forward = max(current_buffer_size - overlap_size, overlap_size)
            self.audio_buffer = self.audio_buffer[move_forward:]
            self.processed_audio_length += move_forward
            
        except Exception as e:
            logger.error(f"Error during streaming transcription: {e}")
    
    def _detect_sentence_boundaries(self, text: str, words: List[WordTiming]) -> Optional[float]:
        """Detect the end time of the SECOND-TO-LAST complete sentence to preserve the last one."""
        if not text or not words:
            return None
            
        # Find all sentence endings
        sentence_endings = re.compile(r'[.!?]+')
        matches = list(sentence_endings.finditer(text))
        
        # We need at least 2 sentences to trim (keep the last complete sentence)
        if len(matches) < 2:
            return None
            
        # Get the second-to-last sentence ending position
        second_last_match = matches[-2]
        sentence_end_pos = second_last_match.end()  # Position after the punctuation
        
        # Find the word that corresponds to this position
        char_count = 0
        for i, word in enumerate(words):
            word_len = len(word.word)
            # If we've reached or passed the sentence boundary position
            if char_count + word_len >= sentence_end_pos:
                return word.end
            char_count += word_len + 1  # +1 for space between words
            
        return None
    
    def _trim_accumulator_to_sentence(self, sentence_end_time: float):
        """Trim the full_audio_accumulator to keep only audio after the last sentence."""
        # Calculate samples to trim from current accumulator
        samples_to_trim = int(sentence_end_time * self.sample_rate)
        total_samples = len(self.full_audio_accumulator)
        
        if samples_to_trim < total_samples:
            # Keep audio from sentence end onwards
            self.full_audio_accumulator = self.full_audio_accumulator[samples_to_trim:].copy()
            
            # Update buffer start time to track where our trimmed buffer starts in absolute time
            self.buffer_start_time += sentence_end_time
            
            # Clean up old word timings that are now before our buffer start
            absolute_trim_time = self.buffer_start_time
            keys_to_remove = [k for k in self.transcribed_words_by_time.keys() if k < absolute_trim_time]
            for key in keys_to_remove:
                del self.transcribed_words_by_time[key]
            
            logger.info(f"Trimmed accumulator: kept {(total_samples - samples_to_trim) / self.sample_rate:.1f}s, buffer now starts at {self.buffer_start_time:.1f}s")
    
    def _extract_word_timings(self, result: Dict[str, Any]) -> List[WordTiming]:
        """Extract word-level timing information from Whisper result."""
        words = []
        
        # Handle different result formats
        if "chunks" in result and result["chunks"]:
            for chunk in result["chunks"]:
                if "timestamp" in chunk and chunk["timestamp"]:
                    start_time, end_time = chunk["timestamp"]
                    if start_time is not None and end_time is not None:
                        # Extract confidence if available (Whisper doesn't always provide this)
                        confidence = getattr(chunk, 'confidence', 0.8)  # Default confidence
                        words.append(WordTiming(
                            word=chunk["text"].strip(),
                            start=float(start_time) + self.buffer_start_time,  # Convert to absolute time
                            end=float(end_time) + self.buffer_start_time,    # Convert to absolute time
                            confidence=confidence
                        ))
        else:
            # Fallback: create single word timing for entire text
            words.append(WordTiming(
                word=result["text"].strip(),
                start=self.buffer_start_time,
                end=self.buffer_start_time + len(self.audio_buffer) / self.sample_rate,
                confidence=0.7
            ))
            
        return words
    
    async def cleanup(self) -> None:
        """Cleanup resources and reset state."""
        # Reset all state
        self.audio_buffer = np.array([], dtype=np.float32)
        self.full_audio_accumulator = np.array([], dtype=np.float32)
        self.processed_audio_length = 0
        self.last_transcription = ""
        self.word_history = []
        self.transcription_count = 0
        self.segment_id = 0
        self.transcribed_words_by_time = {}
        self.buffer_start_time = 0.0
        self.current_buffer_duration_s = self.initial_buffer_duration_s
        self.transcription_pipeline = None
        self.device = None
        self.torch_dtype = None
        logger.info("WhisperNode cleaned up successfully.")

    def get_capabilities(self) -> Optional[Dict[str, Any]]:
        """
        Return capability requirements for Whisper transcription.

        Whisper models benefit significantly from GPU acceleration but can
        run on CPU. Memory requirements vary by model size.

        Returns:
            Capability descriptor with GPU preferences and memory requirements
        """
        # Determine if GPU is required based on device setting
        gpu_required = self._requested_device == "cuda"

        # Estimate memory requirements based on model size
        memory_gb = 4.0  # Base requirement
        if "large" in self.model_id.lower():
            memory_gb = 8.0
        elif "medium" in self.model_id.lower():
            memory_gb = 6.0
        elif "small" in self.model_id.lower():
            memory_gb = 4.0

        capabilities = {"memory_gb": memory_gb}

        # Add GPU requirements if applicable
        if self._requested_device in ["cuda", None]:  # None means GPU optional
            gpu_memory_gb = memory_gb * 0.75  # GPU needs ~75% of total memory
            capabilities["gpu"] = {
                "type": "cuda",
                "min_memory_gb": gpu_memory_gb,
                "required": gpu_required
            }

        return capabilities


__all__ = ["WhisperTranscriptionNode", "WordTiming", "TranscriptionDelta", "WordUpdate"] 