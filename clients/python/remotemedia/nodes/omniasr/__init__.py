"""
OmniASR Streaming Transcription Node

Provides multilingual speech recognition (200+ languages) integrated with
RemoteMedia SDK's real-time streaming architecture.

Key Components:
- OmniASRNode: Main streaming transcription node
- MMSModel: Singleton wrapper for OmniASR Wav2Vec2InferencePipeline
- VADChunker: Voice Activity Detection based audio chunking
- Audio utilities: Format conversion and validation helpers

Integration with Rust Runtime:
- Works with SpeculativeVADGate for ultra-low latency (<250ms P99)
- Handles ControlMessage.CancelSpeculation for speculative forwarding
- Compatible with SileroVAD and AudioBufferAccumulator nodes

Usage:
    from remotemedia.nodes.omniasr import OmniASRNode

    # Basic transcription
    transcriber = OmniASRNode(
        model_card="omniASR_LLM_1B",
        language="eng_Latn",  # or None for auto-detect
        chunking_mode="none"
    )

    # Real-time pipeline with SpeculativeVADGate
    # See examples/omniasr-streaming/ for complete examples
"""

from .node import OmniASRNode

__all__ = ["OmniASRNode"]
__version__ = "0.1.0"
