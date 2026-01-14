//! Speculative VAD Coordinator Benchmark
//!
//! Compares actual latency between:
//! 1. Traditional VAD (SileroVAD) - wait for VAD decision before forwarding to ASR
//! 2. Speculative VAD (SpeculativeVADCoordinator) - forward immediately, VAD in parallel
//!
//! This measures the real-world latency improvement from speculative forwarding.
//!
//! Key metrics:
//! - Time to first audio output (ASR receives data)
//! - Total processing time
//! - Speedup ratio
//!
//! Run with: cargo bench -p remotemedia-runtime-core --bench bench_speculative_coordinator --features silero-vad

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::sync::Arc;
use std::time::{Duration, Instant};

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::transport::{PipelineExecutor, StreamSession, TransportData};
use remotemedia_runtime_core::manifest::{Manifest, ManifestMetadata, NodeManifest};

#[cfg(feature = "silero-vad")]
use remotemedia_runtime_core::nodes::{AsyncStreamingNode, SileroVADNode};

/// Create test audio chunk of specified duration at 16kHz
fn create_test_audio(duration_ms: u64) -> RuntimeData {
    let sample_count = (16000 * duration_ms / 1000) as usize;
    
    // Generate sine wave with some noise to simulate real speech
    let samples: Vec<f32> = (0..sample_count)
        .map(|i| {
            let t = i as f32 / 16000.0;
            // Mix 440Hz + 880Hz + noise for more realistic audio
            let signal = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.2
                + (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.1
                + (i as f32 * 0.001).sin() * 0.05;
            signal
        })
        .collect();

    RuntimeData::Audio {
        samples,
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    }
}

/// Create manifest for SpeculativeVADCoordinator
fn create_speculative_manifest() -> Manifest {
    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "speculative_vad_benchmark".to_string(),
            description: Some("Benchmark speculative VAD coordinator".to_string()),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "coordinator".to_string(),
            node_type: "SpeculativeVADCoordinator".to_string(),
            params: serde_json::json!({
                "vad_threshold": 0.5,
                "sample_rate": 16000,
                "min_speech_duration_ms": 250,
                "min_silence_duration_ms": 100,
                "lookback_ms": 150
            }),
            is_streaming: true,
            ..Default::default()
        }],
        connections: vec![],
    }
}

/// Create manifest for traditional SileroVAD-only pipeline
fn create_traditional_manifest() -> Manifest {
    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "traditional_vad_benchmark".to_string(),
            description: Some("Benchmark traditional VAD-then-forward".to_string()),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "silero_vad".to_string(),
            node_type: "SileroVADNode".to_string(),
            params: serde_json::json!({
                "threshold": 0.5,
                "sampling_rate": 16000
            }),
            is_streaming: true,
            ..Default::default()
        }],
        connections: vec![],
    }
}

/// Results from a benchmark run
#[derive(Debug, Clone)]
struct BenchmarkResult {
    time_to_first_audio: Duration,
    total_time: Duration,
    audio_outputs: usize,
    json_outputs: usize,
    control_outputs: usize,
}

/// Run speculative VAD coordinator through PipelineExecutor
#[cfg(feature = "silero-vad")]
async fn run_speculative_coordinator(audio: RuntimeData) -> BenchmarkResult {
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
    let manifest = Arc::new(create_speculative_manifest());
    
    let start = Instant::now();
    let mut first_audio_time: Option<Instant> = None;
    let mut audio_count = 0;
    let mut json_count = 0;
    let mut control_count = 0;
    
    let mut session = runner
        .create_stream_session(manifest)
        .await
        .expect("Failed to create streaming session");
    
    // Send input
    session
        .send_input(TransportData::new(audio))
        .await
        .expect("Failed to send input");
    
    // Collect outputs with timeout
    let collect_start = Instant::now();
    while collect_start.elapsed() < Duration::from_millis(500) {
        match tokio::time::timeout(Duration::from_millis(50), session.recv_output()).await {
            Ok(Ok(Some(transport_data))) => {
                let data = transport_data.data;
                match &data {
                    RuntimeData::Audio { .. } => {
                        if first_audio_time.is_none() {
                            first_audio_time = Some(Instant::now());
                        }
                        audio_count += 1;
                    }
                    RuntimeData::Json(_) => json_count += 1,
                    RuntimeData::ControlMessage { .. } => control_count += 1,
                    _ => {}
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    
    let _ = session.close().await;
    let total_time = start.elapsed();
    
    BenchmarkResult {
        time_to_first_audio: first_audio_time
            .map(|t| t.duration_since(start))
            .unwrap_or(total_time),
        total_time,
        audio_outputs: audio_count,
        json_outputs: json_count,
        control_outputs: control_count,
    }
}

/// Run traditional VAD directly (wait for VAD decision before forwarding)
/// In a real pipeline, you'd gate the audio forwarding on the VAD decision
#[cfg(feature = "silero-vad")]
async fn run_traditional_vad(audio: RuntimeData) -> BenchmarkResult {
    let vad = SileroVADNode::new(Some(0.5), Some(16000), None, None, None);
    
    let start = Instant::now();
    let mut first_audio_time: Option<Instant> = None;
    let mut audio_count = 0;
    let mut json_count = 0;
    
    // Traditional flow: VAD must complete BEFORE audio is forwarded
    let mut vad_decision_received = false;
    let mut should_forward_audio = false;
    
    let callback = |data: RuntimeData| {
        match &data {
            RuntimeData::Json(json) => {
                json_count += 1;
                // Check if VAD says there's speech
                if let Some(has_speech) = json.get("has_speech").and_then(|v| v.as_bool()) {
                    vad_decision_received = true;
                    should_forward_audio = has_speech;
                }
            }
            _ => {}
        }
        Ok(())
    };
    
    let audio_clone = audio.clone();
    let _ = vad
        .process_streaming(audio_clone, Some("traditional_bench".to_string()), callback)
        .await
        .expect("VAD processing failed");
    
    // In traditional flow, audio is only forwarded AFTER VAD completes
    // This simulates the latency of waiting for VAD before sending to ASR
    if should_forward_audio || vad_decision_received {
        first_audio_time = Some(Instant::now());
        audio_count = 1; // Would forward the audio chunk now
    }
    
    let total_time = start.elapsed();
    
    BenchmarkResult {
        time_to_first_audio: first_audio_time
            .map(|t| t.duration_since(start))
            .unwrap_or(total_time),
        total_time,
        audio_outputs: audio_count,
        json_outputs: json_count,
        control_outputs: 0,
    }
}

/// Run traditional VAD through PipelineExecutor for fair comparison
#[cfg(feature = "silero-vad")]
async fn run_traditional_via_runner(audio: RuntimeData) -> BenchmarkResult {
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
    let manifest = Arc::new(create_traditional_manifest());
    
    let start = Instant::now();
    let mut json_count = 0;
    let mut has_speech_detected = false;
    
    let mut session = runner
        .create_stream_session(manifest)
        .await
        .expect("Failed to create streaming session");
    
    // Send input
    session
        .send_input(TransportData::new(audio))
        .await
        .expect("Failed to send input");
    
    // Collect outputs - traditional VAD only outputs JSON, not audio
    let collect_start = Instant::now();
    while collect_start.elapsed() < Duration::from_millis(500) {
        match tokio::time::timeout(Duration::from_millis(50), session.recv_output()).await {
            Ok(Ok(Some(transport_data))) => {
                let data = transport_data.data;
                if let RuntimeData::Json(json) = &data {
                    json_count += 1;
                    if json.get("has_speech").and_then(|v| v.as_bool()).unwrap_or(false) {
                        has_speech_detected = true;
                    }
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    
    let vad_complete_time = start.elapsed();
    
    // In traditional flow, audio forwarding happens AFTER VAD completes
    // This is the key difference - the latency to ASR is the full VAD processing time
    
    let _ = session.close().await;
    
    BenchmarkResult {
        time_to_first_audio: vad_complete_time, // Audio would be forwarded NOW
        total_time: vad_complete_time,
        audio_outputs: if has_speech_detected { 1 } else { 0 },
        json_outputs: json_count,
        control_outputs: 0,
    }
}

/// Benchmark: Speculative VAD Coordinator (forward immediately)
#[cfg(feature = "silero-vad")]
fn bench_speculative_coordinator(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("speculative_vad_coordinator");
    group.measurement_time(Duration::from_secs(10));
    
    for duration_ms in [20u64, 100, 500].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("time_to_first_audio", format!("{}ms_chunk", duration_ms)),
            duration_ms,
            |b, &duration| {
                b.to_async(&runtime).iter(|| async move {
                    let audio = create_test_audio(duration);
                    let result = run_speculative_coordinator(audio).await;
                    black_box(result.time_to_first_audio)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark: Traditional VAD (wait for decision before forwarding)
#[cfg(feature = "silero-vad")]
fn bench_traditional_vad_pipeline(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("traditional_vad_pipeline");
    group.measurement_time(Duration::from_secs(10));
    
    for duration_ms in [20u64, 100, 500].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("time_to_first_audio", format!("{}ms_chunk", duration_ms)),
            duration_ms,
            |b, &duration| {
                b.to_async(&runtime).iter(|| async move {
                    let audio = create_test_audio(duration);
                    let result = run_traditional_via_runner(audio).await;
                    black_box(result.time_to_first_audio)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark: Direct latency comparison
#[cfg(feature = "silero-vad")]
fn bench_latency_comparison(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("vad_latency_comparison_coordinator");
    group.measurement_time(Duration::from_secs(15));
    
    // 20ms is typical voice chunk size
    let audio = create_test_audio(20);
    
    group.bench_function("speculative_coordinator", |b| {
        b.to_async(&runtime).iter(|| async {
            let audio = create_test_audio(20);
            let result = run_speculative_coordinator(audio).await;
            black_box(result.time_to_first_audio)
        });
    });
    
    group.bench_function("traditional_vad_then_forward", |b| {
        b.to_async(&runtime).iter(|| async {
            let audio = create_test_audio(20);
            let result = run_traditional_vad(audio).await;
            black_box(result.time_to_first_audio)
        });
    });
    
    group.finish();
    
    // Print summary with actual numbers
    println!("\n=== Latency Comparison Summary ===");
    
    let spec_result = runtime.block_on(run_speculative_coordinator(audio.clone()));
    let trad_result = runtime.block_on(run_traditional_vad(audio));
    
    println!("Speculative VAD Coordinator:");
    println!("  Time to first audio: {:?}", spec_result.time_to_first_audio);
    println!("  Total time: {:?}", spec_result.total_time);
    println!("  Outputs: {} audio, {} JSON, {} control", 
             spec_result.audio_outputs, spec_result.json_outputs, spec_result.control_outputs);
    
    println!("\nTraditional VAD (process-then-forward):");
    println!("  Time to first audio: {:?}", trad_result.time_to_first_audio);
    println!("  Total time: {:?}", trad_result.total_time);
    println!("  Outputs: {} audio, {} JSON, {} control",
             trad_result.audio_outputs, trad_result.json_outputs, trad_result.control_outputs);
    
    if trad_result.time_to_first_audio > spec_result.time_to_first_audio {
        let speedup = trad_result.time_to_first_audio.as_micros() as f64 
            / spec_result.time_to_first_audio.as_micros().max(1) as f64;
        let latency_reduction = trad_result.time_to_first_audio
            .saturating_sub(spec_result.time_to_first_audio);
        println!("\nSpeculative VAD achieves:");
        println!("  Speedup: {:.1}x faster time-to-ASR", speedup);
        println!("  Latency reduction: {:?}", latency_reduction);
    }
    
    println!("==================================\n");
}

/// Benchmark: Multiple chunk throughput
#[cfg(feature = "silero-vad")]
fn bench_multi_chunk_throughput(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("multi_chunk_throughput");
    group.measurement_time(Duration::from_secs(10));
    
    for chunk_count in [5usize, 10, 20].iter() {
        group.throughput(Throughput::Elements(*chunk_count as u64));
        group.bench_with_input(
            BenchmarkId::new("speculative_coordinator", format!("{}_chunks", chunk_count)),
            chunk_count,
            |b, &count| {
                b.to_async(&runtime).iter(|| async move {
                    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
                    let manifest = Arc::new(create_speculative_manifest());
                    let mut session = runner
                        .create_stream_session(manifest)
                        .await
                        .expect("Failed to create session");
                    
                    let start = Instant::now();
                    
                    // Send multiple chunks
                    for _ in 0..count {
                        let audio = create_test_audio(20);
                        session
                            .send_input(TransportData::new(audio))
                            .await
                            .expect("Failed to send input");
                    }
                    
                    // Drain outputs
                    let collect_start = Instant::now();
                    let mut output_count = 0;
                    while collect_start.elapsed() < Duration::from_millis(1000) {
                        match tokio::time::timeout(Duration::from_millis(50), session.recv_output()).await {
                            Ok(Ok(Some(_))) => output_count += 1,
                            _ => break,
                        }
                    }
                    
                    let _ = session.close().await;
                    black_box((start.elapsed(), output_count))
                });
            },
        );
    }
    
    group.finish();
}

#[cfg(feature = "silero-vad")]
criterion_group!(
    benches,
    bench_speculative_coordinator,
    bench_traditional_vad_pipeline,
    bench_latency_comparison,
    bench_multi_chunk_throughput
);

#[cfg(not(feature = "silero-vad"))]
fn bench_no_vad(c: &mut Criterion) {
    c.bench_function("no_silero_vad_feature", |b| {
        b.iter(|| {
            println!("Silero VAD feature not enabled. Run with --features silero-vad");
        });
    });
}

#[cfg(not(feature = "silero-vad"))]
criterion_group!(benches, bench_no_vad);

criterion_main!(benches);
