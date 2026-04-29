//! M4.5 integration tests: `Live2DRenderNode` driven through
//! `MockBackend`, asserting the streaming-node contract — frames
//! emitted at the configured framerate, stamped with the
//! configured `stream_id`, regardless of input pressure.

#![cfg(feature = "avatar-render-test-support")]

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::video::PixelFormat;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::live2d_render::{
    Live2DRenderConfig, Live2DRenderNode, MockBackend,
};
use remotemedia_core::nodes::AsyncStreamingNode;
use remotemedia_core::transport::session_control::{wrap_aux_port, BARGE_IN_PORT};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Helper: pump the node by feeding a no-op input + draining the
/// callback. Returns the collected outputs.
async fn drive_once(
    node: &Live2DRenderNode,
    input: RuntimeData,
) -> Vec<RuntimeData> {
    let collected = Arc::new(Mutex::new(Vec::new()));
    let cc = collected.clone();
    node.process_streaming(input, None, move |out| {
        cc.lock().unwrap().push(out);
        Ok(())
    })
    .await
    .expect("process_streaming");
    let v = collected.lock().unwrap().clone();
    v
}

/// Helper: drive the node for `seconds` of wall time, polling at
/// ~60 Hz so the ticker has chances to interleave.
async fn drive_for_seconds(node: &Live2DRenderNode, seconds: f32) -> Vec<RuntimeData> {
    let mut all = Vec::new();
    let poll = Duration::from_millis(16);
    let total = Duration::from_secs_f32(seconds);
    let start = std::time::Instant::now();
    while start.elapsed() < total {
        // No input — just drain anything the ticker queued.
        let drained = drive_once(node, RuntimeData::Text(String::new())).await;
        all.extend(drained);
        tokio::time::sleep(poll).await;
    }
    all
}

fn config_with_framerate(fps: u32, stream_id: &str) -> Live2DRenderConfig {
    Live2DRenderConfig {
        framerate: fps,
        video_stream_id: stream_id.to_string(),
        ..Live2DRenderConfig::default()
    }
}

#[tokio::test]
async fn emits_video_at_configured_framerate_with_stream_id() {
    let backend = Box::new(MockBackend::default_hd());
    let node = Live2DRenderNode::new_with_backend(
        backend,
        config_with_framerate(30, "avatar"),
    );

    let outputs = drive_for_seconds(&node, 1.0).await;
    let video_frames: Vec<_> = outputs
        .iter()
        .filter(|o| matches!(o, RuntimeData::Video { .. }))
        .collect();

    assert!(
        (25..=35).contains(&video_frames.len()),
        "expected ~30 fps over 1s; got {} frames",
        video_frames.len()
    );

    for frame in &video_frames {
        let RuntimeData::Video {
            stream_id, format, width, height, pixel_data, ..
        } = frame else {
            unreachable!()
        };
        assert_eq!(stream_id.as_deref(), Some("avatar"));
        assert_eq!(*format, PixelFormat::Rgb24);
        assert_eq!((*width, *height), (1280, 720));
        // MockBackend returns a black frame; pixel_data length must
        // match the format size.
        assert_eq!(pixel_data.len(), (*width as usize) * (*height as usize) * 3);
    }
}

#[tokio::test]
async fn frame_numbers_are_monotonic_starting_from_zero() {
    let backend = Box::new(MockBackend::new(64, 64));
    let node =
        Live2DRenderNode::new_with_backend(backend, config_with_framerate(60, "avatar"));

    let outputs = drive_for_seconds(&node, 0.6).await;
    let mut last_frame_number: Option<u64> = None;
    let mut count = 0;
    for o in &outputs {
        if let RuntimeData::Video { frame_number, .. } = o {
            if let Some(prev) = last_frame_number {
                assert_eq!(
                    *frame_number,
                    prev + 1,
                    "frame numbers must increase by 1; got {} after {}",
                    frame_number,
                    prev
                );
            } else {
                assert_eq!(*frame_number, 0, "first frame must be #0");
            }
            last_frame_number = Some(*frame_number);
            count += 1;
        }
    }
    assert!(count > 0);
}

#[tokio::test]
async fn renders_continue_when_no_audio_clock_arrives() {
    // Spec §6.1: no input pressure dictates render rate. With
    // zero inputs except the empty Text drains, the ticker still
    // emits frames at ~30 fps.
    let backend = Box::new(MockBackend::new(64, 64));
    let node =
        Live2DRenderNode::new_with_backend(backend, config_with_framerate(30, "avatar"));

    let outputs = drive_for_seconds(&node, 0.6).await;
    let frames: Vec<_> =
        outputs.iter().filter(|o| matches!(o, RuntimeData::Video { .. })).collect();
    assert!(
        frames.len() >= 12,
        "renderer should keep emitting without inputs; got {} frames in 0.6s",
        frames.len()
    );
}

#[tokio::test]
async fn blendshape_input_drives_pose_to_backend() {
    let mock = MockBackend::new(64, 64);
    let mock_handle = mock.clone();
    let node = Live2DRenderNode::new_with_backend(
        Box::new(mock),
        config_with_framerate(60, "avatar"),
    );

    // Push a blendshape envelope with jawOpen at 0.8.
    let mut arkit = [0.0f32; 52];
    arkit[17] = 0.8; // jawOpen index
    let blend_envelope = RuntimeData::Json(serde_json::json!({
        "kind": "blendshapes",
        "arkit_52": arkit.to_vec(),
        "pts_ms": 100,
    }));
    let _ = drive_once(&node, blend_envelope).await;

    // Push an audio clock so the state machine picks the keyframe.
    let clock = RuntimeData::Json(serde_json::json!({
        "kind": "audio_clock",
        "pts_ms": 100,
    }));
    let _ = drive_once(&node, clock).await;

    // Wait for a few render ticks.
    let _ = drive_for_seconds(&node, 0.2).await;

    // Eventually the recorded pose stream should contain a frame
    // with a non-zero ParamJawOpen value.
    let recorded = mock_handle.recorded();
    assert!(!recorded.is_empty(), "MockBackend should have recorded frames");
    let any_jaw_open = recorded
        .iter()
        .any(|r| r.pose.mouth_value("ParamJawOpen") > 0.5);
    assert!(
        any_jaw_open,
        "expected at least one frame with ParamJawOpen > 0.5; \
         max observed = {:?}",
        recorded
            .iter()
            .map(|r| r.pose.mouth_value("ParamJawOpen"))
            .fold(0.0f32, f32::max)
    );
}

#[tokio::test]
async fn emotion_input_routes_to_state_machine() {
    let mock = MockBackend::new(64, 64);
    let mock_handle = mock.clone();
    let node = Live2DRenderNode::new_with_backend(
        Box::new(mock),
        config_with_framerate(60, "avatar"),
    );

    // 🤩 → excited_star + Excited per default emotion mapping.
    let envelope = RuntimeData::Json(serde_json::json!({
        "kind": "emotion",
        "emoji": "\u{1F929}",
    }));
    let _ = drive_once(&node, envelope).await;
    let _ = drive_for_seconds(&node, 0.2).await;

    let recorded = mock_handle.recorded();
    assert!(
        recorded.iter().any(|r| r.pose.expression_id == "excited_star"),
        "expected at least one frame with expression_id = excited_star; \
         observed expressions: {:?}",
        recorded
            .iter()
            .map(|r| r.pose.expression_id.clone())
            .collect::<std::collections::HashSet<_>>()
    );
}

#[tokio::test]
async fn barge_envelope_via_data_path_clears_blendshape_ring() {
    // The renderer accepts both wire formats:
    //   1. Plain Json `{kind: "barge_in"}` on the data path.
    //   2. Aux-port wrapped barge envelope via process_control_message
    //      (covered in the next test).
    let mock = MockBackend::new(64, 64);
    let mock_handle = mock.clone();
    let node = Live2DRenderNode::new_with_backend(
        Box::new(mock),
        config_with_framerate(60, "avatar"),
    );

    // Seed: open mouth + active emotion.
    let mut arkit = [0.0f32; 52];
    arkit[17] = 1.0;
    let _ = drive_once(
        &node,
        RuntimeData::Json(serde_json::json!({
            "kind": "blendshapes", "arkit_52": arkit.to_vec(), "pts_ms": 100,
        })),
    )
    .await;
    let _ = drive_once(
        &node,
        RuntimeData::Json(serde_json::json!({
            "kind": "audio_clock", "pts_ms": 100,
        })),
    )
    .await;
    let _ = drive_once(
        &node,
        RuntimeData::Json(serde_json::json!({"kind": "emotion", "emoji": "\u{1F929}"})),
    )
    .await;
    let _ = drive_for_seconds(&node, 0.05).await;

    // Barge.
    let _ = drive_once(
        &node,
        RuntimeData::Json(serde_json::json!({"kind": "barge_in"})),
    )
    .await;
    mock_handle.reset();
    let _ = drive_for_seconds(&node, 0.15).await;

    let post_barge = mock_handle.recorded();
    assert!(!post_barge.is_empty());
    // After barge: mouth is neutral (audio clock dropped → mouth eased
    // back to 0). Emotion is preserved.
    let last = &post_barge.last().unwrap().pose;
    assert!(
        last.mouth_value("ParamJawOpen") < 0.5,
        "mouth should ease back to neutral after barge; got {}",
        last.mouth_value("ParamJawOpen")
    );
    assert_eq!(
        last.expression_id, "excited_star",
        "barge must NOT clear the active emotion"
    );
}

#[tokio::test]
async fn barge_envelope_via_aux_port_clears_blendshape_ring() {
    // Mirrors the M2.6 router → process_control_message path.
    let mock = MockBackend::new(64, 64);
    let mock_handle = mock.clone();
    let node = Live2DRenderNode::new_with_backend(
        Box::new(mock),
        config_with_framerate(60, "avatar"),
    );

    let mut arkit = [0.0f32; 52];
    arkit[17] = 1.0;
    let _ = drive_once(
        &node,
        RuntimeData::Json(serde_json::json!({
            "kind": "blendshapes", "arkit_52": arkit.to_vec(), "pts_ms": 100,
        })),
    )
    .await;
    let _ = drive_once(
        &node,
        RuntimeData::Json(serde_json::json!({
            "kind": "audio_clock", "pts_ms": 100,
        })),
    )
    .await;
    let _ = drive_for_seconds(&node, 0.05).await;

    // M2.6 router calls `process_control_message` with a wrapped
    // aux-port envelope. The renderer overrides this to push BargeIn.
    let envelope = wrap_aux_port(BARGE_IN_PORT, RuntimeData::Json(serde_json::json!({})));
    let handled = node
        .process_control_message(envelope, Some("session-1".into()))
        .await
        .expect("process_control_message");
    assert!(handled, "renderer must claim the barge_in envelope");

    mock_handle.reset();
    let _ = drive_for_seconds(&node, 0.15).await;

    let last = mock_handle.recorded().last().unwrap().pose.clone();
    assert!(
        last.mouth_value("ParamJawOpen") < 0.5,
        "mouth should ease back to neutral after aux-port barge"
    );
}

#[tokio::test]
async fn renderer_passes_through_audio_for_passthrough_data() {
    // Non-Json envelopes are recognized as "not a renderer input"
    // and silently ignored (the renderer is a sink — its outputs
    // are video; non-recognized inputs aren't forwarded).
    let backend = Box::new(MockBackend::new(64, 64));
    let node =
        Live2DRenderNode::new_with_backend(backend, config_with_framerate(60, "avatar"));

    // Audio is unrelated; the renderer doesn't process it.
    let audio = RuntimeData::Audio {
        samples: AudioSamples::Vec(vec![0.0; 100]),
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    };
    let outs = drive_once(&node, audio).await;

    // Outputs (if any) should be Video frames, never the input
    // audio passing through.
    for o in &outs {
        assert!(
            matches!(o, RuntimeData::Video { .. }),
            "renderer should only emit Video, not pass through audio"
        );
    }
}

#[tokio::test]
async fn frame_dimensions_match_backend_dimensions() {
    let backend = Box::new(MockBackend::new(640, 480));
    let node = Live2DRenderNode::new_with_backend(
        backend,
        config_with_framerate(60, "avatar"),
    );
    let outs = drive_for_seconds(&node, 0.2).await;
    let video: Vec<_> = outs
        .iter()
        .filter_map(|o| match o {
            RuntimeData::Video { width, height, pixel_data, .. } => {
                Some((*width, *height, pixel_data.len()))
            }
            _ => None,
        })
        .collect();
    assert!(!video.is_empty());
    for (w, h, bytes) in video {
        assert_eq!((w, h), (640, 480));
        assert_eq!(bytes, 640 * 480 * 3);
    }
}
