//! Remote mode pipeline configuration

/// Get the pipeline manifest for remote mode
pub fn get_pipeline_manifest(server_url: &str) -> String {
    format!(
        r#"
version: "v1"
metadata:
  name: voice-assistant-remote
  description: Voice assistant with all processing on remote server

nodes:
  - id: mic_input
    node_type: AudioInput
    config:
      sample_rate: 16000
      channels: 1
      format: f32
      source: microphone

  # All processing happens remotely
  - id: remote_pipeline
    node_type: RemotePipelineNode
    config:
      server_url: "{server_url}"
      pipeline: voice-assistant
      transport: grpc
      timeout_ms: 30000

  - id: speaker_output
    node_type: AudioOutput
    config:
      sample_rate: 48000
      channels: 1
      format: f32
      destination: speaker

connections:
  - from: mic_input
    to: remote_pipeline
  - from: remote_pipeline
    to: speaker_output
"#,
        server_url = server_url
    )
}
