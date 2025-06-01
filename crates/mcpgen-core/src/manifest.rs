//! Manifest file format for MCPGen templates.
//!
//! This module defines the structure of the `template.yaml` file that describes
//! how to generate code from templates.

use serde::{Deserialize, Serialize};
use tokio::fs;

/// The root manifest structure for a template.
///
/// This describes the template's metadata and the files it contains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateManifest {
    /// The name of the template
    pub name: String,
    
    /// A short description of what the template generates
    pub description: String,
    
    /// The version of the template (should follow semantic versioning)
    pub version: String,
    
    /// The target programming language (e.g., "rust", "typescript")
    pub language: String,
    
    /// List of files to generate
    pub files: Vec<TemplateFile>,
    
    /// Optional hooks that run before/after generation
    #[serde(default)]
    pub hooks: TemplateHooks,
}

/// Describes a single file to be generated from a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFile {
    /// Path to the template file, relative to the template directory
    pub source: String,
    
    /// Destination path for the generated file, relative to the output directory
    pub destination: String,
    
    /// Optional directive for generating multiple files (e.g., "operation")
    #[serde(default)]
    pub for_each: Option<String>,
    
    /// Additional context to pass to the template
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Hooks that run at specific points during code generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemplateHooks {
    /// Command to run before generation starts
    pub pre_generate: Option<String>,
    
    /// Command to run after generation completes
    pub post_generate: Option<String>,
}

impl TemplateManifest {
    /// Load a template manifest from a directory.
    ///
    /// Looks for a `template.yaml` file in the specified directory and parses it.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist, can't be read, or contains invalid YAML.
    pub async fn load_from_dir(template_dir: &std::path::Path) -> Result<Self, crate::Error> {
        let manifest_path = template_dir.join("template.yaml");
        
        // Read the file content and log it for debugging
        let content = fs::read_to_string(&manifest_path).await.map_err(|e| {
            crate::Error::Template(format!(
                "Failed to read template manifest at {}: {}",
                manifest_path.display(),
                e
            ))
        })?;
        
        // Log the content for debugging
        println!("=== Template manifest content ===\n{}\n===============================", content);
        
        // Try to parse the YAML content
        let manifest: Self = serde_yaml::from_str(&content).map_err(|e| {
            crate::Error::Template(format!(
                "Invalid YAML in template manifest at {}: {}\nContent:\n{}",
                manifest_path.display(),
                e,
                content
            ))
        })?;
        
        Ok(manifest)
    }
}
