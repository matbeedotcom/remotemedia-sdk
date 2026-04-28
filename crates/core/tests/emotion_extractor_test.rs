//! EmotionExtractorNode integration tests (spec 2026-04-27 §3.1, plan M0).
//!
//! Locks the spec invariants:
//! - per input Text frame, emit one Text (tags stripped) + N Json (one per tag, source order)
//! - input channel ("tts" / "ui" / …) round-trips on the Text output
//! - aliases substitute before emit; Json carries both `emoji` and `alias`
//! - construction with malformed pattern returns an error
//!
//! Note (deviation from plan §M0.1 first draft): `RuntimeData::Text` is a
//! tuple variant `Text(String)` with no `metadata` field, so `turn_id`
//! cannot be forwarded from a plain Text input. That assertion is deferred
//! until upstream emits Json envelopes carrying both text and turn_id, or
//! until Text gets a metadata field. Documented in the node's docs.

#![cfg(feature = "avatar-emotion")]

use remotemedia_core::data::text_channel::{split_text_str, tag_text_str};
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::emotion_extractor::EmotionExtractorNode;
use remotemedia_core::nodes::AsyncStreamingNode;

/// Drive the streaming node with one input frame, return all emitted outputs.
async fn drive_one(node: &EmotionExtractorNode, data: RuntimeData) -> Vec<RuntimeData> {
    let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let collected_clone = collected.clone();
    node.process_streaming(data, None, move |out| {
        collected_clone.lock().unwrap().push(out);
        Ok(())
    })
    .await
    .expect("process_streaming");
    let v = collected.lock().unwrap().clone();
    v
}

fn as_text(data: &RuntimeData) -> &str {
    match data {
        RuntimeData::Text(s) => s.as_str(),
        other => panic!("expected RuntimeData::Text, got {:?}", other),
    }
}

fn as_json(data: &RuntimeData) -> &serde_json::Value {
    match data {
        RuntimeData::Json(v) => v,
        other => panic!("expected RuntimeData::Json, got {:?}", other),
    }
}

#[tokio::test]
async fn extracts_tag_emits_text_minus_tag_plus_json() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = drive_one(
        &node,
        RuntimeData::Text(tag_text_str("Hi there [EMOTION:🤩] yes!", "tts")),
    )
    .await;

    assert_eq!(outputs.len(), 2, "expect Text + Json, got {}", outputs.len());

    let (channel, body) = split_text_str(as_text(&outputs[0]));
    assert_eq!(channel, "tts");
    assert_eq!(body, "Hi there  yes!");

    let json = as_json(&outputs[1]);
    assert_eq!(json["kind"], "emotion");
    assert_eq!(json["emoji"], "🤩");
    assert!(json.get("alias").is_none(), "no alias when emoji is direct");
    assert_eq!(
        json["source_offset_chars"].as_u64().expect("offset"),
        "Hi there ".chars().count() as u64
    );
    assert!(
        json["ts_ms"].as_u64().is_some(),
        "ts_ms must be a non-null u64"
    );
}

#[tokio::test]
async fn no_tag_emits_only_text_no_json() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = drive_one(
        &node,
        RuntimeData::Text(tag_text_str("plain text", "tts")),
    )
    .await;
    assert_eq!(outputs.len(), 1);
    assert!(matches!(&outputs[0], RuntimeData::Text(_)));
}

#[tokio::test]
async fn multiple_tags_emit_multiple_jsons_in_source_order() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = drive_one(
        &node,
        RuntimeData::Text(tag_text_str(
            "[EMOTION:😊] hello [EMOTION:🤩]!",
            "tts",
        )),
    )
    .await;
    assert_eq!(outputs.len(), 3, "Text + 2 Json frames");

    let (_, body) = split_text_str(as_text(&outputs[0]));
    assert_eq!(body, " hello !");

    let off0 = as_json(&outputs[1])["source_offset_chars"]
        .as_u64()
        .unwrap();
    let off1 = as_json(&outputs[2])["source_offset_chars"]
        .as_u64()
        .unwrap();
    assert!(
        off0 < off1,
        "Json frames must be in source order: {off0} < {off1}"
    );
    assert_eq!(as_json(&outputs[1])["emoji"], "😊");
    assert_eq!(as_json(&outputs[2])["emoji"], "🤩");
}

#[tokio::test]
async fn channel_is_preserved_on_text_output() {
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = drive_one(
        &node,
        RuntimeData::Text(tag_text_str("[EMOTION:😊] x", "ui")),
    )
    .await;
    let (channel, body) = split_text_str(as_text(&outputs[0]));
    assert_eq!(channel, "ui", "channel tag must round-trip");
    assert_eq!(body, " x");
}

#[tokio::test]
async fn alias_substitution_applied_before_emit() {
    let mut aliases = std::collections::HashMap::new();
    aliases.insert("happy".to_string(), "😊".to_string());
    let node = EmotionExtractorNode::with_default_pattern().with_aliases(aliases);

    let outputs = drive_one(
        &node,
        RuntimeData::Text(tag_text_str("[EMOTION:happy] hi", "tts")),
    )
    .await;

    let json = as_json(&outputs[1]);
    assert_eq!(json["emoji"], "😊", "alias resolved to canonical emoji");
    assert_eq!(json["alias"], "happy", "alias preserved for diagnostics");
}

#[tokio::test]
async fn malformed_regex_at_construction_returns_error() {
    let res = EmotionExtractorNode::with_pattern("[unbalanced");
    assert!(
        res.is_err(),
        "malformed pattern must return Err, got {:?}",
        res.is_ok()
    );
}

#[tokio::test]
async fn unknown_alias_emits_raw_match_with_no_alias_field() {
    // If the captured group isn't a known alias and isn't a recognized emoji,
    // emit it as the emoji string verbatim (no alias field). Spec §3.1 says
    // aliases are *applied before tag emission*; absent an alias, the raw
    // match becomes the emoji.
    let node = EmotionExtractorNode::with_default_pattern();
    let outputs = drive_one(
        &node,
        RuntimeData::Text(tag_text_str("[EMOTION:weird]", "tts")),
    )
    .await;
    let json = as_json(&outputs[1]);
    assert_eq!(json["emoji"], "weird");
    assert!(json.get("alias").is_none());
}

#[tokio::test]
async fn registry_resolves_emotion_extractor_factory() {
    use remotemedia_core::nodes::streaming_node::StreamingNodeRegistry;
    let mut registry = StreamingNodeRegistry::new();
    for provider in remotemedia_core::nodes::provider::iter_providers() {
        provider.register(&mut registry);
    }
    assert!(
        registry.has_node_type("EmotionExtractorNode"),
        "CoreNodesProvider must register EmotionExtractorNode under feature avatar-emotion"
    );
    assert!(
        registry.is_multi_output_streaming("EmotionExtractorNode"),
        "factory must declare is_multi_output_streaming = true"
    );

    // And the factory must instantiate cleanly with default params.
    let _node = registry
        .create_node(
            "EmotionExtractorNode",
            "n1".into(),
            &serde_json::json!({}),
            None,
        )
        .expect("default-config instantiation");
}

#[tokio::test]
async fn factory_rejects_malformed_pattern_with_actionable_error() {
    use remotemedia_core::nodes::streaming_node::StreamingNodeRegistry;
    let mut registry = StreamingNodeRegistry::new();
    for provider in remotemedia_core::nodes::provider::iter_providers() {
        provider.register(&mut registry);
    }
    let res = registry.create_node(
        "EmotionExtractorNode",
        "n1".into(),
        &serde_json::json!({"pattern": "[unbalanced"}),
        None,
    );
    // `Box<dyn StreamingNode>` isn't Debug, so we can't use expect_err.
    let err = match res {
        Ok(_) => panic!("expected factory to reject malformed pattern, got Ok"),
        Err(e) => e,
    };
    let msg = format!("{}", err);
    assert!(
        msg.contains("invalid EmotionExtractorNode pattern"),
        "error must mention the node + 'invalid pattern' so users can act on it; got: {msg}"
    );
}

#[tokio::test]
async fn non_text_input_passes_through_unchanged() {
    // The node only operates on Text frames; everything else must pass
    // through untouched (mirrors silero_vad's pass-through for non-audio).
    let node = EmotionExtractorNode::with_default_pattern();
    let json_in = serde_json::json!({"kind": "something_else", "n": 7});
    let outputs = drive_one(&node, RuntimeData::Json(json_in.clone())).await;
    assert_eq!(outputs.len(), 1);
    match &outputs[0] {
        RuntimeData::Json(v) => assert_eq!(v, &json_in),
        other => panic!("expected pass-through Json, got {:?}", other),
    }
}
