//! mcpgen CLI entrypoint
//! Parses command-line arguments and dispatches to the core generator.

// Internal imports (std, crate)
use std::path::{Path, PathBuf};

// External imports (alphabetized)
use anyhow::Context;
use clap::Parser;
use mcpgen_core::{
    openapi::OpenAPISpec,
    template::TemplateManager,
    template_kind::Template,
    TemplateOptions,
};
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

            // Determine the template directory, honoring --template-dir if provided
            let template_dir_path = if let Some(dir) = template_dir.clone() {
                dir
            } else if template_kind == Template::Custom {
                PathBuf::from("./templates")
            } else {
                // For built-in templates, use workspace templates/<template>
                let manifest_dir = env!("CARGO_MANIFEST_DIR");
                let workspace_root = Path::new(manifest_dir)
                    .parent()
                    .and_then(Path::parent)
                    .ok_or_else(|| anyhow::anyhow!(
                        "Failed to determine workspace root from CARGO_MANIFEST_DIR"
                    ))?;
                let templates_dir = workspace_root.join("templates");
                let built_in_dir = templates_dir.join(template_kind.as_str());
                println!("DEBUG - Full template directory: {}", built_in_dir.display());
                built_in_dir
            };

            println!("Using template directory: {}", template_dir_path.display());

            // For custom templates, ensure the directory exists
            if template_kind == Template::Custom && !template_dir_path.exists() {
                fs::create_dir_all(&template_dir_path)
                    .await
                    .context("Failed to create template directory")?;
                println!("Created template directory at: {}", template_dir_path.display());
            }

            // Initialize the template manager using the resolved template directory
            let template_manager = TemplateManager::new(
                template_kind,
                Some(template_dir_path.clone()),
            )
            .await
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

            // Ensure output directory and all required subdirectories exist
            println!("Creating output directory: {}", output.display());
            fs::create_dir_all(&output)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create output directory: {}", e))?;
            
            // Create directories for all template file destinations
            for file in &template_manager.manifest.files {
                if let Some(parent) = Path::new(&file.destination).parent() {
                    let dir = output.join(parent);
                    if !dir.exists() {
                        println!("Creating directory: {}", dir.display());
                        fs::create_dir_all(&dir).await.map_err(|e| {
                            anyhow::anyhow!("Failed to create directory {}: {}", dir.display(), e)
                        })?;
                    }
                }
            }

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

            // Load OpenAPI spec and generate code
            let spec_obj = OpenAPISpec::from_file(&config.openapi_spec).await?;
            if let Err(e) = template_manager
                .generate(&spec_obj, &config, Some(template_opts))
                .await
            {
                eprintln!("Codegen failed: {e}");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
