"""
VibeVoice TTS Node using RuntimeData API

A streaming text-to-speech node built on the VibeVoice model family. This node mirrors the
shape and async/streaming behavior of KokoroTTSNode, but integrates the VibeVoice
processor/model and its AudioStreamer. It is designed to be used inside the Pipeline system
(which will handle chunking, sentence aggregation, and audio reformatting upstream/downstream).

Inputs:
- RuntimeData.Text (string)

Outputs:
- RuntimeData.Audio (float32 mono @ sample_rate, streamed in chunks)

Key notes:
- Each next() call on the underlying generator is isolated on a worker thread to avoid PyTorch
  heap corruption when called from Rust/PyO3 on Windows
- Optional voice cloning via preloaded reference audio files
- Model/device/attention/dtype choices follow the public VibeVoice demo patterns
"""

import asyncio
import logging
import queue
import threading
from typing import AsyncGenerator, List, Optional, TYPE_CHECKING

import numpy as np

# RuntimeData bindings (type-safe passthrough with Rust runtime)
if TYPE_CHECKING:
    from remotemedia_runtime.runtime_data import RuntimeData

try:  # Runtime bridge
    from remotemedia_runtime.runtime_data import RuntimeData, numpy_to_audio, audio_to_numpy
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    RuntimeData = None  # type: ignore
    numpy_to_audio = None  # type: ignore
    audio_to_numpy = None  # type: ignore
    logging.warning("RuntimeData bindings not available. Using fallback implementation.")

logger = logging.getLogger(__name__)
if not logger.handlers:
    _h = logging.StreamHandler()
    _h.setLevel(logging.INFO)
    _h.setFormatter(logging.Formatter('%(levelname)s:%(name)s:%(message)s'))
    logger.addHandler(_h)
    logger.setLevel(logging.INFO)


class VibeVoiceTTSNode:
    """Text-to-speech synthesis using VibeVoice with RuntimeData API.

    Parameters are intentionally similar to the public Gradio demo so you can
    move between them easily.
    """

    def __init__(
        self,
        node_id: str,
        model_path: str = "/tmp/vibevoice-model",
        device: Optional[str] = None,  # "cuda" | "mps" | "cpu" | None (auto)
        inference_steps: int = 10,
        cfg_scale: float = 1.3,
        sample_rate: int = 24000,
        stream_chunks: bool = True,
        use_voice_cloning: bool = False,
        voice_samples: Optional[List] = None,  # Can be file paths (str) or embedded audio (dict)
        adapter_path: Optional[str] = None,
        skip_tokens: Optional[List[str]] = None,
        **kwargs,
    ) -> None:
        self.node_id = node_id
        self.model_path = model_path
        self.device = device
        self.inference_steps = inference_steps
        self.cfg_scale = float(cfg_scale)
        self.sample_rate = int(sample_rate)
        self.stream_chunks = bool(stream_chunks)
        self.use_voice_cloning = bool(use_voice_cloning)
        self.adapter_path = adapter_path
        self.skip_tokens = skip_tokens or ['<|text_end|>', '<|audio_end|>', '<|im_end|>', '<|im_start|>']

        # Process voice samples - support both file paths and embedded audio data
        self.voice_samples = []
        self._embedded_audio_data = []  # Store pre-loaded audio numpy arrays

        for sample in (voice_samples or []):
            try:
                if isinstance(sample, str):
                    # Legacy: file path to load later
                    self.voice_samples.append(sample)
                elif isinstance(sample, dict) and 'audio' in sample:
                    # New: embedded audio data from require()
                    logger.info(f"Received embedded audio data, keys: {list(sample['audio'].keys())}")
                    parsed_audio = self._parse_embedded_audio(sample['audio'])
                    if parsed_audio.size > 0:
                        self._embedded_audio_data.append(parsed_audio)
                        logger.info(f"Successfully parsed embedded audio: {len(parsed_audio)} samples")
                    else:
                        logger.warning("Parsed audio is empty, skipping")
                else:
                    logger.warning(f"Ignoring unknown voice sample format: {type(sample)}")
            except Exception as e:
                logger.error(f"Error processing voice sample: {e}")
                import traceback
                logger.error(traceback.format_exc())

        self.is_streaming = True

        # Lazy-loaded heavy deps
        self._processor = None
        self._model = None
        self._initialized = False

    async def initialize(self) -> None:
        """Initialize VibeVoice processor and model.

        We mirror the device/dtype/attention fallback logic in the demo code
        (flash_attention_2 → sdpa) and move tensors to the correct device.
        """
        if self._initialized:
            return

        try:
            import torch
            from vibevoice.processor.vibevoice_processor import VibeVoiceProcessor
            from vibevoice.modular.modeling_vibevoice_inference import (
                VibeVoiceForConditionalGenerationInference,
            )
            from vibevoice.modular.lora_loading import load_lora_assets

            # Normalize/auto-detect device
            dev = self.device
            if dev is None:
                if torch.cuda.is_available():
                    dev = "cuda"
                elif getattr(torch.backends, "mps", None) and torch.backends.mps.is_available():
                    dev = "mps"
                else:
                    dev = "cpu"
            if isinstance(dev, str) and dev.lower() == "mpx":
                dev = "mps"
            self.device = dev

            # Attention + dtype selection
            if self.device == "mps":
                torch_dtype = torch.float32
                attn_impl_primary = "sdpa"
            elif self.device == "cuda":
                torch_dtype = torch.bfloat16
                attn_impl_primary = "flash_attention_2"
            else:
                torch_dtype = torch.float32
                attn_impl_primary = "sdpa"

            logger.info(
                f"Initializing VibeVoice: device={self.device}, dtype={torch_dtype}, attn={attn_impl_primary}"
            )

            # Load processor
            self._processor = VibeVoiceProcessor.from_pretrained(self.model_path)

            # Load model with fallback for attention implementation
            try:
                if self.device == "mps":
                    self._model = VibeVoiceForConditionalGenerationInference.from_pretrained(
                        self.model_path,
                        torch_dtype=torch_dtype,
                        attn_implementation=attn_impl_primary,
                        device_map=None,
                    )
                    self._model.to("mps")
                elif self.device == "cuda":
                    self._model = VibeVoiceForConditionalGenerationInference.from_pretrained(
                        self.model_path,
                        torch_dtype=torch_dtype,
                        device_map="cuda",
                        attn_implementation=attn_impl_primary,
                    )
                else:
                    self._model = VibeVoiceForConditionalGenerationInference.from_pretrained(
                        self.model_path,
                        torch_dtype=torch_dtype,
                        device_map="cpu",
                        attn_implementation=attn_impl_primary,
                    )
            except Exception as e:
                if attn_impl_primary == "flash_attention_2":
                    logger.warning(
                        f"VibeVoice load failed with flash_attention_2 ({type(e).__name__}: {e}); falling back to sdpa"
                    )
                    self._model = VibeVoiceForConditionalGenerationInference.from_pretrained(
                        self.model_path,
                        torch_dtype=torch_dtype,
                        device_map=(self.device if self.device in ("cuda", "cpu") else None),
                        attn_implementation="sdpa",
                    )
                    if self.device == "mps":
                        self._model.to("mps")
                else:
                    raise

            # Optional adapters
            if self.adapter_path:
                try:
                    report = load_lora_assets(self._model, self.adapter_path)
                    loaded_components = [
                        name
                        for name, loaded in (
                            ("language LoRA", report.language_model),
                            ("diffusion head LoRA", report.diffusion_head_lora),
                            ("diffusion head weights", report.diffusion_head_full),
                            ("acoustic connector", report.acoustic_connector),
                            ("semantic connector", report.semantic_connector),
                        )
                        if loaded
                    ]
                    if loaded_components:
                        logger.info(f"Loaded adapters: {', '.join(loaded_components)}")
                except Exception as e:
                    logger.warning(f"Failed to load adapter assets: {e}")

            # Eval mode + scheduler + steps
            self._model.eval()
            # Use SDE solver by default as in demo
            try:
                self._model.model.noise_scheduler = self._model.model.noise_scheduler.from_config(
                    self._model.model.noise_scheduler.config,
                    algorithm_type='sde-dpmsolver++',
                    beta_schedule='squaredcos_cap_v2',
                )
            except Exception:
                # Not all builds expose this; safe to continue
                pass
            try:
                self._model.set_ddpm_inference_steps(num_steps=int(self.inference_steps))
            except Exception:
                pass

            self._initialized = True
            logger.info("VibeVoice model initialized successfully")

        except ImportError as e:
            raise ImportError(
                "VibeVoice is not installed. Install with: pip install vibevoice transformers soundfile librosa"
            ) from e
        except Exception as e:
            logger.error(f"Failed to initialize VibeVoice: {e}")
            raise

    async def cleanup(self) -> None:
        self._model = None
        self._processor = None
        self._initialized = False
        logger.info("VibeVoice TTS node cleaned up")

    def _parse_embedded_audio(self, audio_dict: dict) -> np.ndarray:
        """Parse embedded audio data from require() call.

        Args:
            audio_dict: Dict with keys: samples (bytes/list), sampleRate, channels, format, numSamples

        Returns:
            Numpy array of float32 audio samples at target sample_rate
        """
        try:
            import base64

            samples_data = audio_dict.get('samples')
            source_sr = audio_dict.get('sampleRate', self.sample_rate)
            channels = audio_dict.get('channels', 1)
            audio_format = audio_dict.get('format', 1)  # 0=I16, 1=F32

            # Decode samples from base64 if needed (JSON sends bytes as base64)
            if isinstance(samples_data, str):
                samples_bytes = base64.b64decode(samples_data)
            elif isinstance(samples_data, (bytes, bytearray)):
                samples_bytes = bytes(samples_data)
            elif isinstance(samples_data, dict) and 'data' in samples_data:
                # Handle Buffer objects from Node.js which serialize as {type: 'Buffer', data: [...]}
                samples_bytes = bytes(samples_data['data'])
            else:
                logger.error(f"Unexpected samples data type: {type(samples_data)}")
                return np.array([], dtype=np.float32)

            # Convert bytes to numpy array based on format
            if audio_format == 1:  # F32
                wav = np.frombuffer(samples_bytes, dtype=np.float32)
            elif audio_format == 0:  # I16
                wav_i16 = np.frombuffer(samples_bytes, dtype=np.int16)
                wav = wav_i16.astype(np.float32) / 32768.0  # Normalize to [-1, 1]
            else:
                logger.error(f"Unknown audio format: {audio_format}")
                return np.array([], dtype=np.float32)

            # Handle multi-channel audio - convert to mono
            if channels > 1:
                wav = wav.reshape(-1, channels)
                wav = wav.mean(axis=1)

            # Resample if needed
            if source_sr != self.sample_rate:
                try:
                    import librosa
                    wav = librosa.resample(wav, orig_sr=source_sr, target_sr=self.sample_rate)
                except Exception:
                    # Fallback: naive resample
                    ratio = self.sample_rate / float(source_sr)
                    idx = (np.arange(int(len(wav) * ratio)) / ratio).round().astype(int)
                    idx = np.clip(idx, 0, len(wav) - 1)
                    wav = wav[idx]

            logger.info(f"Parsed embedded audio: {len(wav)} samples @ {self.sample_rate} Hz")
            return wav.astype(np.float32)

        except Exception as e:
            import traceback
            logger.error(f"Failed to parse embedded audio: {e}")
            logger.error(f"Traceback:\n{traceback.format_exc()}")
            return np.array([], dtype=np.float32)

    def _read_audio_file(self, path: str, target_sr: int) -> np.ndarray:
        """Read a reference audio file to mono float32 target_sr."""
        try:
            import soundfile as sf
            wav, sr = sf.read(path)
            if wav.ndim > 1:
                wav = wav.mean(axis=1)
            if sr != target_sr:
                try:
                    import librosa
                    wav = librosa.resample(wav, orig_sr=sr, target_sr=target_sr)
                except Exception:
                    # Fallback: naive resample (not recommended for prod, but avoids hard dep)
                    ratio = target_sr / float(sr)
                    idx = (np.arange(int(len(wav) * ratio)) / ratio).round().astype(int)
                    idx = np.clip(idx, 0, len(wav) - 1)
                    wav = wav[idx]
            return wav.astype(np.float32)
        except Exception as e:
            logger.warning(f"Failed to read voice sample '{path}': {e}")
            return np.array([], dtype=np.float32)

    def _build_inputs(self, text: str):
        import torch
        processor_kwargs = {
            "text": [text],
            "padding": True,
            "return_tensors": "pt",
            "return_attention_mask": True,
        }
        # Pack voice samples if enabled and present
        if self.use_voice_cloning:
            voices: List[np.ndarray] = []

            # Add embedded audio data (from require() calls)
            for audio_np in self._embedded_audio_data:
                if audio_np.size > 0:
                    voices.append(audio_np)

            # Add file-based voice samples
            for p in self.voice_samples:
                if not p:
                    continue
                a = self._read_audio_file(p, self.sample_rate)
                if a.size > 0:
                    voices.append(a)

            if voices:
                logger.info(f"Using {len(voices)} voice samples for cloning")
                processor_kwargs["voice_samples"] = [voices]  # batch=1
            else:
                logger.warning("Voice cloning enabled but no valid voice samples available")

        inputs = self._processor(**processor_kwargs)
        # Move tensors to device and handle None values
        target = self.device if self.device in ("cuda", "mps") else "cpu"
        processed_inputs = {}
        for k, v in list(inputs.items()):
            if v is None:
                # VibeVoice model tries to move tensors without checking for None
                # For speech tensors, skip entirely rather than passing None
                if k in ('speech_tensors', 'speech_masks'):
                    logger.debug(f"Skipping None value for key: {k}")
                    continue
                processed_inputs[k] = v
            elif torch.is_tensor(v):
                processed_inputs[k] = v.to(target)
            else:
                processed_inputs[k] = v
        return processed_inputs

    def _generate_sync_thread(self, inputs, cfg_scale, is_prefill: bool, audio_streamer):
        """
        Run model.generate() synchronously in a dedicated thread (PyTorch-safe).

        This runs in the background and populates the audio_streamer.
        The main thread can iterate audio_streamer.get_stream(0) concurrently.
        """
        try:
            logger.info("Generation thread: Starting")
            logger.info(f"Generation thread: Generate params: cfg_scale={cfg_scale}, is_prefill={is_prefill}")

            # Filter out None tensor values to avoid AttributeError in model.generate()
            # The VibeVoice model tries to move tensors to device without checking for None
            import torch
            filtered_inputs = {}
            for k, v in inputs.items():
                if v is None and k in ('speech_tensors', 'speech_masks'):
                    # Skip None speech tensors/masks - model will use default behavior
                    logger.debug(f"Skipping None value for key: {k}")
                    continue
                filtered_inputs[k] = v

            # Call generate with tokenizer in kwargs (required by VibeVoice)
            # This will block until generation completes, but audio chunks are yielded via the streamer
            result = self._model.generate(
                **filtered_inputs,
                max_new_tokens=None,
                cfg_scale=cfg_scale,
                tokenizer=self._processor.tokenizer,
                audio_streamer=audio_streamer,
                stop_check_fn=None,
                is_prefill=is_prefill,
            )
            logger.info(f"Generation thread: model.generate() completed successfully")

        except Exception as e:
            import traceback
            logger.error(f"Generation thread error: {e}")
            logger.error(f"Generation thread traceback:\n{traceback.format_exc()}")
            # Signal end of stream on error
            try:
                audio_streamer.end()
            except:
                pass

    def _get_next_chunk_sync(self, stream_iter):
        """
        Get the next audio chunk from the stream synchronously (thread-safe).

        This runs in a thread to isolate PyTorch operations from PyO3.

        Args:
            stream_iter: Iterator from VibeVoice AudioStreamer

        Returns:
            Audio tensor or None if exhausted
        """
        try:
            chunk = next(stream_iter)
            return chunk
        except StopIteration:
            return None

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """Process RuntimeData.Text to streamed RuntimeData.Audio chunks."""
        try:
            logger.info("VibeVoice process() called")

            if not self._initialized:
                logger.info("VibeVoice not initialized, initializing now")
                await self.initialize()
                logger.info("VibeVoice initialization complete")

            # Validate input type
            if not data.is_text():
                logger.info("VibeVoice: non-text data, passing through")
                # Pass-through for non-text (keeps pipeline robust)
                yield data
                return

            logger.info("VibeVoice: extracting text from RuntimeData")
            try:
                text = data.as_text()  # extract BEFORE awaits - THIS CROSSES RUST BOUNDARY
                logger.info(f"VibeVoice: extracted text successfully, length: {len(text) if text else 0}")
            except Exception as extract_e:
                logger.error(f"VibeVoice: FAILED to extract text from RuntimeData: {extract_e}")
                import traceback
                logger.error(f"Extract text traceback:\n{traceback.format_exc()}")
                return

            if not text or not text.strip():
                logger.info("VibeVoice: empty text, skipping")
                return

            # CRITICAL: Prevent error message feedback loops - skip synthesizing error messages
            if "ERROR:" in text or "CUDA error" in text or "Traceback" in text or "RuntimeError" in text:
                logger.warning(f"VibeVoice: skipping error message to prevent feedback loop: '{text[:100]}...'")
                return
        except Exception as early_e:
            import traceback
            logger.error(f"VibeVoice: Error in early process() setup: {early_e}")
            logger.error(f"Early process() traceback:\n{traceback.format_exc()}")
            return

        # Remove special tokens / sanitize
        for tok in self.skip_tokens:
            text = text.replace(tok, '')
        text = text.replace('`', "'").replace('\t', ' ').strip()
        if not text:
            logger.info("VibeVoice: only special tokens, skipping")
            return
        # Format text with speaker label and newline (required by VibeVoice processor)
        text = f"Speaker 1: {text}"
        logger.info(f"VibeVoice: starting synthesis for: '{text[:100]}{'...' if len(text) > 100 else ''}'")

        # Build inputs and generate audio using thread-safe approach
        # ARCHITECTURE: Like Kokoro, we use asyncio.to_thread() for ALL PyTorch/CUDA operations
        # to avoid heap corruption on Windows when called through Rust/PyO3
        try:
            logger.info("About to import torch")
            import torch
            logger.info("Torch imported successfully")

            # Build inputs in thread (PyTorch-safe)
            logger.info(f"Building inputs from text (device: {self.device})")

            def build_inputs_sync():
                """Build inputs synchronously in thread."""
                return self._build_inputs(text)

            inputs = await asyncio.to_thread(build_inputs_sync)
            logger.info(f"Inputs built successfully: keys={list(inputs.keys())}")

            # Validate input shapes
            if 'input_ids' in inputs:
                input_shape = inputs['input_ids'].shape
                logger.info(f"Input IDs shape: {input_shape}")
                if len(input_shape) != 2 or input_shape[0] != 1:
                    logger.warning(f"Unexpected input_ids shape: {input_shape}, expected (1, seq_len)")
                if input_shape[1] > self._model.config.decoder_config.max_position_embeddings:
                    logger.error(f"Input sequence length {input_shape[1]} exceeds max_position_embeddings")
                    logger.warning("Truncating input to fit model constraints")
                    max_len = self._model.config.decoder_config.max_position_embeddings
                    inputs['input_ids'] = inputs['input_ids'][:, :max_len]
                    if 'attention_mask' in inputs:
                        inputs['attention_mask'] = inputs['attention_mask'][:, :max_len]
                    logger.info(f"Truncated input shape: {inputs['input_ids'].shape}")

            # Start generation in background thread using the pattern from Gradio demo
            # Key: Don't wait for generation to complete - start iterating stream immediately
            logger.info("Starting generation in background thread")

            from vibevoice.modular.streamer import AudioStreamer
            audio_streamer = AudioStreamer(batch_size=1, stop_signal=None, timeout=None)

            # Start generation in a background thread (don't await!)
            import threading
            generation_thread = threading.Thread(
                target=self._generate_sync_thread,
                args=(inputs, float(self.cfg_scale), True, audio_streamer),
                daemon=True
            )
            generation_thread.start()

            # Give it a moment to start
            await asyncio.sleep(0.1)

            # Get stream iterator immediately (while generation is running in background)
            stream_iter = audio_streamer.get_stream(0)
            logger.info("Generation started, beginning to stream audio chunks")

            # Iterate the stream, running EACH next() call in a thread (like Kokoro)
            total_samples = 0
            chunk_idx = 0
            duration = 0.0  # Initialize to avoid UnboundLocalError

            while True:
                # Get next chunk from stream in thread (PyTorch-safe)
                chunk = await asyncio.to_thread(self._get_next_chunk_sync, stream_iter)

                if chunk is None:
                    # Stream exhausted
                    logger.info(f"Stream exhausted after {chunk_idx} chunks")
                    break

                chunk_idx += 1
                logger.info(f"Processing chunk {chunk_idx}")

                # Convert tensor to numpy (can do this in main thread since we're just converting)
                if torch.is_tensor(chunk):
                    logger.info(f"Chunk is tensor with dtype {chunk.dtype}, shape {chunk.shape}")
                    if chunk.dtype == torch.bfloat16:
                        chunk = chunk.float()
                    audio_np = chunk.detach().cpu().numpy().astype(np.float32)
                else:
                    audio_np = np.asarray(chunk, dtype=np.float32)

                if audio_np.ndim > 1:
                    audio_np = audio_np.squeeze()

                total_samples += len(audio_np)
                duration = total_samples / float(self.sample_rate)
                logger.info(
                    f"VibeVoice: yielding chunk {chunk_idx} ({len(audio_np)/self.sample_rate:.2f}s) | total {duration:.2f}s"
                )

                # Convert to RuntimeData.Audio and yield
                audio_data = numpy_to_audio(audio_np, self.sample_rate, channels=1)
                yield audio_data

            logger.info(f"Streaming complete: {chunk_idx} chunks, {total_samples} samples ({duration:.2f}s)")

        except RuntimeError as e:
            import traceback
            if "CUDA error" in str(e):
                logger.error("VibeVoice: CUDA error during synthesis; clearing CUDA context and reinitializing")
                logger.error(f"CUDA error traceback:\n{traceback.format_exc()}")

                # Clear CUDA error state and free memory
                try:
                    import torch
                    if torch.cuda.is_available():
                        logger.info("VibeVoice: Clearing CUDA cache and synchronizing")
                        torch.cuda.empty_cache()
                        torch.cuda.synchronize()
                        # Reset peak memory stats
                        torch.cuda.reset_peak_memory_stats()
                        logger.info("VibeVoice: CUDA context cleared")
                except Exception as cuda_clear_e:
                    logger.error(f"VibeVoice: Error clearing CUDA context: {cuda_clear_e}")

                # Reinitialize the model
                self._initialized = False
                self._processor = None
                self._model = None
                try:
                    logger.info("VibeVoice: Reinitializing model after CUDA error")
                    await self.initialize()
                    logger.info("VibeVoice: Reinitialization completed successfully")
                except Exception as reinit_e:
                    logger.error(f"VibeVoice: reinit failed: {reinit_e}")
                    logger.error(f"Reinit traceback:\n{traceback.format_exc()}")
                return
            else:
                logger.error(f"VibeVoice: runtime error: {e}")
                logger.error(f"Runtime error traceback:\n{traceback.format_exc()}")
                raise
        except Exception as e:
            import traceback
            logger.error(f"VibeVoice: unexpected error: {e}")
            logger.error(f"Unexpected error traceback:\n{traceback.format_exc()}")
            raise
        finally:
            logger.info("VibeVoice: cleanup complete")

        logger.info("VibeVoice: streaming complete")

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "VibeVoiceTTSNode",
            "model_path": self.model_path,
            "device": self.device,
            "inference_steps": self.inference_steps,
            "cfg_scale": self.cfg_scale,
            "sample_rate": self.sample_rate,
            "stream_chunks": self.stream_chunks,
            "use_voice_cloning": self.use_voice_cloning,
            "voice_samples": list(self.voice_samples),
            "adapter_path": self.adapter_path,
        }


# Example usage (manual quick test)
async def main():
    if not RUNTIME_DATA_AVAILABLE:
        print("RuntimeData bindings not available. Please build the Rust extension (cargo build --release).")
        return

    print("=" * 60)
    print("VibeVoice TTS Node with RuntimeData API")
    print("=" * 60)

    import os
    # Get the path to the voice sample relative to the SDK root
    # File structure: python-client/remotemedia/nodes/tts_vibevoice.py -> go up 4 levels to SDK root
    sdk_root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    voice_sample_path = os.path.join(sdk_root, "examples", "transcribe_demo.wav")
    print(f"Voice sample path: {voice_sample_path}")
    
    node = VibeVoiceTTSNode(
        node_id="vibevoice_tts_1",
        model_path="vibevoice/VibeVoice-1.5B",
        device=None,  # auto
        inference_steps=10,
        cfg_scale=1.3,
        sample_rate=24000,
        use_voice_cloning=True,
        voice_samples=[voice_sample_path],
    )

    await node.initialize()

    # Prepare input text (longer text for more audio generation)
    text_in = RuntimeData.text(
        "Hello! This is a demonstration of the VibeVoice text-to-speech node using the RuntimeData API. "
        "The system supports real-time streaming of high-quality speech synthesis with voice cloning capabilities. "
        "You can use custom voice samples to generate speech that matches the characteristics of the reference audio. "
        "This technology enables natural-sounding text-to-speech for various applications including virtual assistants, "
        "audiobook narration, and accessibility tools."
    )

    print("\nSynthesizing...")
    chunks = []
    async for audio_chunk in node.process(text_in):
        arr = audio_to_numpy(audio_chunk)
        chunks.append(arr)
        print(f"  Got chunk: {len(arr)} samples")

    await node.cleanup()

    total = sum(len(c) for c in chunks)
    dur = total / float(node.sample_rate)
    print(f"\nDone. Chunks={len(chunks)}, total={total} samples ({dur:.2f}s) @ {node.sample_rate} Hz")


if __name__ == "__main__":
    asyncio.run(main())
