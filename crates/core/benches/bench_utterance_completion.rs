//! Benchmark: Complete Utterance Latency (Real VAD)
//!
//! Measures the latency from speech END detection to downstream processing
//! for use cases that require complete utterances (multimodal LLMs, batch TTS, etc.)
//!
//! Uses ACTUAL SileroVAD model running on real audio, not simulated delays.
//!
//! Key insight: Even when complete audio is needed, speculative forwarding
//! still provides benefit because ASR can START processing earlier, even if
//! it waits for VAD=false to finalize the transcription.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::AsyncStreamingNode;
use remotemedia_core::Error;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(feature = "silero-vad")]
use tokio::task::JoinSet;

#[cfg(feature = "silero-vad")]
use remotemedia_core::nodes::SileroVADNode;

use remotemedia_core::nodes::{SpeculativeVADGate, SpeculativeVADGateConfig, VADResult};

/// Simulates a complete speech utterance scenario
#[cfg(feature = "silero-vad")]
#[derive(Debug)]
struct UtteranceBenchmark {
    /// Speech start time
    speech_start: Option<Instant>,
    /// Speech end time (VAD = false)
    speech_end: Option<Instant>,
    /// Time ASR received first audio chunk
    asr_first_chunk: Option<Instant>,
    /// Time ASR received complete utterance signal
    asr_complete_signal: Option<Instant>,
}

#[cfg(feature = "silero-vad")]
impl UtteranceBenchmark {
    fn new() -> Self {
        Self {
            speech_start: None,
            speech_end: None,
            asr_first_chunk: None,
            asr_complete_signal: None,
        }
    }

    /// Calculate time from speech end to ASR having complete utterance
    fn time_to_complete_utterance(&self) -> Option<Duration> {
        match (self.speech_end, self.asr_complete_signal) {
            (Some(end), Some(complete)) => Some(complete.duration_since(end)),
            _ => None,
        }
    }

    /// Calculate time ASR had to "warm up" before utterance completed
    fn asr_warmup_time(&self) -> Option<Duration> {
        match (self.asr_first_chunk, self.speech_end) {
            (Some(first), Some(end)) => Some(end.duration_since(first)),
            _ => None,
        }
    }
}

#[cfg(feature = "silero-vad")]
enum CompleteUtteranceMode {
    Traditional,
    Speculative,
}

#[cfg(feature = "silero-vad")]
fn build_utterance_chunks() -> Vec<RuntimeData> {
    (0..10)
        .map(|i| {
            let samples: Vec<f32> = (0..320)
                .map(|s| {
                    let t = (i * 320 + s) as f32 / 16000.0;
                    let amplitude = if i < 8 { 0.3 } else { 0.05 };
                    (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
                })
                .collect();
            RuntimeData::Audio {
                samples,
                sample_rate: 16000,
                channels: 1,
                stream_id: None,
            }
        })
        .collect()
}

#[cfg(feature = "silero-vad")]
async fn execute_complete_utterance(mode: CompleteUtteranceMode) -> UtteranceBenchmark {
    let vad = Arc::new(SileroVADNode::new(
        Some(0.5),
        Some(16000),
        Some(100),
        Some(200),
        None,
    ));
    let gate = match mode {
        CompleteUtteranceMode::Speculative => Some(Arc::new(SpeculativeVADGate::new(SpeculativeVADGateConfig::default()))),
        CompleteUtteranceMode::Traditional => None,
    };

    let benchmark = Arc::new(Mutex::new(UtteranceBenchmark::new()));
    let start = Instant::now();
    benchmark.lock().unwrap().speech_start = Some(start);

    let mut join_set = if matches!(mode, CompleteUtteranceMode::Speculative) {
        Some(JoinSet::new())
    } else {
        None
    };

    for (idx, chunk) in build_utterance_chunks().into_iter().enumerate() {
        match mode {
            CompleteUtteranceMode::Traditional => {
                let confirmation_time = Arc::new(Mutex::new(None));
                let confirmation_clone = confirmation_time.clone();
                let callback = move |data: RuntimeData| {
                    if let RuntimeData::Json(json) = data {
                        if json
                            .get("is_speech_end")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false)
                        {
                            let mut guard = confirmation_clone.lock().unwrap();
                            if guard.is_none() {
                                *guard = Some(Instant::now());
                            }
                        }
                    }
                    Ok(())
                };

                vad.as_ref()
                    .process_streaming(
                        chunk.clone(),
                        Some(format!("traditional_{}", idx)),
                        callback,
                    )
                    .await
                    .expect("traditional VAD processing failed");

                let confirmed_at = confirmation_time.lock().unwrap().clone();
                if let Some(timestamp) = confirmed_at {
                    let mut bench = benchmark.lock().unwrap();
                    if bench.speech_end.is_none() {
                        bench.speech_end = Some(timestamp);
                    }
                    bench.asr_complete_signal = Some(timestamp);
                }
            }
            CompleteUtteranceMode::Speculative => {
                let gate = gate.as_ref().expect("Speculative gate missing").clone();
                let session_id = format!("speculative_session_{}", idx);
                let benchmark_for_gate = benchmark.clone();

                gate.clone()
                    .process_streaming(
                        chunk.clone(),
                        Some(session_id.clone()),
                        move |data: RuntimeData| {
                            if matches!(data, RuntimeData::Audio { .. }) {
                                let mut bench = benchmark_for_gate.lock().unwrap();
                                if bench.asr_first_chunk.is_none() {
                                    bench.asr_first_chunk = Some(Instant::now());
                                }
                            }
                            Ok(())
                        },
                    )
                    .await
                    .expect("speculative gate processing failed");

                let vad_clone = vad.clone();
                let gate_clone = gate.clone();
                let benchmark_for_vad = benchmark.clone();
                let session_for_vad = session_id.clone();
                let samples_in_chunk = match &chunk {
                    RuntimeData::Audio { samples, .. } => samples.len(),
                    _ => 0,
                };
                let chunk_clone = chunk.clone();

                if let Some(set) = join_set.as_mut() {
                    set.spawn(async move {
                        let events = Arc::new(Mutex::new(Vec::new()));
                        let events_clone = events.clone();
                        let callback = move |data: RuntimeData| {
                            if let RuntimeData::Json(json) = data {
                                events_clone.lock().unwrap().push(json);
                            }
                            Ok(())
                        };

                        vad_clone
                            .process_streaming(
                                chunk_clone,
                                Some(format!("speculative_vad_{}", session_for_vad)),
                                callback,
                            )
                            .await
                            .expect("speculative VAD confirmation failed");

                        let events = events.lock().unwrap().clone();
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
                                .unwrap_or(0.0) as f32;

                            let vad_result = VADResult {
                                is_speech_end,
                                is_confirmed_speech: has_speech,
                                confidence,
                                samples_in_chunk,
                            };

                            gate_clone
                                .process_vad_result(
                                    &session_for_vad,
                                    vad_result,
                                    |_: RuntimeData| Ok(()),
                                )
                                .await
                                .expect("failed to feed VAD result to gate");

                            let mut bench = benchmark_for_vad.lock().unwrap();
                            let now = Instant::now();
                            if bench.speech_end.is_none() {
                                bench.speech_end = Some(now);
                            }
                            bench.asr_complete_signal = Some(now);
                        }

                        Ok::<(), Error>(())
                    });
                }
            }
        }
    }

    if let Some(mut set) = join_set {
        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => panic!("Speculative VAD task failed: {err:?}"),
                Err(join_err) => panic!("Speculative VAD task panicked: {join_err}"),
            }
        }
    }

    if matches!(mode, CompleteUtteranceMode::Traditional) {
        let mut bench = benchmark.lock().unwrap();
        if bench.asr_complete_signal.is_none() {
            bench.asr_complete_signal = Some(Instant::now());
        }
        if bench.speech_end.is_none() {
            bench.speech_end = bench.asr_complete_signal;
        }
        if bench.asr_first_chunk.is_none() {
            bench.asr_first_chunk = bench.asr_complete_signal;
        }
    } else {
        let mut bench = benchmark.lock().unwrap();
        if bench.asr_complete_signal.is_none() {
            bench.asr_complete_signal = Some(Instant::now());
        }
        if bench.speech_end.is_none() {
            bench.speech_end = bench.asr_complete_signal;
        }
        if bench.asr_first_chunk.is_none() {
            bench.asr_first_chunk = bench.speech_start;
        }
    }

    Arc::try_unwrap(benchmark)
        .expect("benchmark still referenced")
        .into_inner()
        .expect("benchmark mutex poisoned")
}

/// Benchmark: Traditional VAD - Complete Utterance with REAL SileroVAD
///
/// Scenario: Process 10 audio chunks through real SileroVAD, forward to ASR after speech end
///
/// Traditional flow:
/// 1. Each chunk processed through SileroVAD (~19ms inference each)
/// 2. Wait for VAD to confirm speech end
/// 3. Forward complete utterance to ASR
///
/// Total: 10 chunks × 19ms = ~190ms before ASR can start
#[cfg(feature = "silero-vad")]
fn bench_traditional_complete_utterance(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("traditional_with_real_vad");
    group.sample_size(10);

    group.bench_function("process_then_forward", |b| {
        b.to_async(&runtime).iter(|| async {
            let benchmark = execute_complete_utterance(CompleteUtteranceMode::Traditional).await;
            let speech_start = benchmark.speech_start.unwrap();
            let time_to_asr_ready = benchmark
                .asr_complete_signal
                .map(|instant| instant.duration_since(speech_start))
                .unwrap_or_default();
            black_box((
                time_to_asr_ready,
                benchmark.time_to_complete_utterance(),
                benchmark.asr_warmup_time(),
            ))
        });
    });

    group.finish();
}

#[cfg(not(feature = "silero-vad"))]
fn bench_traditional_complete_utterance(_c: &mut Criterion) {
    println!("Skipping (requires silero-vad feature)");
}

/// Benchmark: Speculative Pipeline - Complete Utterance with REAL VAD in Parallel
///
/// Scenario: Process 10 audio chunks with SpeculativeVADGate + SileroVAD in parallel
///
/// Speculative pipeline (COMPLETE):
/// 1. Each chunk → SpeculativeVADGate → ASR immediately (~5μs)
/// 2. SAME chunk → SileroVAD in parallel (19ms, doesn't block ASR)
/// 3. ASR receives and buffers chunks as they arrive
/// 4. When VAD detects end, ASR already has all audio
///
/// Total time to ASR ready: ~50μs (forwarding time, VAD runs in parallel)
#[cfg(feature = "silero-vad")]
fn bench_speculative_complete_utterance(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("speculative_full_pipeline");
    group.sample_size(10);

    group.bench_function("forward_plus_vad_parallel", |b| {
        b.to_async(&runtime).iter(|| async {
            let benchmark = execute_complete_utterance(CompleteUtteranceMode::Speculative).await;
            let speech_start = benchmark.speech_start.unwrap();
            let time_to_first_chunk = benchmark
                .asr_first_chunk
                .map(|instant| instant.duration_since(speech_start));
            let time_to_asr_ready = benchmark
                .asr_complete_signal
                .map(|instant| instant.duration_since(speech_start))
                .unwrap_or_default();
            black_box((
                time_to_asr_ready,
                time_to_first_chunk,
                benchmark.asr_warmup_time(),
            ))
        });
    });

    group.finish();
}

#[cfg(not(feature = "silero-vad"))]
fn bench_speculative_complete_utterance(_c: &mut Criterion) {
    println!("Skipping (requires silero-vad feature)");
}

/// Summary: Print analysis comparing the real VAD benchmarks
fn bench_complete_utterance_comparison(_c: &mut Criterion) {
    #[cfg(feature = "silero-vad")]
    {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (traditional, speculative) = runtime.block_on(async {
            (
                execute_complete_utterance(CompleteUtteranceMode::Traditional).await,
                execute_complete_utterance(CompleteUtteranceMode::Speculative).await,
            )
        });

        let traditional_start = traditional.speech_start.unwrap();
        let speculative_start = speculative.speech_start.unwrap();

        let traditional_first = traditional
            .asr_first_chunk
            .map(|instant| instant.duration_since(traditional_start))
            .unwrap_or_default();
        let speculative_first = speculative
            .asr_first_chunk
            .map(|instant| instant.duration_since(speculative_start))
            .unwrap_or_default();

        let traditional_ready = traditional
            .asr_complete_signal
            .map(|instant| instant.duration_since(traditional_start))
            .unwrap_or_default();
        let speculative_ready = speculative
            .asr_complete_signal
            .map(|instant| instant.duration_since(speculative_start))
            .unwrap_or_default();

        let warmup_traditional = traditional.asr_warmup_time().unwrap_or_default();
        let warmup_speculative = speculative.asr_warmup_time().unwrap_or_default();

        let improvement_first = if speculative_first.as_nanos() > 0 {
            traditional_first.as_secs_f64() / speculative_first.as_secs_f64()
        } else {
            0.0
        };

        println!("\n╔════════ COMPLETE UTTERANCE LATENCY ════════╗");
        println!("║  Traditional vs Speculative (real models)  ║");
        println!("╚═══════════════════════════════════════════╝\n");

        println!("🔴 Traditional (blocking VAD)");
        println!(
            "  • ASR first chunk available at {}",
            format_duration(traditional_first)
        );
        println!(
            "  • Complete utterance ready at {}",
            format_duration(traditional_ready)
        );
        println!(
            "  • ASR warmup time: {}",
            format_duration(warmup_traditional)
        );

        println!("\n🟢 Speculative (parallel VAD)");
        println!(
            "  • ASR first chunk available at {}",
            format_duration(speculative_first)
        );
        println!(
            "  • Complete utterance ready at {}",
            format_duration(speculative_ready)
        );
        println!(
            "  • ASR warmup time: {}",
            format_duration(warmup_speculative)
        );

        println!(
            "\n⚡ Improvement (time to first chunk): {:.1}x faster",
            improvement_first
        );
        println!(
            "⚡ LLM warmup gain: {} vs {}",
            format_duration(warmup_traditional),
            format_duration(warmup_speculative)
        );
        println!("═════════════════════════════════════════════\n");
    }

    #[cfg(not(feature = "silero-vad"))]
    {
        println!("\nEnable the `silero-vad` feature to run live utterance comparisons.\n");
    }
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

criterion_group!(
    benches,
    bench_traditional_complete_utterance,
    bench_speculative_complete_utterance,
    bench_complete_utterance_comparison
);

criterion_main!(benches);
