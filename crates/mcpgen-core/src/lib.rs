//! MCPGen Core Library
//!
//! This library provides the core functionality for generating MCP (Model-Controller-Presenter)
//! server code from OpenAPI specifications.

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![forbid(unsafe_code)]

use serde_json::{Value as JsonValue, json};
use std::path::PathBuf;
use std::str::FromStr;

// Re-export the Result type and Error enum from the error module
pub use config::Config;
pub use error::{Error, Result};

pub mod config;
pub mod error;
pub mod generator;
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
    pub context: Option<JsonValue>,

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
/// Generate code using a pre-initialized TemplateManager
pub async fn generate_with_template_manager(
    config: &Config,
    template_manager: TemplateManager,
    _template_opts: TemplateOptions, // Prefix with underscore to indicate it's intentionally unused for now
) -> Result<()> {
    // 1. Load OpenAPI spec to validate it exists and is valid
    let _spec = OpenAPISpec::from_file(&config.openapi_spec).await?;

    // 2. Create output directory
    let output_path = PathBuf::from(&config.output_dir);
    tokio::fs::create_dir_all(&output_path).await?;

    // 3. Generate files based on the template manifest
    for file in &template_manager.manifest.files {
        let dest_path = output_path.join(&file.destination);

        // Parent directories will be created by generate_with_context
        log::debug!("Preparing to generate file: {}", dest_path.display());

        // All source files must be Tera templates
        if !file.source.ends_with(".tera") {
            return Err(Error::Template(format!(
                "Template source file '{}' must have .tera extension",
                file.source
            )));
        }

        // Get the template name (relative path with forward slashes)
        // and ensure it matches the name used when loading the template
        let template_name = file
            .source
            .replace('\\', "/")
            .trim_start_matches("./")
            .to_string();

        log::debug!("Looking for template: {}", template_name);

        // Verify the template exists
        if !template_manager.has_template(&template_name) {
            return Err(Error::Template(format!(
                "Template '{}' not found. Available templates: {}",
                template_name,
                template_manager.list_templates().join(", ")
            )));
        }

        // Create a basic context with project name (extracted from output_dir)
        let project_name = config
            .output_dir
            .split(std::path::MAIN_SEPARATOR)
            .last()
            .unwrap_or("my_project")
            .to_string();

        // Create context with required variables for templates
        let context = serde_json::json!({
            "project_name": project_name,
            "app_name": project_name, // Used in Cargo.toml.tera
            // Add an empty handlers string to satisfy the template
            "handlers": ""
        });

        // Render the template
        template_manager
            .generate_with_context(&template_name, &context, &dest_path)
            .await?;
    }

    // 4. Run post-generation hooks if any
    if let Some(cmd) = &template_manager.manifest.hooks.post_generate {
        // Execute the command
        let status = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&output_path)
            .status()
            .await?;

        if !status.success() {
            return Err(Error::Template(format!(
                "Post-generation hook failed with status: {}",
                status
            )));
        }
    }

    Ok(())
}

/// Main entry point for code generation
pub async fn generate(config: &Config, template_opts: Option<TemplateOptions>) -> Result<()> {
    // 1. Load OpenAPI spec
    let spec = OpenAPISpec::from_file(&config.openapi_spec).await?;

    // 2. Initialize template manager
    let template_kind = Template::from_str(&config.template).unwrap_or_default();
    let template_manager = TemplateManager::new(template_kind, None).await?;

    // 3. Create output directory
    let output_path = PathBuf::from(&config.output_dir);
    tokio::fs::create_dir_all(&output_path).await?;

    // 4. Generate files based on template manifest
    for file in &template_manager.manifest.files {
        let dest_path = output_path.join(&file.destination);

        // Create parent directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // If this is a template that should be generated per operation
        if let Some("operation") = file.for_each.as_deref() {
            if let Some(paths) = spec.as_json().get("paths") {
                if let Some(paths_obj) = paths.as_object() {
                    for (path, methods) in paths_obj {
                        if let Some(methods_obj) = methods.as_object() {
                            for (method, details) in methods_obj {
                                let operation_id = details
                                    .get("operationId")
                                    .and_then(|id| id.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| {
                                        format!(
                                            "{}_{}",
                                            method.to_lowercase(),
                                            path.replace('/', "_")
                                        )
                                    });

                                // Check if we should include this operation
                                let include_operation = template_opts
                                    .as_ref()
                                    .map(|opts| {
                                        opts.all_operations
                                            || opts.include_operations.is_empty()
                                            || opts.include_operations.contains(&operation_id)
                                    })
                                    .unwrap_or(true);

                                let exclude_operation = template_opts
                                    .as_ref()
                                    .map(|opts| opts.exclude_operations.contains(&operation_id))
                                    .unwrap_or(false);

                                if include_operation && !exclude_operation {
                                    // Create operation-specific context
                                    let mut context = file.context.clone();
                                    if let serde_json::Value::Object(ref mut obj) = context {
                                        obj.insert("operation_id".to_string(), json!(operation_id));
                                        obj.insert(
                                            "method".to_string(),
                                            json!(method.to_uppercase()),
                                        );
                                        obj.insert("path".to_string(), json!(path));

                                        // Add any additional context from template options
                                        if let Some(opts) = &template_opts {
                                            if let Some(additional_ctx) = &opts.context {
                                                if let Some(additional_obj) =
                                                    additional_ctx.as_object()
                                                {
                                                    for (k, v) in additional_obj {
                                                        obj.insert(k.clone(), v.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Generate the file with the operation context
                                    template_manager
                                        .generate_with_context(&file.source, &context, &dest_path)
                                        .await?;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Generate a single file with any additional context
            let mut context = file.context.clone();
            if let Some(opts) = &template_opts {
                if let Some(additional_ctx) = &opts.context {
                    if let (Some(obj), Some(additional_obj)) =
                        (context.as_object_mut(), additional_ctx.as_object())
                    {
                        for (k, v) in additional_obj {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }

            template_manager
                .generate_with_context(&file.source, &context, &dest_path)
                .await?;
        }
    }

    // 5. Run post-generation hooks if defined
    if let Some(ref hook) = template_manager.manifest.hooks.post_generate {
        match hook.as_str() {
            "cargo_fmt" => {
                // Run cargo fmt for Rust projects
                if let Ok(mut cmd) = std::process::Command::new("cargo")
                    .args(["fmt", "--"])
                    .current_dir(&output_path)
                    .spawn()
                {
                    let _ = cmd.wait();
                }
            }
            _ => {
                log::warn!("Unknown post-generation hook: {}", hook);
            }
        }
    }

    Ok(())
}
