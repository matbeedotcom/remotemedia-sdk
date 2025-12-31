# Stream Health Monitor - Test Generator Suite

Generate synthetic audio/video files with various faults for testing the stream health monitoring system.

## Quick Start

```bash
# Generate all test files
python3 fault_generator.py --output ./test_suite

# Run the complete test suite
./run_test_suite.sh

# Test specific fault type
python3 fault_generator.py --fault silence -o test_silence.wav
```

## Available Fault Types

### Audio Faults

| Fault | Description | Detection | Alert |
|-------|-------------|-----------|-------|
| `silence` | Complete audio dropout (2s silence at 3s mark) | RMS energy drops to zero | `FREEZE` |
| `low_volume` | Audio at -20dB | RMS below threshold | `LOW_LEVEL` |
| `clipping` | Over-amplified (3x gain) causing distortion | Peak saturation ratio | `CLIPPING` |
| `one_sided` | Stereo with one channel muted | Per-channel energy comparison | `CHANNEL_IMBALANCE` |
| `dropouts` | Intermittent 100ms silences (5x) | Burst silence pattern | `FREEZE` |
| `drift` | 10ms/s timing drift | PTS vs arrival delta | `DRIFT_SLOPE` |
| `jitter` | ±50ms timing variance | Inter-arrival variance | `CADENCE_UNSTABLE` |
| `combined` | Multiple faults in sequence | Various | Multiple |

### Video Faults (Future)

| Fault | Description | Detection |
|-------|-------------|-----------|
| `freeze` | Repeated identical frames | Frame similarity hash |
| `black_frame` | Solid black frames | Luma histogram |
| `frame_drop` | Missing frames in sequence | Inter-frame delta |

## Usage

### Generate Single File

```bash
# Silence fault
python3 fault_generator.py --fault silence -o silence_test.wav

# Custom duration
python3 fault_generator.py --fault clipping -o clip_test.wav --duration 30

# Different sample rate
python3 fault_generator.py --fault drift -o drift_test.wav --sample-rate 48000
```

### Generate Complete Test Suite

```bash
# All faults at default settings (16kHz, 10s)
python3 fault_generator.py --output ./test_suite

# This creates:
#   test_suite/
#   ├── clean.wav              # Reference audio
#   ├── fault_silence.wav      # Silence at 3-5s
#   ├── fault_low_volume.wav   # -20dB throughout
#   ├── fault_clipping.wav     # 3x gain distortion
#   ├── fault_one_sided.wav    # Right channel muted
#   ├── fault_dropouts.wav     # 5x 100ms dropouts
#   ├── fault_drift.wav        # 10ms/s drift
#   ├── fault_jitter.wav       # ±50ms jitter
#   ├── fault_combined.wav     # Multiple faults
#   └── manifest.json          # Test descriptions
```

### Run Tests

```bash
# Run all tests
./run_test_suite.sh

# Generate only (no testing)
./run_test_suite.sh --generate-only

# Test only (files must exist)
./run_test_suite.sh --test-only

# Test specific fault
./run_test_suite.sh --fault silence
```

## Integration with Demo Binary

```bash
# Test a generated file
./examples/target/release/remotemedia-demo -i test_suite/fault_silence.wav --json

# Expected output for silence fault:
# {"type":"health","ts":"...","score":0.75,"alerts":["FREEZE"]}

# Pipe FFmpeg output
ffmpeg -i test_suite/fault_clipping.wav -f wav -ar 16000 -ac 1 - | \
  ./examples/target/release/remotemedia-demo -i - --json
```

## Fault Configuration

Default fault parameters can be modified in `fault_generator.py`:

```python
@dataclass
class FaultConfig:
    # Silence/dropout
    silence_start_sec: float = 3.0
    silence_duration_sec: float = 2.0
    
    # Volume reduction
    volume_reduction_db: float = -20.0
    
    # Clipping
    clipping_gain: float = 3.0
    
    # Intermittent dropouts
    dropout_interval_sec: float = 1.0
    dropout_duration_ms: float = 100.0
    dropout_count: int = 5
    
    # Timing drift
    drift_rate_ms_per_sec: float = 10.0
    
    # Timing jitter
    jitter_variance_ms: float = 50.0
```

## File Formats

### WAV Output
- 16-bit PCM
- Mono or stereo as needed
- Configurable sample rate (default: 16kHz)

### Raw F32 Output
For direct piping to the demo binary:

```bash
python3 -c "
from fault_generator import *
config = AudioConfig(sample_rate=16000, duration_secs=5)
gen = AudioGenerator(config)
samples = gen.generate_speech_like()
WavWriter.write_f32_raw('output.raw', samples)
"

# Pipe raw f32 samples
cat output.raw | ./remotemedia-demo -i - --format f32le --rate 16000
```

## Extending the Generator

### Add New Fault Type

1. Add to `AudioFault` enum:
```python
class AudioFault(Enum):
    MY_FAULT = "my_fault"
```

2. Add injection method to `FaultInjector`:
```python
def inject_my_fault(self, samples: List[float]) -> List[float]:
    # Implementation
    pass
```

3. Add generation method to `TestSuiteGenerator`:
```python
def _generate_my_fault(self) -> str:
    path = self.output_dir / "fault_my_fault.wav"
    samples = self.generator.generate_speech_like()
    samples = self.injector.inject_my_fault(samples)
    WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
    return str(path)
```

4. Update manifest in `_generate_manifest()`.

## Limitations

### WAV Files Don't Preserve Timing
- **Drift/Jitter**: WAV files don't store per-sample timestamps. The demo reconstructs timing from sample position.
- **Real timing faults** require streaming with actual timing irregularities (e.g., via named pipes or network transport).

### Workaround for Timing Tests
Use the raw streaming mode with controlled timing:

```python
import time
import subprocess

# Pipe with artificial jitter
proc = subprocess.Popen(['./remotemedia-demo', '-i', '-', '--stream'],
                        stdin=subprocess.PIPE)

for chunk in chunks:
    proc.stdin.write(chunk)
    time.sleep(random.uniform(0.01, 0.05))  # Jitter
```

## Related Files

- `fault_generator.py` - Main generator script
- `run_test_suite.sh` - Automated test runner
- `manifest.json` - Generated test manifest (after running)
- `../src/main.rs` - Demo binary source
- `../../runtime-core/src/nodes/health_emitter.rs` - Health emitter node
