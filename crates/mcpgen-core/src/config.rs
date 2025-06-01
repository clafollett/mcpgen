//! Configuration management for MCPGen code generation.
//!
//! This module defines the `Config` struct and related functionality for managing
//! code generation settings. The configuration can be loaded from a YAML file,
//! created programmatically, or loaded from command-line arguments.
//!
//! # Examples
//!
//! ```no_run
//! use mcpgen_core::config::Config;
//!
//! // Create a new config programmatically
//! let mut config = Config::new("openapi.yaml", "output");
//! config.template = "rust-axum".to_string();
//! config.include_all = true;
//!
//! // Or load from a config file
//! # #[cfg(feature = "yaml")]
//! # {
//! # let config = Config::from_file("mcpgen.yaml").unwrap();
//! # }
//! ```

// Internal imports (std, crate)
use std::path::Path;

// External imports (alphabetized)
use serde::{Deserialize, Serialize};
use tokio::fs;

/// Configuration for MCP server generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the OpenAPI specification file
    pub openapi_spec: String,

    /// Output directory for generated code
    pub output_dir: String,

    /// Template to use for code generation
    #[serde(default = "default_template")]
    pub template: String,

    /// Whether to include all operations by default
    #[serde(default)]
    pub include_all: bool,

    /// List of operations to include (if include_all is false)
    #[serde(default)]
    pub include_operations: Vec<String>,

    /// List of operations to exclude
    #[serde(default)]
    pub exclude_operations: Vec<String>,
}

impl Config {
    /// Create a new Config with default values
    pub fn new(openapi_spec: impl Into<String>, output_dir: impl Into<String>) -> Self {
        Self {
            openapi_spec: openapi_spec.into(),
            output_dir: output_dir.into(),
            template: default_template(),
            include_all: false,
            include_operations: Vec::new(),
            exclude_operations: Vec::new(),
        }
    }

    /// Load configuration from a file
    pub async fn from_file<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let content = fs::read_to_string(path).await?;
        let config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a file
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> crate::Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content).await?;
        Ok(())
    }
}

fn default_template() -> String {
    "rust-axum".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_config_roundtrip() -> crate::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("config.yaml");

        let config = Config::new("openapi.json", "output");
        config.save(&file_path).await?;

        let loaded = Config::from_file(&file_path).await?;
        assert_eq!(config.openapi_spec, loaded.openapi_spec);
        assert_eq!(config.output_dir, loaded.output_dir);
        assert_eq!(config.template, loaded.template);

        Ok(())
    }
}
