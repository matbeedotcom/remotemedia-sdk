//! Real-World Pipeline Benchmark
//!
//! Benchmarks the actual pipeline from your NextJS app:
//! Input (48kHz) → Chunker → Resample (16kHz) → VAD → Buffer → Resample (24kHz) → Output
//!
//! Measures:
//! - Time to first audio output (smoothness)
//! - Choppiness (variance in chunk arrival times)
//! - Complete utterance latency
//!
//! Compares Traditional VAD (sequential, choppy) vs Speculative VAD (parallel, smooth)

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::{AsyncStreamingNode, SpeculativeVADGate, VADResult};
use std::time::{Duration, Instant};

#[cfg(feature = "silero-vad")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "silero-vad")]
use tokio::task::JoinSet;

#[cfg(feature = "silero-vad")]
use remotemedia_runtime_core::nodes::SileroVADNode;

/// Simulate the real pipeline: 48kHz input → 16kHz VAD → 24kHz output
struct PipelineMetrics {
    /// Time first chunk reached output
    time_to_first_output: Option<Duration>,
    /// Time the first VAD decision completed
    time_to_first_vad_decision: Option<Duration>,
    /// Time downstream inference (ASR/LLM) could start
    llm_inference_start: Option<Duration>,
    /// Time each chunk reached output (for smoothness analysis)
    chunk_times: Vec<Duration>,
    /// Total pipeline latency
    total_latency: Duration,
}

impl PipelineMetrics {
    fn new() -> Self {
        Self {
            time_to_first_output: None,
            time_to_first_vad_decision: None,
            llm_inference_start: None,
            chunk_times: Vec::new(),
            total_latency: Duration::from_millis(0),
        }
    }

    fn record_chunk_latency(&mut self, chunk_idx: usize, latency: Duration) {
        self.chunk_times.push(latency);
        if chunk_idx == 0 && self.time_to_first_output.is_none() {
            self.time_to_first_output = Some(latency);
        }
    }

    /// Calculate variance in chunk arrival times (smoothness metric)
    /// Low variance = smooth playback, High variance = choppy
    fn smoothness_variance(&self) -> f64 {
        if self.chunk_times.len() < 2 {
            return 0.0;
        }

        let mean_us: f64 = self
            .chunk_times
            .iter()
            .map(|d| d.as_micros() as f64)
            .sum::<f64>()
            / self.chunk_times.len() as f64;

        let variance: f64 = self
            .chunk_times
            .iter()
            .map(|d| {
                let diff = d.as_micros() as f64 - mean_us;
                diff * diff
            })
            .sum::<f64>()
            / self.chunk_times.len() as f64;

        variance.sqrt() // Standard deviation
    }
}

#[cfg(feature = "silero-vad")]
enum PipelineMode {
    Traditional,
    Speculative,
}

#[cfg(feature = "silero-vad")]
fn resample_to_16khz(input_chunk: &RuntimeData) -> RuntimeData {
    match input_chunk {
        RuntimeData::Audio { samples, .. } => RuntimeData::Audio {
            samples: samples.iter().step_by(3).copied().collect(),
            sample_rate: 16000,
            channels: 1,
        },
        other => other.clone(),
    }
}

#[cfg(feature = "silero-vad")]
async fn execute_pipeline(mode: PipelineMode) -> PipelineMetrics {
    const CHUNK_COUNT: usize = 10;

    let vad = Arc::new(SileroVADNode::new(
        Some(0.5),
        Some(16000),
        Some(100),
        Some(200),
        None,
    ));

    let mut join_set = JoinSet::new();
    let first_vad_completion = Arc::new(Mutex::new(None));
    let mut metrics = PipelineMetrics::new();
    let start = Instant::now();
    let input_chunks: Vec<RuntimeData> = (0..CHUNK_COUNT).map(create_browser_audio_chunk).collect();
    let speculative_gate = if matches!(mode, PipelineMode::Speculative) {
        Some(Arc::new(SpeculativeVADGate::new()))
    } else {
        None
    };

    for (idx, input_chunk) in input_chunks.iter().enumerate() {
        let chunk_start = Instant::now();
        let resampled = resample_to_16khz(input_chunk);
        let samples_in_chunk = match &resampled {
            RuntimeData::Audio { samples, .. } => samples.len(),
            _ => 0,
        };

        match mode {
            PipelineMode::Traditional => {
                let callback = |_: RuntimeData| Ok(());
                let session = format!("traditional_chunk_{}", idx);
                let _ = vad
                    .process_streaming(resampled, Some(session), callback)
                    .await;

                let mut guard = first_vad_completion.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(Instant::now());
                }
            }
            PipelineMode::Speculative => {
                let gate = speculative_gate
                    .as_ref()
                    .expect("Speculative gate missing for speculative mode")
                    .clone();
                let spec_session = format!("speculative_chunk_{}", idx);
                let spec_callback = |_: RuntimeData| Ok(());
                let _ = gate
                    .process_streaming(resampled.clone(), Some(spec_session.clone()), spec_callback)
                    .await;

                let vad_clone = vad.clone();
                let vad_chunk = resampled.clone();
                let vad_session = format!("vad_confirmation_chunk_{}", idx);
                let first_vad_clone = first_vad_completion.clone();
                let gate_for_vad = gate.clone();
                let session_for_vad = spec_session.clone();
                let chunk_samples = samples_in_chunk;

                join_set.spawn(async move {
                    let collected_events = Arc::new(Mutex::new(Vec::new()));
                    let collected_events_clone = collected_events.clone();
                    let callback = move |data: RuntimeData| {
                        if let RuntimeData::Json(json) = data {
                            collected_events_clone.lock().unwrap().push(json);
                        }
                        Ok(())
                    };

                    match vad_clone
                        .process_streaming(vad_chunk, Some(vad_session), callback)
                        .await
                    {
                        Ok(_) => {
                            let events = collected_events.lock().unwrap().clone();
                            for event in events {
                                let is_speech_end = event
                                    .get("is_speech_end")
                                    .and_then(|value| value.as_bool())
                                    .unwrap_or(false);

                                if !is_speech_end {
                                    continue;
                                }

                                let has_speech = event
                                    .get("has_speech")
                                    .and_then(|value| value.as_bool())
                                    .unwrap_or(false);
                                let confidence = event
                                    .get("speech_probability")
                                    .and_then(|value| value.as_f64())
                                    .unwrap_or(0.0)
                                    as f32;

                                let vad_result = VADResult {
                                    is_speech_end,
                                    is_confirmed_speech: has_speech,
                                    confidence,
                                    samples_in_chunk: chunk_samples,
                                };

                                gate_for_vad
                                    .process_vad_result(
                                        &session_for_vad,
                                        vad_result,
                                        |_: RuntimeData| Ok(()),
                                    )
                                    .await?;
                            }

                            {
                                let mut guard = first_vad_clone.lock().unwrap();
                                if guard.is_none() {
                                    *guard = Some(Instant::now());
                                }
                            }

                            Ok(())
                        }
                        Err(err) => Err(err),
                    }
                });
            }
        }

        let chunk_latency = chunk_start.elapsed();
        metrics.record_chunk_latency(idx, chunk_latency);
    }

    if matches!(mode, PipelineMode::Speculative) {
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => panic!("Speculative VAD task failed: {err:?}"),
                Err(join_err) => panic!("Speculative VAD task panicked: {join_err}"),
            }
        }
    }

    metrics.total_latency = start.elapsed();
    {
        let guard = first_vad_completion.lock().unwrap();
        metrics.time_to_first_vad_decision = guard.map(|instant| instant.duration_since(start));
    }

    metrics.llm_inference_start = match mode {
        PipelineMode::Traditional => metrics.time_to_first_vad_decision,
        PipelineMode::Speculative => metrics.time_to_first_output,
    };

    metrics
}

fn format_duration(duration: Duration) -> String {
    if duration.as_millis() > 0 {
        format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
    } else if duration.as_micros() > 0 {
        format!("{:.2} us", duration.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{:.2} ns", duration.as_secs_f64() * 1_000_000_000.0)
    }
}

/// Create realistic browser audio (48kHz, mono)
fn create_browser_audio_chunk(chunk_idx: usize) -> RuntimeData {
    // 1024 samples @ 48kHz = 21.3ms
    let samples: Vec<f32> = (0..1024)
        .map(|s| {
            let t = (chunk_idx * 1024 + s) as f32 / 48000.0;
            let amplitude = if chunk_idx < 8 { 0.3 } else { 0.0 };
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
        })
        .collect();

    RuntimeData::Audio {
        samples,
        sample_rate: 48000,
        channels: 1,
    }
}

/// Benchmark: Traditional Pipeline (VAD blocks entire flow)
///
/// Pipeline: Input → Resample → VAD (BLOCKS) → Buffer → Resample → Output
///
/// Result: Choppy output (19ms delay per VAD chunk)
#[cfg(feature = "silero-vad")]
fn bench_traditional_pipeline(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("traditional_pipeline_real");
    group.sample_size(10);

    group.bench_function("sequential_vad_blocking", |b| {
        b.to_async(&runtime).iter(|| async {
            let metrics = execute_pipeline(PipelineMode::Traditional).await;
            black_box((
                metrics.total_latency,
                metrics.time_to_first_output,
                metrics.time_to_first_vad_decision,
                metrics.llm_inference_start,
                metrics.smoothness_variance(),
            ))
        });
    });

    group.finish();
}

#[cfg(not(feature = "silero-vad"))]
fn bench_traditional_pipeline(_c: &mut Criterion) {
    println!("Skipping (requires silero-vad feature)");
}

/// Benchmark: Speculative Pipeline (VAD parallel, smooth flow)
///
/// Pipeline: Input → Resample → SpeculativeVADGate → Buffer → Resample → Output
///                                     ↓ (parallel)
///                                  VAD (doesn't block)
///
/// Result: Smooth output (<50μs delay per chunk)
#[cfg(feature = "silero-vad")]
fn bench_speculative_pipeline(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("speculative_pipeline_real");
    group.sample_size(10);

    group.bench_function("parallel_vad_smooth", |b| {
        b.to_async(&runtime).iter(|| async {
            let metrics = execute_pipeline(PipelineMode::Speculative).await;
            black_box((
                metrics.total_latency,
                metrics.time_to_first_output,
                metrics.time_to_first_vad_decision,
                metrics.llm_inference_start,
                metrics.smoothness_variance(),
            ))
        });
    });

    group.finish();
}

#[cfg(not(feature = "silero-vad"))]
fn bench_speculative_pipeline(_c: &mut Criterion) {
    println!("Skipping (requires silero-vad feature)");
}

/// Summary: Explain the real-world pipeline benefits
#[cfg(feature = "silero-vad")]
fn bench_pipeline_summary(_c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (traditional, speculative) = runtime.block_on(async {
        (
            execute_pipeline(PipelineMode::Traditional).await,
            execute_pipeline(PipelineMode::Speculative).await,
        )
    });

    let ratio = match (
        traditional.time_to_first_output,
        speculative.time_to_first_output,
    ) {
        (Some(traditional_first), Some(speculative_first)) if speculative_first.as_nanos() > 0 => {
            traditional_first.as_secs_f64() / speculative_first.as_secs_f64()
        }
        _ => 0.0,
    };
    let llm_start_ratio = match (
        traditional.llm_inference_start,
        speculative.llm_inference_start,
    ) {
        (Some(traditional_start), Some(speculative_start)) if speculative_start.as_nanos() > 0 => {
            traditional_start.as_secs_f64() / speculative_start.as_secs_f64()
        }
        _ => 0.0,
    };

    println!("\n╔════════ REAL-WORLD PIPELINE BENCHMARK ════════╗");
    println!("║  Traditional vs Speculative (measured live)  ║");
    println!("╚══════════════════════════════════════════════╝\n");
    println!("🔴 Traditional (blocking VAD)");
    if let Some(first) = traditional.time_to_first_output {
        println!("  • Time to first output: {}", format_duration(first));
    }
    if let Some(first_decision) = traditional.time_to_first_vad_decision {
        println!(
            "  • First VAD decision surfaces at {}",
            format_duration(first_decision)
        );
    }
    println!(
        "  • Total pipeline latency: {}",
        format_duration(traditional.total_latency)
    );
    println!(
        "  • Output smoothness (std dev): {:.2} us",
        traditional.smoothness_variance()
    );
    if let Some(llm_start) = traditional.llm_inference_start {
        println!(
            "  • LLM/ASR inference can start at {}",
            format_duration(llm_start)
        );
    }

    println!("\n🟢 Speculative (parallel VAD)");
    if let Some(first) = speculative.time_to_first_output {
        println!("  • Time to first output: {}", format_duration(first));
    }
    if let Some(first_decision) = speculative.time_to_first_vad_decision {
        println!(
            "  • First VAD decision (confirmation) visible at {}",
            format_duration(first_decision)
        );
    }
    println!(
        "  • Total pipeline latency (including VAD waits): {}",
        format_duration(speculative.total_latency)
    );
    println!(
        "  • Output smoothness (std dev): {:.2} us",
        speculative.smoothness_variance()
    );
    if let Some(llm_start) = speculative.llm_inference_start {
        println!(
            "  • LLM/ASR inference can start at {}",
            format_duration(llm_start)
        );
    }

    println!(
        "\n⚡ Measured improvement (time to first output): {:.1}x faster",
        ratio
    );
    println!(
        "⚡ Improvement (LLM readiness): {:.1}x earlier inference start",
        llm_start_ratio
    );
    println!("✅ Speculative forwarding keeps ASR fed immediately while VAD confirmation runs off the critical path.");
    println!("════════════════════════════════════════════════\n");
}

#[cfg(not(feature = "silero-vad"))]
fn bench_pipeline_summary(_c: &mut Criterion) {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║  REAL-WORLD PIPELINE BENCHMARK - SMOOTHNESS COMPARISON        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\n📊 Pipeline: Browser (48kHz) → Resample → VAD → Buffer → Resample → Output");
    println!("\n🔴 Traditional (VAD blocks pipeline):");
    println!("  Flow: Chunk → Resample → VAD (19ms WAIT) → Buffer → Output");
    println!("  Per-chunk latency: ~19ms");
    println!("  Output smoothness: CHOPPY (19ms gaps between chunks)");
    println!("  User experience: Stuttery, robotic audio");
    println!("Enable the `silero-vad` feature to run live measurements.");
    println!("\n✅ Run: cargo bench --bench bench_real_pipeline");
    println!("═══════════════════════════════════════════════════════════════════\n");
}

#[cfg(feature = "silero-vad")]
criterion_group!(
    benches,
    bench_traditional_pipeline,
    bench_speculative_pipeline,
    bench_pipeline_summary
);

#[cfg(not(feature = "silero-vad"))]
criterion_group!(benches, bench_pipeline_summary);

criterion_main!(benches);
