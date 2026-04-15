"""
CosyVoice3 TTS Node using RuntimeData API and MultiprocessNode base

A text-to-speech node built on FunAudioLLM's CosyVoice3-0.5B model with multiprocess support.
This node mirrors the shape and async/streaming behavior of KokoroTTSNode.

CosyVoice3 supports zero-shot voice cloning, cross-lingual synthesis, and instruction-guided
voice control (dialect, emotion, speed). The model generates 24kHz mono audio.

Inputs:
- RuntimeData.Text (string)

Outputs:
- RuntimeData.Audio (float32 mono @ 24000 Hz, streamed in chunks)

Requirements:
- CosyVoice must be installed from source: https://github.com/FunAudioLLM/CosyVoice
- Model weights: FunAudioLLM/Fun-CosyVoice3-0.5B-2512
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
    node_type="CosyVoice3TTSNode",
    multi_output=True,
    category="tts",
    accepts=["text"],
    produces=["audio"],
    description="Text-to-speech synthesis using CosyVoice3 with zero-shot voice cloning and instruction-guided control"
)
@python_requires(["torch", "torchaudio", "numpy", "tqdm", "huggingface_hub", "modelscope", "hyperpyyaml", "conformer", "onnxruntime-gpu", "diffusers", "lightning", "openai-whisper", "numba>=0.60", "coverage<7.8"])
class CosyVoice3TTSNode(MultiprocessNode):
    """
    Text-to-speech synthesis using CosyVoice3 with RuntimeData API and multiprocess support.

    Supports multiple inference modes:
    - SFT: Use pre-registered speaker voices
    - Zero-shot: Clone any voice from a reference audio clip
    - Instruct: Control dialect, emotion, speed via natural language instructions

    Output is 24kHz mono float32 audio.
    """

    def __init__(
        self,
        node_id: str = None,
        model_dir: str = "pretrained_models/Fun-CosyVoice3-0.5B",
        cosyvoice_repo: Optional[str] = None,
        mode: str = "sft",
        spk_id: str = "",
        prompt_text: str = "",
        prompt_wav: Optional[str] = None,
        instruct_text: str = "",
        speed: float = 1.0,
        stream: bool = False,
        skip_tokens: list = None,
        config: Union[NodeConfig, Dict[str, Any]] = None,
        **kwargs,
    ) -> None:
        """
        Initialize CosyVoice3 TTS node.

        Args:
            node_id: Unique identifier for this node instance
            model_dir: Path to model weights directory
            cosyvoice_repo: Path to CosyVoice repo clone (for sys.path)
            mode: Inference mode - "sft", "zero_shot", "cross_lingual", or "instruct"
            spk_id: Speaker ID for SFT mode
            prompt_text: Transcript of prompt audio for zero-shot/instruct modes
            prompt_wav: Path to reference audio WAV for zero-shot/instruct modes
            instruct_text: Instruction text for instruct mode (e.g., "Speak with excitement")
            speed: Speech speed multiplier (default: 1.0)
            stream: Enable streaming output (default: False)
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

            model_dir = params.get('model_dir', model_dir)
            cosyvoice_repo = params.get('cosyvoice_repo', cosyvoice_repo)
            mode = params.get('mode', mode)
            spk_id = params.get('spk_id', spk_id)
            prompt_text = params.get('prompt_text', prompt_text)
            prompt_wav = params.get('prompt_wav', prompt_wav)
            instruct_text = params.get('instruct_text', instruct_text)
            speed = params.get('speed', speed)
            stream = params.get('stream', stream)
            skip_tokens = params.get('skip_tokens', skip_tokens)
        else:
            from remotemedia.core.multiprocessing.node import NodeConfig as NC
            minimal_config = NC(
                node_id=node_id or "cosyvoice3_tts",
                node_type="CosyVoice3TTSNode",
                params={}
            )
            super().__init__(minimal_config, **kwargs)
            self.logger = logging.getLogger(__name__)

        self.model_dir = model_dir
        self.cosyvoice_repo = cosyvoice_repo
        self.mode = mode
        self.spk_id = spk_id
        self.prompt_text = prompt_text
        self.prompt_wav = prompt_wav
        self.instruct_text = instruct_text
        self.speed = float(speed)
        self.stream = bool(stream)
        self.sample_rate = 24000  # CosyVoice3 outputs 24kHz
        self.skip_tokens = skip_tokens or ['<|text_end|>', '<|audio_end|>', '<|im_end|>', '<|im_start|>']
        self.is_streaming = True

        self._model = None
        self._initialized = False

    def _ensure_cosyvoice_repo(self) -> str:
        """Clone the CosyVoice repo if not present. Returns the repo path."""
        import subprocess
        from pathlib import Path

        if self.cosyvoice_repo:
            repo_path = Path(self.cosyvoice_repo)
            if repo_path.exists() and (repo_path / "cosyvoice").exists():
                return str(repo_path)

        # Default cache location
        cache_dir = Path.home() / ".cache" / "cosyvoice"
        repo_path = cache_dir / "CosyVoice"

        if repo_path.exists() and (repo_path / "cosyvoice").exists():
            logger.info(f"Using cached CosyVoice repo at {repo_path}")
            return str(repo_path)

        logger.info(f"Cloning CosyVoice repo to {repo_path}...")
        cache_dir.mkdir(parents=True, exist_ok=True)

        subprocess.run(
            ["git", "clone", "--recursive", "https://github.com/FunAudioLLM/CosyVoice.git", str(repo_path)],
            check=True,
            capture_output=True,
            text=True,
        )
        logger.info("CosyVoice repo cloned successfully")

        # Install requirements from the repo (skip version pins that conflict)
        req_file = repo_path / "requirements.txt"
        if req_file.exists():
            logger.info("Installing CosyVoice requirements...")
            try:
                subprocess.run(
                    ["pip", "install", "--no-deps", "-r", str(req_file)],
                    capture_output=True,
                    text=True,
                    timeout=300,
                )
            except Exception as e:
                logger.warning(f"Some CosyVoice requirements failed to install: {e}")
            # Also install commonly missed transitive deps
            try:
                subprocess.run(
                    ["pip", "install", "tqdm", "pyyaml", "scipy", "librosa", "inflect", "gdown", "lightning"],
                    capture_output=True,
                    text=True,
                    timeout=120,
                )
            except Exception:
                pass

        return str(repo_path)

    def _ensure_model_weights(self, repo_path: str) -> str:
        """Download model weights if not present. Returns the model directory."""
        from pathlib import Path

        model_dir = Path(self.model_dir)

        # If it's an absolute path that exists, use it
        if model_dir.is_absolute() and model_dir.exists():
            return str(model_dir)

        # Try relative to repo
        repo_model_dir = Path(repo_path) / self.model_dir
        if repo_model_dir.exists():
            return str(repo_model_dir)

        # Download from HuggingFace
        cache_model_dir = Path(repo_path) / "pretrained_models" / "Fun-CosyVoice3-0.5B"
        if cache_model_dir.exists() and any(cache_model_dir.iterdir()):
            return str(cache_model_dir)

        logger.info("Downloading CosyVoice3 model weights from HuggingFace...")
        from huggingface_hub import snapshot_download
        snapshot_download(
            "FunAudioLLM/Fun-CosyVoice3-0.5B-2512",
            local_dir=str(cache_model_dir),
        )
        logger.info(f"Model weights downloaded to {cache_model_dir}")
        return str(cache_model_dir)

    async def initialize(self) -> None:
        """Initialize the CosyVoice3 model."""
        if self._initialized:
            return

        try:
            import sys

            # Auto-clone repo if needed
            repo_path = await asyncio.to_thread(self._ensure_cosyvoice_repo)
            self.cosyvoice_repo = repo_path

            # Add CosyVoice repo and Matcha-TTS submodule to sys.path
            for path in [repo_path, f"{repo_path}/third_party/Matcha-TTS"]:
                if path not in sys.path:
                    sys.path.insert(0, path)

            # Ensure model weights are downloaded
            model_dir = await asyncio.to_thread(self._ensure_model_weights, repo_path)

            logger.info(f"Initializing CosyVoice3 with model_dir='{model_dir}', mode='{self.mode}'")

            def _load_model():
                from cosyvoice.cli.cosyvoice import AutoModel
                return AutoModel(model_dir=model_dir)

            self._model = await asyncio.to_thread(_load_model)

            # Update sample_rate from model
            if hasattr(self._model, 'sample_rate'):
                self.sample_rate = self._model.sample_rate

            # Log available speakers for SFT mode
            if self.mode == "sft" and hasattr(self._model, 'list_available_spks'):
                spks = self._model.list_available_spks()
                logger.info(f"Available speakers: {spks}")
                if not self.spk_id and spks:
                    self.spk_id = spks[0]
                    logger.info(f"Using default speaker: {self.spk_id}")

            self._initialized = True
            logger.info("CosyVoice3 model initialized successfully")

        except ImportError as e:
            raise ImportError(
                "CosyVoice3 is not pip-installable. Manual setup required:\n"
                "  1. git clone --recursive https://github.com/FunAudioLLM/CosyVoice.git\n"
                "  2. cd CosyVoice && pip install -r requirements.txt\n"
                "  3. Download model: python -c \"from huggingface_hub import snapshot_download; "
                "snapshot_download('FunAudioLLM/Fun-CosyVoice3-0.5B-2512', local_dir='pretrained_models/Fun-CosyVoice3-0.5B')\"\n"
                "  4. Set cosyvoice_repo param to the CosyVoice clone path"
            ) from e
        except Exception as e:
            logger.error(f"Failed to initialize CosyVoice3: {e}")
            raise

    async def cleanup(self) -> None:
        """Clean up the TTS model."""
        self._model = None
        self._initialized = False
        logger.info("CosyVoice3 TTS node cleaned up")

    def _generate_sync(self, text: str):
        """
        Run TTS generation synchronously (thread-safe).

        Returns a list of (audio_numpy, sample_rate) tuples from the generator.
        """
        chunks = []

        if self.mode == "sft":
            gen = self._model.inference_sft(
                text, spk_id=self.spk_id,
                stream=self.stream, speed=self.speed
            )
        elif self.mode == "zero_shot":
            gen = self._model.inference_zero_shot(
                text, self.prompt_text, self.prompt_wav,
                stream=self.stream, speed=self.speed
            )
        elif self.mode == "cross_lingual":
            gen = self._model.inference_cross_lingual(
                text, self.prompt_wav,
                stream=self.stream, speed=self.speed
            )
        elif self.mode == "instruct":
            gen = self._model.inference_instruct2(
                text, self.instruct_text, self.prompt_wav,
                stream=self.stream, speed=self.speed
            )
        else:
            raise ValueError(f"Unknown CosyVoice3 mode: {self.mode}. "
                             f"Must be one of: sft, zero_shot, cross_lingual, instruct")

        for chunk in gen:
            # CosyVoice yields dicts with 'tts_speech' key: torch.Tensor (1, N)
            audio_tensor = chunk['tts_speech']
            audio_np = audio_tensor.cpu().numpy().astype(np.float32)
            if audio_np.ndim > 1:
                audio_np = audio_np.squeeze(0)
            chunks.append(audio_np)

        return chunks

    async def process(self, data: RuntimeData) -> Union[RuntimeData, AsyncGenerator[RuntimeData, None], None]:
        """
        Process text input and generate speech audio.

        Args:
            data: RuntimeData containing text to synthesize (RuntimeData.Text)

        Yields:
            RuntimeData.Audio containing synthesized speech chunks
        """
        logger.info("CosyVoice3TTSNode process() called")
        if not self._initialized:
            await self.initialize()

        if not data.is_text():
            logger.info(f"CosyVoice3TTSNode: non-text data (type={data.type}), passing through")
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
            logger.info("CosyVoice3TTSNode: only special tokens, skipping")
            return

        logger.info(f"CosyVoice3TTSNode: starting synthesis for: '{text[:100]}{'...' if len(text) > 100 else ''}'")

        try:
            audio_chunks = await asyncio.to_thread(self._generate_sync, text)

            total_samples = 0
            for idx, audio_np in enumerate(audio_chunks):
                total_samples += len(audio_np)
                duration = total_samples / float(self.sample_rate)

                logger.info(
                    f"CosyVoice3TTSNode: yielding chunk {idx + 1} "
                    f"({len(audio_np) / self.sample_rate:.2f}s) | total {duration:.2f}s"
                )

                yield numpy_to_audio(audio_np, self.sample_rate, channels=1)

            logger.info(f"CosyVoice3TTSNode: synthesis complete - {len(audio_chunks)} chunks, {total_samples} samples")

        except RuntimeError as e:
            if "CUDA error" in str(e):
                logger.error(f"CosyVoice3TTSNode: CUDA error during synthesis: {e}")
                self._initialized = False
                self._model = None
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
                    logger.error(f"CosyVoice3TTSNode: reinit failed: {reinit_e}")
                return
            else:
                raise
        except Exception as e:
            logger.error(f"CosyVoice3TTSNode: unexpected error: {e}")
            raise

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "CosyVoice3TTSNode",
            "model_dir": self.model_dir,
            "mode": self.mode,
            "spk_id": self.spk_id,
            "speed": self.speed,
            "stream": self.stream,
            "sample_rate": self.sample_rate,
        }
