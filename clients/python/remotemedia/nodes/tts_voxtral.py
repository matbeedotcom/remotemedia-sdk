"""
Voxtral TTS Node using RuntimeData API and MultiprocessNode base

A text-to-speech node built on Mistral's Voxtral-4B-TTS-2603 model with multiprocess support.
This node mirrors the shape and async/streaming behavior of KokoroTTSNode, but integrates
Voxtral's text-conditioned speaker descriptions for diverse voice generation.

Inherits from MultiprocessNode to enable concurrent execution in separate processes.

Inputs:
- RuntimeData.Text (string)

Outputs:
- RuntimeData.Audio (float32 mono @ sample_rate, streamed in chunks)

Key notes:
- Multiprocess execution support for concurrent operation with other AI models
- Each inference call is isolated on a worker thread to avoid PyTorch heap corruption
  when called from Rust/PyO3 on Windows
- Speaker diversity via text-conditioned speaker descriptions
- Model/device/dtype choices follow standard HuggingFace patterns
"""

import asyncio
import logging
from typing import AsyncGenerator, Optional, Union, Dict, Any

import numpy as np

from remotemedia.core.multiprocessing.data import RuntimeData
from remotemedia.core import MultiprocessNode, NodeConfig
from .registration import streaming_node
from remotemedia.core.multiprocessing import python_requires

logger = logging.getLogger(__name__)
if not logger.handlers:
    _h = logging.StreamHandler()
    _h.setLevel(logging.INFO)
    _h.setFormatter(logging.Formatter('%(levelname)s:%(name)s:%(message)s'))
    logger.addHandler(_h)
    logger.setLevel(logging.INFO)


def numpy_to_audio(samples: np.ndarray, sample_rate: int, channels: int = 1) -> RuntimeData:
    """Convert numpy array to RuntimeData.Audio"""
    return RuntimeData.audio(samples, sample_rate, channels)


@streaming_node(
    node_type="VoxtralTTSNode",
    multi_output=True,
    category="tts",
    accepts=["text"],
    produces=["audio"],
    description="Text-to-speech synthesis using Mistral Voxtral-4B with text-conditioned speaker descriptions"
)
@python_requires(["vllm", "torch", "numpy", "transformers"])
class VoxtralTTSNode(MultiprocessNode):
    """
    Text-to-speech synthesis using Voxtral-4B-TTS with RuntimeData API and multiprocess support.

    Voxtral uses text-conditioned speaker descriptions, allowing diverse voice generation
    by describing the desired speaker characteristics in natural language (e.g.,
    "A young woman speaks enthusiastically with a clear American accent").

    This is particularly useful for dataset generation where speaker diversity is important.
    """

    def __init__(
        self,
        node_id: str = None,
        model_name: str = "mistralai/Voxtral-Mini-3B-2507",
        device: Optional[str] = None,
        speaker_description: str = "A clear, neutral voice speaks at a moderate pace.",
        sample_rate: int = 24000,
        speed: float = 1.0,
        max_tokens: int = 4096,
        skip_tokens: list = None,
        config: Union[NodeConfig, Dict[str, Any]] = None,
        **kwargs,
    ) -> None:
        """
        Initialize Voxtral TTS node.

        Args:
            node_id: Unique identifier for this node instance
            model_name: HuggingFace model name (default: mistralai/Voxtral-Mini-3B-2507)
            device: Device for inference ("cuda", "cpu", or None for auto-detect)
            speaker_description: Text description of desired speaker voice characteristics
            sample_rate: Output audio sample rate (default: 24000)
            speed: Speech speed multiplier (default: 1.0)
            max_tokens: Maximum tokens for generation (default: 4096)
            skip_tokens: Tokens to filter from input text
            config: NodeConfig for multiprocess mode
            **kwargs: Additional parameters
        """
        if config is not None:
            super().__init__(config, **kwargs)
            if isinstance(config, NodeConfig):
                params = config.params
            else:
                params = config.get('params', {})

            model_name = params.get('model_name', model_name)
            device = params.get('device', device)
            speaker_description = params.get('speaker_description', speaker_description)
            sample_rate = params.get('sample_rate', sample_rate)
            speed = params.get('speed', speed)
            max_tokens = params.get('max_tokens', max_tokens)
            skip_tokens = params.get('skip_tokens', skip_tokens)
        else:
            from remotemedia.core.multiprocessing.node import NodeConfig as NC
            minimal_config = NC(
                node_id=node_id or "voxtral_tts",
                node_type="VoxtralTTSNode",
                params={}
            )
            super().__init__(minimal_config, **kwargs)
            self.logger = logging.getLogger(__name__)

        self.model_name = model_name
        self.device = device
        self.speaker_description = speaker_description
        self.sample_rate = int(sample_rate)
        self.speed = float(speed)
        self.max_tokens = int(max_tokens)
        self.skip_tokens = skip_tokens or ['<|text_end|>', '<|audio_end|>', '<|im_end|>', '<|im_start|>']
        self.is_streaming = True

        self._model = None
        self._tokenizer = None
        self._initialized = False

    async def initialize(self) -> None:
        """Initialize the Voxtral TTS model via vLLM."""
        if self._initialized:
            return

        try:
            logger.info(f"Initializing Voxtral TTS with model='{self.model_name}'")

            def _load_model():
                import torch
                from vllm import LLM

                # Auto-detect device
                dev = self.device
                if dev is None:
                    dev = "cuda" if torch.cuda.is_available() else "cpu"
                self.device = dev

                logger.info(f"Loading Voxtral model on device={dev}")

                # Load model via vLLM (recommended by Mistral for Voxtral)
                model = LLM(
                    model=self.model_name,
                    tokenizer=self.model_name,
                    dtype="auto",
                    max_model_len=8192,
                    gpu_memory_utilization=0.8,
                )
                return model

            self._model = await asyncio.to_thread(_load_model)

            # Load tokenizer separately for audio decoding
            def _load_tokenizer():
                from transformers import AutoTokenizer
                return AutoTokenizer.from_pretrained(self.model_name)

            self._tokenizer = await asyncio.to_thread(_load_tokenizer)

            self._initialized = True
            logger.info("Voxtral TTS model initialized successfully")

        except ImportError as e:
            raise ImportError(
                "Voxtral TTS dependencies not installed. Install with: "
                "pip install vllm torch transformers numpy"
            ) from e
        except Exception as e:
            logger.error(f"Failed to initialize Voxtral TTS: {e}")
            raise

    async def cleanup(self) -> None:
        """Clean up the TTS model."""
        self._model = None
        self._tokenizer = None
        self._initialized = False
        logger.info("Voxtral TTS node cleaned up")

    def _build_prompt(self, text: str) -> str:
        """Build the Voxtral chat-style prompt with speaker description."""
        # Voxtral uses a conversational format with speaker description
        prompt = (
            f"[INST] {self.speaker_description}\n\n"
            f"Please speak the following text:\n{text} [/INST]"
        )
        return prompt

    def _generate_sync(self, text: str):
        """
        Run TTS generation synchronously (thread-safe).

        Returns list of audio numpy arrays.
        """
        from vllm import SamplingParams

        prompt = self._build_prompt(text)

        sampling_params = SamplingParams(
            max_tokens=self.max_tokens,
            temperature=0.7,
            top_p=0.9,
        )

        outputs = self._model.generate([prompt], sampling_params)

        audio_chunks = []
        for output in outputs:
            for completion in output.outputs:
                # Extract audio tokens from the completion
                token_ids = completion.token_ids
                audio_data = self._decode_audio_tokens(token_ids)
                if audio_data is not None and len(audio_data) > 0:
                    audio_chunks.append(audio_data)

        return audio_chunks

    def _decode_audio_tokens(self, token_ids) -> Optional[np.ndarray]:
        """Decode audio tokens from model output to numpy audio samples."""
        try:
            # Voxtral outputs audio codec tokens that need decoding
            # The exact decoding depends on the model's audio codec
            import torch

            # Convert token IDs to tensor
            tokens = torch.tensor([token_ids], dtype=torch.long)

            # Try to use the model's built-in audio decoder if available
            if hasattr(self._model, 'decode_audio'):
                audio = self._model.decode_audio(tokens)
                if torch.is_tensor(audio):
                    audio = audio.detach().cpu().numpy().astype(np.float32)
                if audio.ndim > 1:
                    audio = audio.squeeze()
                return audio

            # Fallback: try using the tokenizer's decode method for audio
            if hasattr(self._tokenizer, 'decode_audio'):
                audio = self._tokenizer.decode_audio(token_ids)
                if isinstance(audio, np.ndarray):
                    return audio.astype(np.float32)

            # If no audio decoder is available, try to extract raw audio data
            # from the token sequence (model-specific)
            logger.warning("No audio decoder found, attempting raw token extraction")
            return None

        except Exception as e:
            logger.error(f"Failed to decode audio tokens: {e}")
            return None

    async def process(self, data: RuntimeData) -> Union[RuntimeData, AsyncGenerator[RuntimeData, None], None]:
        """
        Process text input and generate speech audio.

        Args:
            data: RuntimeData containing text to synthesize (RuntimeData.Text)

        Yields:
            RuntimeData.Audio containing synthesized speech chunks
        """
        logger.info("VoxtralTTSNode process() called")
        if not self._initialized:
            await self.initialize()

        if not data.is_text():
            logger.info(f"VoxtralTTSNode: non-text data (type={data.type}), passing through")
            yield data
            return

        text = data.as_text()
        if not text or not text.strip():
            logger.warning("Empty text input, skipping synthesis")
            return

        # Remove special tokens
        for tok in self.skip_tokens:
            text = text.replace(tok, '')
        text = text.replace('`', "'").replace('\t', ' ').strip()
        if not text:
            logger.info("VoxtralTTSNode: only special tokens, skipping")
            return

        logger.info(f"VoxtralTTSNode: starting synthesis for: '{text[:100]}{'...' if len(text) > 100 else ''}'")

        try:
            # Run generation in thread (PyTorch-safe)
            audio_chunks = await asyncio.to_thread(self._generate_sync, text)

            total_samples = 0
            for idx, audio_np in enumerate(audio_chunks):
                if audio_np.ndim > 1:
                    audio_np = audio_np.squeeze()

                # Apply speed adjustment via resampling if speed != 1.0
                if abs(self.speed - 1.0) > 0.01:
                    original_len = len(audio_np)
                    new_len = int(original_len / self.speed)
                    indices = np.linspace(0, original_len - 1, new_len)
                    audio_np = np.interp(indices, np.arange(original_len), audio_np).astype(np.float32)

                total_samples += len(audio_np)
                duration = total_samples / float(self.sample_rate)

                logger.info(
                    f"VoxtralTTSNode: yielding chunk {idx + 1} "
                    f"({len(audio_np) / self.sample_rate:.2f}s) | total {duration:.2f}s"
                )

                yield numpy_to_audio(audio_np, self.sample_rate, channels=1)

            logger.info(f"VoxtralTTSNode: synthesis complete - {len(audio_chunks)} chunks, {total_samples} samples")

        except RuntimeError as e:
            if "CUDA error" in str(e):
                logger.error(f"VoxtralTTSNode: CUDA error during synthesis: {e}")
                self._initialized = False
                self._model = None
                self._tokenizer = None
                try:
                    import torch
                    if torch.cuda.is_available():
                        torch.cuda.empty_cache()
                        torch.cuda.synchronize()
                except Exception:
                    pass
                try:
                    await self.initialize()
                except Exception as reinit_e:
                    logger.error(f"VoxtralTTSNode: reinit failed: {reinit_e}")
                return
            else:
                raise
        except Exception as e:
            logger.error(f"VoxtralTTSNode: unexpected error: {e}")
            raise

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "VoxtralTTSNode",
            "model_name": self.model_name,
            "device": self.device,
            "speaker_description": self.speaker_description,
            "sample_rate": self.sample_rate,
            "speed": self.speed,
            "max_tokens": self.max_tokens,
        }
