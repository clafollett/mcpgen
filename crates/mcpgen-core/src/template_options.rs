//! Configuration options for template-based code generation in MCPGen.
//!
//! This module provides the [`TemplateOptions`] struct which controls how code is generated
//! from templates. It allows fine-grained control over:
//!
//! - Operation filtering (include/exclude specific operations)
//! - Test generation
//! - File overwrite behavior
//! - Custom template context injection
//! - Server configuration (port, logging)
//!
//! # Example
//!
//! ```rust
//! use mcpgen_core::template_options::TemplateOptions;
//!
//! let options = TemplateOptions {
//!     all_operations: true,
//!     include_tests: true,
//!     overwrite: false,
//!     server_port: Some(8080),
//!     ..Default::default()
//! };
//! ```
//!
// Re-exports (alphabetized)
pub use serde_json::Value as JsonValue;

/// Configuration struct for controlling template-based code generation.
///
/// Provides options to customize which operations are included, whether to generate tests,
/// file overwrite behavior, and additional template context.
#[derive(Debug, Default, Clone)]
pub struct TemplateOptions {
    /// Whether to include all operations by default
    pub all_operations: bool,

    /// Whether to generate tests
    pub include_tests: bool,

    /// Whether to overwrite existing files
    pub overwrite: bool,

    /// Additional context to pass to templates
    pub agent_instructions: Option<JsonValue>,

    /// Specific operations to include (overrides all_operations if not empty)
    pub include_operations: Vec<String>,

    /// Operations to exclude
    pub exclude_operations: Vec<String>,

    /// Server port for the generated application
    pub server_port: Option<u16>,

    /// Log file path for the generated application
    pub log_file: Option<String>,
}
