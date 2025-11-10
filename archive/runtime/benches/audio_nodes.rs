// Audio Node Performance Benchmarks
// Comparing Rust implementations against Python (librosa) baseline
//
// Performance targets (from spec):
// - Resample: <2ms per second of audio (50-100x speedup)
// - VAD: <50μs per 30ms frame (50-100x speedup)
// - Format conversion: <100μs for 1M samples (50-100x speedup)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use remotemedia_runtime::audio::AudioFormat;
use remotemedia_runtime::executor::node_executor::{NodeContext, NodeExecutor};
use remotemedia_runtime::nodes::audio::format_converter::RustFormatConverterNode;
use remotemedia_runtime::nodes::audio::resample::{ResampleQuality, RustResampleNode};
use remotemedia_runtime::nodes::audio::vad::RustVADNode;
use std::collections::HashMap;
use tokio::runtime::Runtime;

// Helper function to create test audio data
fn create_test_audio_f32(num_channels: usize, num_samples: usize) -> Vec<f32> {
    // Generate interleaved audio data (simple sine wave for consistency)
    let mut audio = Vec::with_capacity(num_channels * num_samples);
    let frequency = 440.0; // A4 note
    let sample_rate = 44100.0;

    for sample_idx in 0..num_samples {
        let t = sample_idx as f32 / sample_rate;
        let value = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5;

        // Interleave channels
        for _ in 0..num_channels {
            audio.push(value);
        }
    }

    audio
}

fn create_test_audio_i16(num_channels: usize, num_samples: usize) -> Vec<i16> {
    let f32_audio = create_test_audio_f32(num_channels, num_samples);
    f32_audio.iter().map(|&x| (x * 32767.0) as i16).collect()
}

fn create_test_audio_i32(num_channels: usize, num_samples: usize) -> Vec<i32> {
    let f32_audio = create_test_audio_f32(num_channels, num_samples);
    f32_audio
        .iter()
        .map(|&x| (x * 2147483647.0) as i32)
        .collect()
}

// Helper to create NodeContext
fn create_context() -> NodeContext {
    NodeContext {
        node_id: "bench".to_string(),
        node_type: "bench".to_string(),
        params: serde_json::json!({}),
        metadata: HashMap::new(),
    }
}

// Benchmark: Audio Resampling
// Target: <2ms per second of audio
// Baseline: librosa.resample() in Python typically takes 100-200ms per second
fn bench_resample(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("audio_resample");

    // Test different audio durations
    for duration_secs in [1.0, 5.0, 10.0] {
        let input_sr = 44100;
        let output_sr = 16000;
        let num_channels = 2;
        let num_samples = (input_sr as f32 * duration_secs) as usize;

        let audio_data = create_test_audio_f32(num_channels, num_samples);

        group.throughput(Throughput::Elements(num_samples as u64));

        for quality in [
            ResampleQuality::Low,
            ResampleQuality::Medium,
            ResampleQuality::High,
        ] {
            let quality_str = match quality {
                ResampleQuality::Low => "low",
                ResampleQuality::Medium => "medium",
                ResampleQuality::High => "high",
            };

            group.bench_with_input(
                BenchmarkId::new(
                    format!("rust_{}s_{}", duration_secs, quality_str),
                    num_samples,
                ),
                &audio_data,
                |b, audio| {
                    let q = quality.clone();

                    b.to_async(&rt).iter(move || {
                        let audio = audio.clone();
                        async move {
                            let mut node =
                                RustResampleNode::new(input_sr, output_sr, q.clone(), num_channels);

                            let ctx = create_context();
                            node.initialize(&ctx).await.unwrap();

                            let input = serde_json::json!({
                                "data": audio,
                                "sample_rate": input_sr,
                                "channels": num_channels
                            });

                            black_box(node.process(input).await.unwrap())
                        }
                    });
                },
            );
        }
    }

    group.finish();
}

// Benchmark: Voice Activity Detection (VAD)
// Target: <50μs per 30ms frame
// Baseline: Python energy-based VAD typically takes 5-10ms per frame
fn bench_vad(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("audio_vad");

    let sample_rate = 16000;
    let frame_duration_ms = 30;
    let frame_samples = (sample_rate * frame_duration_ms) / 1000;
    let energy_threshold = 0.02;

    // Test different numbers of frames
    for num_frames in [1, 10, 33] {
        // 33 frames = 1 second
        let num_channels = 1; // VAD typically uses mono
        let total_samples = frame_samples * num_frames;

        let audio_data = create_test_audio_f32(num_channels, total_samples as usize);

        group.throughput(Throughput::Elements(num_frames as u64));

        group.bench_with_input(
            BenchmarkId::new("rust", num_frames),
            &audio_data,
            |b, audio| {
                b.to_async(&rt).iter(move || {
                    let audio = audio.clone();
                    async move {
                        let mut node =
                            RustVADNode::new(sample_rate, frame_duration_ms, energy_threshold);

                        let ctx = create_context();
                        node.initialize(&ctx).await.unwrap();

                        let input = serde_json::json!({
                            "data": audio,
                            "sample_rate": sample_rate,
                            "channels": num_channels
                        });

                        black_box(node.process(input).await.unwrap())
                    }
                });
            },
        );
    }

    group.finish();
}

// Benchmark: Format Conversion
// Target: <100μs for 1M samples
// Baseline: Python numpy conversions typically take 10-50ms for 1M samples
fn bench_format_conversion(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("audio_format_conversion");

    let num_channels = 2;
    let num_samples_per_channel = 500_000; // 1M total samples (2 channels)
    let total_samples = num_channels * num_samples_per_channel;

    group.throughput(Throughput::Elements(total_samples as u64));

    // F32 -> I16 conversion (most common for ML models)
    {
        let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);

        group.bench_with_input(
            BenchmarkId::new("rust_f32_to_i16", total_samples),
            &audio_f32,
            |b, audio| {
                b.to_async(&rt).iter(move || {
                    let audio = audio.clone();
                    async move {
                        let mut node = RustFormatConverterNode::new(AudioFormat::I16);

                        let ctx = create_context();
                        node.initialize(&ctx).await.unwrap();

                        let input = serde_json::json!({
                            "data": audio,
                            "format": "f32",
                            "channels": num_channels,
                            "sample_rate": 44100
                        });

                        black_box(node.process(input).await.unwrap())
                    }
                });
            },
        );
    }

    // I16 -> F32 conversion
    {
        let audio_i16 = create_test_audio_i16(num_channels, num_samples_per_channel);

        group.bench_with_input(
            BenchmarkId::new("rust_i16_to_f32", total_samples),
            &audio_i16,
            |b, audio| {
                b.to_async(&rt).iter(move || {
                    let audio = audio.clone();
                    async move {
                        let mut node = RustFormatConverterNode::new(AudioFormat::F32);

                        let ctx = create_context();
                        node.initialize(&ctx).await.unwrap();

                        let input = serde_json::json!({
                            "data": audio,
                            "format": "i16",
                            "channels": num_channels,
                            "sample_rate": 44100
                        });

                        black_box(node.process(input).await.unwrap())
                    }
                });
            },
        );
    }

    // F32 -> I32 conversion
    {
        let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);

        group.bench_with_input(
            BenchmarkId::new("rust_f32_to_i32", total_samples),
            &audio_f32,
            |b, audio| {
                b.to_async(&rt).iter(move || {
                    let audio = audio.clone();
                    async move {
                        let mut node = RustFormatConverterNode::new(AudioFormat::I32);

                        let ctx = create_context();
                        node.initialize(&ctx).await.unwrap();

                        let input = serde_json::json!({
                            "data": audio,
                            "format": "f32",
                            "channels": num_channels,
                            "sample_rate": 44100
                        });

                        black_box(node.process(input).await.unwrap())
                    }
                });
            },
        );
    }

    // I16 -> I32 conversion
    {
        let audio_i16 = create_test_audio_i16(num_channels, num_samples_per_channel);

        group.bench_with_input(
            BenchmarkId::new("rust_i16_to_i32", total_samples),
            &audio_i16,
            |b, audio| {
                b.to_async(&rt).iter(move || {
                    let audio = audio.clone();
                    async move {
                        let mut node = RustFormatConverterNode::new(AudioFormat::I32);

                        let ctx = create_context();
                        node.initialize(&ctx).await.unwrap();

                        let input = serde_json::json!({
                            "data": audio,
                            "format": "i16",
                            "channels": num_channels,
                            "sample_rate": 44100
                        });

                        black_box(node.process(input).await.unwrap())
                    }
                });
            },
        );
    }

    group.finish();
}

// Combined benchmark: Full audio pipeline
// This tests the end-to-end performance of VAD + Resample + Format conversion
fn bench_full_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("audio_full_pipeline");

    let input_sr = 44100;
    let output_sr = 16000;
    let num_channels = 2;
    let duration_secs = 1.0;
    let num_samples = (input_sr as f32 * duration_secs) as usize;

    let audio_data = create_test_audio_f32(num_channels, num_samples);

    group.throughput(Throughput::Elements(num_samples as u64));

    group.bench_with_input(
        BenchmarkId::new("rust_vad_resample_format", num_samples),
        &audio_data,
        |b, audio| {
            b.to_async(&rt).iter(move || {
                let audio = audio.clone();
                async move {
                    // Create all three nodes
                    let mut vad_node = RustVADNode::new(
                        input_sr, 30,   // frame_duration_ms
                        0.02, // energy_threshold
                    );

                    let mut resample_node = RustResampleNode::new(
                        input_sr,
                        output_sr,
                        ResampleQuality::High,
                        num_channels,
                    );

                    let mut format_node = RustFormatConverterNode::new(AudioFormat::I16);

                    let ctx = create_context();

                    // Initialize all nodes
                    vad_node.initialize(&ctx).await.unwrap();
                    resample_node.initialize(&ctx).await.unwrap();
                    format_node.initialize(&ctx).await.unwrap();

                    // Stage 1: VAD
                    let vad_input = serde_json::json!({
                        "data": audio,
                        "sample_rate": input_sr,
                        "channels": num_channels
                    });
                    let vad_output = vad_node.process(vad_input).await.unwrap();

                    // Stage 2: Resample (use output from VAD)
                    let resample_output =
                        resample_node.process(vad_output[0].clone()).await.unwrap();

                    // Stage 3: Format conversion
                    let final_output = format_node
                        .process(resample_output[0].clone())
                        .await
                        .unwrap();

                    black_box(final_output)
                }
            });
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_resample,
    bench_vad,
    bench_format_conversion,
    bench_full_pipeline
);
criterion_main!(benches);
