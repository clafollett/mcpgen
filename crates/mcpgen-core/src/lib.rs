//! MCPGen Core Library
//!
//! This library provides the core functionality for generating MCP (Model-Controller-Presenter)
//! server code from OpenAPI specifications.

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![forbid(unsafe_code)]

use std::str::FromStr;

// Re-export the Result type and Error enum from the error module
pub use config::Config;
pub use error::{Error, Result};

pub mod config;
pub mod error;
pub mod manifest;
pub mod openapi;
pub mod template;
pub mod template_kind;

use openapi::OpenAPISpec;
use template::TemplateManager;
use template_kind::Template;

/// Result type for MCP generation operations
pub type MCPResult<T> = std::result::Result<T, Error>;

/// Options for template generation
#[derive(Debug, Default, Clone)]
pub struct TemplateOptions {
    /// Whether to include all operations by default
    pub all_operations: bool,

    /// Whether to generate tests
    pub include_tests: bool,

    /// Whether to overwrite existing files
    pub overwrite: bool,

    /// Additional context to pass to templates
    pub context: Option<serde_json::Value>,

    /// Specific operations to include (overrides all_operations if not empty)
    pub include_operations: Vec<String>,

    /// Operations to exclude
    pub exclude_operations: Vec<String>,

    /// Server port for the generated application
    pub server_port: Option<u16>,

    /// Log file path for the generated application
    pub log_file: Option<String>,
}

/// Main entry point for code generation
pub async fn generate(config: &Config, template_opts: Option<TemplateOptions>) -> Result<()> {
    // 1. Load OpenAPI spec
    let spec = OpenAPISpec::from_file(&config.openapi_spec).await?;

    // 2. Initialize template manager
    let template_kind = Template::from_str(&config.template).unwrap_or_default();
    let template_manager = TemplateManager::new(template_kind, None).await?;

    // 3. Delegate to TemplateManager.generate
    template_manager.generate(&spec, config, template_opts).await?;

    Ok(())
}
