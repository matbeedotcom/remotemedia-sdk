//! Device selection and management for Candle inference
//!
//! Provides automatic device detection with fallback chain:
//! CUDA (if available) → Metal (if available) → CPU

use crate::error::{CandleNodeError, Result};
use tracing::{info, warn};

/// Inference device for model execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceDevice {
    /// CPU inference (always available)
    Cpu,
    /// NVIDIA CUDA GPU with device index
    #[cfg(feature = "cuda")]
    Cuda(usize),
    /// Apple Metal GPU
    #[cfg(feature = "metal")]
    Metal,
}

impl InferenceDevice {
    /// Get device name for logging
    pub fn name(&self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            #[cfg(feature = "cuda")]
            Self::Cuda(_) => "cuda",
            #[cfg(feature = "metal")]
            Self::Metal => "metal",
        }
    }

    /// Check if this is a GPU device
    pub fn is_gpu(&self) -> bool {
        match self {
            Self::Cpu => false,
            #[cfg(feature = "cuda")]
            Self::Cuda(_) => true,
            #[cfg(feature = "metal")]
            Self::Metal => true,
        }
    }
}

impl std::fmt::Display for InferenceDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpu => write!(f, "cpu"),
            #[cfg(feature = "cuda")]
            Self::Cuda(idx) => write!(f, "cuda:{}", idx),
            #[cfg(feature = "metal")]
            Self::Metal => write!(f, "metal"),
        }
    }
}

/// Device selector with automatic fallback
pub struct DeviceSelector;

impl DeviceSelector {
    /// Select the best available device with automatic fallback
    ///
    /// Priority: CUDA → Metal → CPU
    pub fn select_best() -> InferenceDevice {
        // Try CUDA first
        #[cfg(feature = "cuda")]
        {
            if Self::cuda_available() {
                info!("CUDA device available, using GPU acceleration");
                return InferenceDevice::Cuda(0);
            }
            warn!("CUDA feature enabled but no CUDA device available");
        }

        // Try Metal on macOS
        #[cfg(feature = "metal")]
        {
            if Self::metal_available() {
                info!("Metal device available, using GPU acceleration");
                return InferenceDevice::Metal;
            }
            warn!("Metal feature enabled but Metal not available");
        }

        // Fall back to CPU
        info!("Using CPU for inference (no GPU acceleration available)");
        InferenceDevice::Cpu
    }

    /// Select device from string configuration
    ///
    /// Accepts: "auto", "cpu", "cuda", "cuda:0", "metal"
    pub fn from_config(config: &str) -> Result<InferenceDevice> {
        match config.to_lowercase().as_str() {
            "auto" => Ok(Self::select_best()),
            "cpu" => Ok(InferenceDevice::Cpu),
            #[cfg(feature = "cuda")]
            s if s.starts_with("cuda") => {
                let idx = if s.contains(':') {
                    s.split(':')
                        .nth(1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0)
                } else {
                    0
                };
                if Self::cuda_available() {
                    Ok(InferenceDevice::Cuda(idx))
                } else {
                    Err(CandleNodeError::DeviceInit {
                        device: s.to_string(),
                        message: "CUDA device not available".to_string(),
                    })
                }
            }
            #[cfg(feature = "metal")]
            "metal" => {
                if Self::metal_available() {
                    Ok(InferenceDevice::Metal)
                } else {
                    Err(CandleNodeError::DeviceInit {
                        device: "metal".to_string(),
                        message: "Metal device not available".to_string(),
                    })
                }
            }
            #[cfg(not(feature = "cuda"))]
            s if s.starts_with("cuda") => Err(CandleNodeError::DeviceInit {
                device: s.to_string(),
                message: "CUDA feature not enabled at compile time".to_string(),
            }),
            #[cfg(not(feature = "metal"))]
            "metal" => Err(CandleNodeError::DeviceInit {
                device: "metal".to_string(),
                message: "Metal feature not enabled at compile time".to_string(),
            }),
            other => Err(CandleNodeError::DeviceInit {
                device: other.to_string(),
                message: format!("Unknown device: {}. Valid options: auto, cpu, cuda, metal", other),
            }),
        }
    }

    /// Check if CUDA is available
    #[cfg(feature = "cuda")]
    pub fn cuda_available() -> bool {
        // Check via candle-core CUDA detection
        candle_core::utils::cuda_is_available()
    }

    #[cfg(not(feature = "cuda"))]
    pub fn cuda_available() -> bool {
        false
    }

    /// Check if Metal is available
    #[cfg(feature = "metal")]
    pub fn metal_available() -> bool {
        // Check via candle-core Metal detection
        candle_core::utils::metal_is_available()
    }

    #[cfg(not(feature = "metal"))]
    pub fn metal_available() -> bool {
        false
    }
}

/// Convert InferenceDevice to candle_core::Device
#[cfg(any(feature = "whisper", feature = "yolo", feature = "llm"))]
impl TryFrom<&InferenceDevice> for candle_core::Device {
    type Error = CandleNodeError;

    fn try_from(device: &InferenceDevice) -> Result<Self> {
        match device {
            InferenceDevice::Cpu => Ok(candle_core::Device::Cpu),
            #[cfg(feature = "cuda")]
            InferenceDevice::Cuda(idx) => {
                candle_core::Device::new_cuda(*idx).map_err(|e| CandleNodeError::DeviceInit {
                    device: format!("cuda:{}", idx),
                    message: e.to_string(),
                })
            }
            #[cfg(feature = "metal")]
            InferenceDevice::Metal => {
                candle_core::Device::new_metal(0).map_err(|e| CandleNodeError::DeviceInit {
                    device: "metal".to_string(),
                    message: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_always_available() {
        let device = DeviceSelector::from_config("cpu").unwrap();
        assert_eq!(device, InferenceDevice::Cpu);
        assert!(!device.is_gpu());
    }

    #[test]
    fn test_auto_returns_device() {
        let device = DeviceSelector::select_best();
        // Should always return something (at minimum CPU)
        // Just verify it returns a valid device name
        assert!(!device.name().is_empty());
    }

    #[test]
    fn test_device_display() {
        assert_eq!(InferenceDevice::Cpu.to_string(), "cpu");
    }
}
