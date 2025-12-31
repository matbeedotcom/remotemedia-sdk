# Stream Health Monitor - Test Suite

Synthetic audio fault generator and test harness for validating stream health detection.

## Quick Start

```bash
# Generate test files and run all tests
./run_test_suite.sh

# Run tests only (use existing files)
./run_test_suite.sh --test-only

# Generate files only
./run_test_suite.sh --generate
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Test Suite Components                    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  fault_generator.py    Generate synthetic WAV files with    │
│                        known faults                         │
│                                                              │
│  parse_events.py       Robust JSON event parser with        │
│                        canonical taxonomy & assertions       │
│                                                              │
│  run_test_suite.sh     Test runner that validates events    │
│                        against expected fault types          │
│                                                              │
│  test_suite/           Generated test audio files +         │
│                        manifest.json                         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Canonical Event Taxonomy

All components use a unified event naming scheme:

| Category | Event Type | Description |
|----------|------------|-------------|
| **Content Faults** | `alert.silence` | Sustained silence detected |
| | `alert.low_volume` | Audio below volume threshold |
| | `alert.clipping` | Audio distortion/saturation |
| | `alert.channel_imbalance` | L/R stereo imbalance |
| | `alert.dropouts` | Intermittent silence bursts |
| **Timing Faults** | `alert.freeze` | Video/timing freeze |
| | `alert.drift` | Timing drift |
| | `alert.cadence` | Cadence instability |
| | `alert.av_skew` | A/V synchronization skew |
| **Semantic** | `alert.keyword` | Keyword detected |
| **System** | `health` | Periodic health score |

## Test Coverage

### ✅ Covered by WAV Tests (Offline PCM)

| Test File | Fault Type | Expected Alerts | Forbidden Alerts |
|-----------|------------|-----------------|------------------|
| `clean.wav` | none | (none) | silence, clipping, low_volume, channel_imbalance, dropouts |
| `fault_silence.wav` | silence | silence | clipping |
| `fault_low_volume.wav` | low_volume | low_volume | clipping, channel_imbalance |
| `fault_clipping.wav` | clipping | clipping | silence, low_volume |
| `fault_one_sided.wav` | channel_imbalance | channel_imbalance | clipping |
| `fault_dropouts.wav` | dropouts | dropouts OR silence | clipping |
| `fault_combined.wav` | combined | clipping, low_volume | (none) |

### ⊘ Not Testable with WAV Files

| Test File | Reason |
|-----------|--------|
| `fault_drift.wav` | WAV playback doesn't preserve timestamps |
| `fault_jitter.wav` | WAV playback doesn't preserve inter-frame timing |

These require real-time streaming tests (FFmpeg → RTMP/SRT → ingest).

### ❌ Not Yet Covered

- Keyword detection (requires transcription)
- A/V sync (requires video)
- True packet loss (requires transport stats)

## False Positive Notes

Some correlated alerts are expected and allowed:

- **channel_imbalance + low_volume**: One-sided audio IS quieter (~3dB drop)
- **dropouts + silence**: Dropouts are detected as short silence periods

## Usage Examples

### Generate Specific Fault Type

```bash
# Single fault file
python3 fault_generator.py --fault clipping -o test_clip.wav

# Custom parameters
python3 fault_generator.py --fault silence \
    --silence-start 2.0 \
    --silence-duration 3.0 \
    -o custom_silence.wav
```

### Parse Events Manually

```bash
# Parse demo output
./remotemedia-demo -i test.wav --json | python3 parse_events.py --summary

# Validate against expected fault
./remotemedia-demo -i fault_clipping.wav --json | \
    python3 parse_events.py --fault clipping --json

# Get JSON summary
./remotemedia-demo -i test.wav --json | \
    python3 parse_events.py --summary --json
```

### Validate Assertions

```python
from parse_events import EventParser, FAULT_ASSERTIONS

parser = EventParser(demo_output)

# Check specific assertions
assertion = FAULT_ASSERTIONS["clipping"]
errors = assertion.validate(parser)

if errors:
    print("FAIL:", errors)
else:
    print("PASS")
```

## Integration Test Mode (TODO)

For testing the full wedge path (FFmpeg → ingest → webhook):

```bash
# Start local webhook receiver
./start_webhook_receiver.sh &

# Start ingest gateway
./start_ingest.sh &

# Push test stream via FFmpeg
ffmpeg -re -i fault_clipping.wav -f flv rtmp://localhost:1935/live/test

# Assert webhook payloads
./assert_webhooks.sh --expect clipping
```

## File Structure

```
test-generators/
├── fault_generator.py      # Synthetic audio generator
├── parse_events.py         # Event parser & validator
├── run_test_suite.sh       # Test runner
├── README.md               # This file
└── test_suite/             # Generated files
    ├── manifest.json       # Test metadata
    ├── clean.wav
    ├── fault_silence.wav
    ├── fault_low_volume.wav
    ├── fault_clipping.wav
    ├── fault_one_sided.wav
    ├── fault_dropouts.wav
    ├── fault_drift.wav     # SKIP for WAV
    ├── fault_jitter.wav    # SKIP for WAV
    └── fault_combined.wav
```

## Extending

### Add New Fault Type

1. Add generation method in `fault_generator.py`:
   ```python
   def inject_new_fault(self, samples):
       # Apply fault transformation
       return modified_samples
   ```

2. Update `_generate_manifest()` with expected/forbidden alerts

3. Add assertion in `parse_events.py`:
   ```python
   FAULT_ASSERTIONS["new_fault"] = TestAssertion(
       fault_type="new_fault",
       expected_alerts={"new_fault"},
       forbidden_alerts={"clipping"},
   )
   ```

4. Add test in `run_test_suite.sh`:
   ```bash
   run_test "fault_new_fault.wav" "new_fault"
   ```

### Add Integration Test

See `integration/` directory (coming soon) for FFmpeg → webhook tests.
