//! Prerequisite checking — verifies Python, GPU, ML model availability

use remotemedia_manifest_analyzer::MlRequirement;
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Result of checking system prerequisites
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrerequisiteCheck {
    pub python_available: bool,
    pub python_version: Option<String>,
    pub gpu_available: bool,
    pub gpu_type: Option<GpuType>,
    pub available_models: Vec<String>,
    pub missing_prerequisites: Vec<MissingPrerequisite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpuType {
    Cuda,
    Rocm,
    Metal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingPrerequisite {
    pub node_id: String,
    pub requirement: String,
    pub message: String,
}

impl PrerequisiteCheck {
    /// Run prerequisite checks for the given ML requirements
    pub fn check(ml_requirements: &[MlRequirement]) -> Self {
        let (python_available, python_version) = check_python();
        let (gpu_available, gpu_type) = check_gpu();

        let mut missing = Vec::new();

        for req in ml_requirements {
            if req.requires_python && !python_available {
                missing.push(MissingPrerequisite {
                    node_id: req.node_id.clone(),
                    requirement: "python".to_string(),
                    message: format!(
                        "Node '{}' ({}) requires Python but no Python interpreter found",
                        req.node_id, req.node_type
                    ),
                });
            }
            if req.requires_gpu && !gpu_available {
                missing.push(MissingPrerequisite {
                    node_id: req.node_id.clone(),
                    requirement: "gpu".to_string(),
                    message: format!(
                        "Node '{}' ({}) requires GPU but none detected",
                        req.node_id, req.node_type
                    ),
                });
            }
        }

        PrerequisiteCheck {
            python_available,
            python_version,
            gpu_available,
            gpu_type,
            available_models: Vec::new(), // TODO: check model cache
            missing_prerequisites: missing,
        }
    }

    /// Whether all prerequisites are met (no missing items)
    pub fn all_met(&self) -> bool {
        self.missing_prerequisites.is_empty()
    }
}

fn check_python() -> (bool, Option<String>) {
    match Command::new("python3")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .replace("Python ", "")
                .to_string();
            (true, Some(version))
        }
        _ => match Command::new("python").arg("--version").output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .replace("Python ", "")
                    .to_string();
                (true, Some(version))
            }
            _ => (false, None),
        },
    }
}

fn check_gpu() -> (bool, Option<GpuType>) {
    // Check CUDA
    if Command::new("nvidia-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return (true, Some(GpuType::Cuda));
    }

    // Check ROCm
    if Command::new("rocm-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return (true, Some(GpuType::Rocm));
    }

    // Check Metal (macOS)
    #[cfg(target_os = "macos")]
    {
        if Command::new("system_profiler")
            .args(["SPDisplaysDataType"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return (true, Some(GpuType::Metal));
        }
    }

    (false, None)
}
