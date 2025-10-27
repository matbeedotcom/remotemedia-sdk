// Fast audio node benchmarks (without JSON overhead)
// Comparing direct buffer processing vs JSON-based processing

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use remotemedia_runtime::nodes::audio::format_converter_fast::FastFormatConverter;
use remotemedia_runtime::nodes::audio::format_converter::RustFormatConverterNode;
use remotemedia_runtime::nodes::audio::fast::FastAudioNode;
use remotemedia_runtime::audio::buffer::{AudioBuffer, AudioData, AudioFormat};
use remotemedia_runtime::executor::node_executor::{NodeExecutor, NodeContext};
use std::collections::HashMap;
use tokio::runtime::Runtime;

fn create_test_audio_f32(num_channels: usize, num_samples: usize) -> Vec<f32> {
    let mut audio = Vec::with_capacity(num_channels * num_samples);
    let frequency = 440.0;
    let sample_rate = 44100.0;
    
    for sample_idx in 0..num_samples {
        let t = sample_idx as f32 / sample_rate;
        let value = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5;
        for _ in 0..num_channels {
            audio.push(value);
        }
    }
    audio
}

fn create_context() -> NodeContext {
    NodeContext {
        node_id: "bench_node".to_string(),
        node_type: "audio_node".to_string(),
        params: serde_json::json!({}),
        metadata: HashMap::new(),
    }
}

// Compare fast path vs JSON path
fn bench_format_conversion_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("format_conversion_fast_vs_json");
    
    let num_channels = 2;
    let num_samples_per_channel = 500_000; // 1M total samples
    let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);
    
    // Fast path: Direct buffer processing
    group.bench_function("fast_path_direct_buffer", |b| {
        b.iter(|| {
            let mut converter = FastFormatConverter::new(AudioFormat::I16);
            let input = AudioData::new(
                AudioBuffer::new_f32(audio_f32.clone()),
                44100,
                num_channels,
            );
            black_box(converter.process_audio(input).unwrap())
        });
    });
    
    // JSON path: Standard node processing
    group.bench_function("json_path_standard_node", |b| {
        b.to_async(&rt).iter(|| {
            let audio = audio_f32.clone();
            async move {
                let mut node = RustFormatConverterNode::new(remotemedia_runtime::audio::AudioFormat::I16);
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
    });
    
    // Pure conversion (baseline)
    group.bench_function("pure_conversion_baseline", |b| {
        b.iter(|| {
            let audio = audio_f32.clone();
            let converted: Vec<i16> = audio.iter()
                .map(|&sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16)
                .collect();
            black_box(converted)
        });
    });
    
    group.finish();
}

// Benchmark different sizes
fn bench_format_conversion_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_conversion_fast_sizes");
    
    let num_channels = 2;
    
    for num_samples_per_channel in [10_000, 100_000, 500_000, 1_000_000] {
        let total_samples = num_channels * num_samples_per_channel;
        let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);
        
        group.bench_with_input(
            BenchmarkId::new("fast_path", total_samples),
            &audio_f32,
            |b, audio| {
                b.iter(|| {
                    let mut converter = FastFormatConverter::new(AudioFormat::I16);
                    let input = AudioData::new(
                        AudioBuffer::new_f32(audio.clone()),
                        44100,
                        num_channels,
                    );
                    black_box(converter.process_audio(input).unwrap())
                });
            },
        );
    }
    
    group.finish();
}

// Benchmark all conversion types
fn bench_all_conversions(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_conversion_all_types");
    
    let num_channels = 2;
    let num_samples_per_channel = 500_000;
    let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);
    let audio_i16: Vec<i16> = audio_f32.iter()
        .map(|&x| (x * 32767.0) as i16)
        .collect();
    
    // F32 -> I16
    group.bench_function("f32_to_i16", |b| {
        b.iter(|| {
            let mut converter = FastFormatConverter::new(AudioFormat::I16);
            let input = AudioData::new(
                AudioBuffer::new_f32(audio_f32.clone()),
                44100,
                num_channels,
            );
            black_box(converter.process_audio(input).unwrap())
        });
    });
    
    // I16 -> F32
    group.bench_function("i16_to_f32", |b| {
        b.iter(|| {
            let mut converter = FastFormatConverter::new(AudioFormat::F32);
            let input = AudioData::new(
                AudioBuffer::new_i16(audio_i16.clone()),
                44100,
                num_channels,
            );
            black_box(converter.process_audio(input).unwrap())
        });
    });
    
    // F32 -> I32
    group.bench_function("f32_to_i32", |b| {
        b.iter(|| {
            let mut converter = FastFormatConverter::new(AudioFormat::I32);
            let input = AudioData::new(
                AudioBuffer::new_f32(audio_f32.clone()),
                44100,
                num_channels,
            );
            black_box(converter.process_audio(input).unwrap())
        });
    });
    
    // I16 -> I32
    group.bench_function("i16_to_i32", |b| {
        b.iter(|| {
            let mut converter = FastFormatConverter::new(AudioFormat::I32);
            let input = AudioData::new(
                AudioBuffer::new_i16(audio_i16.clone()),
                44100,
                num_channels,
            );
            black_box(converter.process_audio(input).unwrap())
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_format_conversion_comparison,
    bench_format_conversion_sizes,
    bench_all_conversions
);
criterion_main!(benches);
