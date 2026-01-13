//! Pipeline template registry
//!
//! Manages loading and retrieval of analysis pipeline templates.
//! This is a shared implementation used by both ingest-srt and stream-health-demo.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use remotemedia_runtime_core::manifest::Manifest;
use serde::{Deserialize, Serialize};

/// A pipeline template definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTemplate {
    /// Unique template ID
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of what this pipeline does
    pub description: String,

    /// YAML manifest content
    pub manifest: String,

    /// Whether this template is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl PipelineTemplate {
    /// Create a new pipeline template
    pub fn new(id: impl Into<String>, name: impl Into<String>, manifest: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            manifest: manifest.into(),
            enabled: true,
        }
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Parse the manifest YAML into a Manifest struct
    pub fn parse_manifest(&self) -> Result<Manifest, serde_yaml::Error> {
        serde_yaml::from_str(&self.manifest)
    }
}

/// Registry of available pipeline templates
pub struct PipelineRegistry {
    /// Templates indexed by ID
    templates: HashMap<String, PipelineTemplate>,
}

impl PipelineRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Create a registry with embedded default templates
    ///
    /// This includes all standard analysis pipelines that are compiled into
    /// the binary. Use `from_directory()` to load additional templates at runtime.
    pub fn embedded() -> Self {
        Self::with_defaults()
    }

    /// Create a registry with default demo templates
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_defaults();
        registry
    }

    /// Register default embedded templates
    fn register_defaults(&mut self) {
        // Audio quality analysis - the default for both CLI and service
        self.register(PipelineTemplate {
            id: "demo_audio_quality_v1".to_string(),
            name: "Audio Quality Analysis".to_string(),
            description: "Detects silence, clipping, low volume, and channel imbalance".to_string(),
            manifest: include_str!("../pipelines/demo_audio_quality_v1.yaml").to_string(),
            enabled: true,
        });

        // Video integrity analysis
        self.register(PipelineTemplate {
            id: "demo_video_integrity_v1".to_string(),
            name: "Video Integrity Analysis".to_string(),
            description: "Detects freeze frames and black frames".to_string(),
            manifest: include_str!("../pipelines/demo_video_integrity_v1.yaml").to_string(),
            enabled: true,
        });

        // Combined A/V quality
        self.register(PipelineTemplate {
            id: "demo_av_quality_v1".to_string(),
            name: "A/V Quality Analysis".to_string(),
            description: "Combined audio and video quality analysis".to_string(),
            manifest: include_str!("../pipelines/demo_av_quality_v1.yaml").to_string(),
            enabled: true,
        });

        // Contact Center QA (business layer)
        self.register(PipelineTemplate {
            id: "contact_center_qa_v1".to_string(),
            name: "Contact Center QA".to_string(),
            description: "Speech presence, conversation flow, and session health".to_string(),
            manifest: include_str!("../pipelines/contact_center_qa_v1.yaml").to_string(),
            enabled: true,
        });

        // Technical stream analysis
        self.register(PipelineTemplate {
            id: "technical_stream_analysis_v1".to_string(),
            name: "Technical Stream Analysis".to_string(),
            description: "Timing drift, event correlation, and audio evidence".to_string(),
            manifest: include_str!("../pipelines/technical_stream_analysis_v1.yaml").to_string(),
            enabled: true,
        });

        // Full stream health (combined)
        self.register(PipelineTemplate {
            id: "full_stream_health_v1".to_string(),
            name: "Full Stream Health".to_string(),
            description: "Complete monitoring with business and technical layers".to_string(),
            manifest: include_str!("../pipelines/full_stream_health_v1.yaml").to_string(),
            enabled: true,
        });

        // Speaker diarization
        self.register(PipelineTemplate {
            id: "speaker_diarization_v1".to_string(),
            name: "Speaker Diarization".to_string(),
            description: "Identify and segment speakers in audio".to_string(),
            manifest: include_str!("../pipelines/speaker_diarization_v1.yaml").to_string(),
            enabled: true,
        });
    }

    /// Register a template
    pub fn register(&mut self, template: PipelineTemplate) {
        self.templates.insert(template.id.clone(), template);
    }

    /// Get a template by ID
    pub fn get(&self, id: &str) -> Option<&PipelineTemplate> {
        self.templates.get(id)
    }

    /// Get a parsed manifest by template ID
    pub fn get_manifest(&self, id: &str) -> Option<Arc<Manifest>> {
        self.get(id)
            .and_then(|t| t.parse_manifest().ok())
            .map(Arc::new)
    }

    /// List all template IDs
    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// List all enabled templates
    pub fn list_enabled(&self) -> Vec<&PipelineTemplate> {
        self.templates.values().filter(|t| t.enabled).collect()
    }

    /// Load templates from a directory
    ///
    /// Reads all .yaml and .yml files in the directory as pipeline templates.
    /// Templates loaded this way override embedded templates with the same ID.
    pub fn load_from_directory(&mut self, dir: &Path) -> Result<usize, std::io::Error> {
        if !dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let extension = path.extension().and_then(|e| e.to_str());
                if extension == Some("yaml") || extension == Some("yml") {
                    match self.load_template_file(&path) {
                        Ok(template) => {
                            tracing::info!(id = %template.id, "Loaded pipeline template");
                            self.register(template);
                            count += 1;
                        }
                        Err(e) => {
                            tracing::warn!(path = ?path, error = %e, "Failed to load template");
                        }
                    }
                }
            }
        }

        Ok(count)
    }

    /// Create a registry from a directory (no embedded templates)
    pub fn from_directory(dir: &Path) -> Result<Self, std::io::Error> {
        let mut registry = Self::new();
        registry.load_from_directory(dir)?;
        Ok(registry)
    }

    /// Load a single template file
    fn load_template_file(&self, path: &Path) -> Result<PipelineTemplate, std::io::Error> {
        let content = std::fs::read_to_string(path)?;

        // Try to parse as a full template definition first
        if let Ok(template) = serde_yaml::from_str::<PipelineTemplate>(&content) {
            return Ok(template);
        }

        // Otherwise, treat as raw manifest - derive ID from filename
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(PipelineTemplate {
            id: id.clone(),
            name: id,
            description: format!("Loaded from {:?}", path),
            manifest: content,
            enabled: true,
        })
    }

    /// Get template count
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

impl Default for PipelineRegistry {
    fn default() -> Self {
        Self::embedded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_embedded() {
        let registry = PipelineRegistry::embedded();
        assert!(registry.get("demo_audio_quality_v1").is_some());
        assert!(registry.get("contact_center_qa_v1").is_some());
    }

    #[test]
    fn test_registry_get_manifest() {
        let registry = PipelineRegistry::embedded();
        let manifest = registry.get_manifest("demo_audio_quality_v1");
        assert!(manifest.is_some());
    }

    #[test]
    fn test_template_new() {
        let template = PipelineTemplate::new("test", "Test", "version: v1\nnodes: []");
        assert_eq!(template.id, "test");
        assert!(template.enabled);
    }
}
