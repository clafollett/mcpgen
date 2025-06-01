//! mcpgen CLI entrypoint
//! Parses command-line arguments and dispatches to the core generator.

use anyhow::Context;
use clap::Parser;
use mcpgen_core::TemplateOptions;
use mcpgen_core::template::TemplateManager;
use mcpgen_core::template_kind::Template;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Parser)]
#[command(name = "mcpgen")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    // TODO: Add future subcommands here (e.g., Validate, ListTemplates, etc.)
    /// Scaffold a new MCP server from an OpenAPI spec
    Scaffold {
        /// Path to OpenAPI spec file (YAML or JSON)
        #[arg(long)]
        spec: PathBuf,
        /// Output directory for generated code
        #[arg(long)]
        output: PathBuf,
        /// Template to use for code generation (e.g., rust-axum, python-fastapi)
        #[arg(short, long, default_value = "rust-axum")]
        template: String,
        /// Custom template directory (only used with --template=custom)
        #[arg(long)]
        template_dir: Option<PathBuf>,
        /// Comma-separated list of policy plugins
        #[arg(long)]
        policy_plugins: Option<String>,
        /// Server port (default: 3000)
        #[arg(long)]
        port: Option<u16>,
        /// Log file name without extension (default: mcp-server)
        #[arg(long)]
        log_file: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match &cli.command {
        Commands::Scaffold {
            spec,
            output,
            template,
            policy_plugins: _,
            port,
            log_file,
            template_dir,
        } => {
            // Parse template
            let template_kind: Template = template
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid template '{}': {}", template, e))?;

            println!("Generating server with template: {}", template_kind);

            // Log the template being used for code generation
            println!(
                "Generating server from OpenAPI spec using template: {}",
                template_kind
            );

            // Get the template directory
            let template_dir_path = if template_kind == Template::Custom {
                // For custom templates, use the provided directory or default to ./templates
                template_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("./templates"))
            } else {
                // For built-in templates, use the workspace templates directory
                let template_dir = env!("CARGO_MANIFEST_DIR");
                let manifest_dir = PathBuf::from(template_dir);

                // Go up to the workspace root (from crates/mcpgen-cli -> mcpgen)
                manifest_dir
                    .parent() // crates
                    .and_then(Path::parent) // workspace root
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Failed to determine workspace root from CARGO_MANIFEST_DIR"
                        )
                    })?
                    .join("templates")
            };

            println!("Using template directory: {}", template_dir_path.display());

            // For custom templates, ensure the directory exists
            if template_kind == Template::Custom && !template_dir_path.exists() {
                fs::create_dir_all(&template_dir_path)
                    .await
                    .context("Failed to create template directory")?;
                println!(
                    "Created template directory at: {}",
                    template_dir_path.display()
                );
            }

            // Initialize template manager with the specified template
            let template_manager = if template_kind == Template::Custom {
                TemplateManager::new(template_kind, Some(template_dir_path)).await
            } else {
                TemplateManager::new(template_kind, None).await
            }
            .context("Failed to initialize template manager")?;

            // List available templates for debugging
            println!("Available templates:");
            for template in template_manager.list_templates() {
                println!("  - {}", template);
            }

            println!(
                "Using templates from: {}",
                template_manager.template_dir().display()
            );

            // Ensure output directory exists
            fs::create_dir_all(&output)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create output directory: {}", e))?;

            // Create template options with default values
            let template_opts = TemplateOptions {
                server_port: *port,
                log_file: log_file.clone(),
                ..Default::default()
            };

            // Create config with template
            let config = mcpgen_core::Config {
                openapi_spec: spec.to_string_lossy().to_string(),
                output_dir: output.to_string_lossy().to_string(),
                template: template.to_string(),
                include_all: true,              // Include all operations by default
                include_operations: Vec::new(), // No specific operations to include
                exclude_operations: Vec::new(), // No operations to exclude
            };

            // Generate the code using our template manager
            if let Err(e) = mcpgen_core::generate_with_template_manager(
                &config,
                template_manager,
                template_opts,
            )
            .await
            {
                eprintln!("Codegen failed: {e}");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
