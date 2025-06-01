//! OpenAPI specification parsing and utilities.
//!
//! This module provides functionality for loading and querying OpenAPI specifications.
//! It supports loading from files and provides convenient accessors for common fields.
//!
//! # Examples
//!
//! ```no_run
//! use mcpgen_core::openapi::OpenAPISpec;
//! use mcpgen_core::error::Result;
//!
//! # fn main() -> Result<()> {
//! // Load an OpenAPI spec from a file
//! let spec = OpenAPISpec::from_file("openapi.json").await?;
//!
//! // Access common fields
//! if let Some(title) = spec.title() {
//!     println!("API Title: {}", title);
//! }
//! if let Some(version) = spec.version() {
//!     println!("API Version: {}", version);
//! }
//! # Ok(())
//! # }
//! ```

use serde_json::Value as JsonValue;
use std::path::Path;
use tokio::fs;

/// Represents an OpenAPI specification
#[derive(Debug)]
pub struct OpenAPISpec {
    /// The raw JSON value of the OpenAPI spec
    pub json: JsonValue,
}

impl OpenAPISpec {
    /// Create a new OpenAPISpec from a file (supports both YAML and JSON)
    pub async fn from_file<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).await?;
        
        // Try to parse as JSON first
        if let Ok(json) = serde_json::from_str(&content) {
            return Ok(Self { json });
        }
        
        // If JSON parsing fails, try YAML
        if let Ok(json) = serde_yaml::from_str(&content) {
            return Ok(Self { json });
        }
        
        // If both parsers fail, return an error
        Err(crate::Error::openapi(format!(
            "Failed to parse OpenAPI spec at {}: invalid JSON or YAML",
            path.display()
        )))
    }

    /// Get a reference to the raw JSON value
    pub fn as_json(&self) -> &JsonValue {
        &self.json
    }

    /// Get the title of the API
    pub fn title(&self) -> Option<&str> {
        self.json.get("info")?.get("title")?.as_str()
    }

    /// Get the version of the API
    pub fn version(&self) -> Option<&str> {
        self.json.get("info")?.get("version")?.as_str()
    }

    /// Get the base path of the API
    pub fn base_path(&self) -> Option<&str> {
        self.json.get("servers")?
            .as_array()?
            .first()?
            .get("url")?
            .as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_from_file() -> crate::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("openapi_async.json");
        let json_content = r#"
        {
            "openapi": "3.0.0",
            "info": {
                "title": "Test API Async",
                "version": "2.0.0"
            },
            "servers": [
                {
                    "url": "https://api.example.com/v2"
                }
            ]
        }
        "#;
        tokio::fs::write(&file_path, json_content).await?;

        let spec = OpenAPISpec::from_file(&file_path).await?;
        assert_eq!(spec.title(), Some("Test API Async"));
        assert_eq!(spec.version(), Some("2.0.0"));
        assert_eq!(spec.base_path(), Some("https://api.example.com/v2"));
        
        Ok(())
    }
}
