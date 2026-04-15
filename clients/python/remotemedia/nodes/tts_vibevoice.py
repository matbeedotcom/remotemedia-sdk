"""
VibeVoice TTS Node using RuntimeData API and MultiprocessNode base

Uses the VibeVoice streaming inference model with pre-cached voice presets (.pt files).
This is the correct approach for TTS generation - the streaming model uses prefilled
prompt outputs for fast inference without requiring raw reference audio.

Inherits from MultiprocessNode to enable concurrent execution in separate processes.

Inputs:
- RuntimeData.Text (string)

Outputs:
- RuntimeData.Audio (float32 mono @ 24000 Hz, streamed in chunks)

Requirements:
- pip install vibevoice transformers torch
- Voice preset .pt files in voices/streaming_model/ under the model directory
"""

import asyncio
import copy
import logging
import threading
from typing import AsyncGenerator, Optional, Union, Dict, Any, List

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
    node_type="VibeVoiceTTSNode",
    multi_output=True,
    category="tts",
    accepts=["text"],
    produces=["audio"],
    description="Text-to-speech synthesis using VibeVoice streaming model with voice presets"
)
@python_requires(["vibevoice @ git+https://github.com/microsoft/VibeVoice.git", "transformers", "torch"])
class VibeVoiceTTSNode(MultiprocessNode):
    """Text-to-speech synthesis using VibeVoice streaming inference model.

    Uses pre-cached voice presets (.pt files) for fast, high-quality synthesis
    without requiring raw reference audio. Supports multiple voice presets for
    diverse output generation.
    """

    def __init__(
        self,
        node_id: str = None,
        model_path: str = "microsoft/VibeVoice-Realtime-0.5B",
        device: Optional[str] = None,
        inference_steps: int = 5,
        cfg_scale: float = 1.5,
        voice: str = "",
        voices_dir: Optional[str] = None,
        skip_tokens: Optional[List[str]] = None,
        config: Union[NodeConfig, Dict[str, Any]] = None,
        **kwargs,
    ) -> None:
        """
        Initialize VibeVoice TTS node.

        Args:
            node_id: Unique identifier for this node instance
            model_path: HuggingFace model ID or local path
            device: Device for inference ("cuda", "mps", "cpu", or None for auto-detect)
            inference_steps: Number of denoising steps (default: 5 for streaming model)
            cfg_scale: Classifier-free guidance scale (default: 1.5)
            voice: Voice preset name (e.g., "en-Carter_man"). Empty = auto-select.
            voices_dir: Path to voice presets directory. Default: auto-discover.
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

            model_path = params.get('model_path', model_path)
            device = params.get('device', device)
            inference_steps = params.get('inference_steps', inference_steps)
            cfg_scale = params.get('cfg_scale', cfg_scale)
            voice = params.get('voice', voice)
            voices_dir = params.get('voices_dir', voices_dir)
            skip_tokens = params.get('skip_tokens', skip_tokens)
        else:
            from remotemedia.core.multiprocessing.node import NodeConfig as NC
            minimal_config = NC(
                node_id=node_id or "vibevoice_tts",
                node_type="VibeVoiceTTSNode",
                params={}
            )
            super().__init__(minimal_config, **kwargs)
            self.logger = logging.getLogger(__name__)

        self.model_path = model_path
        self.device = device
        self.inference_steps = int(inference_steps)
        self.cfg_scale = float(cfg_scale)
        self.voice = voice
        self.voices_dir = voices_dir
        self.sample_rate = 24000
        self.skip_tokens = skip_tokens or ['<|text_end|>', '<|audio_end|>', '<|im_end|>', '<|im_start|>']
        self.is_streaming = True

        self._processor = None
        self._model = None
        self._voice_presets: Dict[str, str] = {}  # name -> path
        self._voice_cache: Dict[str, object] = {}  # name -> prefilled_outputs
        self._initialized = False

    async def initialize(self) -> None:
        """Initialize VibeVoice streaming model, processor, and voice presets."""
        if self._initialized:
            return

        try:
            import torch

            def _load():
                from vibevoice.modular.modeling_vibevoice_streaming_inference import (
                    VibeVoiceStreamingForConditionalGenerationInference,
                )
                from vibevoice.processor.vibevoice_streaming_processor import (
                    VibeVoiceStreamingProcessor,
                )

                # Auto-detect device
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
                torch_device = torch.device(dev)

                # Dtype + attention
                if dev == "mps":
                    load_dtype = torch.float32
                    device_map = None
                    attn_impl = "sdpa"
                elif dev == "cuda":
                    load_dtype = torch.bfloat16
                    device_map = "cuda"
                    attn_impl = "flash_attention_2"
                else:
                    load_dtype = torch.float32
                    device_map = "cpu"
                    attn_impl = "sdpa"

                logger.info(f"Initializing VibeVoice streaming: device={dev}, dtype={load_dtype}, attn={attn_impl}")

                # Load processor
                processor = VibeVoiceStreamingProcessor.from_pretrained(self.model_path)

                # Load model with flash_attention_2 fallback to sdpa
                try:
                    model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
                        self.model_path,
                        torch_dtype=load_dtype,
                        device_map=device_map,
                        attn_implementation=attn_impl,
                    )
                    if dev == "mps":
                        model.to("mps")
                except Exception as e:
                    if attn_impl == "flash_attention_2":
                        logger.warning(f"flash_attention_2 failed ({e}), falling back to sdpa")
                        model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
                            self.model_path,
                            torch_dtype=load_dtype,
                            device_map=device_map if device_map != "cuda" else dev,
                            attn_implementation="sdpa",
                        )
                        if dev == "mps":
                            model.to("mps")
                    else:
                        raise

                model.eval()

                # Configure scheduler
                try:
                    model.model.noise_scheduler = model.model.noise_scheduler.from_config(
                        model.model.noise_scheduler.config,
                        algorithm_type="sde-dpmsolver++",
                        beta_schedule="squaredcos_cap_v2",
                    )
                except Exception:
                    pass
                try:
                    model.set_ddpm_inference_steps(num_steps=self.inference_steps)
                except Exception:
                    pass

                # Discover voice presets
                voice_presets = self._discover_voice_presets(torch_device)

                return processor, model, voice_presets, torch_device

            self._processor, self._model, self._voice_presets, self._torch_device = await asyncio.to_thread(_load)

            # Pre-cache the selected voice
            if self.voice and self.voice in self._voice_presets:
                await asyncio.to_thread(self._ensure_voice_cached, self.voice)
            elif self._voice_presets:
                # Auto-select: prefer en-Carter_man, else first available
                default_key = "en-Carter_man"
                if default_key not in self._voice_presets:
                    default_key = next(iter(self._voice_presets))
                self.voice = default_key
                await asyncio.to_thread(self._ensure_voice_cached, self.voice)
                logger.info(f"Auto-selected voice preset: {self.voice}")
            else:
                logger.warning("No voice presets found - synthesis will fail")

            self._initialized = True
            logger.info(f"VibeVoice streaming model initialized. Voices: {list(self._voice_presets.keys())}")

        except ImportError as e:
            raise ImportError(
                "VibeVoice is not installed. Install with: pip install vibevoice transformers torch"
            ) from e
        except Exception as e:
            logger.error(f"Failed to initialize VibeVoice: {e}")
            raise

    def _discover_voice_presets(self, torch_device) -> Dict[str, str]:
        """Find voice preset .pt files, downloading from GitHub if needed."""
        import os
        from pathlib import Path

        presets: Dict[str, str] = {}

        # Try voices_dir if specified
        search_dirs = []
        if self.voices_dir:
            search_dirs.append(Path(self.voices_dir))

        # Try local model path
        local_voices = Path(self.model_path) / "voices" / "streaming_model"
        if local_voices.exists():
            search_dirs.append(local_voices)

        # Try cache directory
        cache_voices = Path.home() / ".cache" / "vibevoice" / "voices" / "streaming_model"
        search_dirs.append(cache_voices)

        for voices_dir in search_dirs:
            if not voices_dir.exists():
                continue
            for pt_path in voices_dir.rglob("*.pt"):
                presets[pt_path.stem] = str(pt_path)

        # If no presets found, download from GitHub
        if not presets:
            logger.info("No local voice presets found, downloading from GitHub...")
            presets = self._download_voice_presets_from_github(cache_voices)

        if presets:
            logger.info(f"Found {len(presets)} voice presets: {sorted(presets.keys())}")
        else:
            logger.warning("No voice presets found")

        return dict(sorted(presets.items()))

    def _download_voice_presets_from_github(self, target_dir) -> Dict[str, str]:
        """Download voice preset .pt files from the VibeVoice GitHub repo."""
        from pathlib import Path
        import urllib.request
        import json

        presets: Dict[str, str] = {}
        target_dir = Path(target_dir)
        target_dir.mkdir(parents=True, exist_ok=True)

        github_api = "https://api.github.com/repos/microsoft/VibeVoice/contents/demo/voices/streaming_model"
        try:
            req = urllib.request.Request(github_api, headers={"Accept": "application/vnd.github.v3+json"})
            with urllib.request.urlopen(req, timeout=30) as resp:
                files = json.loads(resp.read().decode())

            pt_files = [f for f in files if f["name"].endswith(".pt")]
            logger.info(f"Found {len(pt_files)} voice presets on GitHub, downloading...")

            for f in pt_files:
                name = f["name"]
                download_url = f["download_url"]
                local_path = target_dir / name

                if local_path.exists():
                    presets[local_path.stem] = str(local_path)
                    continue

                logger.info(f"Downloading voice preset: {name}")
                urllib.request.urlretrieve(download_url, str(local_path))
                presets[local_path.stem] = str(local_path)

        except Exception as e:
            logger.error(f"Failed to download voice presets from GitHub: {e}")

        return presets

    def _ensure_voice_cached(self, key: str) -> object:
        """Load and cache a voice preset."""
        if key in self._voice_cache:
            return self._voice_cache[key]

        if key not in self._voice_presets:
            raise RuntimeError(f"Voice preset '{key}' not found. Available: {list(self._voice_presets.keys())}")

        import torch
        preset_path = self._voice_presets[key]
        logger.info(f"Loading voice preset '{key}' from {preset_path}")
        prefilled_outputs = torch.load(preset_path, map_location=self._torch_device, weights_only=False)
        self._voice_cache[key] = prefilled_outputs
        return prefilled_outputs

    def _generate_sync(self, text: str, voice_key: str) -> list:
        """Run TTS generation synchronously (thread-safe). Returns list of numpy audio chunks."""
        import torch
        from vibevoice.modular.streamer import AudioStreamer

        prefilled_outputs = self._ensure_voice_cached(voice_key)

        # Prepare inputs using streaming processor with cached prompt
        inputs = self._processor.process_input_with_cached_prompt(
            text=text.strip(),
            cached_prompt=prefilled_outputs,
            padding=True,
            return_tensors="pt",
            return_attention_mask=True,
        )
        inputs = {
            k: v.to(self._torch_device) if hasattr(v, "to") else v
            for k, v in inputs.items()
        }

        # Set up streamer and generation thread
        audio_streamer = AudioStreamer(batch_size=1, stop_signal=None, timeout=None)
        errors = []
        stop_event = threading.Event()

        def run_generation():
            try:
                self._model.generate(
                    **inputs,
                    max_new_tokens=None,
                    cfg_scale=self.cfg_scale,
                    tokenizer=self._processor.tokenizer,
                    generation_config={
                        "do_sample": False,
                        "temperature": 1.0,
                        "top_p": 1.0,
                    },
                    audio_streamer=audio_streamer,
                    stop_check_fn=stop_event.is_set,
                    verbose=False,
                    refresh_negative=True,
                    all_prefilled_outputs=copy.deepcopy(prefilled_outputs),
                )
            except Exception as e:
                errors.append(e)
                logger.error(f"Generation thread error: {e}")
                audio_streamer.end()

        gen_thread = threading.Thread(target=run_generation, daemon=True)
        gen_thread.start()

        # Collect audio chunks from streamer
        chunks = []
        try:
            stream = audio_streamer.get_stream(0)
            for audio_chunk in stream:
                if torch.is_tensor(audio_chunk):
                    audio_chunk = audio_chunk.detach().cpu().to(torch.float32).numpy()
                else:
                    audio_chunk = np.asarray(audio_chunk, dtype=np.float32)

                if audio_chunk.ndim > 1:
                    audio_chunk = audio_chunk.reshape(-1)

                # Normalize if clipping
                peak = np.max(np.abs(audio_chunk)) if audio_chunk.size else 0.0
                if peak > 1.0:
                    audio_chunk = audio_chunk / peak

                chunks.append(audio_chunk.astype(np.float32))
        finally:
            stop_event.set()
            audio_streamer.end()
            gen_thread.join(timeout=10)

        if errors:
            raise errors[0]

        return chunks

    async def cleanup(self) -> None:
        self._model = None
        self._processor = None
        self._voice_cache.clear()
        self._voice_presets.clear()
        self._initialized = False
        logger.info("VibeVoice TTS node cleaned up")

    async def process(self, data: RuntimeData) -> Union[RuntimeData, AsyncGenerator[RuntimeData, None], None]:
        """Process RuntimeData.Text to streamed RuntimeData.Audio chunks."""
        logger.info("VibeVoiceTTSNode process() called")

        if not self._initialized:
            await self.initialize()

        if not data.is_text():
            logger.info(f"VibeVoice: non-text data (type={data.type}), passing through")
            yield data
            return

        text = data.as_text()
        if not text or not text.strip():
            logger.warning("VibeVoice: empty text, skipping")
            return

        # Skip error messages to prevent feedback loops
        if any(marker in text for marker in ("ERROR:", "CUDA error", "Traceback", "RuntimeError")):
            logger.warning(f"VibeVoice: skipping error message: '{text[:80]}...'")
            return

        # Remove special tokens
        for tok in self.skip_tokens:
            text = text.replace(tok, '')
        text = text.replace('`', "'").replace('\t', ' ').strip()
        if not text:
            return

        logger.info(f"VibeVoice: synthesizing '{text[:80]}{'...' if len(text) > 80 else ''}' with voice='{self.voice}'")

        try:
            audio_chunks = await asyncio.to_thread(self._generate_sync, text, self.voice)

            total_samples = 0
            for idx, audio_np in enumerate(audio_chunks):
                total_samples += len(audio_np)
                duration = total_samples / float(self.sample_rate)
                logger.info(
                    f"VibeVoice: yielding chunk {idx + 1} "
                    f"({len(audio_np) / self.sample_rate:.2f}s) | total {duration:.2f}s"
                )
                yield numpy_to_audio(audio_np, self.sample_rate, channels=1)

            logger.info(f"VibeVoice: synthesis complete - {len(audio_chunks)} chunks, {total_samples} samples")

        except RuntimeError as e:
            if "CUDA error" in str(e):
                logger.error(f"VibeVoice: CUDA error: {e}")
                self._initialized = False
                self._model = None
                self._processor = None
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
                    logger.error(f"VibeVoice: reinit failed: {reinit_e}")
                return
            raise
        except Exception as e:
            logger.error(f"VibeVoice: unexpected error: {e}")
            raise

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "VibeVoiceTTSNode",
            "model_path": self.model_path,
            "device": self.device,
            "inference_steps": self.inference_steps,
            "cfg_scale": self.cfg_scale,
            "voice": self.voice,
            "sample_rate": self.sample_rate,
            "available_voices": list(self._voice_presets.keys()) if self._voice_presets else [],
        }
