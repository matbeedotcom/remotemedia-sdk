//! Pipeline template registry
//!
//! Manages loading and retrieval of analysis pipeline templates.

use std::collections::HashMap;
use std::path::Path;

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

    /// Create a registry with default demo templates
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_defaults();
        registry
    }

    /// Register default demo templates
    pub fn register_defaults(&mut self) {
        // Demo audio quality template
        self.register(PipelineTemplate {
            id: "demo_audio_quality_v1".to_string(),
            name: "Audio Quality Demo".to_string(),
            description: "Detects silence, clipping, low volume, and channel imbalance".to_string(),
            manifest: DEFAULT_AUDIO_QUALITY_MANIFEST.to_string(),
            enabled: true,
        });

        // Demo video integrity template
        self.register(PipelineTemplate {
            id: "demo_video_integrity_v1".to_string(),
            name: "Video Integrity Demo".to_string(),
            description: "Detects freeze frames and black frames".to_string(),
            manifest: DEFAULT_VIDEO_INTEGRITY_MANIFEST.to_string(),
            enabled: true,
        });

        // Combined A/V template
        self.register(PipelineTemplate {
            id: "demo_av_quality_v1".to_string(),
            name: "A/V Quality Demo".to_string(),
            description: "Combined audio and video quality analysis".to_string(),
            manifest: DEFAULT_AV_QUALITY_MANIFEST.to_string(),
            enabled: true,
        });

        // Contact center QA template (business layer)
        self.register(PipelineTemplate {
            id: "contact_center_qa_v1".to_string(),
            name: "Contact Center QA".to_string(),
            description: "Speech presence, conversation flow metrics, and session health for contact center quality assurance".to_string(),
            manifest: CONTACT_CENTER_QA_MANIFEST.to_string(),
            enabled: true,
        });

        // Technical stream analysis template (technical layer)
        self.register(PipelineTemplate {
            id: "technical_stream_analysis_v1".to_string(),
            name: "Technical Stream Analysis".to_string(),
            description: "Timing drift, event correlation, and audio evidence capture for infrastructure debugging".to_string(),
            manifest: TECHNICAL_STREAM_ANALYSIS_MANIFEST.to_string(),
            enabled: true,
        });

        // Full stream health template (combined business + technical)
        self.register(PipelineTemplate {
            id: "full_stream_health_v1".to_string(),
            name: "Full Stream Health".to_string(),
            description: "Complete stream health monitoring with both business metrics and technical diagnostics".to_string(),
            manifest: FULL_STREAM_HEALTH_MANIFEST.to_string(),
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

    /// List all template IDs
    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// Load templates from a directory
    ///
    /// Reads all .yaml and .yml files in the directory as pipeline templates.
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
                            tracing::info!("Loaded pipeline template: {}", template.id);
                            self.register(template);
                            count += 1;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load template {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        Ok(count)
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
        Self::with_defaults()
    }
}

// Pipeline manifests are loaded from external YAML files at compile time
const DEFAULT_AUDIO_QUALITY_MANIFEST: &str = include_str!("../../pipelines/demo_audio_quality_v1.yaml");
const DEFAULT_VIDEO_INTEGRITY_MANIFEST: &str = include_str!("../../pipelines/demo_video_integrity_v1.yaml");
const DEFAULT_AV_QUALITY_MANIFEST: &str = include_str!("../../pipelines/demo_av_quality_v1.yaml");
const CONTACT_CENTER_QA_MANIFEST: &str = include_str!("../../pipelines/contact_center_qa_v1.yaml");
const TECHNICAL_STREAM_ANALYSIS_MANIFEST: &str = include_str!("../../pipelines/technical_stream_analysis_v1.yaml");
const FULL_STREAM_HEALTH_MANIFEST: &str = include_str!("../../pipelines/full_stream_health_v1.yaml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_with_defaults() {
        let registry = PipelineRegistry::with_defaults();
        assert!(registry.get("demo_audio_quality_v1").is_some());
        assert!(registry.get("demo_video_integrity_v1").is_some());
        assert!(registry.get("demo_av_quality_v1").is_some());
        // Stream health monitoring templates
        assert!(registry.get("contact_center_qa_v1").is_some());
        assert!(registry.get("technical_stream_analysis_v1").is_some());
        assert!(registry.get("full_stream_health_v1").is_some());
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = PipelineRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_template_new() {
        let template = PipelineTemplate::new("test", "Test Pipeline", "manifest: content");
        assert_eq!(template.id, "test");
        assert_eq!(template.name, "Test Pipeline");
        assert!(template.enabled);
    }

    #[test]
    fn test_registry_list() {
        let registry = PipelineRegistry::with_defaults();
        let ids = registry.list();
        assert!(ids.contains(&"demo_audio_quality_v1"));
        assert!(ids.contains(&"demo_video_integrity_v1"));
        assert!(ids.contains(&"contact_center_qa_v1"));
        assert!(ids.contains(&"full_stream_health_v1"));
    }

    #[test]
    fn test_stream_health_templates() {
        let registry = PipelineRegistry::with_defaults();

        // Contact center QA template should have speech presence
        let qa = registry.get("contact_center_qa_v1").unwrap();
        assert!(qa.manifest.contains("SpeechPresenceNode"));
        assert!(qa.manifest.contains("ConversationFlowNode"));
        assert!(qa.manifest.contains("SessionHealthNode"));

        // Technical template should have timing drift
        let tech = registry.get("technical_stream_analysis_v1").unwrap();
        assert!(tech.manifest.contains("TimingDriftNode"));
        assert!(tech.manifest.contains("EventCorrelatorNode"));
        assert!(tech.manifest.contains("AudioEvidenceNode"));

        // Full template should have both layers
        let full = registry.get("full_stream_health_v1").unwrap();
        assert!(full.manifest.contains("SpeechPresenceNode"));
        assert!(full.manifest.contains("TimingDriftNode"));
        assert!(full.manifest.contains("HealthEmitterNode"));
    }
}
