//! Local mode pipeline configuration

/// Get the pipeline manifest for local mode
/// 
/// This pipeline uses node types:
/// - SileroVADNode: Voice activity detection (from remotemedia-core)
/// - AudioBufferAccumulatorNode: Accumulates audio during speech
/// - candle-whisper: Speech-to-text via Candle ML (from remotemedia-candle-nodes)
/// 
/// Note: Mic input and speaker output are handled by the Tauri app directly (cpal)
/// since they require native audio device access.
pub fn get_pipeline_manifest() -> String {
    r#"
version: "v1"
metadata:
  name: voice-assistant-local
  description: Voice assistant running entirely locally

nodes:
  - id: resample_to_16k
    node_type: FastResampleNode
    params:
      target_rate: 16000
      quality: Medium

  - id: vad
    node_type: SileroVADNode
    params:
      threshold: 0.5
      min_speech_duration_ms: 250
      min_silence_duration_ms: 500
      pre_speech_pad_ms: 300

  - id: accumulator
    node_type: AudioBufferAccumulatorNode
    params:
      min_utterance_duration_ms: 500
      max_utterance_duration_ms: 30000

  - id: whisper
    node_type: candle-whisper
    params:
      model: tiny
      language: en
      device: auto

connections:
  - from: resample_to_16k
    to: vad
  - from: vad
    to: accumulator
  - from: accumulator
    to: whisper
"#
    .to_string()
}
