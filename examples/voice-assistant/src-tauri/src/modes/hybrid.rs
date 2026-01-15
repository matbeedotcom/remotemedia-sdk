//! Hybrid mode pipeline configuration with local fallback

/// Get the pipeline manifest for hybrid mode
pub fn get_pipeline_manifest(remote_url: &str) -> String {
    format!(
        r#"
version: "v1"
metadata:
  name: voice-assistant-hybrid
  description: Voice assistant with local VAD and remote inference (fallback to local)

nodes:
  - id: mic_input
    node_type: AudioInput
    config:
      sample_rate: 16000
      channels: 1
      format: f32
      source: microphone

  # Local VAD for responsive speech detection
  - id: vad
    node_type: SileroVAD
    config:
      threshold: 0.5
      min_speech_duration_ms: 250
      min_silence_duration_ms: 500
      pre_speech_pad_ms: 300

  # Remote inference with automatic local fallback
  - id: remote_inference
    node_type: RemotePipelineNode
    config:
      server_url: "{remote_url}"
      pipeline: inference
      timeout_ms: 5000
      fallback_enabled: true
      fallback_pipeline: |
        nodes:
          - id: whisper
            node_type: WhisperSTT
            executor: multiprocess
            config:
              model: base.en
              language: en
          - id: llm
            node_type: OllamaLLM
            executor: multiprocess
            config:
              model: llama3.2:1b
              system_prompt: |
                You are a helpful voice assistant. Keep responses concise.
              max_tokens: 150
        connections:
          - from: whisper
            to: llm

  - id: tts
    node_type: KokoroTTS
    executor: multiprocess
    config:
      voice: af_bella
      speed: 1.0
      sample_rate: 24000

  - id: resample
    node_type: AudioResample
    config:
      target_sample_rate: 48000
      quality: high

  - id: speaker_output
    node_type: AudioOutput
    config:
      sample_rate: 48000
      channels: 1
      format: f32
      destination: speaker

connections:
  - from: mic_input
    to: vad
  - from: vad
    to: remote_inference
  - from: remote_inference
    to: tts
  - from: tts
    to: resample
  - from: resample
    to: speaker_output
"#,
        remote_url = remote_url
    )
}
