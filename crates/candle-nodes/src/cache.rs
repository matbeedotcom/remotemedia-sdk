//! Model cache management for Candle nodes
//!
//! Provides model weight caching via HuggingFace Hub with local storage.

use crate::error::{CandleNodeError, Result};
use std::path::PathBuf;
use tracing::{debug, info};

/// Information about a cached model
#[derive(Debug, Clone)]
pub struct CachedModel {
    /// HuggingFace model identifier (e.g., "openai/whisper-base")
    pub model_id: String,
    /// Model revision/commit hash
    pub revision: String,
    /// Local file path
    pub path: PathBuf,
    /// File size in bytes
    pub size_bytes: u64,
    /// When the model was downloaded
    pub downloaded_at: std::time::SystemTime,
}

/// Model cache manager
pub struct ModelCache {
    /// Cache root directory
    cache_dir: PathBuf,
}

impl ModelCache {
    /// Create a new model cache with default HuggingFace cache location
    pub fn new() -> Self {
        Self {
            cache_dir: Self::default_cache_dir(),
        }
    }

    /// Create a model cache with custom directory
    pub fn with_dir(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Get the default cache directory
    ///
    /// Uses HF_HOME environment variable or falls back to ~/.cache/huggingface/hub
    pub fn default_cache_dir() -> PathBuf {
        if let Ok(hf_home) = std::env::var("HF_HOME") {
            PathBuf::from(hf_home).join("hub")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".cache").join("huggingface").join("hub")
        } else {
            PathBuf::from(".cache").join("huggingface").join("hub")
        }
    }

    /// Get the cache directory path
    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    /// Check if a model is cached locally
    pub fn is_cached(&self, model_id: &str, filename: &str) -> bool {
        self.get_cached_path(model_id, filename).is_some()
    }

    /// Get the local path for a cached model file if it exists
    pub fn get_cached_path(&self, model_id: &str, filename: &str) -> Option<PathBuf> {
        // HuggingFace hub stores files in a specific structure
        // models--{org}--{model}/snapshots/{revision}/{filename}
        let model_dir_name = format!("models--{}", model_id.replace('/', "--"));
        let model_dir = self.cache_dir.join(&model_dir_name);
        
        if !model_dir.exists() {
            return None;
        }

        // Look for the file in any snapshot
        let snapshots_dir = model_dir.join("snapshots");
        if let Ok(entries) = std::fs::read_dir(&snapshots_dir) {
            for entry in entries.flatten() {
                let file_path = entry.path().join(filename);
                if file_path.exists() {
                    return Some(file_path);
                }
            }
        }

        None
    }

    /// Download a model file from HuggingFace Hub
    #[cfg(any(feature = "whisper", feature = "yolo", feature = "llm", feature = "vad"))]
    pub async fn download_model(
        &self,
        model_id: &str,
        filename: &str,
        revision: Option<&str>,
    ) -> Result<PathBuf> {
        use hf_hub::api::tokio::Api;
        
        info!("Downloading model file: {}/{}", model_id, filename);
        
        let api = Api::new().map_err(|e| CandleNodeError::ModelDownload {
            model: model_id.to_string(),
            download_source: "huggingface.co".to_string(),
            message: e.to_string(),
        })?;

        let repo = if let Some(rev) = revision {
            api.repo(hf_hub::Repo::with_revision(
                model_id.to_string(),
                hf_hub::RepoType::Model,
                rev.to_string(),
            ))
        } else {
            api.model(model_id.to_string())
        };

        let path = repo.get(filename).await.map_err(|e| CandleNodeError::ModelDownload {
            model: model_id.to_string(),
            download_source: "huggingface.co".to_string(),
            message: format!("Download failed: {}. Try setting HF_ENDPOINT=https://huggingface.co", e),
        })?;

        info!("Downloaded model to: {:?}", path);
        Ok(path)
    }

    /// Download a model file (sync version for non-async contexts)
    #[cfg(any(feature = "whisper", feature = "yolo", feature = "llm"))]
    pub fn download_model_sync(
        &self,
        model_id: &str,
        filename: &str,
        revision: Option<&str>,
    ) -> Result<PathBuf> {
        use hf_hub::api::sync::Api;
        
        info!("Downloading model file (sync): {}/{}", model_id, filename);
        
        let api = Api::new().map_err(|e| CandleNodeError::ModelDownload {
            model: model_id.to_string(),
            download_source: "huggingface.co".to_string(),
            message: e.to_string(),
        })?;

        let repo = if let Some(rev) = revision {
            api.repo(hf_hub::Repo::with_revision(
                model_id.to_string(),
                hf_hub::RepoType::Model,
                rev.to_string(),
            ))
        } else {
            api.model(model_id.to_string())
        };

        let path = repo.get(filename).map_err(|e| CandleNodeError::ModelDownload {
            model: model_id.to_string(),
            download_source: "huggingface.co".to_string(),
            message: e.to_string(),
        })?;

        info!("Downloaded model to: {:?}", path);
        Ok(path)
    }

    /// List all cached models
    pub fn list_cached_models(&self) -> Result<Vec<CachedModel>> {
        let mut models = Vec::new();

        if !self.cache_dir.exists() {
            return Ok(models);
        }

        let entries = std::fs::read_dir(&self.cache_dir).map_err(|e| CandleNodeError::Cache {
            message: format!("Failed to read cache directory: {}", e),
        })?;

        for entry in entries.flatten() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            
            // Parse model directories (format: models--org--name)
            if dir_name.starts_with("models--") {
                let model_id = dir_name
                    .strip_prefix("models--")
                    .unwrap_or(&dir_name)
                    .replace("--", "/");

                let snapshots_dir = entry.path().join("snapshots");
                if let Ok(snapshots) = std::fs::read_dir(&snapshots_dir) {
                    for snapshot in snapshots.flatten() {
                        let revision = snapshot.file_name().to_string_lossy().to_string();
                        
                        // Calculate total size of snapshot
                        let mut total_size = 0u64;
                        if let Ok(files) = std::fs::read_dir(snapshot.path()) {
                            for file in files.flatten() {
                                if let Ok(metadata) = file.metadata() {
                                    total_size += metadata.len();
                                }
                            }
                        }

                        let downloaded_at = snapshot
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                        models.push(CachedModel {
                            model_id: model_id.clone(),
                            revision,
                            path: snapshot.path(),
                            size_bytes: total_size,
                            downloaded_at,
                        });
                    }
                }
            }
        }

        Ok(models)
    }

    /// Get total cache size in bytes
    pub fn total_size(&self) -> Result<u64> {
        let models = self.list_cached_models()?;
        Ok(models.iter().map(|m| m.size_bytes).sum())
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        let models = self.list_cached_models()?;
        let total_size = models.iter().map(|m| m.size_bytes).sum();
        
        Ok(CacheStats {
            model_count: models.len(),
            total_size_bytes: total_size,
            cache_dir: self.cache_dir.clone(),
        })
    }

    /// Remove a specific model from cache
    pub fn remove_model(&self, model_id: &str) -> Result<bool> {
        let model_dir_name = format!("models--{}", model_id.replace('/', "--"));
        let model_dir = self.cache_dir.join(&model_dir_name);

        if model_dir.exists() {
            std::fs::remove_dir_all(&model_dir).map_err(|e| CandleNodeError::Cache {
                message: format!("Failed to remove model directory: {}", e),
            })?;
            info!("Removed cached model: {}", model_id);
            Ok(true)
        } else {
            debug!("Model not in cache: {}", model_id);
            Ok(false)
        }
    }
}

impl Default for ModelCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached models
    pub model_count: usize,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Cache directory path
    pub cache_dir: PathBuf,
}

impl CacheStats {
    /// Format size as human-readable string
    pub fn size_human(&self) -> String {
        format_bytes(self.total_size_bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cache_dir() {
        let cache = ModelCache::new();
        let dir = cache.cache_dir();
        assert!(dir.to_string_lossy().contains("huggingface"));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 bytes");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }
}
