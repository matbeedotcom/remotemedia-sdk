//! Analyze pipeline manifest and detect capabilities for each node
//!
//! Usage:
//!   cargo run --bin analyze_pipeline -- <pipeline.json>

use remotemedia_runtime::capabilities::{detect_node_capabilities, detect_pipeline_capabilities};
use remotemedia_runtime::manifest::Manifest;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <pipeline.json>", args[0]);
        std::process::exit(1);
    }

    let manifest_path = &args[1];
    
    // Read manifest
    let manifest_json = fs::read_to_string(manifest_path)
        .expect("Failed to read manifest file");
    
    let manifest: Manifest = serde_json::from_str(&manifest_json)
        .expect("Failed to parse manifest JSON");

    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║              Pipeline Capability Analysis                         ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Pipeline: {}", manifest.metadata.name);
    if let Some(desc) = &manifest.metadata.description {
        println!("Description: {}", desc);
    }
    println!("Nodes: {}", manifest.nodes.len());
    println!();

    // Analyze each node
    println!("┌────────────────────────────────────────────────────────────────────┐");
    println!("│                        Node Capabilities                           │");
    println!("└────────────────────────────────────────────────────────────────────┘");
    println!();

    for node in &manifest.nodes {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Node: {} ({})", node.id, node.node_type);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let caps = detect_node_capabilities(node);

        // Environment compatibility
        println!("\n  Environment Compatibility:");
        println!("    Browser:        {}", if caps.supports_browser { "✓ Yes" } else { "✗ No" });
        println!("    WASM:           {}", if caps.supports_wasm { "✓ Yes" } else { "✗ No" });
        
        // Requirements
        println!("\n  Requirements:");
        println!("    Threads:        {}", if caps.requires_threads { "⚠ Required" } else { "○ Not required" });
        println!("    Native libs:    {}", if caps.requires_native_libs { "⚠ Required" } else { "○ Not required" });
        println!("    GPU:            {}", if caps.requires_gpu { 
            format!("⚠ Required ({:?})", caps.gpu_type.as_ref().unwrap_or(&remotemedia_runtime::capabilities::GpuType::Any))
        } else { 
            "○ Not required".to_string() 
        });
        println!("    Large memory:   {}", if caps.requires_large_memory { "⚠ Required" } else { "○ Not required" });
        
        // Resources
        println!("\n  Resources:");
        println!("    Est. memory:    {} MB", caps.estimated_memory_mb);
        
        // Execution placement
        println!("\n  Execution:");
        println!("    Placement:      {:?}", caps.placement);
        if let Some(fallback) = &caps.fallback_node {
            println!("    Fallback:       {}", fallback);
        }
        
        // Parameters (if any interesting ones)
        if let Some(params_obj) = node.params.as_object() {
            let interesting_params: Vec<_> = params_obj.iter()
                .filter(|(k, _)| {
                    matches!(k.as_str(), "device" | "threads" | "n_threads" | "model" | "model_size" | "model_source")
                })
                .collect();
            
            if !interesting_params.is_empty() {
                println!("\n  Key Parameters:");
                for (key, value) in interesting_params {
                    println!("    {}: {}", key, value);
                }
            }
        }
        
        println!();
    }

    // Pipeline-level analysis
    println!("┌────────────────────────────────────────────────────────────────────┐");
    println!("│                   Pipeline-Level Capabilities                      │");
    println!("└────────────────────────────────────────────────────────────────────┘");
    println!();

    let pipeline_caps = detect_pipeline_capabilities(&manifest.nodes);

    println!("  Can run fully locally:  {}", if pipeline_caps.fully_local { "✓ Yes" } else { "✗ No" });
    println!("  Requires remote:        {}", if pipeline_caps.requires_remote { "⚠ Yes" } else { "○ No" });
    
    if !pipeline_caps.remote_nodes.is_empty() {
        println!("\n  Nodes requiring remote execution:");
        for node_id in &pipeline_caps.remote_nodes {
            println!("    • {}", node_id);
        }
    }
    
    println!("\n  Aggregate Requirements:");
    println!("    Max memory:     {} MB", pipeline_caps.max_memory_mb);
    println!("    Requires GPU:   {}", if pipeline_caps.requires_gpu { "⚠ Yes" } else { "○ No" });
    println!("    Requires threads: {}", if pipeline_caps.requires_threads { "⚠ Yes" } else { "○ No" });
    
    if !pipeline_caps.environment_requirements.is_empty() {
        println!("\n  Environment Requirements:");
        for (node_id, requirement) in &pipeline_caps.environment_requirements {
            println!("    {}: {}", node_id, requirement);
        }
    }

    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║                     Analysis Complete                              ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
}
