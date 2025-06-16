//! Agenterra MCP Server and Client Generation
//!
//! This library provides functionality for generating MCP (Model Context Protocol)
//! servers and clients from OpenAPI specifications.

pub mod builders;
pub mod generate;
pub mod manifest;
pub mod templates;

// Re-exports
pub use generate::{ClientConfig, generate, generate_client};
pub use templates::{ClientTemplateKind, ServerTemplateKind, TemplateManager, TemplateOptions};
