//! Code generation functionality for Agenterra

use std::{path::PathBuf, str::FromStr};

use crate::templates::{ClientTemplateKind, ServerTemplateKind, TemplateManager, TemplateOptions};
use agenterra_core::{config::Config, error::Result, openapi::OpenApiContext};

/// Generates MCP server code from an OpenAPI specification.
///
/// This is the main entry point for Agenterra's code generation functionality.
/// It loads the OpenAPI schema, initializes the appropriate template system,
/// and generates the complete MCP server code structure.
///
/// # Arguments
/// * `config` - Configuration containing schema path, output directory, and template settings
/// * `template_opts` - Optional template-specific options for customizing generation
///
/// # Returns
/// `Result<()>` indicating success or failure of the generation process
///
/// # Errors
/// This function will return an error if:
/// - The OpenAPI schema file cannot be loaded or parsed
/// - The specified template kind is invalid or unavailable
/// - The output directory cannot be created or written to
/// - Template rendering fails due to invalid schema or template errors
///
/// # Examples
/// ```no_run
/// use agenterra_mcp::{generate::generate};
/// use agenterra_core::Config;
///
/// # async fn example() -> agenterra_core::Result<()> {
/// let config = Config {
///     project_name: "my_server".to_string(),
///     openapi_schema_path: "./petstore.json".to_string(),
///     output_dir: "./generated".to_string(),
///     template_kind: "rust_axum".to_string(),
///     template_dir: None,
///     include_all: true,
///     include_operations: Vec::new(),
///     exclude_operations: Vec::new(),
///     base_url: None,
/// };
///
/// generate(&config, None).await?;
/// # Ok(())
/// # }
/// ```
pub async fn generate(config: &Config, template_opts: Option<TemplateOptions>) -> Result<()> {
    // 1. Load OpenAPI schema
    let schema = OpenApiContext::from_file(&config.openapi_schema_path).await?;

    // 2. Initialize template manager with template_dir from config if available
    let template_kind = ServerTemplateKind::from_str(&config.template_kind).unwrap_or_default();
    let template_dir = config.template_dir.as_ref().map(PathBuf::from);
    let template_manager = TemplateManager::new(template_kind, template_dir).await?;

    // 3. Delegate to TemplateManager.generate
    template_manager
        .generate(&schema, config, template_opts)
        .await?;

    Ok(())
}

/// Configuration for client generation
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Name of the client project
    pub project_name: String,
    /// Output directory for generated client code
    pub output_dir: String,
    /// Template kind for client generation
    pub template_kind: String,
    /// Optional custom template directory
    pub template_dir: Option<String>,
}

/// Generates MCP client code from a template.
///
/// This function creates an MCP client that can connect to MCP servers
/// and discover tools at runtime. Unlike server generation, client generation
/// does not require an OpenAPI schema since tools are discovered dynamically.
///
/// # Arguments
/// * `config` - Configuration containing project name, output directory, and template settings
/// * `template_opts` - Optional template-specific options for customizing generation
///
/// # Returns
/// `Result<()>` indicating success or failure of the generation process
///
/// # Errors
/// This function will return an error if:
/// - The specified template kind is invalid or unavailable
/// - The output directory cannot be created or written to
/// - Template rendering fails due to template errors
///
/// # Examples
/// ```no_run
/// use agenterra_mcp::{generate_client, ClientConfig, TemplateOptions};
///
/// # async fn example() -> agenterra_core::Result<()> {
/// let config = ClientConfig {
///     project_name: "my_mcp_client".to_string(),
///     output_dir: "./client".to_string(),
///     template_kind: "rust_reqwest".to_string(),
///     template_dir: None,
/// };
///
/// generate_client(&config, None).await?;
/// # Ok(())
/// # }
/// ```
pub async fn generate_client(
    config: &ClientConfig,
    template_opts: Option<TemplateOptions>,
) -> Result<()> {
    // 1. Parse template kind
    let template_kind = ClientTemplateKind::from_str(&config.template_kind).unwrap_or_default();
    let template_dir = config.template_dir.as_ref().map(PathBuf::from);

    // 2. Initialize template manager for client generation
    let template_manager = TemplateManager::new_client(template_kind, template_dir).await?;

    // 3. Create core config for client generation (no OpenAPI needed)
    let core_config = agenterra_core::Config {
        project_name: config.project_name.clone(),
        openapi_schema_path: String::new(), // Not needed for client generation
        output_dir: config.output_dir.clone(),
        template_kind: config.template_kind.clone(),
        template_dir: config.template_dir.clone(),
        include_all: true,
        include_operations: Vec::new(),
        exclude_operations: Vec::new(),
        base_url: None,
    };

    // 4. Generate client code (no OpenAPI context needed)
    template_manager
        .generate_client(&core_config, template_opts)
        .await?;

    Ok(())
}
