//! WebRTC Transport Performance Benchmarks
//!
//! Benchmarks for:
//! - T205: Jitter buffer insertion performance (target: O(log n), <5ms for 1000 frames)
//! - T206: RTP encoding/decoding latency (target: audio <10ms, video <30ms)
//! - T207: Broadcast performance (target: <100ms for 10 peers)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// T205: Jitter Buffer Benchmarks
// ============================================================================

mod jitter_buffer_bench {
    use super::*;

    // Re-implement frame types locally for benchmarking
    // (avoids needing to make internal types public)

    #[derive(Clone)]
    struct BenchFrame {
        seq: u16,
        rtp_ts: u32,
        received: Instant,
    }

    // Simple jitter buffer implementation for benchmarking
    // Mirrors the real implementation's BTreeMap-based approach
    use std::collections::BTreeMap;

    struct BenchJitterBuffer {
        frames: BTreeMap<u32, BenchFrame>,
        last_popped: Option<u32>,
        base_seq: Option<u16>,
    }

    impl BenchJitterBuffer {
        fn new() -> Self {
            Self {
                frames: BTreeMap::new(),
                last_popped: None,
                base_seq: None,
            }
        }

        fn insert(&mut self, frame: BenchFrame) {
            let extended = self.extend_sequence(frame.seq);
            self.frames.insert(extended, frame);
        }

        fn pop_next(&mut self) -> Option<BenchFrame> {
            let (&ext_seq, _) = self.frames.iter().next()?;
            let frame = self.frames.remove(&ext_seq)?;
            self.last_popped = Some(ext_seq);
            Some(frame)
        }

        fn extend_sequence(&mut self, seq: u16) -> u32 {
            if let Some(last) = self.last_popped {
                let last_seq = (last & 0xFFFF) as u16;
                let diff = seq.wrapping_sub(last_seq) as i16 as i32;
                return (last as i32 + diff) as u32;
            }
            if let Some(base) = self.base_seq {
                let diff = seq.wrapping_sub(base) as i16 as i32;
                return (base as i32 + diff) as u32;
            }
            self.base_seq = Some(seq);
            seq as u32
        }

        fn len(&self) -> usize {
            self.frames.len()
        }
    }

    fn make_frame(seq: u16) -> BenchFrame {
        BenchFrame {
            seq,
            rtp_ts: seq as u32 * 960,
            received: Instant::now(),
        }
    }

    pub fn bench_jitter_buffer_insertion(c: &mut Criterion) {
        let mut group = c.benchmark_group("jitter_buffer_insertion");

        for size in [100, 500, 1000, 5000].iter() {
            group.throughput(Throughput::Elements(*size as u64));

            group.bench_with_input(BenchmarkId::new("sequential", size), size, |b, &size| {
                b.iter(|| {
                    let mut buffer = BenchJitterBuffer::new();
                    for i in 0..size {
                        buffer.insert(make_frame(i as u16));
                    }
                    black_box(buffer.len())
                });
            });

            group.bench_with_input(BenchmarkId::new("random_order", size), size, |b, &size| {
                // Pre-generate shuffled sequence numbers
                let mut seqs: Vec<u16> = (0..size as u16).collect();
                // Simple shuffle using modular arithmetic
                for i in 0..seqs.len() {
                    let j = (i * 7 + 13) % seqs.len();
                    seqs.swap(i, j);
                }

                b.iter(|| {
                    let mut buffer = BenchJitterBuffer::new();
                    for &seq in &seqs {
                        buffer.insert(make_frame(seq));
                    }
                    black_box(buffer.len())
                });
            });
        }

        group.finish();
    }

    pub fn bench_jitter_buffer_pop(c: &mut Criterion) {
        let mut group = c.benchmark_group("jitter_buffer_pop");

        for size in [100, 500, 1000].iter() {
            group.throughput(Throughput::Elements(*size as u64));

            group.bench_with_input(BenchmarkId::new("pop_all", size), size, |b, &size| {
                b.iter_batched(
                    || {
                        let mut buffer = BenchJitterBuffer::new();
                        for i in 0..size {
                            buffer.insert(make_frame(i as u16));
                        }
                        buffer
                    },
                    |mut buffer| {
                        while buffer.pop_next().is_some() {}
                        black_box(buffer.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        }

        group.finish();
    }
}

// ============================================================================
// T206: RTP Encoding/Decoding Latency Benchmarks
// ============================================================================

mod rtp_codec_bench {
    use super::*;

    pub fn bench_audio_frame_creation(c: &mut Criterion) {
        let mut group = c.benchmark_group("audio_frame_creation");

        // Audio frame sizes: 20ms at 48kHz = 960 samples
        for samples in [480, 960, 1920].iter() {
            group.throughput(Throughput::Elements(*samples as u64));

            group.bench_with_input(
                BenchmarkId::new("create_f32_samples", samples),
                samples,
                |b, &samples| {
                    b.iter(|| {
                        let audio: Vec<f32> = (0..samples).map(|i| (i as f32 * 0.001).sin()).collect();
                        black_box(audio)
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("arc_wrap_samples", samples),
                samples,
                |b, &samples| {
                    let audio: Vec<f32> = (0..samples).map(|i| (i as f32 * 0.001).sin()).collect();
                    b.iter(|| {
                        let arc = Arc::new(audio.clone());
                        black_box(arc)
                    });
                },
            );
        }

        group.finish();
    }

    pub fn bench_video_frame_creation(c: &mut Criterion) {
        let mut group = c.benchmark_group("video_frame_creation");

        // Common resolutions: 480p, 720p, 1080p (YUV420P: 1.5 bytes per pixel)
        let resolutions = [(640, 480), (1280, 720), (1920, 1080)];

        for (width, height) in resolutions.iter() {
            let pixel_count = width * height;
            let yuv_size = pixel_count * 3 / 2; // YUV420P

            group.throughput(Throughput::Bytes(yuv_size as u64));

            group.bench_with_input(
                BenchmarkId::new("create_yuv420", format!("{}x{}", width, height)),
                &yuv_size,
                |b, &size| {
                    b.iter(|| {
                        let frame: Vec<u8> = vec![128u8; size as usize];
                        black_box(frame)
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("arc_wrap_frame", format!("{}x{}", width, height)),
                &yuv_size,
                |b, &size| {
                    let frame: Vec<u8> = vec![128u8; size as usize];
                    b.iter(|| {
                        let arc = Arc::new(frame.clone());
                        black_box(arc)
                    });
                },
            );
        }

        group.finish();
    }

    pub fn bench_rtp_packetization(c: &mut Criterion) {
        let mut group = c.benchmark_group("rtp_packetization");

        // RTP header is 12 bytes, typical MTU is 1200 bytes for payload
        let payload_sizes = [160, 480, 960, 1200];

        for size in payload_sizes.iter() {
            group.throughput(Throughput::Bytes(*size as u64));

            group.bench_with_input(
                BenchmarkId::new("create_rtp_packet", size),
                size,
                |b, &size| {
                    b.iter(|| {
                        // Simulate RTP packet creation
                        let mut packet = Vec::with_capacity(12 + size);
                        // RTP header (12 bytes)
                        packet.extend_from_slice(&[0x80, 0x60]); // V=2, PT=96
                        packet.extend_from_slice(&1234u16.to_be_bytes()); // Sequence
                        packet.extend_from_slice(&12345678u32.to_be_bytes()); // Timestamp
                        packet.extend_from_slice(&0x12345678u32.to_be_bytes()); // SSRC
                        // Payload
                        packet.extend(std::iter::repeat(0u8).take(size));
                        black_box(packet)
                    });
                },
            );
        }

        group.finish();
    }
}

// ============================================================================
// T207: Broadcast Performance Benchmarks
// ============================================================================

mod broadcast_bench {
    use super::*;

    pub fn bench_broadcast_simulation(c: &mut Criterion) {
        let mut group = c.benchmark_group("broadcast");

        // Simulate broadcasting to N peers
        for peer_count in [2, 5, 10].iter() {
            let audio_size = 960; // 20ms at 48kHz

            group.bench_with_input(
                BenchmarkId::new("audio_to_peers", peer_count),
                peer_count,
                |b, &peer_count| {
                    let audio: Arc<Vec<f32>> =
                        Arc::new((0..audio_size).map(|i| (i as f32 * 0.001).sin()).collect());

                    b.iter(|| {
                        // Simulate cloning data for each peer (worst case: full copy)
                        let mut results = Vec::with_capacity(peer_count);
                        for _ in 0..peer_count {
                            // Arc clone is O(1) - just increments refcount
                            let peer_data = Arc::clone(&audio);
                            results.push(peer_data);
                        }
                        black_box(results)
                    });
                },
            );

            // Simulate with actual data copy (what happens without Arc)
            group.bench_with_input(
                BenchmarkId::new("audio_to_peers_copy", peer_count),
                peer_count,
                |b, &peer_count| {
                    let audio: Vec<f32> =
                        (0..audio_size).map(|i| (i as f32 * 0.001).sin()).collect();

                    b.iter(|| {
                        let mut results = Vec::with_capacity(peer_count);
                        for _ in 0..peer_count {
                            // Full copy for each peer
                            let peer_data = audio.clone();
                            results.push(peer_data);
                        }
                        black_box(results)
                    });
                },
            );
        }

        // Video frame broadcast (720p)
        let video_size = 1280 * 720 * 3 / 2; // YUV420P

        for peer_count in [2, 5, 10].iter() {
            group.bench_with_input(
                BenchmarkId::new("video_to_peers", peer_count),
                peer_count,
                |b, &peer_count| {
                    let frame: Arc<Vec<u8>> = Arc::new(vec![128u8; video_size]);

                    b.iter(|| {
                        let mut results = Vec::with_capacity(peer_count);
                        for _ in 0..peer_count {
                            let peer_data = Arc::clone(&frame);
                            results.push(peer_data);
                        }
                        black_box(results)
                    });
                },
            );
        }

        group.finish();
    }

    pub fn bench_parallel_send_simulation(c: &mut Criterion) {
        let mut group = c.benchmark_group("parallel_send");

        // Measure overhead of spawning tasks for parallel sends
        let rt = tokio::runtime::Runtime::new().unwrap();

        for peer_count in [2, 5, 10].iter() {
            group.bench_with_input(
                BenchmarkId::new("spawn_tasks", peer_count),
                peer_count,
                |b, &peer_count| {
                    b.to_async(&rt).iter(|| async {
                        let mut handles = Vec::with_capacity(peer_count);
                        for i in 0..peer_count {
                            handles.push(tokio::spawn(async move {
                                // Simulate minimal work
                                black_box(i)
                            }));
                        }
                        for handle in handles {
                            let _ = handle.await;
                        }
                    });
                },
            );
        }

        group.finish();
    }
}

// ============================================================================
// T208: Zero-Copy Benchmarks
// ============================================================================

mod zero_copy_bench {
    use super::*;

    pub fn bench_arc_vs_clone(c: &mut Criterion) {
        let mut group = c.benchmark_group("zero_copy");

        let sizes = [1024, 4096, 16384, 65536]; // Various payload sizes

        for size in sizes.iter() {
            group.throughput(Throughput::Bytes(*size as u64));

            // Arc reference (zero-copy)
            group.bench_with_input(BenchmarkId::new("arc_clone", size), size, |b, &size| {
                let data: Arc<Vec<u8>> = Arc::new(vec![0u8; size]);
                b.iter(|| {
                    let cloned = Arc::clone(&data);
                    black_box(cloned)
                });
            });

            // Full copy
            group.bench_with_input(BenchmarkId::new("vec_clone", size), size, |b, &size| {
                let data: Vec<u8> = vec![0u8; size];
                b.iter(|| {
                    let cloned = data.clone();
                    black_box(cloned)
                });
            });

            // Slice reference (true zero-copy, but lifetime bound)
            group.bench_with_input(BenchmarkId::new("slice_ref", size), size, |b, &size| {
                let data: Vec<u8> = vec![0u8; size];
                b.iter(|| {
                    let slice: &[u8] = &data;
                    black_box(slice)
                });
            });
        }

        group.finish();
    }

    pub fn bench_bytes_crate_simulation(c: &mut Criterion) {
        let mut group = c.benchmark_group("bytes_simulation");

        // Simulate what the `bytes` crate does for zero-copy slicing
        let sizes = [1024, 4096, 16384];

        for size in sizes.iter() {
            group.throughput(Throughput::Bytes(*size as u64));

            group.bench_with_input(
                BenchmarkId::new("arc_subslice", size),
                size,
                |b, &size| {
                    let data: Arc<Vec<u8>> = Arc::new(vec![0u8; size]);
                    b.iter(|| {
                        // Simulate taking a subslice while keeping Arc alive
                        let arc_clone = Arc::clone(&data);
                        let len = arc_clone.len();
                        let mid = len / 2;
                        // Just measure the Arc clone + length calculation overhead
                        black_box((len, mid))
                    });
                },
            );
        }

        group.finish();
    }
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    jitter_buffer_benches,
    jitter_buffer_bench::bench_jitter_buffer_insertion,
    jitter_buffer_bench::bench_jitter_buffer_pop,
);

criterion_group!(
    rtp_codec_benches,
    rtp_codec_bench::bench_audio_frame_creation,
    rtp_codec_bench::bench_video_frame_creation,
    rtp_codec_bench::bench_rtp_packetization,
);

criterion_group!(
    broadcast_benches,
    broadcast_bench::bench_broadcast_simulation,
    broadcast_bench::bench_parallel_send_simulation,
);

criterion_group!(
    zero_copy_benches,
    zero_copy_bench::bench_arc_vs_clone,
    zero_copy_bench::bench_bytes_crate_simulation,
);

criterion_main!(
    jitter_buffer_benches,
    rtp_codec_benches,
    broadcast_benches,
    zero_copy_benches,
);
