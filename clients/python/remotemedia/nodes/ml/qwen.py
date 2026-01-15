import asyncio
import logging
from typing import Optional, Any, List, Dict, Tuple, AsyncGenerator
import numpy as np
import tempfile
import os
import requests
import copy
import soundfile as sf
import librosa

from remotemedia.core.node import Node
from remotemedia.core.exceptions import NodeError, ConfigurationError

# Configure basic logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

try:
    import torch
    import av
    from transformers import Qwen2_5OmniForConditionalGeneration, Qwen2_5OmniProcessor
    from qwen_omni_utils import process_mm_info
except ImportError:
    logger.warning("ML libraries not found. Qwen2_5OmniNode will not be available.")
    torch = None
    av = None
    Qwen2_5OmniForConditionalGeneration = None
    Qwen2_5OmniProcessor = None
    process_mm_info = None

class Qwen2_5OmniNode(Node):
    """
    A node that uses the Qwen2.5-Omni model for multimodal generation from a stream.
    https://huggingface.co/Qwen/Qwen2.5-Omni-3B
    """

    def __init__(self,
                 model_id: str = "Qwen/Qwen2.5-Omni-3B",
                 device: Optional[str] = None,
                 torch_dtype: str = "auto",
                 attn_implementation: Optional[str] = None,
                 conversation_template: Optional[List[Dict[str, Any]]] = None,
                 buffer_duration_s: float = 5.0,
                 video_fps: int = 10,
                 audio_sample_rate: int = 16000,
                 speaker: Optional[str] = None,
                 use_audio_in_video: bool = True,
                 **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.is_streaming = True
        self.model_id = model_id
        self._requested_device = device
        self.torch_dtype_str = torch_dtype
        self.attn_implementation = attn_implementation
        self.conversation_template = conversation_template or []
        self.buffer_duration_s = buffer_duration_s
        self.video_fps = video_fps
        self.audio_sample_rate = audio_sample_rate
        self.speaker = speaker
        self.use_audio_in_video = use_audio_in_video

        self.model = None
        self.processor = None
        self.device = None
        self.torch_dtype = None
        
        self.video_buffer = []
        self.audio_buffer = []
        self.video_buffer_max_frames = int(self.buffer_duration_s * self.video_fps)

    async def initialize(self) -> None:
        """
        Load the model and processor. This runs on the execution environment (local or remote).
        """
        if not all([torch, av, Qwen2_5OmniForConditionalGeneration, Qwen2_5OmniProcessor, process_mm_info]):
             raise NodeError("Required ML libraries (torch, transformers, soundfile, pyav, qwen_omni_utils) are not installed.")

        if self._requested_device:
            self.device = self._requested_device
        elif torch.cuda.is_available():
            self.device = "cuda:0"
        elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            self.device = "mps"
        else:
            self.device = "cpu"

        if self.torch_dtype_str == "auto":
            self.torch_dtype = "auto"
        else:
            try:
                self.torch_dtype = getattr(torch, self.torch_dtype_str)
            except AttributeError:
                raise ConfigurationError(f"Invalid torch_dtype '{self.torch_dtype_str}'")

        logger.info(f"Qwen2.5-Omni configured for model '{self.model_id}' on device '{self.device}'")
        
        model_kwargs = {
            "torch_dtype": self.torch_dtype,
            "device_map": "auto" if self.device != "cpu" else "cpu"
        }

        if self.attn_implementation:
            model_kwargs["attn_implementation"] = self.attn_implementation
            logger.info(f"Using attn_implementation: {self.attn_implementation}")
        
        try:
            self.model = await asyncio.to_thread(
                Qwen2_5OmniForConditionalGeneration.from_pretrained, self.model_id, **model_kwargs)
            self.processor = await asyncio.to_thread(
                Qwen2_5OmniProcessor.from_pretrained, self.model_id)
            
            if self.device == "mps":
                self.model.to(self.device)

            logger.info("Qwen2.5-Omni model initialized successfully.")
        except Exception as e:
            raise NodeError(f"Failed to initialize Qwen2.5-Omni model: {e}")

    async def _run_inference(self) -> Optional[Tuple[List[str], Optional[np.ndarray]]]:
        if not self.video_buffer and not self.audio_buffer:
            return None
        
        self.logger.info(
            f"Starting inference with "
            f"{len(self.video_buffer)} video frames and "
            f"{len(self.audio_buffer)} audio chunks."
        )
        
        with tempfile.TemporaryDirectory() as temp_dir:
            final_conversation = copy.deepcopy(self.conversation_template)
            video_path, audio_path = None, None
            
            # Save buffered video to a temporary file
            if self.video_buffer:
                video_path = await self._save_video_buffer(temp_dir)

            # Save buffered audio to a temporary file
            if self.audio_buffer:
                audio_path = await self._save_audio_buffer(temp_dir)
            
            # Inject media paths into conversation template
            self._inject_media_paths(final_conversation, video_path, audio_path)

            def _inference_thread():
                text = self.processor.apply_chat_template(final_conversation, add_generation_prompt=True, tokenize=False)
                audios, images, videos = process_mm_info(final_conversation, use_audio_in_video=self.use_audio_in_video)
                
                inputs = self.processor(text=text, audio=audios, images=images, videos=videos, return_tensors="pt", padding=True, use_audio_in_video=self.use_audio_in_video)
                inputs = inputs.to(self.model.device)
                if self.torch_dtype != 'auto':
                    inputs = inputs.to(self.torch_dtype)

                generate_kwargs = {"use_audio_in_video": self.use_audio_in_video}
                if self.speaker:
                    generate_kwargs["speaker"] = self.speaker

                text_ids, audio_tensor = self.model.generate(**inputs, **generate_kwargs)
                
                decoded_text = self.processor.batch_decode(text_ids, skip_special_tokens=True, clean_up_tokenization_spaces=False)
                audio_np = audio_tensor.reshape(-1).detach().cpu().numpy() if audio_tensor is not None and audio_tensor.numel() > 0 else None
                return decoded_text, audio_np

            return await asyncio.to_thread(_inference_thread)

    async def process(self, data_stream: AsyncGenerator[Any, None]) -> AsyncGenerator[Any, None]:
        # Lazy initialization: ensure the model is loaded before processing.
        if not self.is_initialized:
            await self.initialize()

        async for item in data_stream:
            # This node expects dictionaries from MediaReaderNode
            if not isinstance(item, dict):
                self.logger.warning(f"Qwen node received non-dict item, skipping. Got {type(item)}")
                continue

            if 'video' in item and isinstance(item['video'], av.VideoFrame):
                frame = item['video']
                self.video_buffer.append(frame.to_ndarray(format='rgb24'))
                self.logger.info(f"QwenNode: Buffered video frame #{len(self.video_buffer)}.")

            elif 'audio' in item and isinstance(item['audio'], av.AudioFrame):
                frame = item['audio']
                # Resample audio to the rate expected by the model
                resampled_chunk = librosa.resample(
                    frame.to_ndarray().astype(np.float32).mean(axis=0),
                    orig_sr=frame.sample_rate,
                    target_sr=self.audio_sample_rate
                )
                self.audio_buffer.append(resampled_chunk)
                self.logger.info(f"QwenNode: Buffered audio chunk #{len(self.audio_buffer)}.")

            # Check if video buffer is full enough to trigger processing
            if len(self.video_buffer) >= self.video_buffer_max_frames:
                self.logger.info(f"Video buffer full ({len(self.video_buffer)} frames). Triggering inference.")
                result = await self._run_inference()
                if result:
                    yield result
                self.video_buffer.clear()
                self.audio_buffer.clear()
        
        # After the stream is exhausted, process any remaining data in the buffer
        if self.video_buffer or self.audio_buffer:
            self.logger.info(
                f"Input stream finished. Processing remaining buffer with "
                f"{len(self.video_buffer)} video frames and {len(self.audio_buffer)} audio chunks."
            )
            result = await self._run_inference()
            if result:
                yield result
            self.video_buffer.clear()
            self.audio_buffer.clear()

    async def cleanup(self) -> None:
        # Buffers are now flushed in process(), so cleanup just releases resources.
        self.video_buffer.clear()
        self.audio_buffer.clear()
        
        logger.info(f"Cleaning up node '{self.name}'.")
        del self.model
        del self.processor
        self.model = None
        self.processor = None
        if torch and torch.cuda.is_available():
            torch.cuda.empty_cache()

    async def _save_video_buffer(self, temp_dir: str) -> str:
        video_path = os.path.join(temp_dir, "temp_video.mp4")
        first_frame = self.video_buffer[0]
        height, width, _ = first_frame.shape
        
        output_container = av.open(video_path, mode='w')
        stream = output_container.add_stream('libx264', rate=self.video_fps)
        stream.width = width
        stream.height = height
        stream.pix_fmt = 'yuv420p'

        for frame_data in self.video_buffer:
            frame = av.VideoFrame.from_ndarray(frame_data, format='rgb24')
            for packet in stream.encode(frame):
                output_container.mux(packet)
        
        for packet in stream.encode(): # Flush
            output_container.mux(packet)
        output_container.close()
        return video_path

    async def _save_audio_buffer(self, temp_dir: str) -> str:
        audio_path = os.path.join(temp_dir, "temp_audio.wav")
        full_audio = np.concatenate(self.audio_buffer)
        await asyncio.to_thread(sf.write, audio_path, full_audio, self.audio_sample_rate)
        return audio_path

    def _inject_media_paths(self, conversation, video_path, audio_path):
        for turn in conversation:
            content = turn.get("content")
            if isinstance(content, list):
                for item in content:
                    if video_path and item.get("type") == "video" and item.get("video") == "<video_placeholder>":
                        item["video"] = video_path
                    if audio_path and item.get("type") == "audio" and item.get("audio") == "<audio_placeholder>":
                        item["audio"] = audio_path

__all__ = ["Qwen2_5OmniNode"] 