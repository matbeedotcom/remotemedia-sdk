// Fast path benchmarks for Resample and VAD nodes
// Comparing against Python librosa (resample) and numpy (VAD)

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use remotemedia_runtime::nodes::audio::resample_fast::{FastResampleNode, ResampleQuality};
use remotemedia_runtime::nodes::audio::vad_fast::FastVADNode;
use remotemedia_runtime::nodes::audio::fast::FastAudioNode;
use remotemedia_runtime::audio::buffer::{AudioBuffer, AudioData};

fn create_test_audio_f32(num_channels: usize, num_samples: usize, sample_rate: f32) -> Vec<f32> {
    let mut audio = Vec::with_capacity(num_channels * num_samples);
    let frequency = 440.0;
    
    for sample_idx in 0..num_samples {
        let t = sample_idx as f32 / sample_rate;
        let value = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5;
        for _ in 0..num_channels {
            audio.push(value);
        }
    }
    audio
}

// Resample benchmarks - compare against librosa
fn bench_fast_resample(c: &mut Criterion) {
    let mut group = c.benchmark_group("resample_rust_vs_python");
    
    // Python librosa baselines (from benchmark_python_baseline.py):
    // 1 second (44100→16000): 0.44ms
    // 5 seconds: 1.39ms (0.28ms/sec)
    // 10 seconds: 2.64ms (0.26ms/sec)
    
    let num_channels = 2;
    
    // 1 second of audio
    group.bench_function("rust_fast_1sec", |b| {
        b.iter(|| {
            let mut node = FastResampleNode::new(44100, 16000, ResampleQuality::High, num_channels).unwrap();
            let audio = create_test_audio_f32(num_channels, 44100, 44100.0);
            let input = AudioData::new(AudioBuffer::new_f32(audio), 44100, num_channels);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    // 5 seconds of audio
    group.bench_function("rust_fast_5sec", |b| {
        b.iter(|| {
            let mut node = FastResampleNode::new(44100, 16000, ResampleQuality::High, num_channels).unwrap();
            let audio = create_test_audio_f32(num_channels, 220500, 44100.0);
            let input = AudioData::new(AudioBuffer::new_f32(audio), 44100, num_channels);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    // 10 seconds of audio
    group.bench_function("rust_fast_10sec", |b| {
        b.iter(|| {
            let mut node = FastResampleNode::new(44100, 16000, ResampleQuality::High, num_channels).unwrap();
            let audio = create_test_audio_f32(num_channels, 441000, 44100.0);
            let input = AudioData::new(AudioBuffer::new_f32(audio), 44100, num_channels);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    group.finish();
}

// VAD benchmarks - compare against numpy
fn bench_fast_vad(c: &mut Criterion) {
    let mut group = c.benchmark_group("vad_rust_vs_python");
    
    // Python numpy baselines (from benchmark_python_baseline.py):
    // Per-frame time: 6 μs/frame (30ms frames)
    // 1 second: 33 frames = 0.20ms total
    // 10 seconds: 333 frames = 1.67ms total
    
    let sample_rate = 16000;
    let frame_duration_ms = 30;
    let energy_threshold = 0.01;
    let num_channels = 1;
    
    // 1 second of audio (33 frames)
    group.bench_function("rust_fast_1sec", |b| {
        b.iter(|| {
            let mut node = FastVADNode::new(sample_rate, frame_duration_ms, energy_threshold);
            let audio = create_test_audio_f32(num_channels, 16000, sample_rate as f32);
            let input = AudioData::new(AudioBuffer::new_f32(audio), sample_rate, num_channels);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    // 10 seconds of audio (333 frames)
    group.bench_function("rust_fast_10sec", |b| {
        b.iter(|| {
            let mut node = FastVADNode::new(sample_rate, frame_duration_ms, energy_threshold);
            let audio = create_test_audio_f32(num_channels, 160000, sample_rate as f32);
            let input = AudioData::new(AudioBuffer::new_f32(audio), sample_rate, num_channels);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    // 33 seconds of audio (1100 frames)
    group.bench_function("rust_fast_33sec", |b| {
        b.iter(|| {
            let mut node = FastVADNode::new(sample_rate, frame_duration_ms, energy_threshold);
            let audio = create_test_audio_f32(num_channels, 528000, sample_rate as f32);
            let input = AudioData::new(AudioBuffer::new_f32(audio), sample_rate, num_channels);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    group.finish();
}

// Quality comparison - different resample qualities
fn bench_resample_quality(c: &mut Criterion) {
    let mut group = c.benchmark_group("resample_quality_comparison");
    
    let num_channels = 2;
    let audio = create_test_audio_f32(num_channels, 44100, 44100.0);
    
    for quality in [ResampleQuality::Low, ResampleQuality::Medium, ResampleQuality::High] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:?}", quality)),
            &quality,
            |b, &q| {
                b.iter(|| {
                    let mut node = FastResampleNode::new(44100, 16000, q, num_channels).unwrap();
                    let input = AudioData::new(AudioBuffer::new_f32(audio.clone()), 44100, num_channels);
                    black_box(node.process_audio(input).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

// Per-frame VAD timing (to match Python's per-frame metrics)
fn bench_vad_per_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("vad_per_frame");
    
    let sample_rate = 16000;
    let frame_duration_ms = 30;
    let energy_threshold = 0.01;
    
    // Single frame (480 samples at 16kHz, 30ms)
    group.bench_function("rust_single_frame", |b| {
        b.iter(|| {
            let mut node = FastVADNode::new(sample_rate, frame_duration_ms, energy_threshold);
            let audio = create_test_audio_f32(1, 480, sample_rate as f32);
            let input = AudioData::new(AudioBuffer::new_f32(audio), sample_rate, 1);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    // 10 frames
    group.bench_function("rust_10_frames", |b| {
        b.iter(|| {
            let mut node = FastVADNode::new(sample_rate, frame_duration_ms, energy_threshold);
            let audio = create_test_audio_f32(1, 4800, sample_rate as f32);
            let input = AudioData::new(AudioBuffer::new_f32(audio), sample_rate, 1);
            
            black_box(node.process_audio(input).unwrap());
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_fast_resample,
    bench_fast_vad,
    bench_resample_quality,
    bench_vad_per_frame
);
criterion_main!(benches);
