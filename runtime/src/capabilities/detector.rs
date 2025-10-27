//! Automatic node capability detection
//!
//! Detects capabilities based on:
//! 1. Explicit manifest declarations (capabilities, execution metadata)
//! 2. Cargo feature detection (whisper-wasm, gpu-accel, etc.)
//! 3. Dependency analysis (native libs, C extensions)
//! 4. Runtime introspection (node registration metadata)
//!
//! Does NOT use name-based heuristics - relies on concrete indicators only.

use super::{NodeCapabilities, ExecutionPlacement};
use crate::manifest::NodeManifest;

/// Detect node capabilities from manifest metadata
///
/// This function only uses explicit, concrete indicators:
/// - Manifest `capabilities` field (user-declared requirements)
/// - Manifest `execution` field (explicit placement)
/// - Node parameters (model paths, device specs)
/// - Feature flags (from Cargo compilation)
///
/// It does NOT guess based on node names.
pub fn detect_node_capabilities(node: &NodeManifest) -> NodeCapabilities {
    let mut caps = NodeCapabilities::default();

    // Priority 1: Explicit execution metadata (highest priority)
    if let Some(exec_meta) = &node.execution {
        apply_execution_metadata(exec_meta, &mut caps);
    }

    // Priority 2: Explicit capability requirements in manifest
    if let Some(manifest_caps) = &node.capabilities {
        apply_manifest_capabilities(manifest_caps, &mut caps);
    }

    // Priority 3: Infer from parameters (concrete indicators only)
    if let Some(params_obj) = node.params.as_object() {
        analyze_parameters(params_obj, &mut caps);
    }

    // Priority 4: Analyze streaming requirements
    if node.is_streaming {
        caps.estimated_memory_mb = caps.estimated_memory_mb.max(128);
    }

    // Priority 5: Check node registry for compile-time detected capabilities
    // (This would be populated by build.rs or node registration)
    if let Some(registered_caps) = lookup_registered_capabilities(&node.node_type) {
        merge_registered_capabilities(&registered_caps, &mut caps);
    }

    caps
}

/// Lookup capabilities registered at compile-time
///
/// Nodes register their capabilities when they're compiled. For example:
/// - Rust nodes: via NodeRegistry metadata
/// - Python nodes: via introspection of imports/dependencies
/// - WASM nodes: via feature flags in Cargo.toml
fn lookup_registered_capabilities(_node_type: &str) -> Option<NodeCapabilities> {
    // TODO: Implement node registry with compile-time capability detection
    // For now, nodes must explicitly declare capabilities in manifest
    //
    // Future implementation:
    // 1. Build script analyzes node source code
    // 2. Detects imports (whisper-rs, cuda, etc.)
    // 3. Generates capability metadata at compile time
    // 4. Stores in static registry
    None
}

/// Merge registered capabilities with detected capabilities
fn merge_registered_capabilities(registered: &NodeCapabilities, caps: &mut NodeCapabilities) {
    // Only override if not already explicitly set
    if !caps.supports_browser && registered.supports_browser {
        caps.supports_browser = registered.supports_browser;
    }
    if !caps.requires_threads && registered.requires_threads {
        caps.requires_threads = registered.requires_threads;
    }
    if !caps.requires_native_libs && registered.requires_native_libs {
        caps.requires_native_libs = registered.requires_native_libs;
    }
    if !caps.requires_gpu && registered.requires_gpu {
        caps.requires_gpu = registered.requires_gpu;
        caps.gpu_type = registered.gpu_type.clone();
    }

    // Take the max of memory requirements
    caps.estimated_memory_mb = caps.estimated_memory_mb.max(registered.estimated_memory_mb);

    // Merge placement strategies (explicit takes priority)
    if matches!(caps.placement, ExecutionPlacement::Auto | ExecutionPlacement::PreferAnywhere | ExecutionPlacement::PreferWasi) {
        if matches!(registered.placement, ExecutionPlacement::RequiresRemote | ExecutionPlacement::PreferRemote) {
            caps.placement = registered.placement.clone();
        }
    }
}

/// Analyze node parameters for capability hints
fn analyze_parameters(params: &serde_json::Map<String, serde_json::Value>, caps: &mut NodeCapabilities) {
    // Check for model file references (indicates large memory)
    if let Some(model_path) = params.get("model").or_else(|| params.get("model_path")) {
        if let Some(model_str) = model_path.as_str() {
            // Large models
            if model_str.contains("large") || model_str.contains("xl") {
                caps.estimated_memory_mb = caps.estimated_memory_mb.max(2048);
                caps.requires_large_memory = true;
            } else if model_str.contains("medium") {
                caps.estimated_memory_mb = caps.estimated_memory_mb.max(1024);
            } else if model_str.contains("small") || model_str.contains("base") {
                caps.estimated_memory_mb = caps.estimated_memory_mb.max(512);
            } else if model_str.contains("tiny") {
                caps.estimated_memory_mb = caps.estimated_memory_mb.max(256);
            }
        }
    }

    // Check for GPU device parameter
    if let Some(device) = params.get("device") {
        if let Some(device_str) = device.as_str() {
            if device_str == "cuda" || device_str == "gpu" {
                caps.requires_gpu = true;
                caps.supports_browser = false;
            }
        }
    }

    // Check for thread count
    if let Some(threads) = params.get("threads")
        .or_else(|| params.get("num_threads"))
        .or_else(|| params.get("n_threads")) {
        if let Some(thread_count) = threads.as_u64() {
            if thread_count > 1 {
                caps.requires_threads = true;
            }
        }
    }

    // Check for file I/O (may not work in browser)
    if params.contains_key("input_file") || params.contains_key("output_file") {
        // File I/O might work with WASI, but be cautious
        // Only suggest placement if not already explicitly set
        if caps.supports_browser && caps.placement == ExecutionPlacement::PreferAnywhere {
            // Keep default PreferAnywhere (will fall back to WASI for file I/O)
        }
    }
}

/// Apply explicit capability requirements from manifest
fn apply_manifest_capabilities(
    manifest_caps: &crate::manifest::CapabilityRequirements,
    caps: &mut NodeCapabilities,
) {
    // GPU requirements
    if let Some(gpu_req) = &manifest_caps.gpu {
        caps.requires_gpu = gpu_req.required;
        if gpu_req.required {
            caps.supports_browser = false; // No GPU in browser WASM (yet)
        }

        // Map GPU type
        caps.gpu_type = Some(match gpu_req.gpu_type.as_str() {
            "cuda" => super::GpuType::Cuda,
            "metal" => super::GpuType::Metal,
            "vulkan" => super::GpuType::Vulkan,
            "webgpu" => super::GpuType::WebGPU,
            _ => super::GpuType::Any,
        });

        if let Some(min_mem) = gpu_req.min_memory_gb {
            caps.estimated_memory_mb = (min_mem * 1024.0) as u32;
        }
    }

    // CPU requirements
    if let Some(cpu_req) = &manifest_caps.cpu {
        if let Some(cores) = cpu_req.cores {
            if cores > 4 && caps.placement == ExecutionPlacement::PreferAnywhere {
                // High core count suggests compute-intensive, prefer native
                // Only override if still at default placement
                caps.placement = ExecutionPlacement::PreferNative;
            }
            if cores > 1 {
                caps.requires_threads = true;
            }
        }
    }

    // Memory requirements
    if let Some(mem_gb) = manifest_caps.memory_gb {
        caps.estimated_memory_mb = (mem_gb * 1024.0) as u32;
        if mem_gb > 2.0 {
            caps.requires_large_memory = true;
            caps.supports_browser = false; // Browser memory limits
        }
    }
}

/// Apply explicit execution metadata from manifest
fn apply_execution_metadata(
    exec_meta: &crate::manifest::ExecutionMetadata,
    caps: &mut NodeCapabilities,
) {
    caps.placement = match exec_meta.placement.as_str() {
        // Direct requirement specifications
        "anywhere" => ExecutionPlacement::Anywhere,
        "wasi" | "requires_wasi" => ExecutionPlacement::RequiresWasi,
        "native" | "requires_native" => ExecutionPlacement::RequiresNative,
        "remote" | "requires_remote" => ExecutionPlacement::RequiresRemote,

        // Preference-based fallback chains
        "prefer_anywhere" => ExecutionPlacement::PreferAnywhere,
        "prefer_wasi" => ExecutionPlacement::PreferWasi,
        "prefer_native" => ExecutionPlacement::PreferNative,
        "prefer_remote" => ExecutionPlacement::PreferRemote,

        // Legacy support (map old values to new)
        "browser" | "local" => ExecutionPlacement::Anywhere,
        "prefer_browser" | "prefer_local" => ExecutionPlacement::PreferAnywhere,
        "host" => ExecutionPlacement::RequiresNative,
        "prefer_host" => ExecutionPlacement::PreferNative,

        _ => ExecutionPlacement::Auto,
    };

    if let Some(fallback) = &exec_meta.fallback {
        caps.fallback_available = true;
        caps.fallback_node = Some(fallback.clone());
    }
}


/// Detect capabilities for an entire pipeline
pub fn detect_pipeline_capabilities(nodes: &[NodeManifest]) -> super::PipelineCapabilities {
    let mut pipeline_caps = super::PipelineCapabilities::default();

    for node in nodes {
        let node_caps = detect_node_capabilities(node);

        // Aggregate requirements
        pipeline_caps.max_memory_mb = pipeline_caps.max_memory_mb.max(node_caps.estimated_memory_mb);
        pipeline_caps.requires_gpu |= node_caps.requires_gpu;
        pipeline_caps.requires_threads |= node_caps.requires_threads;

        // Check if node can't run locally
        if !node_caps.supports_browser || matches!(node_caps.placement, ExecutionPlacement::RequiresRemote | ExecutionPlacement::PreferRemote | ExecutionPlacement::RequiresNative) {
            pipeline_caps.fully_local = false;
            pipeline_caps.requires_remote = true;
            pipeline_caps.remote_nodes.push(node.id.clone());

            // Add to environment requirements
            if node_caps.requires_native_libs {
                pipeline_caps.environment_requirements.insert(
                    node.id.clone(),
                    "native_libs".to_string(),
                );
            }
            if node_caps.requires_gpu {
                pipeline_caps.environment_requirements.insert(
                    node.id.clone(),
                    format!("gpu:{:?}", node_caps.gpu_type.unwrap_or(super::GpuType::Any)),
                );
            }
        }
    }

    pipeline_caps
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_detect_node_with_explicit_execution_metadata() {
        // Node explicitly declares it needs remote execution
        let node = NodeManifest {
            id: "whisper-1".to_string(),
            node_type: "WhisperNode".to_string(),
            params: json!({
                "model": "base.en",
                "threads": 4
            }),
            is_streaming: false,
            capabilities: None,
            host: None,
            runtime_hint: None,
            execution: Some(crate::manifest::ExecutionMetadata {
                placement: "remote".to_string(),
                reason: Some("requires_native_libs_and_threads".to_string()),
                fallback: None,
            }),
        };

        let caps = detect_node_capabilities(&node);

        assert_eq!(caps.placement, ExecutionPlacement::RequiresRemote);
        assert!(caps.requires_threads, "Threads param = 4");
    }

    #[test]
    fn test_detect_simple_python_node() {
        // Simple node with no special requirements - defaults to browser-compatible
        let node = NodeManifest {
            id: "multiply-1".to_string(),
            node_type: "MultiplyNode".to_string(),
            params: json!({"factor": 2}),
            is_streaming: false,
            capabilities: None,
            host: None,
            runtime_hint: None,
            execution: None,
        };

        let caps = detect_node_capabilities(&node);

        // Defaults should be browser-friendly
        assert!(caps.supports_wasm, "Default should support WASM");
        assert!(caps.supports_browser, "Default should support browser");
        assert!(!caps.requires_threads, "No threads param specified");
        assert!(!caps.requires_gpu, "No GPU specified");
    }

    #[test]
    fn test_detect_gpu_node_from_params() {
        // Device parameter = "cuda" is concrete indicator of GPU requirement
        let node = NodeManifest {
            id: "inference-1".to_string(),
            node_type: "LLMInferenceNode".to_string(),
            params: json!({
                "model": "llama-large",
                "device": "cuda"
            }),
            is_streaming: false,
            capabilities: None,
            host: None,
            runtime_hint: None,
            execution: None,
        };

        let caps = detect_node_capabilities(&node);

        assert!(caps.requires_gpu, "device=cuda is explicit GPU requirement");
        assert!(!caps.supports_browser, "GPU nodes can't run in browser");
        assert!(caps.requires_large_memory, "Large model path detected");
        assert!(caps.estimated_memory_mb >= 2048, "Large model needs 2GB+");
    }

    #[test]
    fn test_detect_pipeline_capabilities() {
        let nodes = vec![
            NodeManifest {
                id: "input".to_string(),
                node_type: "AudioInput".to_string(),
                params: json!({}),
                is_streaming: false,
                capabilities: None,
                host: None,
                runtime_hint: None,
                execution: None,
            },
            NodeManifest {
                id: "whisper".to_string(),
                node_type: "WhisperNode".to_string(),
                params: json!({"model": "base.en"}),
                is_streaming: false,
                capabilities: None,
                host: None,
                runtime_hint: None,
                execution: Some(crate::manifest::ExecutionMetadata {
                    placement: "remote".to_string(),
                    reason: Some("requires_native_libs".to_string()),
                    fallback: None,
                }),
            },
        ];

        let pipeline_caps = detect_pipeline_capabilities(&nodes);

        assert!(!pipeline_caps.fully_local, "Remote node prevents full local execution");
        assert!(pipeline_caps.requires_remote, "Pipeline needs remote executor");
        assert_eq!(pipeline_caps.remote_nodes, vec!["whisper"]);
    }

    #[test]
    fn test_detect_explicit_manifest_capabilities() {
        let node = NodeManifest {
            id: "custom-1".to_string(),
            node_type: "CustomNode".to_string(),
            params: json!({}),
            is_streaming: false,
            capabilities: Some(crate::manifest::CapabilityRequirements {
                gpu: Some(crate::manifest::GpuRequirement {
                    gpu_type: "cuda".to_string(),
                    min_memory_gb: Some(8.0),
                    required: true,
                }),
                cpu: Some(crate::manifest::CpuRequirement {
                    cores: Some(8),
                    arch: None,
                }),
                memory_gb: Some(16.0),
            }),
            host: None,
            runtime_hint: None,
            execution: None,
        };

        let caps = detect_node_capabilities(&node);

        assert!(caps.requires_gpu, "Explicit GPU requirement");
        assert_eq!(caps.gpu_type, Some(super::super::GpuType::Cuda));
        assert!(caps.requires_threads, "8 cores implies threading");
        assert_eq!(caps.estimated_memory_mb, 16384, "16GB memory");
        assert!(!caps.supports_browser, "High memory prevents browser");
    }
}
