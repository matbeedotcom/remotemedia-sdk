//! M2.6: avatar coordinator `barge_in_targets` propagation against the
//! real session router.
//!
//! Asserts that when a coordinator publishes `<node>.in.barge_in` via
//! [`SessionControl::publish`], the runtime delivers the wrapped
//! envelope to the target node's `process_control_message` (the
//! plumbing landed in M2.6: `AsyncNodeWrapper` forward + filter task
//! forward + main task dispatch). The lip-sync node clears its
//! cumulative-ms clock on receipt, so the next audio chunk's
//! `BlendshapeFrame.pts_ms` starts at zero again.
//!
//! Uses [`SyntheticLipSyncNode`] for the assertion target — same wire
//! format + barge contract as the real `Audio2FaceLipSyncNode`, but
//! deterministic and dependency-free, so the test runs on every CI
//! host. Tier-2 coverage of the same flow against the actual model
//! ships in `audio2face_lipsync_node_test.rs::process_control_message_barge_in_clears_state`.

#![cfg(feature = "avatar-lipsync")]

use std::sync::Arc;
use std::time::Duration;

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::nodes::lip_sync::BlendshapeFrame;
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use remotemedia_core::transport::session_control::{ControlAddress, SessionControl};
use remotemedia_core::transport::session_router::{
    DataPacket, SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
};
use tokio::sync::mpsc;

fn synth_lipsync_pipeline() -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "lipsync-barge-e2e".to_string(),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "lipsync".to_string(),
            node_type: "SyntheticLipSyncNode".to_string(),
            params: serde_json::json!({"sampleRate": 16000, "gain": 6.0}),
            ..Default::default()
        }],
        connections: Vec::<Connection>::new(),
        python_env: None,
    }
}

fn audio_packet(session_id: &str, samples: Vec<f32>, seq: u64) -> DataPacket {
    DataPacket {
        data: RuntimeData::Audio {
            samples: AudioSamples::Vec(samples),
            sample_rate: 16_000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        },
        from_node: "client".to_string(),
        to_node: None,
        session_id: session_id.to_string(),
        sequence: seq,
        sub_sequence: 0,
    }
}

fn extract_blendshape(data: &RuntimeData) -> BlendshapeFrame {
    match data {
        RuntimeData::Json(v) => BlendshapeFrame::from_json(v).expect("BlendshapeFrame"),
        other => panic!("expected Json BlendshapeFrame, got {:?}", other),
    }
}

#[tokio::test]
async fn manifest_barge_in_resets_lipsync_pts_ms() {
    let session_id = "lipsync-barge".to_string();
    let manifest = Arc::new(synth_lipsync_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // 100 ms chunk → pts_ms == 100.
    input_tx
        .send(audio_packet(&session_id, vec![0.1; 1600], 0))
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("first output timeout")
        .expect("client channel closed");
    assert_eq!(extract_blendshape(&first).pts_ms, 100);

    // Another 100 ms chunk → pts_ms == 200. (Confirms the clock
    // advances under normal flow, which is what makes the post-barge
    // restart-to-100 assertion meaningful.)
    input_tx
        .send(audio_packet(&session_id, vec![0.1; 1600], 1))
        .await
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("second output timeout")
        .expect("client channel closed");
    assert_eq!(extract_blendshape(&second).pts_ms, 200);

    // Coordinator-style barge publish. Mirrors what
    // `ConversationCoordinatorNode::dispatch_barge` does for every
    // entry in `barge_in_targets`.
    ctrl.publish(
        &ControlAddress::node_in("lipsync").with_port("barge_in"),
        RuntimeData::Json(serde_json::json!({})),
    )
    .await
    .unwrap();

    // Give the router's filter + main tasks a beat to propagate.
    // (Bounded — much shorter than the timeout below; we're guarding
    // against test-host scheduling jitter, not waiting on real work.)
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Next 100 ms chunk → pts_ms restarts at 100.
    input_tx
        .send(audio_packet(&session_id, vec![0.1; 1600], 2))
        .await
        .unwrap();
    let post = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("post-barge output timeout")
        .expect("client channel closed");
    assert_eq!(
        extract_blendshape(&post).pts_ms,
        100,
        "barge published via SessionControl must reach the lip-sync \
         node's process_control_message and reset its clock"
    );

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

/// Sanity: barge publishes addressed to a *different* node id don't
/// affect the lip-sync. Pins the routing so a future refactor that
/// accidentally broadcasts barges sees this fail.
#[tokio::test]
async fn manifest_barge_in_to_other_node_is_no_op() {
    let session_id = "lipsync-barge-other".to_string();
    let manifest = Arc::new(synth_lipsync_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Advance clock to 100 ms.
    input_tx
        .send(audio_packet(&session_id, vec![0.1; 1600], 0))
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("first output timeout");

    // Try to barge a node that doesn't exist in this manifest — the
    // publish call returns Ok (bus accepts the packet), but no node
    // handler runs. The lip-sync's clock should NOT reset.
    let _ = ctrl
        .publish(
            &ControlAddress::node_in("nonexistent").with_port("barge_in"),
            RuntimeData::Json(serde_json::json!({})),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    input_tx
        .send(audio_packet(&session_id, vec![0.1; 1600], 1))
        .await
        .unwrap();
    let next = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("second output timeout")
        .expect("client channel closed");
    assert_eq!(
        extract_blendshape(&next).pts_ms,
        200,
        "wrong-node barge must not reset the lip-sync clock"
    );

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}
