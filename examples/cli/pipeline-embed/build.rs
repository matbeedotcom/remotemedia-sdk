//! Build script for pipeline-embed
//!
//! Reads PIPELINE_YAML environment variable (path to YAML file) and embeds it
//! into the binary at compile time.
//!
//! Usage:
//!   PIPELINE_YAML=path/to/pipeline.yaml cargo build --release

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_pipeline.rs");

    // Get the YAML path from environment
    let yaml_path = env::var("PIPELINE_YAML").unwrap_or_default();

    let (yaml_content, source_comment) = if yaml_path.is_empty() {
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
        (default.to_string(), "(default empty)".to_string())
    } else {
        // Rerun if the YAML file changes
        println!("cargo:rerun-if-changed={}", yaml_path);
        
        let content = fs::read_to_string(&yaml_path)
            .unwrap_or_else(|e| panic!("Failed to read PIPELINE_YAML '{}': {}", yaml_path, e));
        
        (content, yaml_path.clone())
    };

    // Extract pipeline name from metadata for logging
    if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&yaml_content) {
        if let Some(name) = yaml.get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str()) 
        {
            eprintln!("cargo:warning=Embedding pipeline: {}", name);
        }
        if let Some(desc) = yaml.get("metadata")
            .and_then(|m| m.get("description"))
            .and_then(|d| d.as_str())
        {
            eprintln!("cargo:warning=Description: {}", desc);
        }
    }

    // Escape the YAML content for inclusion in a raw string
    // Use enough # to avoid conflicts
    let hashes = "####";
    
    let generated = format!(
        "/// Pipeline YAML embedded at compile time\n\
         /// Source: {source}\n\
         pub const PIPELINE_YAML: &str = r{h}\"{content}\"{h};\n",
        source = source_comment,
        content = yaml_content,
        h = hashes
    );

    fs::write(&dest_path, generated).unwrap();

    // Rerun if build script changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PIPELINE_YAML");
}
