"""
ML node subpackage.

Every submodule is imported behind a best-effort ``try/except`` because
managed venvs are per-node — a venv for WhisperSTT won't have
``liquid_audio``, a venv for Kokoro-only TTS won't have ``torch``, etc.
If any submodule's module-level import raised unchecked here, the
whole ``remotemedia.nodes.ml`` package would fail to load in those
venvs, which in turn breaks ``from remotemedia.nodes.ml.<x> import ...``
for every OTHER node too.

We catch ``BaseException`` rather than only ``ImportError`` / ``OSError``
because broken dependency combos in the real world surface as a grab
bag of errors:

- ``ImportError`` / ``ModuleNotFoundError`` — missing Python packages.
- ``OSError`` — broken native extensions (dlopen failures, ABI drift).
- ``AttributeError`` — module imported but expected symbol vanished
  (common when an upstream library renames / removes private APIs).
- ``RuntimeError`` — a module's top-level code decided to abort (e.g.
  ``torch`` refusing to load on a half-initialised CUDA context).

Every failure is logged at WARN with the submodule name and the actual
exception type + message so operators aren't stuck debugging an
``Available: []`` error with no hint as to why.
"""

import logging as _logging

_log = _logging.getLogger(__name__)


def _try_import(module, attrs):
    """Import `attrs` from ``.<module>``; log and swallow any failure."""
    out = {}
    try:
        mod = __import__(f"{__name__}.{module}", fromlist=attrs)
    except BaseException as exc:  # noqa: BLE001 — intentional broad catch
        _log.warning(
            "remotemedia.nodes.ml: skipping %s (%s): %s",
            module, type(exc).__name__, exc,
        )
        return {name: None for name in attrs}
    for name in attrs:
        out[name] = getattr(mod, name, None)
    return out


_loaded = {}
for _mod, _attrs in [
    ("whisper_transcription", ["WhisperTranscriptionNode"]),
    ("ultravox", ["UltravoxNode"]),
    ("transformers_pipeline", ["TransformersPipelineNode"]),
    ("qwen", ["Qwen2_5OmniNode"]),
    ("lfm2_audio", ["LFM2AudioNode"]),
    ("lfm2_audio_mlx", ["LFM2AudioMlxNode"]),
    ("lfm2_text", ["LFM2TextNode"]),
    ("personaplex_audio_mlx", ["PersonaPlexAudioMlxNode"]),
    ("qwen_text_mlx", ["QwenTextMlxNode"]),
    ("qwen_tts_mlx", ["QwenTTSMlxNode"]),
    ("whisper_stt", ["WhisperSTTNode"]),
]:
    _loaded.update(_try_import(_mod, _attrs))

# Expose whatever imported successfully; the rest stay bound to None so
# `from remotemedia.nodes.ml import X` keeps the name space consistent
# for downstream code that checks `X is None`.
for _name, _value in _loaded.items():
    globals()[_name] = _value

__all__ = [name for name, value in _loaded.items() if value is not None]
