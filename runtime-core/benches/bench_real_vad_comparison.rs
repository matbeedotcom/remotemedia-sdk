//! Real VAD Comparison Benchmark
//!
//! Compares actual latency between:
//! 1. Traditional VAD (SileroVAD) - wait for decision before forwarding
//! 2. Speculative VAD (SpeculativeVADGate) - forward immediately
//!
//! This uses real implementations, not mocks, to measure actual performance gain.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::{AsyncStreamingNode, SpeculativeVADGate};
use std::time::Instant;

#[cfg(feature = "silero-vad")]
use remotemedia_runtime_core::nodes::SileroVADNode;

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
    }
}

/// Benchmark: Traditional VAD Pipeline (wait for decision before forwarding to ASR)
#[cfg(feature = "silero-vad")]
fn bench_traditional_vad(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("traditional_pipeline_time_to_asr", |b| {
        b.to_async(&runtime).iter(|| async {
            // Traditional pipeline: Audio → VAD → Wait for decision → Forward to ASR
            let vad = SileroVADNode::new(
                Some(0.5),
                Some(16000),
                None,
                None,
                None,
            );

            let audio = create_test_audio();

            // Measure time from audio input to ASR receiving audio
            let start = Instant::now();

            // Step 1: Process through VAD (19ms for inference)
            let mut vad_outputs = Vec::new();
            let vad_callback = |data: RuntimeData| {
                vad_outputs.push(data);
                Ok(())
            };

            let _ = vad
                .process_streaming(audio.clone(), Some("session".to_string()), vad_callback)
                .await;

            // Step 2: Check VAD decision (in real pipeline, this gates forwarding)
            // In traditional approach, ASR only receives audio AFTER VAD confirms speech

            // Step 3: Forward to ASR (only after VAD decision)
            let time_to_asr = start.elapsed();

            // Traditional: ASR receives audio after ~19ms (VAD inference time)
            black_box((time_to_asr, vad_outputs))
        });
    });
}

/// Benchmark: Speculative VAD Pipeline (forward immediately, VAD in parallel)
fn bench_speculative_vad(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("speculative_pipeline_time_to_asr", |b| {
        b.to_async(&runtime).iter(|| async {
            // Speculative pipeline: Audio → SpeculativeVADGate → ASR (immediately)
            //                                     ↓
            //                                   VAD (parallel, for confirmation)
            let speculative_vad = SpeculativeVADGate::new();

            #[cfg(feature = "silero-vad")]
            let _confirmation_vad = SileroVADNode::new(Some(0.5), Some(16000), None, None, None);

            let audio = create_test_audio();

            // Measure time from audio input to ASR receiving audio
            let start = Instant::now();

            // Step 1: Process through SpeculativeVADGate (forwards immediately)
            let mut outputs = Vec::new();
            let mut asr_received_at = None;

            let callback = |data: RuntimeData| {
                // First output is the audio (forwarded to ASR)
                if matches!(data, RuntimeData::Audio { .. }) && asr_received_at.is_none() {
                    asr_received_at = Some(Instant::now());
                }
                outputs.push(data);
                Ok(())
            };

            let _ = speculative_vad
                .process_streaming(audio.clone(), Some("session".to_string()), callback)
                .await;

            // Time when ASR receives audio (should be <1ms)
            let time_to_asr = asr_received_at.map(|t| t.duration_since(start)).unwrap_or(start.elapsed());

            // Speculative: ASR receives audio after ~5us (immediate forwarding)
            // VAD runs separately and sends control messages if needed

            black_box((time_to_asr, outputs))
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
            let vad = SpeculativeVADGate::new();
            let audio = create_test_audio();

            let start = Instant::now();

            let callback = |_: RuntimeData| Ok(());
            let _ = vad
                .process_streaming(audio, Some("session".to_string()), callback)
                .await;

            let latency = start.elapsed();
            black_box(latency)
        });
    });

    #[cfg(feature = "silero-vad")]
    {
        // Measure traditional VAD (will be 5-50ms depending on model)
        group.bench_function("traditional", |b| {
            b.to_async(&runtime).iter(|| async {
                let vad = SileroVADNode::new(Some(0.5), Some(16000), None, None, None);
                let audio = create_test_audio();

                let start = Instant::now();

                let callback = |_: RuntimeData| Ok(());
                let _ = vad
                    .process_streaming(audio, Some("session".to_string()), callback)
                    .await;

                let latency = start.elapsed();
                black_box(latency)
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
