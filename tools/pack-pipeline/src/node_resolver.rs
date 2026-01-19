//! Node resolution using the core runtime registry
//!
//! This module uses `remotemedia-core`'s pipeline analysis functionality
//! to dynamically resolve nodes from the actual registry instead of
//! hardcoded mappings.

use anyhow::{bail, Context, Result};
use remotemedia_core::manifest::Manifest;
use remotemedia_core::nodes::pipeline_analysis::{analyze_pipeline, PipelineAnalysis};
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Analyze a pipeline YAML and return information about all nodes
///
/// This queries the actual node registry to determine:
/// - Which node types are registered (available)
/// - Which are Python vs Rust nodes
/// - Which node types are missing
pub fn analyze_pipeline_yaml(yaml_content: &str) -> Result<PipelineAnalysis> {
    // Parse the manifest
    let manifest: Manifest = serde_yaml::from_str(yaml_content)
        .context("Failed to parse pipeline YAML")?;
    
    // Get the default streaming registry with all built-in nodes
    let registry = create_default_streaming_registry();
    
    // Analyze the pipeline against the registry
    Ok(analyze_pipeline(&manifest, &registry))
}

/// Check if all nodes in a pipeline are available
pub fn validate_pipeline_nodes(yaml_content: &str) -> Result<()> {
    let analysis = analyze_pipeline_yaml(yaml_content)?;
    
    if !analysis.is_valid {
        bail!(
            "Pipeline has missing node types: {:?}\n\nAvailable node types: {:?}",
            analysis.missing_types,
            analysis.registered_types
        );
    }
    
    Ok(())
}

/// Information about an embedded Python node file
#[derive(Debug, Clone)]
pub struct PythonNodeFile {
    /// Relative path from the nodes directory (e.g., "tts.py" or "ml/lfm2_audio.py")
    pub relative_path: String,
    /// Full path to the source file
    pub full_path: PathBuf,
    /// Source code content
    pub content: String,
    /// Node class names defined in this file
    pub node_classes: Vec<String>,
    /// Whether the Python syntax was validated
    pub syntax_validated: bool,
}

/// Validate Python syntax at build time using py_compile
/// 
/// This ensures we catch syntax errors during packaging rather than at runtime.
pub fn validate_python_syntax(files: &[PythonNodeFile]) -> Result<()> {
    use std::process::Command;
    
    for file in files {
        tracing::debug!("Validating Python syntax: {}", file.relative_path);
        
        // Use Python's py_compile module to check syntax
        // python3 -m py_compile <file>
        let output = Command::new("python3")
            .args(["-m", "py_compile", file.full_path.to_str().unwrap_or("")])
            .output()
            .with_context(|| format!("Failed to run Python syntax check for {}", file.relative_path))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "Python syntax error in {}:\n{}",
                file.relative_path,
                stderr
            );
        }
        
        tracing::debug!("Syntax OK: {}", file.relative_path);
    }
    
    Ok(())
}

/// Scan Python nodes directory and find files that define the requested node types
///
/// This dynamically discovers Python files by:
/// 1. Walking the nodes directory recursively
/// 2. Parsing each .py file to find class definitions
/// 3. Matching class names against the requested node types
pub fn find_python_node_files(
    python_nodes_dir: &Path,
    node_types: &[String],
) -> Result<Vec<PythonNodeFile>> {
    if !python_nodes_dir.exists() {
        bail!(
            "Python nodes directory not found: {:?}",
            python_nodes_dir
        );
    }

    // Build a map of node_type -> file info
    let mut node_type_to_file: HashMap<String, PythonNodeFile> = HashMap::new();
    
    // Recursively scan the directory
    scan_python_directory(python_nodes_dir, python_nodes_dir, &mut node_type_to_file)?;
    
    // Collect files that contain the requested node types
    let mut result: HashMap<PathBuf, PythonNodeFile> = HashMap::new();
    
    for node_type in node_types {
        if let Some(file_info) = node_type_to_file.get(node_type) {
            // Use the file path as key to deduplicate
            result.entry(file_info.full_path.clone())
                .or_insert_with(|| file_info.clone());
        } else {
            tracing::warn!(
                "Python node type '{}' not found in any .py file under {:?}",
                node_type,
                python_nodes_dir
            );
        }
    }
    
    Ok(result.into_values().collect())
}

/// Recursively scan a directory for Python files and extract node class definitions
fn scan_python_directory(
    base_dir: &Path,
    current_dir: &Path,
    node_map: &mut HashMap<String, PythonNodeFile>,
) -> Result<()> {
    let entries = fs::read_dir(current_dir)
        .with_context(|| format!("Failed to read directory: {:?}", current_dir))?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            // Skip __pycache__ and hidden directories
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !dir_name.starts_with('_') && !dir_name.starts_with('.') {
                scan_python_directory(base_dir, &path, node_map)?;
            }
        } else if path.extension().map(|e| e == "py").unwrap_or(false) {
            // Parse Python file for node class definitions
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            
            // Skip __init__.py and test files
            if file_name == "__init__.py" || file_name.starts_with("test_") {
                continue;
            }
            
            if let Ok(content) = fs::read_to_string(&path) {
                let relative_path = path
                    .strip_prefix(base_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                
                let node_classes = extract_node_classes(&content);
                
                if !node_classes.is_empty() {
                    let file_info = PythonNodeFile {
                        relative_path: relative_path.clone(),
                        full_path: path.clone(),
                        content: content.clone(),
                        node_classes: node_classes.clone(),
                        syntax_validated: false, // Will be validated separately
                    };
                    
                    // Map each node class to this file
                    for class_name in &node_classes {
                        node_map.insert(class_name.clone(), file_info.clone());
                    }
                    
                    tracing::debug!(
                        "Found node classes {:?} in {:?}",
                        node_classes,
                        relative_path
                    );
                }
            }
        }
    }
    
    Ok(())
}

/// Extract node class names from Python source code
///
/// Looks for patterns like:
/// - `class FooNode(...):`
/// - `class FooNode:` 
/// - `@register_node("FooNode")` followed by class definition
fn extract_node_classes(source: &str) -> Vec<String> {
    let mut classes = Vec::new();
    
    // Pattern 1: class definitions ending with "Node"
    // Regex: class (\w+Node)\s*[:(]
    for line in source.lines() {
        let trimmed = line.trim();
        
        // Look for class definitions
        if trimmed.starts_with("class ") {
            // Extract class name
            if let Some(rest) = trimmed.strip_prefix("class ") {
                // Find the class name (ends at '(' or ':' or whitespace)
                let class_name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                
                // Only include classes that look like nodes (end with "Node" or contain "Node")
                if class_name.ends_with("Node") || class_name.contains("Node") {
                    classes.push(class_name);
                }
            }
        }
    }
    
    classes
}

/// Get the path to Python nodes directory
pub fn get_python_nodes_dir(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join("clients")
        .join("python")
        .join("remotemedia")
        .join("nodes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_pipeline_yaml() {
        let yaml = r#"
version: "v1"
metadata:
  name: "Test Pipeline"
nodes:
  - id: passthrough
    node_type: PassThrough
    params: {}
connections: []
"#;
        let analysis = analyze_pipeline_yaml(yaml).unwrap();
        assert!(analysis.is_valid);
        assert!(analysis.registered_types.contains(&"PassThrough".to_string()));
    }

    #[test]
    fn test_validate_missing_node() {
        let yaml = r#"
version: "v1"
metadata:
  name: "Test Pipeline"
nodes:
  - id: unknown
    node_type: NonExistentNode
    params: {}
connections: []
"#;
        let result = validate_pipeline_nodes(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_node_classes() {
        let source = r#"
class KokoroTTSNode(MultiprocessNode):
    pass

class HelperClass:
    pass

class AudioProcessorNode:
    pass
"#;
        let classes = extract_node_classes(source);
        assert!(classes.contains(&"KokoroTTSNode".to_string()));
        assert!(classes.contains(&"AudioProcessorNode".to_string()));
        assert!(!classes.contains(&"HelperClass".to_string()));
    }
}
