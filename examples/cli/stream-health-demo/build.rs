//! Build script for stream-health-demo
//!
//! Embeds the stream-health.yaml pipeline at compile time, similar to pipeline-embed.
//! This creates a demo binary with the health monitoring pipeline baked in.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_pipeline.rs");

    // Read the pipeline YAML from the pipelines directory
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let yaml_path = Path::new(&manifest_dir).join("pipelines/stream-health.yaml");
    
    let yaml_content = if yaml_path.exists() {
        println!("cargo:rerun-if-changed={}", yaml_path.display());
        fs::read_to_string(&yaml_path)
            .unwrap_or_else(|e| panic!("Failed to read pipeline YAML '{}': {}", yaml_path.display(), e))
    } else {
        // Default pipeline if file doesn't exist yet
        eprintln!("cargo:warning=Pipeline file not found at {}, using default", yaml_path.display());
        r#"version: v1
metadata:
  name: stream-health-monitor
  description: Real-time drift, freeze, and health monitoring
  cli_defaults:
    stream: true
    sample_rate: 16000
    channels: 1

nodes:
  - id: health
    node_type: HealthEmitterNode
    params:
      lead_threshold_ms: 50
      freeze_threshold_ms: 500
      health_emit_interval_ms: 1000

connections: []
"#.to_string()
    };

    // Extract metadata
    let (pipeline_name, pipeline_description) = if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&yaml_content) {
        let name = yaml.get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("stream-health-monitor");
        let desc = yaml.get("metadata")
            .and_then(|m| m.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("Stream health monitoring demo");
        
        eprintln!("cargo:warning=Embedding pipeline: {}", name);
        (name.to_string(), desc.to_string())
    } else {
        ("stream-health-monitor".to_string(), "Stream health monitoring demo".to_string())
    };

    // Generate the embedded pipeline module
    let hashes = "####";
    let generated = format!(
        r#"/// Pipeline YAML embedded at compile time
pub const PIPELINE_YAML: &str = r{h}"{content}"{h};

/// Pipeline display name from metadata
pub const PIPELINE_NAME: &str = "{name}";

/// Pipeline description from metadata  
pub const PIPELINE_DESCRIPTION: &str = "{description}";

/// Binary name
pub const BINARY_NAME: &str = "remotemedia-demo";

/// Demo mode limits
pub struct DemoConfig;

impl DemoConfig {{
    /// Maximum session duration in seconds (15 minutes)
    pub const SESSION_DURATION_SECS: u64 = 900;
    
    /// Maximum sessions per day
    pub const MAX_SESSIONS_PER_DAY: u32 = 3;
    
    /// Warning time before session end (1 minute)
    pub const WARNING_SECS: u64 = 60;
}}
"#,
        content = yaml_content,
        h = hashes,
        name = pipeline_name,
        description = pipeline_description.replace('"', "\\\""),
    );

    fs::write(&dest_path, generated).unwrap();

    // Rerun if build script changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=pipelines/stream-health.yaml");
}
