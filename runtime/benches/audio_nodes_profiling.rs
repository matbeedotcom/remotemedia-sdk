// Detailed profiling benchmark to identify performance bottlenecks
// Measures overhead of: JSON parsing, node creation, initialization, processing, serialization

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use remotemedia_runtime::nodes::audio::resample::{RustResampleNode, ResampleQuality};
use remotemedia_runtime::nodes::audio::vad::RustVADNode;
use remotemedia_runtime::nodes::audio::format_converter::RustFormatConverterNode;
use remotemedia_runtime::audio::AudioFormat;
use remotemedia_runtime::executor::node_executor::{NodeExecutor, NodeContext};
use std::collections::HashMap;
use serde_json::Value;
use tokio::runtime::Runtime;
use std::time::Instant;

// Helper function to create test audio data
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

fn create_test_audio_i16(num_channels: usize, num_samples: usize) -> Vec<i16> {
    let f32_audio = create_test_audio_f32(num_channels, num_samples);
    f32_audio.iter().map(|&x| (x * 32767.0) as i16).collect()
}

fn create_context() -> NodeContext {
    NodeContext {
        node_id: "bench_node".to_string(),
        node_type: "audio_node".to_string(),
        params: serde_json::json!({}),
        metadata: HashMap::new(),
    }
}

// Profile Format Conversion - most problematic operation
fn profile_format_conversion(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("format_conversion_profiling");
    
    let num_channels = 2;
    let num_samples_per_channel = 500_000; // 1M total samples
    let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);
    
    // 1. Baseline: Just create the input JSON (no processing)
    group.bench_function("1_json_creation_only", |b| {
        b.iter(|| {
            black_box(serde_json::json!({
                "data": &audio_f32,
                "format": "f32",
                "channels": num_channels,
                "sample_rate": 44100
            }))
        });
    });
    
    // 2. Node creation only
    group.bench_function("2_node_creation_only", |b| {
        b.iter(|| {
            black_box(RustFormatConverterNode::new(AudioFormat::I16))
        });
    });
    
    // 3. Node creation + initialization
    group.bench_function("3_node_creation_and_init", |b| {
        b.to_async(&rt).iter(|| async {
            let mut node = RustFormatConverterNode::new(AudioFormat::I16);
            let ctx = create_context();
            black_box(node.initialize(&ctx).await.unwrap())
        });
    });
    
    // 4. Everything except processing (setup overhead)
    group.bench_function("4_setup_overhead", |b| {
        b.to_async(&rt).iter(|| {
            let audio = audio_f32.clone();
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
                black_box(input)
            }
        });
    });
    
    // 5. Full operation (for comparison)
    group.bench_function("5_full_operation", |b| {
        b.to_async(&rt).iter(|| {
            let audio = audio_f32.clone();
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
    });
    
    // 6. Direct conversion without node wrapper (pure performance)
    group.bench_function("6_direct_conversion_no_wrapper", |b| {
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

// Profile Resample operation
fn profile_resample(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("resample_profiling");
    
    let num_channels = 2;
    let sample_rate = 44100;
    let duration_secs = 1;
    let num_samples = sample_rate * duration_secs;
    let audio = create_test_audio_f32(num_channels, num_samples);
    
    // 1. JSON serialization overhead
    group.bench_function("1_json_serialization", |b| {
        b.iter(|| {
            black_box(serde_json::json!({
                "data": &audio,
                "sample_rate": sample_rate,
                "channels": num_channels
            }))
        });
    });
    
    // 2. Node creation
    group.bench_function("2_node_creation", |b| {
        b.iter(|| {
            black_box(RustResampleNode::new(44100, 16000, ResampleQuality::High, num_channels))
        });
    });
    
    // 3. Full operation
    group.bench_function("3_full_operation", |b| {
        b.to_async(&rt).iter(|| {
            let audio = audio.clone();
            async move {
                let mut node = RustResampleNode::new(44100, 16000, ResampleQuality::High, num_channels);
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
    });
    
    group.finish();
}

// Profile VAD operation
fn profile_vad(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("vad_profiling");
    
    let num_channels = 2;
    let sample_rate = 44100;
    let duration_secs = 1;
    let num_samples = sample_rate * duration_secs;
    let audio = create_test_audio_f32(num_channels, num_samples);
    
    // 1. JSON overhead
    group.bench_function("1_json_overhead", |b| {
        b.iter(|| {
            black_box(serde_json::json!({
                "data": &audio,
                "sample_rate": sample_rate,
                "channels": num_channels
            }))
        });
    });
    
    // 2. Node creation
    group.bench_function("2_node_creation", |b| {
        b.iter(|| {
            black_box(RustVADNode::new(sample_rate as u32, 30, 0.01))
        });
    });
    
    // 3. Full operation
    group.bench_function("3_full_operation", |b| {
        b.to_async(&rt).iter(|| {
            let audio = audio.clone();
            async move {
                let mut node = RustVADNode::new(sample_rate as u32, 30, 0.01);
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
    });
    
    group.finish();
}

// Detailed step-by-step profiling
fn profile_step_by_step(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("step_by_step_profiling");
    
    let num_channels = 2;
    let num_samples_per_channel = 500_000;
    let audio_f32 = create_test_audio_f32(num_channels, num_samples_per_channel);
    
    println!("\n=== STEP-BY-STEP PROFILING ===");
    
    // Measure each step individually
    let start = Instant::now();
    let input_json = serde_json::json!({
        "data": audio_f32.clone(),
        "format": "f32",
        "channels": num_channels,
        "sample_rate": 44100
    });
    let json_time = start.elapsed();
    println!("1. JSON creation: {:?}", json_time);
    
    let start = Instant::now();
    let mut node = RustFormatConverterNode::new(AudioFormat::I16);
    let node_creation_time = start.elapsed();
    println!("2. Node creation: {:?}", node_creation_time);
    
    let start = Instant::now();
    let ctx = create_context();
    rt.block_on(async {
        node.initialize(&ctx).await.unwrap();
    });
    let init_time = start.elapsed();
    println!("3. Initialization: {:?}", init_time);
    
    let start = Instant::now();
    let result = rt.block_on(async {
        node.process(input_json).await.unwrap()
    });
    let process_time = start.elapsed();
    println!("4. Processing: {:?}", process_time);
    
    let start = Instant::now();
    let _serialized = serde_json::to_string(&result).unwrap();
    let serialize_time = start.elapsed();
    println!("5. Output serialization: {:?}", serialize_time);
    
    println!("\nTotal overhead (all except processing): {:?}", 
        json_time + node_creation_time + init_time + serialize_time);
    println!("Processing time: {:?}", process_time);
    println!("Overhead as % of total: {:.1}%", 
        (json_time + node_creation_time + init_time + serialize_time).as_micros() as f64 
        / (json_time + node_creation_time + init_time + process_time + serialize_time).as_micros() as f64 
        * 100.0);
    
    // Comparative benchmark
    group.bench_function("overhead_only", |b| {
        b.iter(|| {
            let input = serde_json::json!({
                "data": audio_f32.clone(),
                "format": "f32",
                "channels": num_channels,
                "sample_rate": 44100
            });
            black_box(input)
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    profile_format_conversion,
    profile_resample,
    profile_vad,
    profile_step_by_step
);
criterion_main!(benches);
