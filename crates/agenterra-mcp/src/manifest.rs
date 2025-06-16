//! Manifest file format for Agenterra templates.
//!
//! This module defines the structure of the `template.yaml` file that describes
//! how to generate code from templates.

use serde::{Deserialize, Deserializer, Serialize};
use serde_value::Value as SerdeValue;
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
    /// Commands to run before code generation
    #[serde(default, deserialize_with = "deserialize_commands")]
    pub pre_generate: Vec<String>,

    /// Commands to run after code generation
    #[serde(default, deserialize_with = "deserialize_commands")]
    pub post_generate: Vec<String>,
}

impl Default for TemplateManifest {
    fn default() -> Self {
        Self {
            name: String::from("default"),
            description: String::from("Default template"),
            version: String::from("0.1.0"),
            language: String::from("rust"),
            files: Vec::new(),
            hooks: TemplateHooks::default(),
        }
    }
}

impl Default for TemplateFile {
    fn default() -> Self {
        Self {
            source: String::new(),
            destination: String::new(),
            for_each: None,
            context: serde_json::Value::Null,
        }
    }
}

impl TemplateManifest {
    /// Load a template manifest from a directory.
    ///
    /// Looks for a `manifest.yaml` file in the specified directory and parses it.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist, can't be read, or contains invalid YAML.
    pub async fn load_from_dir(
        template_dir: &std::path::Path,
    ) -> Result<Self, agenterra_core::Error> {
        let manifest_path = template_dir.join("manifest.yaml");

        println!(
            "DEBUG - Attempting to read manifest from: {}",
            manifest_path.display()
        );
        // Read the file content and log it for debugging
        let content = fs::read_to_string(&manifest_path).await.map_err(|e| {
            agenterra_core::Error::Template(format!(
                "Failed to read template manifest at full path {}: {}",
                manifest_path.display(),
                e
            ))
        })?;

        // Log the content for debugging
        println!(
            "=== Template manifest content ===\n{}\n===============================",
            content
        );

        // Try to parse the YAML content
        let manifest: Self = serde_yaml::from_str(&content).map_err(|e| {
            agenterra_core::Error::Template(format!(
                "Invalid YAML in template manifest at {}: {}\nContent:\n{}",
                manifest_path.display(),
                e,
                content
            ))
        })?;

        Ok(manifest)
    }
}

/// Helper function to deserialize either a single command or a list of commands
fn deserialize_commands<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // Try to deserialize as a single string or a vector of strings
    let value = SerdeValue::deserialize(deserializer)?;

    match value {
        SerdeValue::String(s) => Ok(vec![s.to_owned()]),
        SerdeValue::Seq(seq) => {
            let mut result = Vec::new();
            for item in seq {
                if let SerdeValue::String(s) = item {
                    result.push(s.to_owned());
                } else {
                    return Err(serde::de::Error::custom(
                        "Expected string or array of strings",
                    ));
                }
            }
            Ok(result)
        }
        _ => Err(serde::de::Error::custom(
            "Expected string or array of strings",
        )),
    }
}
