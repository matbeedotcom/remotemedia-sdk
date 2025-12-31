#!/usr/bin/env python3
"""
Fault Generator Suite for Stream Health Monitor Testing

Generates synthetic audio/video files with various faults for testing
the stream health monitoring system.

Usage:
    python fault_generator.py --fault silence --duration 5 -o silence_test.wav
    python fault_generator.py --fault all -o test_suite/
    python fault_generator.py --list-faults

Supported Audio Faults:
    - silence: Complete audio dropout
    - low_volume: Audio at -20dB
    - clipping: Over-amplified audio causing distortion
    - one_sided: Mono audio in stereo (L/R imbalance)
    - dropouts: Intermittent short silences
    - drift: Timing drift over duration
    - jitter: Variable timing intervals

Supported Video Faults (generates raw frames):
    - freeze: Repeated identical frames
    - black_frame: Solid black frames
    - frame_drop: Missing frames in sequence
"""

import argparse
import math
import os
import struct
import wave
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import List, Optional, Tuple, Callable
import random


class AudioFault(Enum):
    """Audio fault types"""
    CLEAN = "clean"
    SILENCE = "silence"
    LOW_VOLUME = "low_volume"
    CLIPPING = "clipping"
    ONE_SIDED = "one_sided"
    DROPOUTS = "dropouts"
    DRIFT = "drift"
    JITTER = "jitter"
    COMBINED = "combined"


class VideoFault(Enum):
    """Video fault types"""
    CLEAN = "clean"
    FREEZE = "freeze"
    BLACK_FRAME = "black_frame"
    FRAME_DROP = "frame_drop"


@dataclass
class AudioConfig:
    """Audio generation configuration"""
    sample_rate: int = 16000
    channels: int = 1
    duration_secs: float = 10.0
    frequency: float = 440.0  # A4 tone
    amplitude: float = 0.5


@dataclass
class FaultConfig:
    """Fault injection configuration"""
    # Silence/dropout settings
    silence_start_sec: float = 3.0
    silence_duration_sec: float = 2.0
    
    # Low volume settings
    volume_reduction_db: float = -20.0
    
    # Clipping settings
    clipping_gain: float = 3.0  # 3x amplitude causes clipping
    
    # Dropout settings (intermittent)
    dropout_interval_sec: float = 1.0
    dropout_duration_ms: float = 100.0
    dropout_count: int = 5
    
    # Drift settings
    drift_rate_ms_per_sec: float = 10.0  # 10ms drift per second
    
    # Jitter settings
    jitter_variance_ms: float = 50.0
    
    # Channel imbalance
    muted_channel: int = 1  # 0=left, 1=right


class AudioGenerator:
    """Generates audio samples with various characteristics"""
    
    def __init__(self, config: AudioConfig):
        self.config = config
    
    def generate_tone(self, duration_secs: Optional[float] = None) -> List[float]:
        """Generate a pure sine tone"""
        duration = duration_secs or self.config.duration_secs
        sample_count = int(self.config.sample_rate * duration)
        
        samples = []
        for i in range(sample_count):
            t = i / self.config.sample_rate
            sample = self.config.amplitude * math.sin(2 * math.pi * self.config.frequency * t)
            samples.append(sample)
        
        return samples
    
    def generate_speech_like(self, duration_secs: Optional[float] = None) -> List[float]:
        """Generate speech-like audio with multiple frequencies and amplitude variation"""
        duration = duration_secs or self.config.duration_secs
        sample_count = int(self.config.sample_rate * duration)
        
        samples = []
        for i in range(sample_count):
            t = i / self.config.sample_rate
            
            # Mix of frequencies to simulate speech harmonics
            fundamental = 150.0  # ~male voice
            sample = 0.0
            sample += 0.4 * math.sin(2 * math.pi * fundamental * t)
            sample += 0.2 * math.sin(2 * math.pi * fundamental * 2 * t)
            sample += 0.1 * math.sin(2 * math.pi * fundamental * 3 * t)
            sample += 0.05 * math.sin(2 * math.pi * fundamental * 4 * t)
            
            # Amplitude envelope (syllable-like variation)
            envelope = 0.5 + 0.5 * math.sin(2 * math.pi * 3 * t)  # 3 Hz envelope
            sample *= envelope * self.config.amplitude
            
            samples.append(sample)
        
        return samples
    
    def generate_noise(self, duration_secs: Optional[float] = None) -> List[float]:
        """Generate white noise"""
        duration = duration_secs or self.config.duration_secs
        sample_count = int(self.config.sample_rate * duration)
        
        return [random.uniform(-1, 1) * self.config.amplitude for _ in range(sample_count)]


class FaultInjector:
    """Injects faults into audio samples"""
    
    def __init__(self, config: FaultConfig, audio_config: AudioConfig):
        self.config = config
        self.audio_config = audio_config
    
    def inject_silence(self, samples: List[float]) -> List[float]:
        """Replace a portion of audio with silence"""
        result = samples.copy()
        
        start_sample = int(self.config.silence_start_sec * self.audio_config.sample_rate)
        end_sample = int((self.config.silence_start_sec + self.config.silence_duration_sec) 
                        * self.audio_config.sample_rate)
        
        for i in range(start_sample, min(end_sample, len(result))):
            result[i] = 0.0
        
        return result
    
    def inject_low_volume(self, samples: List[float]) -> List[float]:
        """Reduce volume by specified dB"""
        gain = 10 ** (self.config.volume_reduction_db / 20)
        return [s * gain for s in samples]
    
    def inject_clipping(self, samples: List[float]) -> List[float]:
        """Over-amplify audio to cause clipping"""
        result = []
        for s in samples:
            clipped = s * self.config.clipping_gain
            # Hard clip at ±1.0
            clipped = max(-1.0, min(1.0, clipped))
            result.append(clipped)
        return result
    
    def inject_one_sided(self, samples: List[float]) -> Tuple[List[float], List[float]]:
        """Create stereo with one channel muted"""
        if self.config.muted_channel == 0:
            return ([0.0] * len(samples), samples)  # Left muted
        else:
            return (samples, [0.0] * len(samples))  # Right muted
    
    def inject_dropouts(self, samples: List[float]) -> List[float]:
        """Insert intermittent short silences"""
        result = samples.copy()
        
        dropout_samples = int(self.config.dropout_duration_ms / 1000 * self.audio_config.sample_rate)
        interval_samples = int(self.config.dropout_interval_sec * self.audio_config.sample_rate)
        
        for dropout_num in range(self.config.dropout_count):
            start = int(2 * self.audio_config.sample_rate) + dropout_num * interval_samples
            if start + dropout_samples > len(result):
                break
            
            for i in range(start, start + dropout_samples):
                result[i] = 0.0
        
        return result
    
    def inject_drift(self, samples: List[float]) -> Tuple[List[float], List[int]]:
        """
        Simulate timing drift by stretching/compressing audio over time.
        Returns samples and their "presentation timestamps" in microseconds.
        """
        result = []
        timestamps_us = []
        
        drift_per_sample = self.config.drift_rate_ms_per_sec / 1000 / self.audio_config.sample_rate
        
        for i, sample in enumerate(samples):
            result.append(sample)
            
            # Calculate drifted timestamp
            base_time_us = int(i * 1_000_000 / self.audio_config.sample_rate)
            drift_us = int(i * drift_per_sample * 1_000_000)
            timestamps_us.append(base_time_us + drift_us)
        
        return result, timestamps_us
    
    def inject_jitter(self, samples: List[float]) -> Tuple[List[float], List[int]]:
        """
        Simulate timing jitter with variable inter-sample intervals.
        Returns samples and their "arrival timestamps" in microseconds.
        """
        result = samples.copy()
        timestamps_us = []
        
        variance_us = self.config.jitter_variance_ms * 1000
        
        cumulative_time = 0
        nominal_interval = 1_000_000 / self.audio_config.sample_rate
        
        for i in range(len(samples)):
            # Add random jitter to arrival time
            jitter = random.gauss(0, variance_us / 3)  # 3-sigma within variance
            cumulative_time += nominal_interval + jitter
            timestamps_us.append(int(cumulative_time))
        
        return result, timestamps_us


class WavWriter:
    """Writes audio samples to WAV files"""
    
    @staticmethod
    def write_mono(path: str, samples: List[float], sample_rate: int):
        """Write mono audio to WAV file (16-bit PCM)"""
        with wave.open(path, 'w') as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)  # 16-bit
            wav.setframerate(sample_rate)
            
            # Convert float samples to 16-bit integers
            int_samples = [int(max(-32767, min(32767, s * 32767))) for s in samples]
            wav.writeframes(struct.pack('<' + 'h' * len(int_samples), *int_samples))
    
    @staticmethod
    def write_stereo(path: str, left: List[float], right: List[float], sample_rate: int):
        """Write stereo audio to WAV file (16-bit PCM)"""
        assert len(left) == len(right), "Left and right channels must have same length"
        
        with wave.open(path, 'w') as wav:
            wav.setnchannels(2)
            wav.setsampwidth(2)  # 16-bit
            wav.setframerate(sample_rate)
            
            # Interleave channels
            interleaved = []
            for l, r in zip(left, right):
                interleaved.append(int(max(-32767, min(32767, l * 32767))))
                interleaved.append(int(max(-32767, min(32767, r * 32767))))
            
            wav.writeframes(struct.pack('<' + 'h' * len(interleaved), *interleaved))
    
    @staticmethod
    def write_f32_raw(path: str, samples: List[float]):
        """Write raw f32 samples (for piping to demo)"""
        with open(path, 'wb') as f:
            for sample in samples:
                f.write(struct.pack('<f', sample))


class TestSuiteGenerator:
    """Generates a complete test suite with all fault types"""
    
    def __init__(self, output_dir: str, audio_config: Optional[AudioConfig] = None):
        self.output_dir = Path(output_dir)
        self.audio_config = audio_config or AudioConfig()
        self.generator = AudioGenerator(self.audio_config)
        self.fault_config = FaultConfig()
        self.injector = FaultInjector(self.fault_config, self.audio_config)
    
    def generate_all(self) -> List[str]:
        """Generate all test files"""
        self.output_dir.mkdir(parents=True, exist_ok=True)
        
        generated = []
        
        # Clean reference
        generated.append(self._generate_clean())
        
        # Audio faults
        generated.append(self._generate_silence())
        generated.append(self._generate_low_volume())
        generated.append(self._generate_clipping())
        generated.append(self._generate_one_sided())
        generated.append(self._generate_dropouts())
        generated.append(self._generate_drift())
        generated.append(self._generate_jitter())
        generated.append(self._generate_combined())
        
        # Generate manifest
        self._generate_manifest(generated)
        
        return generated
    
    def _generate_clean(self) -> str:
        """Generate clean reference audio"""
        path = self.output_dir / "clean.wav"
        samples = self.generator.generate_speech_like()
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        print(f"Generated: {path} (clean reference)")
        return str(path)
    
    def _generate_silence(self) -> str:
        """Generate audio with silence/dropout"""
        path = self.output_dir / "fault_silence.wav"
        samples = self.generator.generate_speech_like()
        samples = self.injector.inject_silence(samples)
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        print(f"Generated: {path} (silence at {self.fault_config.silence_start_sec}s for {self.fault_config.silence_duration_sec}s)")
        return str(path)
    
    def _generate_low_volume(self) -> str:
        """Generate low volume audio"""
        path = self.output_dir / "fault_low_volume.wav"
        samples = self.generator.generate_speech_like()
        samples = self.injector.inject_low_volume(samples)
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        print(f"Generated: {path} (volume {self.fault_config.volume_reduction_db}dB)")
        return str(path)
    
    def _generate_clipping(self) -> str:
        """Generate clipped/distorted audio"""
        path = self.output_dir / "fault_clipping.wav"
        samples = self.generator.generate_speech_like()
        samples = self.injector.inject_clipping(samples)
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        print(f"Generated: {path} (gain {self.fault_config.clipping_gain}x causing clipping)")
        return str(path)
    
    def _generate_one_sided(self) -> str:
        """Generate stereo with one channel muted"""
        path = self.output_dir / "fault_one_sided.wav"
        samples = self.generator.generate_speech_like()
        left, right = self.injector.inject_one_sided(samples)
        WavWriter.write_stereo(str(path), left, right, self.audio_config.sample_rate)
        channel_name = "left" if self.fault_config.muted_channel == 0 else "right"
        print(f"Generated: {path} ({channel_name} channel muted)")
        return str(path)
    
    def _generate_dropouts(self) -> str:
        """Generate audio with intermittent dropouts"""
        path = self.output_dir / "fault_dropouts.wav"
        samples = self.generator.generate_speech_like()
        samples = self.injector.inject_dropouts(samples)
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        print(f"Generated: {path} ({self.fault_config.dropout_count} dropouts of {self.fault_config.dropout_duration_ms}ms)")
        return str(path)
    
    def _generate_drift(self) -> str:
        """Generate audio with timing drift"""
        path = self.output_dir / "fault_drift.wav"
        samples = self.generator.generate_speech_like()
        samples, timestamps = self.injector.inject_drift(samples)
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        
        # Also save timestamps for reference
        ts_path = self.output_dir / "fault_drift_timestamps.txt"
        with open(ts_path, 'w') as f:
            f.write(f"# Drift rate: {self.fault_config.drift_rate_ms_per_sec}ms/s\n")
            f.write(f"# Total drift: {self.fault_config.drift_rate_ms_per_sec * self.audio_config.duration_secs}ms\n")
            for i, ts in enumerate(timestamps[::1000]):  # Every 1000th sample
                f.write(f"{i * 1000}: {ts}\n")
        
        print(f"Generated: {path} (drift {self.fault_config.drift_rate_ms_per_sec}ms/s)")
        return str(path)
    
    def _generate_jitter(self) -> str:
        """Generate audio with timing jitter"""
        path = self.output_dir / "fault_jitter.wav"
        samples = self.generator.generate_speech_like()
        samples, timestamps = self.injector.inject_jitter(samples)
        WavWriter.write_mono(str(path), samples, self.audio_config.sample_rate)
        
        # Also save timestamps for reference
        ts_path = self.output_dir / "fault_jitter_timestamps.txt"
        with open(ts_path, 'w') as f:
            f.write(f"# Jitter variance: {self.fault_config.jitter_variance_ms}ms\n")
            for i, ts in enumerate(timestamps[::1000]):  # Every 1000th sample
                f.write(f"{i * 1000}: {ts}\n")
        
        print(f"Generated: {path} (jitter ±{self.fault_config.jitter_variance_ms}ms)")
        return str(path)
    
    def _generate_combined(self) -> str:
        """Generate audio with multiple faults"""
        path = self.output_dir / "fault_combined.wav"
        samples = self.generator.generate_speech_like()
        
        # Apply multiple faults in sequence
        # 1. Start clean for 2s
        # 2. Low volume for 2s  
        # 3. Dropout
        # 4. Clean for 2s
        # 5. Clipping for 2s
        # 6. Clean to end
        
        result = []
        sr = self.audio_config.sample_rate
        
        # Clean section (0-2s)
        result.extend(samples[:2*sr])
        
        # Low volume section (2-4s)
        low_vol = self.injector.inject_low_volume(samples[2*sr:4*sr])
        result.extend(low_vol)
        
        # Section with dropout (4-6s)
        dropout_section = samples[4*sr:6*sr].copy()
        # Insert 200ms silence at 4.5s
        dropout_start = int(0.5 * sr)
        dropout_len = int(0.2 * sr)
        for i in range(dropout_start, dropout_start + dropout_len):
            if i < len(dropout_section):
                dropout_section[i] = 0.0
        result.extend(dropout_section)
        
        # Clean section (6-8s)
        result.extend(samples[6*sr:8*sr])
        
        # Clipping section (8-10s)
        clipped = self.injector.inject_clipping(samples[8*sr:10*sr])
        result.extend(clipped)
        
        WavWriter.write_mono(str(path), result, self.audio_config.sample_rate)
        print(f"Generated: {path} (combined: low_volume@2s, dropout@4.5s, clipping@8s)")
        return str(path)
    
    def _generate_manifest(self, files: List[str]):
        """Generate a manifest file describing all test files"""
        manifest_path = self.output_dir / "manifest.json"
        
        import json
        manifest = {
            "version": "1.0",
            "description": "Stream Health Monitor Test Suite",
            "audio_config": {
                "sample_rate": self.audio_config.sample_rate,
                "channels": self.audio_config.channels,
                "duration_secs": self.audio_config.duration_secs,
            },
            "files": [
                {
                    "path": "clean.wav",
                    "fault": "none",
                    "expected_health": 1.0,
                    "description": "Clean reference audio"
                },
                {
                    "path": "fault_silence.wav",
                    "fault": "silence",
                    "expected_alerts": ["FREEZE"],
                    "fault_start_sec": self.fault_config.silence_start_sec,
                    "fault_duration_sec": self.fault_config.silence_duration_sec,
                    "description": "Complete audio dropout"
                },
                {
                    "path": "fault_low_volume.wav",
                    "fault": "low_volume",
                    "expected_alerts": ["LOW_LEVEL"],
                    "volume_db": self.fault_config.volume_reduction_db,
                    "description": "Audio at reduced volume"
                },
                {
                    "path": "fault_clipping.wav",
                    "fault": "clipping",
                    "expected_alerts": ["CLIPPING"],
                    "gain": self.fault_config.clipping_gain,
                    "description": "Over-amplified audio causing distortion"
                },
                {
                    "path": "fault_one_sided.wav",
                    "fault": "channel_imbalance",
                    "expected_alerts": ["CHANNEL_IMBALANCE"],
                    "muted_channel": "left" if self.fault_config.muted_channel == 0 else "right",
                    "description": "Stereo audio with one channel muted"
                },
                {
                    "path": "fault_dropouts.wav",
                    "fault": "intermittent_dropouts",
                    "expected_alerts": ["FREEZE"],
                    "dropout_count": self.fault_config.dropout_count,
                    "dropout_duration_ms": self.fault_config.dropout_duration_ms,
                    "description": "Intermittent short silences"
                },
                {
                    "path": "fault_drift.wav",
                    "fault": "drift",
                    "expected_alerts": ["DRIFT_SLOPE"],
                    "drift_rate_ms_per_sec": self.fault_config.drift_rate_ms_per_sec,
                    "description": "Audio with timing drift"
                },
                {
                    "path": "fault_jitter.wav",
                    "fault": "jitter",
                    "expected_alerts": ["CADENCE_UNSTABLE"],
                    "jitter_variance_ms": self.fault_config.jitter_variance_ms,
                    "description": "Audio with timing jitter"
                },
                {
                    "path": "fault_combined.wav",
                    "fault": "combined",
                    "expected_alerts": ["FREEZE", "CLIPPING"],
                    "description": "Multiple faults in sequence"
                }
            ]
        }
        
        with open(manifest_path, 'w') as f:
            json.dump(manifest, f, indent=2)
        
        print(f"\nGenerated manifest: {manifest_path}")


def main():
    parser = argparse.ArgumentParser(
        description="Generate test audio files with various faults",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    
    parser.add_argument(
        '--fault', '-f',
        choices=['clean', 'silence', 'low_volume', 'clipping', 'one_sided', 
                 'dropouts', 'drift', 'jitter', 'combined', 'all'],
        default='all',
        help="Fault type to generate (default: all)"
    )
    
    parser.add_argument(
        '--output', '-o',
        default='./test_suite',
        help="Output file or directory (default: ./test_suite)"
    )
    
    parser.add_argument(
        '--duration', '-d',
        type=float,
        default=10.0,
        help="Audio duration in seconds (default: 10)"
    )
    
    parser.add_argument(
        '--sample-rate', '-r',
        type=int,
        default=16000,
        help="Sample rate in Hz (default: 16000)"
    )
    
    parser.add_argument(
        '--list-faults',
        action='store_true',
        help="List all available fault types"
    )
    
    args = parser.parse_args()
    
    if args.list_faults:
        print("Available fault types:")
        print("  clean       - Clean reference audio")
        print("  silence     - Complete audio dropout")
        print("  low_volume  - Audio at -20dB")
        print("  clipping    - Over-amplified audio")
        print("  one_sided   - Stereo with one channel muted")
        print("  dropouts    - Intermittent short silences")
        print("  drift       - Timing drift over duration")
        print("  jitter      - Variable timing intervals")
        print("  combined    - Multiple faults in sequence")
        print("  all         - Generate entire test suite")
        return
    
    audio_config = AudioConfig(
        sample_rate=args.sample_rate,
        duration_secs=args.duration,
    )
    
    if args.fault == 'all':
        generator = TestSuiteGenerator(args.output, audio_config)
        files = generator.generate_all()
        print(f"\n✓ Generated {len(files)} test files in {args.output}/")
    else:
        # Generate single fault file
        generator = AudioGenerator(audio_config)
        fault_config = FaultConfig()
        injector = FaultInjector(fault_config, audio_config)
        
        samples = generator.generate_speech_like()
        
        if args.fault == 'clean':
            pass  # Use as-is
        elif args.fault == 'silence':
            samples = injector.inject_silence(samples)
        elif args.fault == 'low_volume':
            samples = injector.inject_low_volume(samples)
        elif args.fault == 'clipping':
            samples = injector.inject_clipping(samples)
        elif args.fault == 'one_sided':
            left, right = injector.inject_one_sided(samples)
            WavWriter.write_stereo(args.output, left, right, audio_config.sample_rate)
            print(f"✓ Generated: {args.output}")
            return
        elif args.fault == 'dropouts':
            samples = injector.inject_dropouts(samples)
        elif args.fault == 'drift':
            samples, _ = injector.inject_drift(samples)
        elif args.fault == 'jitter':
            samples, _ = injector.inject_jitter(samples)
        elif args.fault == 'combined':
            # Use the generator for combined
            generator_suite = TestSuiteGenerator(os.path.dirname(args.output) or '.', audio_config)
            generator_suite._generate_combined()
            return
        
        WavWriter.write_mono(args.output, samples, audio_config.sample_rate)
        print(f"✓ Generated: {args.output}")


if __name__ == '__main__':
    main()
