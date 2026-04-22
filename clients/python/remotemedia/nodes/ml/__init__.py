from .whisper_transcription import WhisperTranscriptionNode
from .ultravox import UltravoxNode
from .transformers_pipeline import TransformersPipelineNode
from .qwen import Qwen2_5OmniNode

# Optional imports tolerate both missing packages (ImportError) and
# broken native extensions / ABI mismatches (OSError from dlopen).
try:
    from .lfm2_audio import LFM2AudioNode
    _has_lfm2 = True
except (ImportError, OSError):
    LFM2AudioNode = None
    _has_lfm2 = False

try:
    from .lfm2_text import LFM2TextNode
    _has_lfm2_text = True
except (ImportError, OSError):
    LFM2TextNode = None
    _has_lfm2_text = False

__all__ = [
    "WhisperTranscriptionNode",
    "UltravoxNode",
    "TransformersPipelineNode",
    "Qwen2_5OmniNode",
]

if _has_lfm2:
    __all__.append("LFM2AudioNode")
if _has_lfm2_text:
    __all__.append("LFM2TextNode")
