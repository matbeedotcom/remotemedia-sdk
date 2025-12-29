//! Build script for pipeline-embed
//!
//! Reads PIPELINE_YAML environment variable (path to YAML file) and embeds it
//! into the binary at compile time. Also extracts CLI defaults from the
//! pipeline metadata or environment variables.
//!
//! # Usage
//!
//! ```bash
//! # Basic: just embed a pipeline
//! PIPELINE_YAML=path/to/pipeline.yaml cargo build --release
//!
//! # With build-time defaults (override CLI defaults)
//! PIPELINE_YAML=path/to/pipeline.yaml \
//! PIPELINE_STREAM=true \
//! PIPELINE_MIC=true \
//! PIPELINE_SAMPLE_RATE=16000 \
//! cargo build --release
//! ```
//!
//! # Pipeline Metadata Defaults
//!
//! Defaults can also be specified in the pipeline YAML metadata:
//!
//! ```yaml
//! version: v1
//! metadata:
//!   name: my-pipeline
//!   cli_defaults:
//!     stream: true
//!     mic: true
//!     speaker: true
//!     sample_rate: 16000
//!     channels: 1
//!     chunk_size: 4000
//!     timeout_secs: 300
//! ```
//!
//! Environment variables take precedence over metadata defaults.

use std::env;
use std::fs;
use std::path::Path;

/// CLI defaults that can be configured at build time
#[derive(Debug, Default)]
struct CliDefaults {
    /// Enable streaming mode by default
    stream: Option<bool>,
    /// Enable microphone input by default
    mic: Option<bool>,
    /// Enable speaker output by default
    speaker: Option<bool>,
    /// Default sample rate in Hz
    sample_rate: Option<u32>,
    /// Default number of channels
    channels: Option<u16>,
    /// Default chunk size in samples
    chunk_size: Option<usize>,
    /// Default timeout in seconds
    timeout_secs: Option<u64>,
    /// Default input device
    input_device: Option<String>,
    /// Default output device
    output_device: Option<String>,
    /// Default audio host/backend
    audio_host: Option<String>,
    /// Default buffer size in ms
    buffer_ms: Option<u32>,
}

impl CliDefaults {
    /// Load defaults from environment variables (ignores empty strings)
    fn from_env() -> Self {
        Self {
            stream: env::var("PIPELINE_STREAM").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            mic: env::var("PIPELINE_MIC").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            speaker: env::var("PIPELINE_SPEAKER").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            sample_rate: env::var("PIPELINE_SAMPLE_RATE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            channels: env::var("PIPELINE_CHANNELS").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            chunk_size: env::var("PIPELINE_CHUNK_SIZE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            timeout_secs: env::var("PIPELINE_TIMEOUT").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
            input_device: env::var("PIPELINE_INPUT_DEVICE").ok().filter(|s| !s.is_empty()),
            output_device: env::var("PIPELINE_OUTPUT_DEVICE").ok().filter(|s| !s.is_empty()),
            audio_host: env::var("PIPELINE_AUDIO_HOST").ok().filter(|s| !s.is_empty()),
            buffer_ms: env::var("PIPELINE_BUFFER_MS").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse().ok()),
        }
    }

    /// Load defaults from pipeline metadata YAML
    fn from_yaml_metadata(yaml: &serde_yaml::Value) -> Self {
        let defaults = yaml
            .get("metadata")
            .and_then(|m| m.get("cli_defaults"));

        let Some(defaults) = defaults else {
            return Self::default();
        };

        Self {
            stream: defaults.get("stream").and_then(|v| v.as_bool()),
            mic: defaults.get("mic").and_then(|v| v.as_bool()),
            speaker: defaults.get("speaker").and_then(|v| v.as_bool()),
            sample_rate: defaults.get("sample_rate").and_then(|v| v.as_u64()).map(|v| v as u32),
            channels: defaults.get("channels").and_then(|v| v.as_u64()).map(|v| v as u16),
            chunk_size: defaults.get("chunk_size").and_then(|v| v.as_u64()).map(|v| v as usize),
            timeout_secs: defaults.get("timeout_secs").and_then(|v| v.as_u64()),
            input_device: defaults.get("input_device").and_then(|v| v.as_str()).map(String::from),
            output_device: defaults.get("output_device").and_then(|v| v.as_str()).map(String::from),
            audio_host: defaults.get("audio_host").and_then(|v| v.as_str()).map(String::from),
            buffer_ms: defaults.get("buffer_ms").and_then(|v| v.as_u64()).map(|v| v as u32),
        }
    }

    /// Merge with another CliDefaults, preferring self's values (env vars override yaml)
    fn merge_with(self, other: Self) -> Self {
        Self {
            stream: self.stream.or(other.stream),
            mic: self.mic.or(other.mic),
            speaker: self.speaker.or(other.speaker),
            sample_rate: self.sample_rate.or(other.sample_rate),
            channels: self.channels.or(other.channels),
            chunk_size: self.chunk_size.or(other.chunk_size),
            timeout_secs: self.timeout_secs.or(other.timeout_secs),
            input_device: self.input_device.or(other.input_device),
            output_device: self.output_device.or(other.output_device),
            audio_host: self.audio_host.or(other.audio_host),
            buffer_ms: self.buffer_ms.or(other.buffer_ms),
        }
    }

    /// Generate Rust code for the defaults struct
    fn generate_code(&self) -> String {
        let stream = self.stream.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let mic = self.mic.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let speaker = self.speaker.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let sample_rate = self.sample_rate.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let channels = self.channels.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let chunk_size = self.chunk_size.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let timeout_secs = self.timeout_secs.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());
        let input_device = self.input_device.as_ref().map(|v| format!("Some(\"{}\")", v)).unwrap_or_else(|| "None".to_string());
        let output_device = self.output_device.as_ref().map(|v| format!("Some(\"{}\")", v)).unwrap_or_else(|| "None".to_string());
        let audio_host = self.audio_host.as_ref().map(|v| format!("Some(\"{}\")", v)).unwrap_or_else(|| "None".to_string());
        let buffer_ms = self.buffer_ms.map(|v| format!("Some({})", v)).unwrap_or_else(|| "None".to_string());

        format!(
            r#"/// CLI defaults configured at build time
pub struct PipelineDefaults;

impl PipelineDefaults {{
    /// Whether streaming mode is enabled by default
    pub const STREAM: Option<bool> = {stream};
    /// Whether microphone input is enabled by default
    pub const MIC: Option<bool> = {mic};
    /// Whether speaker output is enabled by default
    pub const SPEAKER: Option<bool> = {speaker};
    /// Default sample rate in Hz
    pub const SAMPLE_RATE: Option<u32> = {sample_rate};
    /// Default number of channels
    pub const CHANNELS: Option<u16> = {channels};
    /// Default chunk size in samples
    pub const CHUNK_SIZE: Option<usize> = {chunk_size};
    /// Default timeout in seconds
    pub const TIMEOUT_SECS: Option<u64> = {timeout_secs};
    /// Default input device
    pub const INPUT_DEVICE: Option<&'static str> = {input_device};
    /// Default output device
    pub const OUTPUT_DEVICE: Option<&'static str> = {output_device};
    /// Default audio host/backend
    pub const AUDIO_HOST: Option<&'static str> = {audio_host};
    /// Default buffer size in milliseconds
    pub const BUFFER_MS: Option<u32> = {buffer_ms};
}}
"#
        )
    }
}

/// Sanitize a pipeline name for use as a binary filename
fn sanitize_binary_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(|c| c == '-' || c == '_')
        .to_string()
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_pipeline.rs");

    // Get the YAML path from environment
    let yaml_path = env::var("PIPELINE_YAML").unwrap_or_default();
    
    // Allow overriding the binary name (ignore empty strings)
    let binary_name_override = env::var("PIPELINE_BIN_NAME")
        .ok()
        .filter(|s| !s.is_empty());

    // Load environment variable defaults first
    let env_defaults = CliDefaults::from_env();

    let (yaml_content, source_comment, yaml_defaults, pipeline_name) = if yaml_path.is_empty() {
        // Default to a sample pipeline for development
        eprintln!("cargo:warning=PIPELINE_YAML not set, using default empty pipeline");
        eprintln!("cargo:warning=Set PIPELINE_YAML=/path/to/pipeline.yaml to embed a pipeline");
        
        let default = concat!(
            "version: v1\n",
            "metadata:\n",
            "  name: empty\n",
            "  description: Empty pipeline - set PIPELINE_YAML to embed a real pipeline\n",
            "nodes: []\n",
            "connections: []\n"
        );
        (default.to_string(), "(default empty)".to_string(), CliDefaults::default(), "pipeline-runner".to_string())
    } else {
        // Rerun if the YAML file changes
        println!("cargo:rerun-if-changed={}", yaml_path);
        
        let content = fs::read_to_string(&yaml_path)
            .unwrap_or_else(|e| panic!("Failed to read PIPELINE_YAML '{}': {}", yaml_path, e));
        
        // Parse YAML to extract metadata
        let (yaml_defaults, name) = if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
            let defaults = CliDefaults::from_yaml_metadata(&yaml);
            let name = yaml.get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| sanitize_binary_name(s))
                .unwrap_or_else(|| "pipeline-runner".to_string());
            (defaults, name)
        } else {
            (CliDefaults::default(), "pipeline-runner".to_string())
        };
        
        (content, yaml_path.clone(), yaml_defaults, name)
    };

    // Use override if provided, otherwise use extracted name
    let final_binary_name = binary_name_override.unwrap_or(pipeline_name);

    // Merge defaults: env vars take precedence over yaml metadata
    let merged_defaults = env_defaults.merge_with(yaml_defaults);

    // Extract pipeline info for logging and code generation
    let (display_name, description) = if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&yaml_content) {
        let name = yaml.get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unnamed");
        let desc = yaml.get("metadata")
            .and_then(|m| m.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("");
        
        eprintln!("cargo:warning=Embedding pipeline: {}", name);
        if !desc.is_empty() {
            eprintln!("cargo:warning=Description: {}", desc);
        }
        eprintln!("cargo:warning=Binary name: {}", final_binary_name);
        
        // Log any configured defaults
        if merged_defaults.stream == Some(true) {
            eprintln!("cargo:warning=Default mode: streaming");
        }
        if merged_defaults.mic == Some(true) {
            eprintln!("cargo:warning=Default input: microphone");
        }
        if merged_defaults.speaker == Some(true) {
            eprintln!("cargo:warning=Default output: speaker");
        }
        if let Some(sr) = merged_defaults.sample_rate {
            eprintln!("cargo:warning=Default sample rate: {}Hz", sr);
        }
        
        (name.to_string(), desc.to_string())
    } else {
        ("unnamed".to_string(), String::new())
    };

    // Write binary name to a file for Makefile to read
    // Use OUT_DIR which is guaranteed to be writable, then also try manifest dir
    let bin_name_path = Path::new(&out_dir).join("pipeline-bin-name");
    fs::write(&bin_name_path, &final_binary_name).expect("Failed to write binary name file");
    
    // Also write to a predictable location relative to the crate
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let alt_bin_name_path = Path::new(&manifest_dir).join("target").join("pipeline-bin-name");
    if let Some(parent) = alt_bin_name_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&alt_bin_name_path, &final_binary_name);
    
    // Print the binary name so the user can see it in build output
    println!("cargo:warning=Embedding pipeline '{}' -> binary '{}'", display_name, final_binary_name);

    // Escape the YAML content for inclusion in a raw string
    // Use enough # to avoid conflicts
    let hashes = "####";
    
    let generated = format!(
        r#"/// Pipeline YAML embedded at compile time
/// Source: {source}
pub const PIPELINE_YAML: &str = r{h}"{content}"{h};

/// Pipeline display name from metadata
pub const PIPELINE_NAME: &str = "{display_name}";

/// Pipeline description from metadata
pub const PIPELINE_DESCRIPTION: &str = "{description}";

/// Binary name (sanitized from pipeline name or overridden)
pub const BINARY_NAME: &str = "{binary_name}";

{defaults}"#,
        source = source_comment,
        content = yaml_content,
        h = hashes,
        display_name = display_name,
        description = description.replace('"', "\\\""),
        binary_name = final_binary_name,
        defaults = merged_defaults.generate_code()
    );

    fs::write(&dest_path, generated).unwrap();

    // Rerun if build script changes or relevant env vars change
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PIPELINE_YAML");
    println!("cargo:rerun-if-env-changed=PIPELINE_BIN_NAME");
    println!("cargo:rerun-if-env-changed=PIPELINE_STREAM");
    println!("cargo:rerun-if-env-changed=PIPELINE_MIC");
    println!("cargo:rerun-if-env-changed=PIPELINE_SPEAKER");
    println!("cargo:rerun-if-env-changed=PIPELINE_SAMPLE_RATE");
    println!("cargo:rerun-if-env-changed=PIPELINE_CHANNELS");
    println!("cargo:rerun-if-env-changed=PIPELINE_CHUNK_SIZE");
    println!("cargo:rerun-if-env-changed=PIPELINE_TIMEOUT");
    println!("cargo:rerun-if-env-changed=PIPELINE_INPUT_DEVICE");
    println!("cargo:rerun-if-env-changed=PIPELINE_OUTPUT_DEVICE");
    println!("cargo:rerun-if-env-changed=PIPELINE_AUDIO_HOST");
    println!("cargo:rerun-if-env-changed=PIPELINE_BUFFER_MS");
}
