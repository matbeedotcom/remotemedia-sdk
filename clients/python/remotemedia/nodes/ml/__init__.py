from .whisper_transcription import WhisperTranscriptionNode
from .ultravox import UltravoxNode
from .transformers_pipeline import TransformersPipelineNode
from .qwen import Qwen2_5OmniNode

# Optional import: LFM2AudioNode requires liquid_audio package
try:
    from .lfm2_audio import LFM2AudioNode
    _has_lfm2 = True
except ImportError:
    LFM2AudioNode = None
    _has_lfm2 = False

__all__ = [
    "WhisperTranscriptionNode",
    "UltravoxNode",
    "TransformersPipelineNode",
    "Qwen2_5OmniNode",
]

if _has_lfm2:
    __all__.append("LFM2AudioNode") 