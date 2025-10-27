//! Node capability detection and execution placement
//!
//! This module implements automatic detection of node execution requirements
//! and enables hybrid local/remote execution based on capabilities.
//!
//! Related tasks:
//! - Phase 1.3.6: capability-aware execution placement
//! - Phase 1.3.7: local-first execution (default if no host specified)
//! - Phase 1.3.8: fallback logic (local → remote if capabilities not met)

mod detector;
mod build_detector;

pub use detector::{detect_node_capabilities, detect_pipeline_capabilities};
pub use build_detector::{
    detect_via_compilation,
    detect_python_node_capabilities,
    detect_and_print,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Node execution capabilities and requirements
///
/// Automatically detected at compile/export time to determine
/// which runtime environment can execute a node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCapabilities {
    /// Can this node compile to WASM?
    pub supports_wasm: bool,

    /// Can this node run in a browser environment?
    pub supports_browser: bool,

    /// Requires pthread/threading support
    pub requires_threads: bool,

    /// Requires native libraries (whisper.cpp, ffmpeg, etc.)
    pub requires_native_libs: bool,

    /// Requires GPU acceleration
    pub requires_gpu: bool,

    /// GPU type required (if any)
    pub gpu_type: Option<GpuType>,

    /// Requires large memory allocation (>2GB)
    pub requires_large_memory: bool,

    /// Estimated memory usage in MB
    pub estimated_memory_mb: u32,

    /// Requires specific CPU features (AVX, NEON, etc.)
    pub cpu_features: Vec<String>,

    /// Fallback node available if requirements not met
    pub fallback_available: bool,

    /// Fallback node type/name
    pub fallback_node: Option<String>,

    /// Execution placement hints
    pub placement: ExecutionPlacement,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            supports_wasm: true,
            supports_browser: true,
            requires_threads: false,
            requires_native_libs: false,
            requires_gpu: false,
            gpu_type: None,
            requires_large_memory: false,
            estimated_memory_mb: 64,
            cpu_features: Vec::new(),
            fallback_available: false,
            fallback_node: None,
            placement: ExecutionPlacement::PreferAnywhere,
        }
    }
}

/// GPU types for capability matching
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum GpuType {
    /// NVIDIA CUDA
    Cuda,
    /// Apple Metal
    Metal,
    /// Vulkan API
    Vulkan,
    /// Web GPU API (browser)
    WebGPU,
    /// Any GPU type
    Any,
}

/// Execution placement strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionPlacement {
    /// Pure WASM - runs anywhere (browser, server WASM, native)
    /// Compiled as: wasm32-unknown-unknown
    /// No special requirements - most portable option
    Anywhere,

    /// Requires WASI APIs - runs in server WASM or native
    /// Compiled as: wasm32-wasip1
    /// Cannot run in browser (needs WASI file I/O, etc.)
    RequiresWasi,

    /// Requires native execution - native libraries or won't compile to WASM
    /// Cannot compile to WASM, needs native host
    /// Examples: C-extensions (numpy), native libs (whisper.cpp)
    RequiresNative,

    /// Requires remote executor - external server/cluster
    /// Examples: GPU clusters, specialized hardware
    RequiresRemote,

    /// Prefer pure WASM (most portable), fall back chain if needed
    /// Fallback: anywhere → WASI → native → remote
    PreferAnywhere,

    /// Prefer WASI (server WASM), fall back to native/remote
    /// Fallback: WASI → native → remote
    PreferWasi,

    /// Prefer native execution, fall back to remote
    /// Fallback: native → remote
    PreferNative,

    /// Prefer remote, fall back to native if available
    /// Fallback: remote → native
    PreferRemote,

    /// Automatic selection based on detected capabilities
    Auto,
}

/// Execution environment capabilities
///
/// Describes what an execution environment (browser, server, native) can support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorCapabilities {
    /// Environment type
    pub environment: ExecutionEnvironment,

    /// Supports WASM execution
    pub wasm_support: bool,

    /// Supports native execution
    pub native_support: bool,

    /// Supports threading
    pub threading_support: bool,

    /// Available memory in MB
    pub available_memory_mb: u32,

    /// GPU availability
    pub gpu_available: bool,

    /// GPU type (if available)
    pub gpu_type: Option<GpuType>,

    /// Available CPU features
    pub cpu_features: Vec<String>,

    /// Can execute remotely (has network access)
    pub remote_execution_available: bool,
}

impl Default for ExecutorCapabilities {
    fn default() -> Self {
        Self {
            environment: ExecutionEnvironment::Native,
            wasm_support: false,
            native_support: true,
            threading_support: true,
            available_memory_mb: 4096,
            gpu_available: false,
            gpu_type: None,
            cpu_features: Vec::new(),
            remote_execution_available: false,
        }
    }
}

/// Execution environment types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionEnvironment {
    /// Browser WASM environment
    Browser,

    /// Native OS process
    Native,

    /// Server-side WASM (wasmtime/wasmer)
    WasmServer,

    /// Remote executor
    Remote,
}

/// Result of capability matching
#[derive(Debug, Clone)]
pub struct CapabilityMatch {
    /// Can this executor run this node?
    pub can_execute: bool,

    /// Reason why it can't execute (if can_execute is false)
    pub reason: Option<String>,

    /// Should use remote execution instead?
    pub use_remote: bool,

    /// Fallback node available?
    pub has_fallback: bool,

    /// Compatibility score (0-100, higher is better)
    pub score: u8,
}

/// Check if executor can run a node with given capabilities
pub fn check_compatibility(
    node_caps: &NodeCapabilities,
    executor_caps: &ExecutorCapabilities,
) -> CapabilityMatch {
    let mut score = 100u8;
    let mut reasons = Vec::new();

    // Check WASM support
    if !node_caps.supports_wasm && executor_caps.environment == ExecutionEnvironment::Browser {
        return CapabilityMatch {
            can_execute: false,
            reason: Some("Node cannot compile to WASM".to_string()),
            use_remote: true,
            has_fallback: node_caps.fallback_available,
            score: 0,
        };
    }

    // Check browser support
    if !node_caps.supports_browser && executor_caps.environment == ExecutionEnvironment::Browser {
        return CapabilityMatch {
            can_execute: false,
            reason: Some("Node requires native environment (threads, native libs, etc.)".to_string()),
            use_remote: true,
            has_fallback: node_caps.fallback_available,
            score: 0,
        };
    }

    // Check threading
    if node_caps.requires_threads && !executor_caps.threading_support {
        reasons.push("threading required but not available");
        score = score.saturating_sub(40);
    }

    // Check memory
    if node_caps.estimated_memory_mb > executor_caps.available_memory_mb {
        reasons.push("insufficient memory");
        score = score.saturating_sub(50);
    }

    // Check GPU
    if node_caps.requires_gpu {
        if !executor_caps.gpu_available {
            reasons.push("GPU required but not available");
            score = score.saturating_sub(60);
        } else if let (Some(required_gpu), Some(available_gpu)) =
            (&node_caps.gpu_type, &executor_caps.gpu_type)
        {
            if required_gpu != available_gpu && *required_gpu != GpuType::Any {
                reasons.push("GPU type mismatch");
                score = score.saturating_sub(30);
            }
        }
    }

    // Check CPU features
    for feature in &node_caps.cpu_features {
        if !executor_caps.cpu_features.contains(feature) {
            score = score.saturating_sub(10);
        }
    }

    // Determine if we should use remote execution
    let use_remote = if score < 50 && executor_caps.remote_execution_available {
        true
    } else {
        match node_caps.placement {
            ExecutionPlacement::RequiresRemote => true,
            ExecutionPlacement::PreferRemote if executor_caps.remote_execution_available => true,

            // Native-only nodes might need remote execution if local host unavailable
            ExecutionPlacement::RequiresNative if executor_caps.environment != ExecutionEnvironment::Native => {
                executor_caps.remote_execution_available
            },

            // WASI nodes need remote if not in WASI environment
            ExecutionPlacement::RequiresWasi if executor_caps.environment != ExecutionEnvironment::WasmServer => {
                executor_caps.remote_execution_available
            },

            _ => false,
        }
    };

    let can_execute = if use_remote {
        executor_caps.remote_execution_available
    } else {
        score >= 50 || reasons.is_empty()
    };

    CapabilityMatch {
        can_execute,
        reason: if !can_execute {
            Some(reasons.join(", "))
        } else {
            None
        },
        use_remote,
        has_fallback: node_caps.fallback_available,
        score,
    }
}

/// Detect browser executor capabilities
pub fn detect_browser_capabilities() -> ExecutorCapabilities {
    ExecutorCapabilities {
        environment: ExecutionEnvironment::Browser,
        wasm_support: true,
        native_support: false,
        threading_support: false, // SharedArrayBuffer not always available
        available_memory_mb: 2048, // Conservative estimate
        gpu_available: true, // WebGPU might be available
        gpu_type: Some(GpuType::WebGPU),
        cpu_features: vec!["wasm".to_string(), "simd".to_string()],
        remote_execution_available: true, // Can use WebRTC/HTTP
    }
}

/// Detect native executor capabilities
pub fn detect_native_capabilities() -> ExecutorCapabilities {
    ExecutorCapabilities {
        environment: ExecutionEnvironment::Native,
        wasm_support: true, // Can use wasmtime
        native_support: true,
        threading_support: true,
        available_memory_mb: 8192, // Assume reasonable RAM
        gpu_available: false, // Would need actual detection
        gpu_type: None,
        cpu_features: detect_cpu_features(),
        remote_execution_available: false, // By default
    }
}

/// Detect CPU features (simplified)
fn detect_cpu_features() -> Vec<String> {
    let features = Vec::new();

    #[cfg(target_arch = "x86_64")]
    {
        #[cfg(target_feature = "avx2")]
        features.push("avx2".to_string());

        #[cfg(target_feature = "sse4.2")]
        features.push("sse4.2".to_string());
    }

    #[cfg(target_arch = "aarch64")]
    {
        features.push("neon".to_string());
    }

    features
}

/// Pipeline-level capabilities (aggregate of all nodes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCapabilities {
    /// Can run fully locally?
    pub fully_local: bool,

    /// Requires remote executor?
    pub requires_remote: bool,

    /// Node IDs that require remote execution
    pub remote_nodes: Vec<String>,

    /// Maximum memory requirement
    pub max_memory_mb: u32,

    /// Requires GPU?
    pub requires_gpu: bool,

    /// Requires threads?
    pub requires_threads: bool,

    /// Execution environment requirements
    pub environment_requirements: HashMap<String, String>,
}

impl Default for PipelineCapabilities {
    fn default() -> Self {
        Self {
            fully_local: true,
            requires_remote: false,
            remote_nodes: Vec::new(),
            max_memory_mb: 0,
            requires_gpu: false,
            requires_threads: false,
            environment_requirements: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_node_browser_compatible() {
        let node_caps = NodeCapabilities {
            supports_wasm: true,
            supports_browser: true,
            ..Default::default()
        };

        let executor_caps = detect_browser_capabilities();
        let result = check_compatibility(&node_caps, &executor_caps);

        assert!(result.can_execute);
        assert!(!result.use_remote);
        assert!(result.score >= 90);
    }

    #[test]
    fn test_whisper_node_requires_remote() {
        let whisper_caps = NodeCapabilities {
            supports_wasm: true,
            supports_browser: false, // Requires threads + native libs
            requires_threads: true,
            requires_native_libs: true,
            estimated_memory_mb: 512,
            placement: ExecutionPlacement::RequiresNative, // Needs native host for whisper.cpp
            ..Default::default()
        };

        let executor_caps = detect_browser_capabilities();
        let result = check_compatibility(&whisper_caps, &executor_caps);

        assert!(!result.can_execute || result.use_remote);
        assert_eq!(result.reason, Some("Node requires native environment (threads, native libs, etc.)".to_string()));
    }

    #[test]
    fn test_gpu_node_scoring() {
        let gpu_node = NodeCapabilities {
            requires_gpu: true,
            gpu_type: Some(GpuType::Cuda),
            estimated_memory_mb: 2048,
            ..Default::default()
        };

        let executor_no_gpu = ExecutorCapabilities {
            gpu_available: false,
            ..Default::default()
        };

        let result = check_compatibility(&gpu_node, &executor_no_gpu);
        assert!(result.score < 50); // Should prefer remote
    }

    #[test]
    fn test_native_executor_high_compatibility() {
        let complex_node = NodeCapabilities {
            requires_threads: true,
            requires_native_libs: true,
            estimated_memory_mb: 1024,
            ..Default::default()
        };

        let executor_caps = detect_native_capabilities();
        let result = check_compatibility(&complex_node, &executor_caps);

        assert!(result.can_execute);
        assert!(result.score >= 70);
    }
}
