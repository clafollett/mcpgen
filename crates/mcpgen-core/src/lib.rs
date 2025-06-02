//! MCPGen Core Library
//!
//! This library provides the core functionality for generating MCP (Model-Controller-Presenter)
//! server code from OpenAPI specifications.

// Internal imports (std, crate)
use std::str::FromStr;

// Module declarations (alphabetized)
pub mod config;
pub mod error;
pub mod manifest;
pub mod openapi;
pub mod template;
pub mod template_manager;
pub mod template_options;

// Internal crate imports (alphabetized)
use openapi::OpenAPISpec;
use template::Template;
use template_manager::TemplateManager;

// Re-exports (alphabetized)
pub use config::Config;
pub use error::{Error, Result};
pub use template_options::TemplateOptions;

/// Result type for MCP generation operations
pub type MCPResult<T> = std::result::Result<T, Error>;

/// Main entry point for code generation
pub async fn generate(config: &Config, template_opts: Option<TemplateOptions>) -> Result<()> {
    // 1. Load OpenAPI spec
    let spec = OpenAPISpec::from_file(&config.openapi_spec).await?;

    // 2. Initialize template manager
    let template_kind = Template::from_str(&config.template).unwrap_or_default();
    let template_manager = TemplateManager::new(template_kind, None).await?;

    // 3. Delegate to TemplateManager.generate
    template_manager
        .generate(&spec, config, template_opts)
        .await?;

    Ok(())
}
