//! agenterra CLI entrypoint
//! Parses command-line arguments and dispatches to the core generator.
#![deny(unsafe_code)]
mod core;
mod mcp;

// Internal imports (std, crate)
use core::{
    openapi::OpenApiContext,
    protocol::Protocol,
    templates::{
        ClientTemplateKind, ServerTemplateKind, TemplateManager, TemplateOptions,
        dir::resolve_output_dir,
    },
};
use std::path::PathBuf;

// External imports (alphabetized)
use anyhow::Context;
use clap::Parser;
use reqwest::Url;
use tracing::{Level, error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "agenterra")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Scaffold servers and clients for various targets
    Scaffold {
        #[command(subcommand)]
        target: TargetCommands,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum TargetCommands {
    /// Model Context Protocol (MCP) servers and clients
    Mcp {
        #[command(subcommand)]
        role: McpCommands,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum McpCommands {
    /// Generate MCP server from OpenAPI specification that exposes API endpoints as MCP tools
    Server {
        /// Project name for the generated MCP server
        #[arg(long, default_value = "agenterra_mcp_server")]
        project_name: String,
        /// Path or URL to OpenAPI schema (YAML or JSON)
        #[arg(long)]
        schema_path: String,
        /// Template to use for code generation
        #[arg(long, default_value = "rust_axum")]
        template: String,
        /// Custom template directory
        #[arg(long)]
        template_dir: Option<PathBuf>,
        /// Output directory for generated code
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Log file name without extension
        #[arg(long)]
        log_file: Option<String>,
        /// Server port
        #[arg(long)]
        port: Option<u16>,
        /// Base URL of the OpenAPI specification
        #[arg(long)]
        base_url: Option<Url>,
    },
    /// Generate MCP client that can connect to MCP servers (no OpenAPI spec required)
    Client {
        /// Project name for the generated MCP client
        #[arg(long, default_value = "agenterra_mcp_client")]
        project_name: String,
        /// Template to use for client generation
        #[arg(long, default_value = "rust_reqwest")]
        template: String,
        /// Custom template directory
        #[arg(long)]
        template_dir: Option<PathBuf>,
        /// Output directory for generated code
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging with default level INFO
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    info!("Starting Agenterra CLI");
    let cli = Cli::parse();
    match &cli.command {
        Commands::Scaffold { target } => match target {
            TargetCommands::Mcp { role } => match role {
                McpCommands::Server {
                    project_name,
                    schema_path,
                    template,
                    template_dir,
                    output_dir,
                    log_file,
                    port,
                    base_url,
                } => {
                    generate_mcp_server(ServerGenParams {
                        project_name,
                        schema_path,
                        template,
                        template_dir,
                        output_dir,
                        log_file,
                        port,
                        base_url,
                    })
                    .await?
                }
                McpCommands::Client {
                    project_name,
                    template,
                    template_dir,
                    output_dir,
                } => generate_mcp_client(project_name, template, template_dir, output_dir).await?,
            },
        },
    }
    Ok(())
}

/// Parameters for MCP server generation
struct ServerGenParams<'a> {
    project_name: &'a str,
    schema_path: &'a str,
    template: &'a str,
    template_dir: &'a Option<PathBuf>,
    output_dir: &'a Option<PathBuf>,
    log_file: &'a Option<String>,
    port: &'a Option<u16>,
    base_url: &'a Option<Url>,
}

/// Generate MCP server from OpenAPI specification
async fn generate_mcp_server(params: ServerGenParams<'_>) -> anyhow::Result<()> {
    info!(
        template = %params.template,
        "Generating MCP server"
    );

    // Parse template
    let template_kind_enum: ServerTemplateKind = params
        .template
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid server template '{}': {}", params.template, e))?;

    // Resolve output directory with workspace-aware defaults
    let output_path = resolve_output_dir(params.project_name, params.output_dir.as_deref())
        .context("Failed to resolve output directory")?;

    // Initialize the template manager with MCP protocol
    let template_manager = TemplateManager::new_with_protocol(
        Protocol::Mcp,
        template_kind_enum,
        params.template_dir.clone(),
    )
    .await
    .context("Failed to initialize server template manager")?;

    // Load and validate OpenAPI schema BEFORE creating directories
    let schema_obj = OpenApiContext::from_file_or_url(params.schema_path)
        .await
        .context("Failed to load OpenAPI schema")?;

    // Create output directory only after all validations pass
    if !output_path.exists() {
        info!(path = %output_path.display(), "Creating output directory");
        tokio::fs::create_dir_all(&output_path).await.map_err(|e| {
            error!(path = %output_path.display(), error = %e, "Failed to create output directory");
            anyhow::anyhow!("Failed to create output directory: {}", e)
        })?
    }

    // Create config
    let config = crate::core::config::Config {
        project_name: params.project_name.to_string(),
        openapi_schema_path: params.schema_path.to_string(),
        output_dir: output_path.to_string_lossy().to_string(),
        template_kind: params.template.to_string(),
        template_dir: params
            .template_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        include_all: true,
        include_operations: Vec::new(),
        exclude_operations: Vec::new(),
        base_url: params.base_url.clone(),
    };

    // Create template options
    let template_opts = TemplateOptions {
        server_port: *params.port,
        log_file: params.log_file.clone(),
        ..Default::default()
    };

    // Generate the server code
    info!("Generating MCP server code...");
    template_manager
        .generate(&schema_obj, &config, Some(template_opts))
        .await
        .map_err(|e| {
            error!("Failed to generate server code: {}", e);
            anyhow::anyhow!("Failed to generate server code: {}", e)
        })?;

    info!(
        output_path = %output_path.display(),
        "Successfully generated MCP server"
    );
    Ok(())
}

/// Generate MCP client
async fn generate_mcp_client(
    project_name: &str,
    template: &str,
    template_dir: &Option<PathBuf>,
    output_dir: &Option<PathBuf>,
) -> anyhow::Result<()> {
    info!(
        template = %template,
        "Generating MCP client"
    );

    // Parse and validate template
    let template_kind_enum: ClientTemplateKind = template
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid client template '{}': {}", template, e))?;

    // Resolve output directory with workspace-aware defaults
    let output_path = resolve_output_dir(project_name, output_dir.as_deref())
        .context("Failed to resolve output directory")?;

    // Initialize template manager for the chosen client template with MCP protocol
    let template_manager = TemplateManager::new_client_with_protocol(
        Protocol::Mcp,
        template_kind_enum,
        template_dir.clone(),
    )
    .await?;

    // Build a core config (no OpenAPI schema needed for clients)
    let core_config = crate::core::config::Config {
        project_name: project_name.to_string(),
        openapi_schema_path: String::new(),
        output_dir: output_path.to_string_lossy().to_string(),
        template_kind: template_kind_enum.as_str().to_string(),
        template_dir: template_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        include_all: true,
        include_operations: Vec::new(),
        exclude_operations: Vec::new(),
        base_url: None,
    };

    // Generate the client directly via TemplateManager
    info!("Generating MCP client code...");
    template_manager.generate_client(&core_config, None).await?;

    info!(
        output_path = %output_path.display(),
        "Successfully generated MCP client"
    );
    Ok(())
}
