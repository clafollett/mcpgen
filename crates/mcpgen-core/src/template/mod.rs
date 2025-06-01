//! Template system for code generation

use crate::error::Result;
use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};
use tera::{Context, Tera};

use crate::manifest::TemplateManifest;
use crate::template_kind::Template;

use crate::TemplateOptions;
use crate::config::Config;
use crate::openapi::OpenAPISpec;
use serde_json::{Map, Value as JsonValue, json};
use tokio::fs;
use tokio::task;

/// Manages loading and rendering of code generation templates
#[derive(Debug)]
pub struct TemplateManager {
    /// Tera template engine instance
    pub tera: Tera,
    /// Path to the template directory
    pub template_dir: PathBuf,
    /// The template kind (language/framework)
    pub template_kind: Template,
    /// The template manifest
    pub manifest: TemplateManifest,
}

impl TemplateManager {
    /// Create a new template manager for the given language
    ///
    /// If `template_dir` is provided, it will be used directly. Otherwise, the template
    /// directory will be discovered based on the language and framework.
    pub async fn new(template_kind: Template, template_dir: Option<PathBuf>) -> Result<Self> {
        let template_dir = if let Some(dir) = template_dir {
            // Use the provided template directory directly
            if !dir.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Template directory not found: {}", dir.display()),
                )
                .into());
            }
            tokio::fs::canonicalize(&dir)
                .await
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to canonicalize template path: {}", e),
                    )
                })?
                .to_path_buf()
        } else {
            // Use the template kind's template directory discovery
            template_kind
                .template_dir()
                .await
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to get template directory: {}", e),
                    )
                })?
                .to_path_buf()
        };

        // Load the template manifest
        let manifest = TemplateManifest::load_from_dir(&template_dir)
            .await
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to load template manifest: {}", e),
                )
            })?;

        let mut manager = Self {
            tera: Tera::default(),
            template_dir: template_dir.clone(),
            template_kind,
            manifest,
        };

        // Load all templates from the directory
        manager.reload_templates().await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to load templates: {}", e),
            )
        })?;

        Ok(manager)
    }

    /// Get the template kind this template manager is configured for
    pub fn template_kind(&self) -> Template {
        self.template_kind
    }

    /// Get the path to the template directory
    pub fn template_dir(&self) -> &Path {
        &self.template_dir
    }

    /// Reload all templates from the template directory.
    /// This will discover all `.tera` files in the template directory.
    pub async fn reload_templates(&mut self) -> Result<()> {
        // Create a new Tera instance with lenient parsing
        let mut tera = Tera::default();
        tera.autoescape_on(vec![]);

        // Find all .tera files in the template directory
        let template_files = Self::discover_template_files(&self.template_dir).await?;

        // Add each template to Tera with its relative path as the name
        for template_path in template_files {
            // Get the relative path from the template directory
            let relative_path = template_path
                .strip_prefix(&self.template_dir)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "Failed to get relative path for template {}: {}",
                            template_path.display(),
                            e
                        ),
                    )
                })?;

            // Convert Windows backslashes to forward slashes for consistency
            let template_name = relative_path.to_string_lossy().replace('\\', "/");

            log::debug!("Loading template: {}", template_name);

            // Read the template content
            let template_content =
                tokio::fs::read_to_string(&template_path)
                    .await
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!(
                                "Failed to read template file {}: {}",
                                template_path.display(),
                                e
                            ),
                        )
                    })?;

            // Add the template with lenient parsing
            tera.add_raw_template(&template_name, &template_content)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to parse template {}: {}", template_name, e),
                    )
                })?;
        }

        self.tera = tera;
        log::debug!(
            "Loaded templates: {:?}",
            self.tera.get_template_names().collect::<Vec<_>>()
        );
        Ok(())
    }

    /// Discover all template files in a directory recursively (uses spawn_blocking to avoid blocking async runtime)
    pub async fn discover_template_files(dir: &Path) -> Result<Vec<PathBuf>> {
        let dir_buf = dir.to_path_buf();
        let templates = task::spawn_blocking(move || -> std::io::Result<Vec<PathBuf>> {
            fn walk(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
                let mut templates = Vec::new();
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_dir() {
                        templates.extend(walk(&path)?);
                    } else if path.extension().and_then(|s| s.to_str()) == Some("tera") {
                        templates.push(path);
                    }
                }
                Ok(templates)
            }
            walk(&dir_buf)
        })
        .await
        .map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("Blocking join error: {}", e))
        })??;
        Ok(templates)
    }

    /// Generate a handler file from a template
    pub async fn generate_handler<T: Serialize>(
        &self,
        template_name: &str,
        context: &T,
        output_path: impl AsRef<Path>,
    ) -> Result<()> {
        self.generate_with_context(template_name, context, output_path)
            .await
    }

    /// Generate a file from a template with a custom context
    pub async fn generate_with_context<T: Serialize>(
        &self,
        template_name: &str,
        context: &T,
        output_path: impl AsRef<Path>,
    ) -> Result<()> {
        let output_path = output_path.as_ref();
        let parent = output_path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid output path: {}", output_path.display()),
            )
        })?;

        tokio::fs::create_dir_all(parent).await?;

        // Log the template being rendered
        log::debug!("Rendering template: {}", template_name);
        log::debug!("Output path: {}", output_path.display());
        log::debug!("Parent directory: {}", parent.display());

        // Build Tera Context manually from JSON object
        let mut tera_context = Context::new();
        // Serialize provided context into JSON value
        let data_value = serde_json::to_value(context)
            .map_err(|e| crate::Error::template(format!("Failed to serialize context: {}", e)))?;
        if let Some(map) = data_value.as_object() {
            for (k, v) in map {
                tera_context.insert(k, v);
            }
        } else {
            return Err(crate::Error::template(
                "Context must be a JSON object".to_string(),
            ));
        }

        // Verify template exists
        log::debug!("Checking if template exists: {}", template_name);
        self.tera.get_template(template_name).map_err(|e| {
            crate::Error::template(format!("Template not found: {} - {}", template_name, e))
        })?;

        log::debug!("Found template: {}", template_name);
        log::debug!(
            "Available templates: {:?}",
            self.tera.get_template_names().collect::<Vec<_>>()
        );

        // Render the template with detailed error reporting
        let content = match self.tera.render(template_name, &tera_context) {
            Ok(content) => content,
            Err(e) => {
                // Get the template source for better error reporting
                let template_source =
                    match std::fs::read_to_string(self.template_dir.join(template_name)) {
                        Ok(source) => source,
                        Err(_) => "<unable to read template file>".to_string(),
                    };

                return Err(crate::Error::template(format!(
                    "Failed to render template '{}': {}\nTemplate source:\n{}",
                    template_name, e, template_source
                )));
            }
        };

        log::debug!(
            "Rendered content for {} ({} bytes):\n{}",
            template_name,
            content.len(),
            if content.len() > 200 {
                format!("{}... (truncated)", &content[..200])
            } else {
                content.clone()
            }
        );

        // Ensure the parent directory exists
        log::debug!("Ensuring parent directory exists: {}", parent.display());
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            log::error!("Failed to create directory: {}", e);
            return Err(crate::Error::Io(e));
        }

        // Write the output file
        log::debug!("Writing to output file: {}", output_path.display());
        tokio::fs::write(&output_path, &content).await?;

        log::debug!("Successfully wrote template to: {}", output_path.display());
        Ok(())
    }

    /// List all available templates
    pub fn list_templates(&self) -> Vec<String> {
        self.tera
            .get_template_names()
            .map(|s| s.to_string())
            .collect()
    }

    /// Check if a template exists
    pub fn has_template(&self, name: &str) -> bool {
        self.tera.get_template(name).is_ok()
    }

    /// Generate code from loaded templates based on the OpenAPI spec and options
    pub async fn generate(
        &self,
        spec: &OpenAPISpec,
        config: &Config,
        template_opts: Option<TemplateOptions>,
    ) -> Result<()> {
        // Create output directory
        let output_path = PathBuf::from(&config.output_dir);
        fs::create_dir_all(&output_path).await?;
        // Build base context with project_name and api_version
        let mut base_map: Map<String, JsonValue> = Map::new();
        if let Some(proj_name) = output_path.file_name().and_then(|s| s.to_str()) {
            base_map.insert("project_name".to_string(), json!(proj_name));
        }
        base_map.insert("api_version".to_string(), json!("1.0.0"));
        // Pre-load endpoint contexts for operation templates
        let endpoint_contexts = spec.parse_endpoints().await?;
        // Iterate over manifest files
        for file in &self.manifest.files {
            let dest_path = output_path.join(&file.destination);
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            // Per-operation generation using parsed endpoint contexts
            if file.for_each.as_deref() == Some("operation") {
                for ctx in &endpoint_contexts {
                    let fn_name = ctx.fn_name.clone();
                    let include = template_opts
                        .as_ref()
                        .map(|opts| {
                            opts.all_operations
                                || opts.include_operations.is_empty()
                                || opts.include_operations.contains(&fn_name)
                        })
                        .unwrap_or(true);
                    let exclude = template_opts
                        .as_ref()
                        .map(|opts| opts.exclude_operations.contains(&fn_name))
                        .unwrap_or(false);
                    if include && !exclude {
                        // Merge base map, file-specific, endpoint context, and additional ctx
                        let mut context_map = base_map.clone();
                        if let Some(file_ctx) = file.context.as_object() {
                            for (k, v) in file_ctx {
                                context_map.insert(k.clone(), v.clone());
                            }
                        }
                        // Insert EndpointContext fields
                        let ctx_value = serde_json::to_value(ctx).map_err(|e| {
                            crate::Error::template(format!(
                                "Failed to serialize endpoint context: {}",
                                e
                            ))
                        })?;
                        if let Some(fields) = ctx_value.as_object() {
                            for (k, v) in fields {
                                context_map.insert(k.clone(), v.clone());
                            }
                        }
                        if let Some(opts) = &template_opts {
                            if let Some(additional) = &opts.context {
                                if let Some(add_obj) = additional.as_object() {
                                    for (k, v) in add_obj {
                                        context_map.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                        }
                        self.generate_with_context(&file.source, &context_map, &dest_path)
                            .await?;
                    }
                }
            } else {
                // Single-file generation
                // Initialize context with base_map and merge file-specific context
                let mut context = JsonValue::Object(base_map.clone());
                if let Some(file_ctx) = file.context.as_object() {
                    if let JsonValue::Object(ref mut obj) = context {
                        for (k, v) in file_ctx {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
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
                self.generate_with_context(&file.source, &context, &dest_path)
                    .await?;
            }
        }
        // Post-generation hook
        if let Some(ref hook) = self.manifest.hooks.post_generate {
            match hook.as_str() {
                "cargo_fmt" => {
                    if let Ok(mut cmd) = std::process::Command::new("cargo")
                        .args(["fmt", "--"])
                        .current_dir(&output_path)
                        .spawn()
                    {
                        let _ = cmd.wait();
                    }
                }
                _ => log::warn!("Unknown post-generation hook: {}", hook),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::openapi::OpenAPISpec;
    use crate::template_kind::Template;
    use serde_json::json;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_template_manager() -> Result<()> {
        use std::collections::HashMap;
        use tempfile::TempDir;

        // Create a test directory with template files
        let temp_dir = tempfile::tempdir().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to create temp dir: {}", e),
            )
        })?;

        let template_dir = temp_dir.path().join("templates/rust");
        tokio::fs::create_dir_all(&template_dir)
            .await
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to create template dir: {}", e),
                )
            })?;

        // Create a simple template file
        let template_content =
            "pub fn {{ handler_name }}() -> &'static str {\n    \"Hello, world!\"\n}\n";
        tokio::fs::write(template_dir.join("handler.rs.tera"), template_content)
            .await
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to write template file: {}", e),
                )
            })?;

        // Create a simple manifest file
        let manifest_content = r#"
        name: test-template
        description: Test template for unit tests
        version: 0.1.0
        language: rust
        files:
          - source: handler.rs.tera
            destination: handler.rs
        "#;
        tokio::fs::write(template_dir.join("manifest.yaml"), manifest_content)
            .await
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to write manifest: {}", e),
                )
            })?;

        // Create the template manager with explicit template directory
        let manager = TemplateManager::new(Template::RustAxum, Some(template_dir)).await?;

        // Verify that the handler template was loaded
        assert!(
            manager.tera.get_template("handler.rs.tera").is_ok(),
            "Template handler.rs.tera should be loaded"
        );

        // Test rendering a template
        let mut context = HashMap::new();
        context.insert("handler_name", "test_handler");

        // Create a temporary output directory
        let output_temp_dir = TempDir::new().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to create output temp dir: {}", e),
            )
        })?;

        let output_path = output_temp_dir.path().join("test_output.rs");

        // Render the template
        manager
            .generate_with_context("handler.rs.tera", &context, &output_path)
            .await?;

        // Verify the file was created
        assert!(output_path.exists(), "Output file should be created");
        let content = tokio::fs::read_to_string(&output_path).await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to read output file: {}", e),
            )
        })?;

        assert!(!content.is_empty(), "Generated file should not be empty");
        assert!(
            content.contains("test_handler"),
            "Generated file should contain the handler name"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_generate_single_file() -> Result<()> {
        // Setup a temporary directory for templates and output
        let temp = TempDir::new().unwrap();
        let td = temp.path();
        // Write a simple template file
        let tera_content = "Message: {{message}}";
        tokio::fs::write(td.join("foo.tera"), tera_content)
            .await
            .unwrap();
        // Write the manifest.yaml
        let manifest = r#"
name: test-template
description: Test template
version: 0.1.0
language: rust
files:
  - source: foo.tera
    destination: foo.txt
    context:
      message: hello
"#;
        tokio::fs::write(td.join("manifest.yaml"), manifest)
            .await
            .unwrap();
        // Create a minimal OpenAPI spec file
        let spec_json = json!({"paths": {}});
        let spec_file = td.join("spec.json");
        tokio::fs::write(&spec_file, spec_json.to_string())
            .await
            .unwrap();
        // Prepare config pointing to output directory
        let out_dir = temp.path().join("out");
        let config = Config::new(spec_file.to_str().unwrap(), out_dir.to_str().unwrap());
        // Initialize TemplateManager with our temp dir
        let manager = TemplateManager::new(Template::RustAxum, Some(td.to_path_buf())).await?;
        // Load spec and generate
        let spec = OpenAPISpec::from_file(&spec_file).await?;
        manager.generate(&spec, &config, None).await?;
        // Read and verify the generated file
        let result = tokio::fs::read_to_string(out_dir.join("foo.txt"))
            .await
            .unwrap();
        assert_eq!(result.trim(), "Message: hello");
        Ok(())
    }
}
