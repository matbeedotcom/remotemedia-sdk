#!/usr/bin/env python3
"""
Direct WhisperX test to isolate VAD issue.
"""

import whisperx
import numpy as np
import librosa

# Load audio file
audio_path = "examples/transcribe_demo.wav"
print(f"Loading audio: {audio_path}")

# Load with librosa (same as WhisperX expects)
audio, sr = librosa.load(audio_path, sr=16000, mono=True)

print(f"\nAudio properties:")
print(f"  Shape: {audio.shape}")
print(f"  Sample rate: {sr} Hz")
print(f"  Duration: {len(audio) / sr:.2f}s")
print(f"  dtype: {audio.dtype}")
print(f"  Range: [{audio.min():.6f}, {audio.max():.6f}]")
print(f"  Mean: {audio.mean():.6f}, Std: {audio.std():.6f}")

# Test 1: With default VAD settings
print("\n" + "="*70)
print("Test 1: WhisperX with default VAD settings")
print("="*70)
model = whisperx.load_model("tiny", device="cpu", compute_type="float32", language="en")
result = model.transcribe(audio, batch_size=16, language="en")
print(f"Result: {result}")
print(f"Text: '{result.get('text', '')}'")
print(f"Segments: {len(result.get('segments', []))}")

# Test 2: With more sensitive VAD settings
print("\n" + "="*70)
print("Test 2: WhisperX with sensitive VAD (onset=0.2, offset=0.2)")
print("="*70)
model2 = whisperx.load_model(
    "tiny",
    device="cpu",
    compute_type="float32",
    language="en",
    vad_options={"vad_onset": 0.200, "vad_offset": 0.200}
)
result2 = model2.transcribe(audio, batch_size=16, language="en")
print(f"Result: {result2}")
print(f"Text: '{result2.get('text', '')}'")
print(f"Segments: {len(result2.get('segments', []))}")

# Test 3: With VAD disabled
print("\n" + "="*70)
print("Test 3: WhisperX with VAD disabled (vad_filter=False)")
print("="*70)
result3 = model.transcribe(audio, batch_size=16, language="en", vad_filter=False)
print(f"Result: {result3}")
print(f"Text: '{result3.get('text', '')}'")
print(f"Segments: {len(result3.get('segments', []))}")
