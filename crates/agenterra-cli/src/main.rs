//! agenterra CLI entrypoint
//! Parses command-line arguments and dispatches to the core generator.

// Internal imports (std, crate)
use reqwest::Url;
use std::path::PathBuf;

// External imports (alphabetized)
use agenterra_mcp::{
    ClientConfig, ClientTemplateKind, ServerTemplateKind, TemplateManager, TemplateOptions,
    generate_client,
};
use anyhow::Context;
use clap::Parser;

#[derive(Parser)]
#[command(name = "agenterra")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Scaffold MCP servers and clients
    Scaffold {
        #[command(subcommand)]
        protocol: ProtocolCommands,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum ProtocolCommands {
    /// Model Context Protocol (MCP) scaffolding
    Mcp {
        #[command(subcommand)]
        role: McpRoleCommands,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum McpRoleCommands {
    /// Generate MCP server from OpenAPI specification
    Server {
        /// Project name
        #[arg(long, default_value = "mcp_server")]
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
    /// Generate MCP client
    Client {
        /// Project name
        #[arg(long, default_value = "mcp_client")]
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
    // Initialize logging
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match &cli.command {
        Commands::Scaffold { protocol } => match protocol {
            ProtocolCommands::Mcp { role } => match role {
                McpRoleCommands::Server {
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
                McpRoleCommands::Client {
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
    println!(
        "🚀 Generating MCP server with template: {}",
        params.template
    );

    // Parse template
    let template_kind_enum: ServerTemplateKind = params
        .template
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid server template '{}': {}", params.template, e))?;

    // Resolve output directory - use project_name if not specified
    let output_path = params
        .output_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(params.project_name));

    // Initialize the template manager
    let template_manager = TemplateManager::new(template_kind_enum, params.template_dir.clone())
        .await
        .context("Failed to initialize server template manager")?;

    // Create output directory if it doesn't exist
    if !output_path.exists() {
        println!("📁 Creating output directory: {}", output_path.display());
        tokio::fs::create_dir_all(&output_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create output directory: {}", e))?;
    }

    // Load OpenAPI schema
    println!("📖 Loading OpenAPI schema from: {}", params.schema_path);
    let schema_obj = load_openapi_schema(params.schema_path).await?;

    // Create config
    let config = agenterra_core::Config {
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

    // Generate the server
    template_manager
        .generate(&schema_obj, &config, Some(template_opts))
        .await?;

    println!(
        "✅ Successfully generated MCP server in: {}",
        output_path.display()
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
    println!("🚀 Generating MCP client with template: {}", template);

    // Parse and validate template
    let template_kind_enum: ClientTemplateKind = template
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid client template '{}': {}", template, e))?;

    // Resolve output directory - use project_name if not specified
    let output_path = output_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(project_name));

    // Create client config
    let client_config = ClientConfig {
        project_name: project_name.to_string(),
        output_dir: output_path.to_string_lossy().to_string(),
        template_kind: template_kind_enum.as_str().to_string(),
        template_dir: template_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
    };

    // Generate the client
    generate_client(&client_config, None).await?;

    println!(
        "✅ Successfully generated MCP client in: {}",
        output_path.display()
    );
    Ok(())
}

/// Load OpenAPI schema from file or URL
async fn load_openapi_schema(
    schema_path: &str,
) -> anyhow::Result<agenterra_core::openapi::OpenApiContext> {
    if schema_path.starts_with("http://") || schema_path.starts_with("https://") {
        // It's a URL
        let response = reqwest::get(schema_path).await.map_err(|e| {
            anyhow::anyhow!("Failed to fetch OpenAPI schema from {}: {}", schema_path, e)
        })?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch OpenAPI schema from {}: HTTP {}",
                schema_path,
                response.status()
            ));
        }

        let content = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response from {}: {}", schema_path, e))?;

        // Save to temporary file
        let temp_dir = tempfile::tempdir()?;
        let temp_file = temp_dir.path().join("openapi_schema.json");
        tokio::fs::write(&temp_file, &content).await?;

        agenterra_core::openapi::OpenApiContext::from_file(&temp_file)
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to parse OpenAPI schema from {}: {}", schema_path, e)
            })
    } else {
        // It's a file path
        agenterra_core::openapi::OpenApiContext::from_file(schema_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load OpenAPI schema: {}", e))
    }
}
