//! `audio.out.clock` tap tests (avatar plan M1, spec 2026-04-27 §3.6).
//!
//! Locks the spec invariants:
//! - one tap publish per dequeued audio frame, on key (node_id, "clock")
//! - envelope shape: {kind: "audio_clock", pts_ms: <u64>, stream_id?: <str>}
//! - `pts_ms` monotonic and tracks cumulative audio playback duration
//! - no publish when the ring buffer is empty (spec: "that is the signal
//!   the renderer uses to start interpolating to neutral pose")
//! - sender without a clock tap does not panic, does not publish

use remotemedia_core::data::RuntimeData;
use remotemedia_core::transport::session_control::{ControlAddress, SessionControl};
use remotemedia_webrtc::media::audio_sender::{AudioSender, ClockTap};
use std::sync::Arc;
use std::time::Duration;
use webrtc::api::media_engine::MIME_TYPE_OPUS;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

fn opus_track() -> Arc<TrackLocalStaticSample> {
    Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_string(),
            ..Default::default()
        },
        "audio-clock-tap-test".to_string(),
        "stream-clock-tap-test".to_string(),
    ))
}

/// Push N synthetic 20-ms Opus frames into the ring buffer.
async fn push_n_frames(sender: &AudioSender, n: usize) {
    for _ in 0..n {
        sender
            .enqueue_frame(vec![0u8; 32], 960, Duration::from_millis(20))
            .await
            .expect("enqueue");
    }
}

/// Drain a broadcast receiver until it idles for `idle` duration.
async fn drain_until_idle(
    rx: &mut tokio::sync::broadcast::Receiver<RuntimeData>,
    idle: Duration,
) -> Vec<RuntimeData> {
    let mut out = Vec::new();
    loop {
        match tokio::time::timeout(idle, rx.recv()).await {
            Ok(Ok(item)) => out.push(item),
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => break,
        }
    }
    out
}

#[tokio::test]
async fn audio_sender_publishes_clock_tap_per_dequeued_frame() {
    let ctrl = SessionControl::new("test-clock-tap");
    let mut rx = ctrl
        .subscribe(&ControlAddress::node_out("audio").with_port("clock"))
        .expect("subscribe");

    let sender = AudioSender::new(opus_track(), 64);
    sender.set_clock_tap(ClockTap {
        control: Arc::clone(&ctrl),
        node_id: "audio".to_string(),
        stream_id: Some("avatar-stream".to_string()),
    });

    push_n_frames(&sender, 3).await;

    // The transmission thread paces at frame.duration (20 ms each), so
    // 3 frames take ~60 ms to fully drain. Wait conservatively then
    // collect every publish that landed.
    let publishes = drain_until_idle(&mut rx, Duration::from_millis(120)).await;

    assert_eq!(
        publishes.len(),
        3,
        "expected one publish per dequeued frame, got {}",
        publishes.len()
    );

    let mut last_pts: i64 = -1;
    for p in &publishes {
        let json = match p {
            RuntimeData::Json(v) => v,
            other => panic!("expected Json envelope, got {:?}", other),
        };
        assert_eq!(json["kind"], "audio_clock", "envelope kind");
        let pts = json["pts_ms"].as_u64().expect("pts_ms must be u64") as i64;
        assert!(pts > last_pts, "pts_ms monotonic ({pts} > {last_pts})");
        last_pts = pts;
        assert_eq!(
            json["stream_id"], "avatar-stream",
            "stream_id round-trips into envelope"
        );
    }

    // First frame is 20 ms long, so first pts_ms reflects ~20 ms cumulative.
    let first_pts = publishes[0].as_json().unwrap()["pts_ms"].as_u64().unwrap();
    assert!(
        (15..=30).contains(&first_pts),
        "first pts_ms ≈ frame.duration (20 ms), got {first_pts}"
    );
    let _ = sender.shutdown().await;
}

#[tokio::test]
async fn no_publishes_when_ring_buffer_empty() {
    // Spec §3.6: "When the audio ring buffer is empty, no publishes
    // happen — that is the signal the renderer uses to start
    // interpolating to neutral pose."
    let ctrl = SessionControl::new("test-clock-tap-empty");
    let mut rx = ctrl
        .subscribe(&ControlAddress::node_out("audio").with_port("clock"))
        .expect("subscribe");

    let sender = AudioSender::new(opus_track(), 64);
    sender.set_clock_tap(ClockTap {
        control: Arc::clone(&ctrl),
        node_id: "audio".to_string(),
        stream_id: None,
    });

    // Don't push anything. Wait long enough that the transmission
    // thread would have published *if* it were emitting heartbeats.
    let res = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await;
    assert!(
        res.is_err(),
        "no publish expected when buffer is empty, got {:?}",
        res
    );
    let _ = sender.shutdown().await;
}

#[tokio::test]
async fn no_clock_tap_means_no_publish_no_panic() {
    // The clock tap is opt-in — sending audio without one must not
    // panic, must not publish, must not regress audio output.
    let sender = AudioSender::new(opus_track(), 64);
    push_n_frames(&sender, 3).await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    // Test passes if no panic. (We can't subscribe to a tap that was
    // never published to — there's no state to assert except absence
    // of crash.)
    let _ = sender.shutdown().await;
}

#[tokio::test]
async fn pts_ms_resets_after_buffer_clear() {
    // Barge-in calls `flush_buffer()`, which drains the ring. The
    // *next* frame's pts_ms continues monotonically from the prior
    // cumulative duration — it's a clock, not a per-utterance reset.
    // Renderer's stale-pts ring eviction (pts_ms < audio_clock_ms - 200)
    // handles the barge-cleanup; spec §6.3.
    let ctrl = SessionControl::new("test-clock-tap-flush");
    let mut rx = ctrl
        .subscribe(&ControlAddress::node_out("audio").with_port("clock"))
        .expect("subscribe");

    let sender = AudioSender::new(opus_track(), 64);
    sender.set_clock_tap(ClockTap {
        control: Arc::clone(&ctrl),
        node_id: "audio".to_string(),
        stream_id: None,
    });

    push_n_frames(&sender, 2).await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    let first_batch = drain_until_idle(&mut rx, Duration::from_millis(20)).await;
    assert_eq!(first_batch.len(), 2);

    // Flush the buffer (simulates barge-in), enqueue more frames.
    sender.flush_buffer();
    push_n_frames(&sender, 2).await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    let second_batch = drain_until_idle(&mut rx, Duration::from_millis(20)).await;

    let last_first = first_batch[1].as_json().unwrap()["pts_ms"].as_u64().unwrap();
    let first_second = second_batch[0].as_json().unwrap()["pts_ms"]
        .as_u64()
        .unwrap();
    assert!(
        first_second > last_first,
        "pts_ms keeps advancing after flush (it is a wall-of-played-audio clock); \
         got last_first={last_first}, first_second={first_second}"
    );
    let _ = sender.shutdown().await;
}

/// Helper to make `[i].as_json()` chains terse in the assertions above.
trait AsJson {
    fn as_json(&self) -> Option<&serde_json::Value>;
}
impl AsJson for RuntimeData {
    fn as_json(&self) -> Option<&serde_json::Value> {
        if let RuntimeData::Json(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
