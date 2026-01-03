//! Build script for stream-health-demo
//!
//! Embeds a pipeline YAML at compile time. The pipeline can be specified via:
//!
//! 1. `REMOTEMEDIA_PIPELINE` env var - path to any YAML file, OR a pipeline name
//!    from the shared pipelines directory (e.g., "demo_audio_quality_v1")
//! 2. Default: `pipelines/stream-health.yaml` (local to this crate)
//!
//! ## Pipeline Selection
//!
//! ```bash
//! # Use a specific pipeline by name (from libs/pipeline-runner/pipelines/)
//! REMOTEMEDIA_PIPELINE=demo_audio_quality_v1 cargo build
//!
//! # Use a specific pipeline by path
//! REMOTEMEDIA_PIPELINE=/path/to/custom.yaml cargo build
//!
//! # Use the default stream-health.yaml
//! cargo build
//! ```
//!
//! ## Embedded License
//!
//! To embed a license at build time, set these environment variables:
//!
//! ```bash
//! REMOTEMEDIA_LICENSE=/path/to/license.json \
//! REMOTEMEDIA_PUBLIC_KEY=/path/to/public.key \
//!   cargo build --release
//! ```
//!
//! The embedded license will be checked before any filesystem-based license.
//! The public key is required to verify the embedded license signature.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::env;
use std::fs;
use std::path::Path;

/// Default pipeline if no file is found
const DEFAULT_PIPELINE: &str = r#"version: v1
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
"#;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_pipeline.rs");
    let license_dest = Path::new(&out_dir).join("embedded_license.rs");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    
    // Determine which pipeline to embed
    // Priority: REMOTEMEDIA_PIPELINE env var > local pipelines/stream-health.yaml
    let (yaml_content, yaml_source) = if let Ok(pipeline_spec) = env::var("REMOTEMEDIA_PIPELINE") {
        let pipeline_path = Path::new(&pipeline_spec);
        
        if pipeline_path.exists() {
            // It's a direct path to a YAML file
            println!("cargo:rerun-if-changed={}", pipeline_spec);
            let content = fs::read_to_string(pipeline_path)
                .unwrap_or_else(|e| panic!("Failed to read pipeline '{}': {}", pipeline_spec, e));
            (content, pipeline_spec)
        } else {
            // Try to find it in the shared pipelines directory
            let shared_pipelines = Path::new(&manifest_dir)
                .join("../../../libs/pipeline-runner/pipelines");
            
            // Try with and without .yaml extension
            let names_to_try = vec![
                format!("{}.yaml", pipeline_spec),
                pipeline_spec.clone(),
            ];
            
            let mut found = None;
            for name in names_to_try {
                let path = shared_pipelines.join(&name);
                if path.exists() {
                    println!("cargo:rerun-if-changed={}", path.display());
                    let content = fs::read_to_string(&path)
                        .unwrap_or_else(|e| panic!("Failed to read pipeline '{}': {}", path.display(), e));
                    found = Some((content, format!("shared:{}", name)));
                    break;
                }
            }
            
            if let Some(result) = found {
                result
            } else {
                panic!("Pipeline '{}' not found. Available pipelines in shared directory:\n  - demo_audio_quality_v1\n  - demo_av_quality_v1\n  - demo_video_integrity_v1\n  - full_stream_health_v1\n  - contact_center_qa_v1\n  - speaker_diarization_v1\n  - technical_stream_analysis_v1",
                    pipeline_spec);
            }
        }
    } else {
        // Default: local stream-health.yaml
        let yaml_path = Path::new(&manifest_dir).join("pipelines/stream-health.yaml");
        
        if yaml_path.exists() {
            println!("cargo:rerun-if-changed={}", yaml_path.display());
            let content = fs::read_to_string(&yaml_path)
                .unwrap_or_else(|e| panic!("Failed to read pipeline '{}': {}", yaml_path.display(), e));
            (content, yaml_path.display().to_string())
        } else {
            // Default pipeline if file doesn't exist yet
            eprintln!("cargo:warning=Pipeline file not found at {}, using default", yaml_path.display());
            (DEFAULT_PIPELINE.to_string(), "builtin-default".to_string())
        }
    };

    eprintln!("cargo:warning=Embedding pipeline from: {}", yaml_source);

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

    // Handle embedded license
    let license_content = if let Ok(license_path) = env::var("REMOTEMEDIA_LICENSE") {
        let path = Path::new(&license_path);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", license_path);
            match fs::read_to_string(path) {
                Ok(content) => {
                    // Validate it's valid JSON
                    if serde_json::from_str::<serde_json::Value>(&content).is_ok() {
                        eprintln!("cargo:warning=Embedding license from: {}", license_path);
                        Some(content)
                    } else {
                        eprintln!("cargo:warning=Invalid JSON in license file: {}", license_path);
                        None
                    }
                }
                Err(e) => {
                    eprintln!("cargo:warning=Failed to read license file {}: {}", license_path, e);
                    None
                }
            }
        } else {
            eprintln!("cargo:warning=License file not found: {}", license_path);
            None
        }
    } else {
        None
    };

    // Handle embedded public key (stored as base64)
    let public_key_bytes = if let Ok(key_path) = env::var("REMOTEMEDIA_PUBLIC_KEY") {
        let path = Path::new(&key_path);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", key_path);
            match fs::read_to_string(path) {
                Ok(content) => {
                    // Decode from base64
                    match BASE64.decode(content.trim()) {
                        Ok(bytes) => {
                            if bytes.len() == 32 {
                                eprintln!("cargo:warning=Embedding public key from: {}", key_path);
                                Some(bytes)
                            } else {
                                eprintln!("cargo:warning=Invalid public key length: expected 32 bytes, got {}", bytes.len());
                                None
                            }
                        }
                        Err(e) => {
                            eprintln!("cargo:warning=Failed to decode public key base64: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    eprintln!("cargo:warning=Failed to read public key file {}: {}", key_path, e);
                    None
                }
            }
        } else {
            eprintln!("cargo:warning=Public key file not found: {}", key_path);
            None
        }
    } else {
        None
    };

    // Generate the embedded license module with optional public key
    let mut license_generated = String::new();

    // Extract license expiry date for CLI about text
    let license_expiry = license_content.as_ref().and_then(|content| {
        serde_json::from_str::<serde_json::Value>(content)
            .ok()
            .and_then(|v| v.get("expires_at").and_then(|e| e.as_str()).map(|s| {
                // Extract just the date part (YYYY-MM-DD) from ISO 8601
                s.split('T').next().unwrap_or(s).to_string()
            }))
    });

    // License JSON constant
    if let Some(content) = license_content {
        let escaped = content.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        license_generated.push_str(&format!(
            r#"/// Embedded license JSON (set at build time via REMOTEMEDIA_LICENSE env var)
pub const EMBEDDED_LICENSE_JSON: Option<&str> = Some("{}");
"#,
            escaped
        ));
    } else {
        license_generated.push_str(
            r#"/// No embedded license (build without REMOTEMEDIA_LICENSE env var)
pub const EMBEDDED_LICENSE_JSON: Option<&str> = None;
"#
        );
    }

    // License expiry constant for CLI about text
    if let Some(expiry) = license_expiry {
        license_generated.push_str(&format!(
            r#"
/// Embedded license expiry date (extracted at build time)
pub const EMBEDDED_LICENSE_EXPIRY: Option<&str> = Some("{}");
"#,
            expiry
        ));
    } else {
        license_generated.push_str(
            r#"
/// No embedded license expiry
pub const EMBEDDED_LICENSE_EXPIRY: Option<&str> = None;
"#
        );
    }

    license_generated.push('\n');

    // Public key constant
    if let Some(bytes) = public_key_bytes {
        let hex_bytes: Vec<String> = bytes.iter().map(|b| format!("0x{:02x}", b)).collect();
        license_generated.push_str(&format!(
            r#"/// Embedded Ed25519 public key for license verification (set at build time via REMOTEMEDIA_PUBLIC_KEY env var)
pub const EMBEDDED_PUBLIC_KEY: Option<[u8; 32]> = Some([
    {}, {}, {}, {},
    {}, {}, {}, {},
    {}, {}, {}, {},
    {}, {}, {}, {},
    {}, {}, {}, {},
    {}, {}, {}, {},
    {}, {}, {}, {},
    {}, {}, {}, {},
]);
"#,
            hex_bytes[0], hex_bytes[1], hex_bytes[2], hex_bytes[3],
            hex_bytes[4], hex_bytes[5], hex_bytes[6], hex_bytes[7],
            hex_bytes[8], hex_bytes[9], hex_bytes[10], hex_bytes[11],
            hex_bytes[12], hex_bytes[13], hex_bytes[14], hex_bytes[15],
            hex_bytes[16], hex_bytes[17], hex_bytes[18], hex_bytes[19],
            hex_bytes[20], hex_bytes[21], hex_bytes[22], hex_bytes[23],
            hex_bytes[24], hex_bytes[25], hex_bytes[26], hex_bytes[27],
            hex_bytes[28], hex_bytes[29], hex_bytes[30], hex_bytes[31],
        ));
    } else {
        license_generated.push_str(
            r#"/// No embedded public key (build without REMOTEMEDIA_PUBLIC_KEY env var)
pub const EMBEDDED_PUBLIC_KEY: Option<[u8; 32]> = None;
"#
        );
    }

    fs::write(&license_dest, license_generated).unwrap();

    // Rerun if build script changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=pipelines/stream-health.yaml");
    println!("cargo:rerun-if-env-changed=REMOTEMEDIA_PIPELINE");
    println!("cargo:rerun-if-env-changed=REMOTEMEDIA_LICENSE");
    println!("cargo:rerun-if-env-changed=REMOTEMEDIA_PUBLIC_KEY");
}
