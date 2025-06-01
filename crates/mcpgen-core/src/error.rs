//! Error handling for the MCPGen code generation library.
//!
//! This module defines the main error type `Error` used throughout the library,
//! along with a convenient `Result` type alias. It uses `thiserror` for easy
//! error handling and implements conversions from common error types.
//!
//! # Examples
//!
//! ```
//! use mcpgen_core::error::{Error, Result};
//!
//! fn might_fail() -> Result<()> {
//!     // Operations that might fail...
//!     Ok(())
//! }
//! ```

use thiserror::Error;

/// Result type for MCP generation operations
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for MCP generation operations
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parsing error
    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    /// OpenAPI error
    #[error("OpenAPI error: {0}")]
    OpenApi(String),

    /// Template error
    #[error("Template error: {0}")]
    Template(String),

    /// Template engine error
    #[error("Template engine error: {0}")]
    Tera(#[from] tera::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
}

impl Error {
    /// Create a new configuration error
    pub fn config<S: Into<String>>(msg: S) -> Self {
        Self::Config(msg.into())
    }

    /// Create a new OpenAPI error
    pub fn openapi<S: Into<String>>(msg: S) -> Self {
        Self::OpenApi(msg.into())
    }

    /// Create a new template error
    pub fn template<S: Into<String>>(msg: S) -> Self {
        Self::Template(msg.into())
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self::Config(s.to_string())
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Config(s)
    }
}
