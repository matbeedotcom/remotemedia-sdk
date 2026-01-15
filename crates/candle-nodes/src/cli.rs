//! CLI commands for Candle model management
//!
//! Provides subcommands for listing, downloading, and managing cached models.

use crate::cache::{CacheStats, CachedModel, ModelCache};
use crate::error::Result;

/// CLI command results
pub enum CliOutput {
    /// List of cached models
    ModelList(Vec<CachedModel>),
    /// Cache statistics
    Stats(CacheStats),
    /// Download progress
    DownloadProgress { model: String, progress: f32 },
    /// Download complete
    DownloadComplete { model: String, path: String },
    /// Model removed
    Removed { model: String, freed_bytes: u64 },
    /// Error message
    Error(String),
    /// Success message
    Success(String),
}

/// Model management CLI
pub struct ModelCli {
    cache: ModelCache,
}

impl ModelCli {
    /// Create new CLI with default cache
    pub fn new() -> Self {
        Self {
            cache: ModelCache::new(),
        }
    }

    /// Create CLI with custom cache directory
    pub fn with_cache_dir(cache_dir: std::path::PathBuf) -> Self {
        Self {
            cache: ModelCache::with_dir(cache_dir),
        }
    }

    /// List all cached models
    pub fn list(&self) -> Result<Vec<CachedModel>> {
        self.cache.list_cached_models()
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        self.cache.stats()
    }

    /// Download a model for pre-caching
    #[cfg(any(feature = "whisper", feature = "yolo", feature = "llm"))]
    pub async fn download(&self, model_id: &str, filename: &str) -> Result<std::path::PathBuf> {
        self.cache.download_model(model_id, filename, None).await
    }

    /// Remove a model from cache
    pub fn remove(&self, model_id: &str) -> Result<bool> {
        self.cache.remove_model(model_id)
    }

    /// Print model list to stdout
    pub fn print_list(&self) -> Result<()> {
        let models = self.list()?;
        
        if models.is_empty() {
            println!("No cached models found.");
            println!("\nCache directory: {}", self.cache.cache_dir().display());
            return Ok(());
        }

        println!("Cached Models:");
        println!("{:-<80}", "");
        
        for model in &models {
            let size = format_bytes(model.size_bytes);
            let age = format_age(model.downloaded_at);
            println!(
                "  {} ({})\n    Revision: {}\n    Size: {}, Downloaded: {}",
                model.model_id,
                model.path.display(),
                &model.revision[..model.revision.len().min(12)],
                size,
                age
            );
        }
        
        println!("{:-<80}", "");
        println!("Total: {} models", models.len());
        
        Ok(())
    }

    /// Print cache statistics to stdout
    pub fn print_stats(&self) -> Result<()> {
        let stats = self.stats()?;
        
        println!("Cache Statistics:");
        println!("{:-<40}", "");
        println!("  Directory: {}", stats.cache_dir.display());
        println!("  Models: {}", stats.model_count);
        println!("  Total Size: {}", stats.size_human());
        
        Ok(())
    }
}

impl Default for ModelCli {
    fn default() -> Self {
        Self::new()
    }
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Format age as human-readable string
fn format_age(time: std::time::SystemTime) -> String {
    match time.elapsed() {
        Ok(duration) => {
            let secs = duration.as_secs();
            if secs < 60 {
                "just now".to_string()
            } else if secs < 3600 {
                format!("{} minutes ago", secs / 60)
            } else if secs < 86400 {
                format!("{} hours ago", secs / 3600)
            } else {
                format!("{} days ago", secs / 86400)
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

/// Supported model download targets
pub struct ModelDownloadTarget {
    pub model_id: &'static str,
    pub files: &'static [&'static str],
    pub description: &'static str,
}

/// Pre-defined model download targets
pub const DOWNLOAD_TARGETS: &[ModelDownloadTarget] = &[
    ModelDownloadTarget {
        model_id: "openai/whisper-tiny",
        files: &["config.json", "model.safetensors"],
        description: "Whisper Tiny (39M params)",
    },
    ModelDownloadTarget {
        model_id: "openai/whisper-base",
        files: &["config.json", "model.safetensors"],
        description: "Whisper Base (74M params)",
    },
    ModelDownloadTarget {
        model_id: "openai/whisper-small",
        files: &["config.json", "model.safetensors"],
        description: "Whisper Small (244M params)",
    },
    ModelDownloadTarget {
        model_id: "lmz/candle-yolo-v8",
        files: &["yolov8n.safetensors"],
        description: "YOLOv8 Nano (3.2M params)",
    },
    ModelDownloadTarget {
        model_id: "lmz/candle-yolo-v8",
        files: &["yolov8s.safetensors"],
        description: "YOLOv8 Small (11.2M params)",
    },
    ModelDownloadTarget {
        model_id: "microsoft/phi-2",
        files: &["model.safetensors", "tokenizer.json"],
        description: "Phi-2 (2.7B params)",
    },
];

/// List available models for download
pub fn list_available_models() {
    println!("Available Models for Download:");
    println!("{:-<60}", "");
    
    for target in DOWNLOAD_TARGETS {
        println!("  {} - {}", target.model_id, target.description);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 bytes");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn test_cli_creation() {
        let cli = ModelCli::new();
        assert!(cli.cache.cache_dir().to_string_lossy().contains("huggingface"));
    }
}
