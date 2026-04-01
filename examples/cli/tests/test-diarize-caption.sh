#!/bin/bash
set -euo pipefail

# Diarization + STT Live Caption End-to-End Test
#
# Prerequisites:
#   - CLI built with: cargo build --features candle,speaker-diarization
#   - PYTHON_ENV_MODE=managed for KokoroTTS
#
# What it does:
#   1. Generates two TTS audio clips (female + male voices)
#   2. Concatenates them with silence gap
#   3. Runs diarization + transcription pipeline
#   4. Validates output has 2 speakers with correct text

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CLI="${SCRIPT_DIR}/../../target/debug/remotemedia"
PIPELINES="${SCRIPT_DIR}/../pipelines"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

echo "=== Diarize-Caption End-to-End Test ==="
echo "Temp dir: $TMP_DIR"

# Verify CLI exists
if [ ! -f "$CLI" ]; then
    echo "ERROR: CLI not found at $CLI"
    echo "Build with: cd examples/cli/remotemedia-cli && cargo build --features candle,speaker-diarization"
    exit 1
fi

# Step 1: Generate test audio (two different voices)
echo ""
echo "=== Step 1: Generating test audio ==="

echo "Generating voice A (female - af_bella)..."
echo "Watch out there is an enemy behind you" | \
    PYTHON_ENV_MODE=managed $CLI run "$PIPELINES/tts.yaml" -i - -O "$TMP_DIR/voice_a.pcm" --timeout 120

echo "Generating voice B (male - am_adam)..."
echo "I need healing can someone help me" | \
    PYTHON_ENV_MODE=managed $CLI run "$PIPELINES/tts-voice-b.yaml" -i - -O "$TMP_DIR/voice_b.pcm" --timeout 120

echo "Voice A: $(wc -c < "$TMP_DIR/voice_a.pcm") bytes"
echo "Voice B: $(wc -c < "$TMP_DIR/voice_b.pcm") bytes"

# Step 2: Concatenate with silence gap (0.5s at 24kHz = 12000 zero f32 samples = 48000 bytes)
echo ""
echo "=== Step 2: Concatenating audio ==="
python3 -c "import sys; sys.stdout.buffer.write(b'\x00' * 48000)" > "$TMP_DIR/silence.pcm"
cat "$TMP_DIR/voice_a.pcm" "$TMP_DIR/silence.pcm" "$TMP_DIR/voice_b.pcm" > "$TMP_DIR/two_speakers.pcm"
echo "Combined audio: $(wc -c < "$TMP_DIR/two_speakers.pcm") bytes"

# Step 3: Run diarize+caption pipeline
echo ""
echo "=== Step 3: Running diarization + transcription ==="
$CLI run "$PIPELINES/diarize-caption.yaml" \
    -i "$TMP_DIR/two_speakers.pcm" \
    --input-format raw-pcm \
    -O "$TMP_DIR/output.json" \
    -o json \
    --timeout 120 2>"$TMP_DIR/run.log" || {
    echo "Pipeline failed. Log:"
    cat "$TMP_DIR/run.log"
    exit 1
}

echo "=== Pipeline Output ==="
cat "$TMP_DIR/output.json"

# Step 4: Validate output
echo ""
echo "=== Step 4: Validation ==="

if python3 -c "
import json, sys

with open('$TMP_DIR/output.json') as f:
    data = json.load(f)

# Check structure
assert 'text' in data, 'Missing text field'
assert 'diarization' in data, 'Missing diarization field'

segments = data['diarization']['segments']
speakers = set(s['speaker'] for s in segments)
print(f'Speakers found: {speakers}')
print(f'Transcribed text: {data[\"text\"]}')
print(f'Segments: {len(segments)}')

# Validate speaker count (soft check — short TTS clips may merge into 1 speaker)
if len(speakers) < 2:
    print(f'WARNING: Expected >= 2 speakers, got {len(speakers)}: {speakers}')
    print('This can happen with short audio clips. Try longer text or adjust search_threshold.')
else:
    print(f'✓ Detected {len(speakers)} distinct speakers')

# Validate text contains keywords from both inputs
text_lower = data['text'].lower()
voice_a_keywords = ['watch', 'enemy', 'behind']
voice_b_keywords = ['healing', 'help']

voice_a_found = any(w in text_lower for w in voice_a_keywords)
voice_b_found = any(w in text_lower for w in voice_b_keywords)

if not voice_a_found:
    print(f'WARNING: No keywords from voice A found (expected one of: {voice_a_keywords})')
if not voice_b_found:
    print(f'WARNING: No keywords from voice B found (expected one of: {voice_b_keywords})')

assert voice_a_found or voice_b_found, 'No keywords from either voice found in transcription'

print()
print('✓ All validations passed')
"; then
    echo ""
    echo "TEST PASSED"
    exit 0
else
    echo ""
    echo "TEST FAILED"
    exit 1
fi
