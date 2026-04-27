//! Integration test: SessionRouter + SessionControl end-to-end.
//!
//! Exercises the full path:
//!   client input -> router -> node -> on_node_output hook -> tap subscribers
//!                                                          -> intercept
//! Plus `publish` injection and close-signal propagation on shutdown.

use std::sync::Arc;
use std::time::Duration;

use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use remotemedia_core::transport::session_control::{
    CloseReason, ControlAddress, InterceptDecision, NodeState, SessionControl,
};
use remotemedia_core::transport::session_router::{
    DataPacket, SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
};
use tokio::sync::mpsc;

fn calc_pipeline() -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "ctrl-integration".to_string(),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "calc".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: serde_json::json!({}),
            ..Default::default()
        }],
        connections: Vec::<Connection>::new(),
        python_env: None,
    }
}

fn add_packet(session_id: &str, a: f64, b: f64, seq: u64) -> DataPacket {
    let input = serde_json::json!({
        "operation": "add",
        "operands": [a, b],
    });
    DataPacket {
        data: RuntimeData::Json(input),
        from_node: "client".to_string(),
        to_node: None,
        session_id: session_id.to_string(),
        sequence: seq,
        sub_sequence: 0,
    }
}

fn extract_result(data: &RuntimeData) -> f64 {
    match data {
        RuntimeData::Json(v) => v
            .get("result")
            .and_then(|r| r.as_f64())
            .expect("result field"),
        RuntimeData::Text(s) => serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .and_then(|v| v.get("result").and_then(|r| r.as_f64()))
            .expect("result in text json"),
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[tokio::test]
async fn tap_observes_node_output_end_to_end() {
    let session_id = "tap-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    // Subscribe BEFORE starting so we don't miss the first output.
    let mut tap = ctrl.subscribe(&ControlAddress::node_out("calc")).unwrap();

    let input_tx = router.get_input_sender();
    let handle = router.start();

    input_tx
        .send(add_packet(&session_id, 2.0, 5.0, 0))
        .await
        .unwrap();

    // Sink output should arrive at the client channel.
    let client_out = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("client output timeout")
        .expect("client channel closed");
    assert_eq!(extract_result(&client_out), 7.0);

    // The tap must have seen the same output via on_node_output.
    let tapped = tokio::time::timeout(Duration::from_secs(1), tap.recv())
        .await
        .expect("tap timeout")
        .expect("tap closed");
    assert_eq!(extract_result(&tapped), 7.0);

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn intercept_replaces_downstream_value() {
    let session_id = "intercept-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let addr = ControlAddress::node_out("calc");
    let mut intercept_rx = ctrl
        .intercept(&addr, Some(Duration::from_millis(500)))
        .unwrap();

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Client-side handler: listen for the intercept request, replace the
    // calc output with a hard-coded value.
    let ctrl_handler = ctrl.clone();
    let handler = tokio::spawn(async move {
        use remotemedia_core::transport::session_control::ControlEvent;
        while let Some(event) = intercept_rx.recv().await {
            if let ControlEvent::InterceptRequest {
                correlation_id, ..
            } = event
            {
                ctrl_handler.complete_intercept(
                    correlation_id,
                    InterceptDecision::Replace(RuntimeData::Json(serde_json::json!({
                        "result": 999.0,
                        "operation": "replaced",
                    }))),
                );
            }
        }
    });

    input_tx
        .send(add_packet(&session_id, 1.0, 1.0, 0))
        .await
        .unwrap();

    // Client should see the *replaced* value, not 2.0.
    let client_out = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("client output timeout")
        .expect("client channel closed");
    assert_eq!(extract_result(&client_out), 999.0);

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    handler.abort();
}

#[tokio::test]
async fn publish_injects_input_into_pipeline() {
    let session_id = "publish-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let _input_tx = router.get_input_sender(); // kept alive to prevent shutdown
    let handle = router.start();

    // NOTE: `publish` needs to route to the "calc" node specifically because
    // it's the source in this single-node pipeline (no upstream edges).
    ctrl.publish(
        &ControlAddress::node_in("calc"),
        RuntimeData::Json(serde_json::json!({
            "operation": "multiply",
            "operands": [6.0, 7.0],
        })),
    )
    .await
    .unwrap();

    let client_out = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("client output timeout")
        .expect("client channel closed");
    assert_eq!(extract_result(&client_out), 42.0);

    drop(_input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

/// Extract the operands from a bypassed/calc input (when bypass forwards
/// input JSON unchanged to the client).
fn extract_operands(data: &RuntimeData) -> Vec<f64> {
    match data {
        RuntimeData::Json(v) => v
            .get("operands")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|n| n.as_f64()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[tokio::test]
async fn node_bypass_forwards_inputs_to_sink() {
    let session_id = "bypass-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    // Bypass calc: the input dict should come through unchanged.
    ctrl.set_node_state("calc", NodeState::Bypass);

    let input_tx = router.get_input_sender();
    let handle = router.start();

    input_tx
        .send(add_packet(&session_id, 10.0, 32.0, 0))
        .await
        .unwrap();

    let client_out = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("client output timeout")
        .expect("client channel closed");

    // Bypass means we see the raw input dict, not a calc result.
    assert_eq!(extract_operands(&client_out), vec![10.0, 32.0]);
    match &client_out {
        RuntimeData::Json(v) => {
            assert!(v.get("result").is_none(), "bypass should skip calc execution");
        }
        _ => panic!("expected Json"),
    }

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn node_disabled_drops_output() {
    let session_id = "disabled-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;
    ctrl.set_node_state("calc", NodeState::Disabled);

    let input_tx = router.get_input_sender();
    let handle = router.start();

    input_tx
        .send(add_packet(&session_id, 1.0, 1.0, 0))
        .await
        .unwrap();

    // Give the router time to process (or drop) the packet.
    let maybe_out =
        tokio::time::timeout(Duration::from_millis(500), output_rx.recv()).await;
    assert!(
        maybe_out.is_err(),
        "disabled node must not produce output, got {:?}",
        maybe_out
    );

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn node_state_toggles_at_runtime() {
    let session_id = "toggle-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // 1. Start Enabled (default) — expect calculated result.
    input_tx
        .send(add_packet(&session_id, 3.0, 4.0, 0))
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("first output timeout")
        .expect("first channel closed");
    assert_eq!(extract_result(&first), 7.0);

    // 2. Flip to Bypass — expect raw input.
    ctrl.set_node_state("calc", NodeState::Bypass);
    input_tx
        .send(add_packet(&session_id, 9.0, 10.0, 1))
        .await
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("second output timeout")
        .expect("second channel closed");
    assert_eq!(extract_operands(&second), vec![9.0, 10.0]);

    // 3. Flip to Disabled — expect no output.
    ctrl.set_node_state("calc", NodeState::Disabled);
    input_tx
        .send(add_packet(&session_id, 99.0, 99.0, 2))
        .await
        .unwrap();
    let gap = tokio::time::timeout(Duration::from_millis(400), output_rx.recv()).await;
    assert!(gap.is_err(), "disabled must produce nothing");

    // 4. Clear state (back to Enabled) — expect calculated result.
    ctrl.clear_node_state("calc");
    input_tx
        .send(add_packet(&session_id, 100.0, 1.0, 3))
        .await
        .unwrap();
    let fourth = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("fourth output timeout")
        .expect("fourth channel closed");
    assert_eq!(extract_result(&fourth), 101.0);

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn bypass_still_fans_out_to_taps() {
    let session_id = "bypass-tap-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, _shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let mut tap = ctrl.subscribe(&ControlAddress::node_out("calc")).unwrap();
    ctrl.set_node_state("calc", NodeState::Bypass);

    let input_tx = router.get_input_sender();
    let handle = router.start();

    input_tx
        .send(add_packet(&session_id, 2.0, 2.0, 0))
        .await
        .unwrap();

    let client_out = tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
        .await
        .expect("client output timeout")
        .expect("client channel closed");
    assert_eq!(extract_operands(&client_out), vec![2.0, 2.0]);

    let tapped = tokio::time::timeout(Duration::from_secs(1), tap.recv())
        .await
        .expect("tap timeout")
        .expect("tap closed");
    assert_eq!(extract_operands(&tapped), vec![2.0, 2.0]);

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn close_signal_fires_on_router_shutdown() {
    let session_id = "close-e2e".to_string();
    let manifest = Arc::new(calc_pipeline());
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (mut router, shutdown_tx) =
        SessionRouter::new(session_id.clone(), manifest, registry, output_tx).unwrap();

    let ctrl = SessionControl::new(session_id.clone());
    router.attach_control(ctrl.clone()).await;

    let mut close_rx = ctrl.close_subscriber();

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Trigger shutdown. `drop(input_tx)` alone is not sufficient: the bus's
    // SessionControl also clones the input sender internally via
    // `attach_input_sender`. Using the explicit shutdown_tx is the
    // deterministic path.
    drop(input_tx);
    shutdown_tx.send(()).await.unwrap();

    let reason = tokio::time::timeout(Duration::from_secs(10), close_rx.recv())
        .await
        .expect("close-signal timeout")
        .expect("close channel lagged");
    assert!(matches!(reason, CloseReason::Normal));

    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}
