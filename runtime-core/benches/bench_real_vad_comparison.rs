//! Real VAD Comparison Benchmark
//!
//! Compares actual latency between:
//! 1. Traditional VAD (SileroVAD) - wait for decision before forwarding
//! 2. Speculative VAD (SpeculativeVADGate) - forward immediately
//!
//! This uses real implementations, not mocks, to measure actual performance gain.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::{AsyncStreamingNode, SpeculativeVADGate, SpeculativeVADGateConfig};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(feature = "silero-vad")]
use remotemedia_runtime_core::nodes::{SileroVADNode, VADResult};

/// Create test audio chunk (20ms @ 16kHz)
fn create_test_audio() -> RuntimeData {
    // 320 samples = 20ms @ 16kHz
    // Generate simple sine wave for testing
    let samples: Vec<f32> = (0..320)
        .map(|i| {
            let t = i as f32 / 16000.0;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3
        })
        .collect();

    RuntimeData::Audio {
        samples,
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
    }
}

#[cfg(feature = "silero-vad")]
fn audio_sample_count(data: &RuntimeData) -> usize {
    match data {
        RuntimeData::Audio { samples, .. } => samples.len(),
        _ => 0,
    }
}

#[cfg(feature = "silero-vad")]
async fn run_traditional_flow(audio: RuntimeData) -> (Duration, Duration) {
    let vad = SileroVADNode::new(Some(0.5), Some(16000), None, None, None);
    let start = Instant::now();
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

    let _ = vad
        .process_streaming(audio, Some("traditional_flow".to_string()), callback)
        .await
        .expect("traditional VAD processing failed");

    let time_to_asr = start.elapsed();
    let confirmation = confirmation_time
        .lock()
        .unwrap()
        .map(|instant| instant.duration_since(start))
        .unwrap_or(time_to_asr);

    (time_to_asr, confirmation)
}

#[cfg(feature = "silero-vad")]
async fn run_speculative_flow(audio: RuntimeData) -> (Duration, Duration) {
    let gate = Arc::new(SpeculativeVADGate::new(SpeculativeVADGateConfig::default()));
    let vad = Arc::new(SileroVADNode::new(Some(0.5), Some(16000), None, None, None));

    let start = Instant::now();
    let mut asr_received_at = None;
    let session_id = "speculative_flow_session".to_string();

    let gate_clone = gate.clone();
    let callback = |data: RuntimeData| {
        if matches!(data, RuntimeData::Audio { .. }) && asr_received_at.is_none() {
            asr_received_at = Some(Instant::now());
        }
        Ok(())
    };

    gate_clone
        .process_streaming(audio.clone(), Some(session_id.clone()), callback)
        .await
        .expect("speculative gate processing failed");

    let confirmation_time = Arc::new(Mutex::new(None));
    let confirmation_clone = confirmation_time.clone();
    let gate_for_vad = gate.clone();
    let vad_clone = vad.clone();
    let vad_session = "speculative_vad_confirmation".to_string();
    let samples_in_chunk = audio_sample_count(&audio);

    let handle = tokio::spawn(async move {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let callback = move |data: RuntimeData| {
            if let RuntimeData::Json(json) = data {
                events_clone.lock().unwrap().push(json);
            }
            Ok(())
        };

        vad_clone
            .process_streaming(audio, Some(vad_session), callback)
            .await
            .expect("confirmation VAD processing failed");

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

            gate_for_vad
                .process_vad_result(&session_id, vad_result, |_: RuntimeData| Ok(()))
                .await
                .expect("failed to process VAD result in gate");

            let mut guard = confirmation_clone.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Instant::now());
            }
        }
    });

    handle.await.expect("speculative VAD task panicked");

    let time_to_asr = asr_received_at
        .map(|instant| instant.duration_since(start))
        .unwrap_or_else(|| start.elapsed());
    let confirmation = confirmation_time
        .lock()
        .unwrap()
        .map(|instant| instant.duration_since(start))
        .unwrap_or_else(|| start.elapsed());

    (time_to_asr, confirmation)
}

/// Benchmark: Traditional VAD Pipeline (wait for decision before forwarding to ASR)
#[cfg(feature = "silero-vad")]
fn bench_traditional_vad(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("traditional_pipeline_time_to_asr", |b| {
        b.to_async(&runtime).iter(|| async {
            let audio = create_test_audio();
            let (time_to_asr, confirmation_time) = run_traditional_flow(audio).await;
            black_box((time_to_asr, confirmation_time))
        });
    });
}

/// Benchmark: Speculative VAD Pipeline (forward immediately, VAD in parallel)
fn bench_speculative_vad(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("speculative_pipeline_time_to_asr", |b| {
        b.to_async(&runtime).iter(|| async {
            #[cfg(feature = "silero-vad")]
            {
                let audio = create_test_audio();
                let (time_to_asr, confirmation_time) = run_speculative_flow(audio).await;
                black_box((time_to_asr, confirmation_time))
            }

            #[cfg(not(feature = "silero-vad"))]
            {
                let speculative_vad = SpeculativeVADGate::new();
                let audio = create_test_audio();
                let start = Instant::now();
                let mut asr_received_at = None;

                let callback = |data: RuntimeData| {
                    if matches!(data, RuntimeData::Audio { .. }) && asr_received_at.is_none() {
                        asr_received_at = Some(Instant::now());
                    }
                    Ok(())
                };

                let _ = speculative_vad
                    .process_streaming(audio.clone(), Some("session".to_string()), callback)
                    .await;

                let time_to_asr = asr_received_at
                    .map(|t| t.duration_since(start))
                    .unwrap_or(start.elapsed());

                black_box((time_to_asr, start.elapsed()))
            }
        });
    });
}

/// Benchmark: Latency comparison summary
fn bench_vad_latency_comparison(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("vad_latency_comparison");

    // Measure speculative VAD (should be <1ms)
    group.bench_function("speculative", |b| {
        b.to_async(&runtime).iter(|| async {
            #[cfg(feature = "silero-vad")]
            {
                let audio = create_test_audio();
                let (time_to_asr, _) = run_speculative_flow(audio).await;
                black_box(time_to_asr)
            }
            #[cfg(not(feature = "silero-vad"))]
            {
                let vad = SpeculativeVADGate::new();
                let audio = create_test_audio();
                let start = Instant::now();
                let callback = |_: RuntimeData| Ok(());
                let _ = vad
                    .process_streaming(audio, Some("session".to_string()), callback)
                    .await;
                black_box(start.elapsed())
            }
        });
    });

    #[cfg(feature = "silero-vad")]
    {
        // Measure traditional VAD (will be 5-50ms depending on model)
        group.bench_function("traditional", |b| {
            b.to_async(&runtime).iter(|| async {
                let audio = create_test_audio();
                let (time_to_asr, _) = run_traditional_flow(audio).await;
                black_box(time_to_asr)
            });
        });
    }

    group.finish();
}

// Conditional compilation based on features
#[cfg(feature = "silero-vad")]
criterion_group!(
    benches,
    bench_traditional_vad,
    bench_speculative_vad,
    bench_vad_latency_comparison
);

#[cfg(not(feature = "silero-vad"))]
criterion_group!(benches, bench_speculative_vad, bench_vad_latency_comparison);

criterion_main!(benches);
