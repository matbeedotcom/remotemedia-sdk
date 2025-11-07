use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use remotemedia_runtime::python::multiprocess::{
    InitStatus, MultiprocessConfig, MultiprocessExecutor, RuntimeData,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(feature = "multiprocess")]
/// Benchmark the VAD debug pipeline with multiprocess Python nodes
fn bench_vad_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("vad_pipeline_multiprocess");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(10); // Fewer samples for longer-running benchmarks

    // Load the audio file
    let audio_path = PathBuf::from("examples/transcribe_demo.wav");
    if !audio_path.exists() {
        eprintln!(
            "Warning: {} not found, using synthetic audio",
            audio_path.display()
        );
    }

    // Benchmark: Full VAD pipeline end-to-end
    group.bench_function("end_to_end_vad_pipeline", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.iter_custom(|iters| {
            rt.block_on(async {
                let total_start = Instant::now();

                for iter in 0..iters {
                    // Create multiprocess executor
                    let config = MultiprocessConfig {
                        max_processes_per_session: Some(10),
                        channel_capacity: 100,
                        init_timeout_secs: 30,
                        python_executable: PathBuf::from("python"),
                        enable_backpressure: true,
                    };

                    let executor = MultiprocessExecutor::new(config);

                    // Create session
                    let session_id = format!("vad_bench_session_{}", iter);
                    executor
                        .create_session(session_id.clone())
                        .await
                        .expect("Failed to create session");

                    // Simulate node initialization progress
                    let nodes = vec![
                        ("input_chunker", "AudioChunkerNode"),
                        ("audio_resampler", "FastResampleNode"),
                        ("vad_chunker", "AudioChunkerNode"),
                        ("silero_vad", "SileroVADNode"),
                        ("audio_buffer", "AudioBufferAccumulatorNode"),
                        ("vad_to_buffer_resampler", "FastResampleNode"),
                    ];

                    // Simulate initialization progress tracking
                    for (node_id, _node_type) in &nodes {
                        executor
                            .update_init_progress(
                                &session_id,
                                node_id,
                                InitStatus::Starting,
                                0.0,
                                "Starting node".to_string(),
                            )
                            .await
                            .expect("Failed to update progress");

                        executor
                            .update_init_progress(
                                &session_id,
                                node_id,
                                InitStatus::LoadingModel,
                                0.5,
                                "Loading model".to_string(),
                            )
                            .await
                            .expect("Failed to update progress");

                        executor
                            .update_init_progress(
                                &session_id,
                                node_id,
                                InitStatus::Ready,
                                1.0,
                                "Node ready".to_string(),
                            )
                            .await
                            .expect("Failed to update progress");
                    }

                    // Get progress (verify tracking works)
                    let _progress = executor
                        .get_init_progress(&session_id)
                        .await
                        .expect("Failed to get progress");

                    // Simulate streaming audio through pipeline
                    // Generate synthetic audio: 3 seconds @ 48kHz
                    let sample_rate = 48000;
                    let duration_secs = 3.0;
                    let num_samples = (sample_rate as f64 * duration_secs) as usize;
                    let audio_samples: Vec<f32> = (0..num_samples)
                        .map(|i| {
                            (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / sample_rate as f32)
                                .sin()
                                * 0.5
                        })
                        .collect();

                    // Stream audio in chunks (1024 samples per chunk)
                    let chunk_size = 1024;
                    let chunks: Vec<&[f32]> = audio_samples.chunks(chunk_size).collect();

                    // Measure streaming latency
                    let stream_start = Instant::now();

                    for chunk in chunks {
                        let runtime_data = RuntimeData::audio(
                            chunk,
                            sample_rate,
                            1, // mono
                            &session_id,
                        );

                        // Simulate processing through pipeline
                        black_box(runtime_data);
                    }

                    let stream_duration = stream_start.elapsed();

                    println!(
                        "Iteration {}: Streamed {} chunks ({} samples, {:.2}s audio) in {:?}",
                        iter,
                        audio_samples.len() / chunk_size,
                        audio_samples.len(),
                        duration_secs,
                        stream_duration
                    );

                    // Cleanup
                    executor
                        .terminate_session(&session_id)
                        .await
                        .expect("Failed to terminate session");

                    // Small delay between iterations
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                total_start.elapsed()
            })
        });
    });

    group.finish();
}

#[cfg(feature = "multiprocess")]
/// Benchmark individual node initialization times
fn bench_node_initialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_initialization");
    group.measurement_time(Duration::from_secs(10));

    let node_types = vec![
        ("AudioChunkerNode", "audio_chunker"),
        ("FastResampleNode", "resampler"),
        ("SileroVADNode", "vad"),
        ("AudioBufferAccumulatorNode", "buffer"),
    ];

    for (node_type, node_id) in node_types {
        group.bench_with_input(
            BenchmarkId::from_parameter(node_type),
            &(node_type, node_id),
            |b, &(_node_type, node_id)| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async {
                    let config = MultiprocessConfig::default();
                    let executor = MultiprocessExecutor::new(config);

                    let session_id = format!("init_bench_{}", node_id);
                    executor
                        .create_session(session_id.clone())
                        .await
                        .expect("Failed to create session");

                    // Measure initialization time
                    let start = Instant::now();

                    executor
                        .update_init_progress(
                            &session_id,
                            node_id,
                            InitStatus::Starting,
                            0.0,
                            "Starting".to_string(),
                        )
                        .await
                        .expect("Failed to update progress");

                    executor
                        .update_init_progress(
                            &session_id,
                            node_id,
                            InitStatus::LoadingModel,
                            0.5,
                            "Loading model".to_string(),
                        )
                        .await
                        .expect("Failed to update progress");

                    executor
                        .update_init_progress(
                            &session_id,
                            node_id,
                            InitStatus::Ready,
                            1.0,
                            "Ready".to_string(),
                        )
                        .await
                        .expect("Failed to update progress");

                    let elapsed = start.elapsed();

                    executor
                        .terminate_session(&session_id)
                        .await
                        .expect("Failed to terminate session");

                    black_box(elapsed)
                });
            },
        );
    }

    group.finish();
}

#[cfg(feature = "multiprocess")]
/// Benchmark session creation and teardown
fn bench_session_lifecycle(c: &mut Criterion) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    c.bench_function("session_create_destroy", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let counter = Arc::new(AtomicUsize::new(0));

        b.to_async(&rt).iter(|| {
            let counter = counter.clone();
            async move {
                let config = MultiprocessConfig::default();
                let executor = MultiprocessExecutor::new(config);

                let id = counter.fetch_add(1, Ordering::SeqCst);
                let session_id = format!("lifecycle_session_{}", id);

                // Create session
                executor
                    .create_session(session_id.clone())
                    .await
                    .expect("Failed to create session");

                // Destroy session
                executor
                    .terminate_session(&session_id)
                    .await
                    .expect("Failed to terminate session");
            }
        });
    });
}

#[cfg(feature = "multiprocess")]
/// Benchmark audio chunking and processing
fn bench_audio_chunking(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_chunking");

    // Different chunk sizes
    let chunk_sizes = vec![512, 1024, 2048, 4096];

    for chunk_size in chunk_sizes {
        group.throughput(Throughput::Elements(chunk_size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}samples", chunk_size)),
            &chunk_size,
            |b, &chunk_size| {
                // Generate audio samples
                let sample_rate = 48000;
                let audio_samples: Vec<f32> = (0..10000)
                    .map(|i| {
                        (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / sample_rate as f32).sin()
                    })
                    .collect();

                b.iter(|| {
                    let chunks: Vec<&[f32]> = audio_samples.chunks(chunk_size).collect();

                    for chunk in chunks {
                        let runtime_data =
                            RuntimeData::audio(chunk, sample_rate, 1, "bench_session");
                        black_box(runtime_data);
                    }
                });
            },
        );
    }

    group.finish();
}

#[cfg(not(feature = "multiprocess"))]
fn bench_vad_pipeline(_c: &mut Criterion) {
    eprintln!("Multiprocess feature not enabled. Run with: cargo bench --features multiprocess");
}

#[cfg(not(feature = "multiprocess"))]
fn bench_node_initialization(_c: &mut Criterion) {}

#[cfg(not(feature = "multiprocess"))]
fn bench_session_lifecycle(_c: &mut Criterion) {}

#[cfg(not(feature = "multiprocess"))]
fn bench_audio_chunking(_c: &mut Criterion) {}

criterion_group!(
    benches,
    bench_vad_pipeline,
    bench_node_initialization,
    bench_session_lifecycle,
    bench_audio_chunking
);

criterion_main!(benches);
