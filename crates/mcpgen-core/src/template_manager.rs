//! Template system for code generation

// Internal imports (std, crate)
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// External imports (alphabetized)
use serde::Serialize;
use serde_json::{Map, Value as JsonValue, json};
use tera::{Context, Tera};
use tokio::{fs, task};

use crate::{
    config::Config, error::Result, manifest::TemplateManifest, openapi::OpenAPISpec,
    template::Template, template_options::TemplateOptions,
};

type TeraCache = std::collections::HashMap<String, Arc<Tera>>;

/// Manages loading and rendering of code generation templates
#[derive(Debug, Clone)]
pub struct TemplateManager {
    /// Cached Tera template engine instance
    tera: Arc<Tera>,
    /// Path to the template directory
    template_dir: PathBuf,
    /// The template kind (language/framework)
    template_kind: Template,
    /// The template manifest
    manifest: TemplateManifest,
}

impl TemplateManager {
    /// Create a new template manager for the given language
    ///
    /// If `template_dir` is provided, it will be used directly. Otherwise, the template
    /// directory will be discovered based on the language and framework.
    /// Creates a new TemplateManager with a cached Tera instance
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
            tokio::fs::canonicalize(&dir).await.map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Failed to canonicalize template directory: {}", e),
                )
            })?
        } else {
            // Discover the template directory based on the template kind
            Self::discover_template_dir(&template_kind).await?
        };

        // Convert template_dir to string for caching
        let template_dir_str = template_dir.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Template path contains invalid UTF-8",
            )
        })?;

        // Get or initialize the cached Tera instance
        let tera = {
            use once_cell::sync::Lazy;
            use std::sync::Mutex;

            static TERA_CACHE: Lazy<Mutex<TeraCache>> = Lazy::new(|| Mutex::new(TeraCache::new()));

            let mut cache = TERA_CACHE.lock().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to acquire Tera cache lock: {}", e),
                )
            })?;

            // Check if we have a cached Tera instance for this template directory
            if let Some(cached_tera) = cache.get(template_dir_str) {
                cached_tera.clone()
            } else {
                // Initialize a new Tera instance and cache it
                let mut tera =
                    Tera::new(&format!("{}/**/*.tera", template_dir_str)).map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Failed to initialize Tera: {}", e),
                        )
                    })?;

                // Auto-escape all files
                tera.autoescape_on(vec![".html", ".htm", ".xml", ".md"]);

                let tera_arc = Arc::new(tera);
                cache.insert(template_dir_str.to_string(), tera_arc.clone());
                tera_arc
            }
        };

        // Load the template manifest
        let manifest_path = template_dir.join("manifest.yaml");
        let manifest = if manifest_path.exists() {
            let manifest_content = tokio::fs::read_to_string(&manifest_path).await?;
            serde_yaml::from_str(&manifest_content).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed to parse manifest: {}", e),
                )
            })?
        } else {
            // Default manifest if none exists
            TemplateManifest::default()
        };

        Ok(Self {
            tera,
            template_dir,
            template_kind,
            manifest,
        })
    }

    /// Discover the template directory based on the template kind
    async fn discover_template_dir(template_kind: &Template) -> Result<PathBuf> {
        // Try to get the template directory from the template kind
        let template_dir = template_kind.template_dir().await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Failed to discover template directory: {}", e),
            )
        })?;

        // Verify the directory exists
        if !template_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Template directory not found: {}", template_dir.display()),
            )
            .into());
        }

        // Convert to absolute path
        tokio::fs::canonicalize(&template_dir).await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to canonicalize template path: {}", e),
            )
            .into()
        })
    }

    /// Get the template kind this template manager is configured for
    pub fn template_kind(&self) -> Template {
        self.template_kind
    }

    /// Get the path to the template directory
    pub fn template_dir(&self) -> &Path {
        &self.template_dir
    }

    /// Get a reference to the Tera template engine
    pub fn tera(&self) -> &Tera {
        &self.tera
    }

    /// Reload all templates from the template directory.
    /// This is a no-op in the cached implementation since templates are loaded on demand.
    pub async fn reload_templates(&self) -> Result<()> {
        // No-op in the cached implementation
        // Templates are loaded on demand and cached automatically
        Ok(())
    }

    /// Discovers all template files in the given directory and its subdirectories.
    ///
    /// This function uses `spawn_blocking` to avoid blocking the async runtime
    /// during filesystem operations.
    ///
    /// # Arguments
    /// * `dir` - The directory to search for template files
    ///
    /// # Returns
    /// A `Result` containing a vector of paths to template files with the `.tera` extension
    pub async fn discover_template_files(dir: &Path) -> Result<Vec<PathBuf>> {
        let dir_buf = dir.to_path_buf();

        task::spawn_blocking(move || {
            let mut templates = Vec::new();

            fn walk_dir(dir: &Path, templates: &mut Vec<PathBuf>) -> std::io::Result<()> {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();

                    if path.is_dir() {
                        walk_dir(&path, templates)?;
                    } else if path.extension().and_then(|s| s.to_str()) == Some("tera") {
                        templates.push(path);
                    }
                }
                Ok(())
            }

            walk_dir(&dir_buf, &mut templates)?;
            Ok(templates)
        })
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to join blocking task: {}", e),
            )
        })?
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

    /// Get a reference to the template manifest
    pub fn manifest(&self) -> &TemplateManifest {
        &self.manifest
    }

    /// Generate a file from a template with a custom context
    pub async fn generate_with_context<T: Serialize>(
        &self,
        template_name: &str,
        context: &T,
        output_path: impl AsRef<Path>,
    ) -> Result<()> {
        let output_path = output_path.as_ref();

        // First validate required context variables
        let context_value = serde_json::to_value(context)
            .map_err(|e| crate::Error::template(format!("Failed to serialize context: {}", e)))?;

        let context_map = context_value
            .as_object()
            .ok_or_else(|| crate::Error::template("Context must be a JSON object".to_string()))?;

        // Define required variables per template type
        let required_vars: &[&str] = match template_name {
            // Add template-specific required variables here
            // Example: "handlers/endpoint.rs" => &["endpoint", "parameters_type"],
            _ => &[],
        };

        validate_context(template_name, context_map, required_vars)?;

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

        // Build Tera Context from the already parsed context_map
        let mut tera_context = Context::new();
        for (k, v) in context_map {
            tera_context.insert(k, v);
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

        // Build the context for template rendering
        let context = self
            .build_context(spec, &output_path, &template_opts)
            .await?;

        log::debug!("Starting template processing with context: {:#?}", context);
        // Process all template files
        self.process_template_files(&context, &output_path, &template_opts, spec)
            .await
            .map_err(|e| {
                log::error!("Template processing failed: {}", e);
                e
            })?;

        // Run post-generation hooks if any
        self.execute_post_generation_hooks(&output_path).await?;

        Ok(())
    }

    /// Build the context for template rendering
    async fn build_context(
        &self,
        spec: &OpenAPISpec,
        output_path: &Path,
        template_opts: &Option<TemplateOptions>,
    ) -> Result<serde_json::Value> {
        // Build base context with project_name and api_version
        let mut base_map: Map<String, JsonValue> = Map::new();

        // Add project name from output directory
        if let Some(proj_name) = output_path.file_name().and_then(|s| s.to_str()) {
            base_map.insert("project_name".to_string(), json!(proj_name));
        }

        // Add API version from spec
        if let Some(api_version) = spec.version() {
            base_map.insert("api_version".to_string(), json!(api_version));
        }

        // Add MCP Agent instructions if provided
        if let Some(opts) = template_opts {
            if let Some(instructions) = &opts.agent_instructions {
                base_map.insert("agent_instructions".to_string(), instructions.clone());
            }
        }

        // Add the full spec to the context if needed
        if let Ok(spec_value) = serde_json::to_value(spec) {
            base_map.insert("spec".to_string(), spec_value);
        }

        // Add spec file name for reference in templates
        base_map.insert("spec_file_name".to_string(), json!("openapi.json"));

        // Extract endpoints from the OpenAPI spec
        let endpoints = spec.parse_endpoints().await?;

        base_map.insert("endpoints".to_string(), json!(endpoints));

        // Add server configuration variables needed by templates
        base_map.insert("log_file".to_string(), json!("mcpgen"));
        base_map.insert("server_port".to_string(), json!(8080));

        // Add any template options to the context if provided
        if let Some(opts) = template_opts {
            // Override defaults with template options if provided
            if let Some(port) = opts.server_port {
                base_map.insert("server_port".to_string(), json!(port));
            }
            if let Some(log_file) = &opts.log_file {
                base_map.insert("log_file".to_string(), json!(log_file));
            }
        }

        // For debugging, log the context keys
        let keys_str: Vec<String> = base_map.keys().map(|k| k.to_string()).collect();
        log::debug!("Template context keys: {}", keys_str.join(", "));

        Ok(serde_json::Value::Object(base_map))
    }

    /// Process all template files with the given context
    async fn process_template_files(
        &self,
        base_context: &serde_json::Value,
        output_path: &Path,
        template_opts: &Option<TemplateOptions>,
        spec: &OpenAPISpec,
    ) -> Result<()> {
        // Pre-load endpoint contexts for operation templates if needed
        let needs_endpoints = self
            .manifest
            .files
            .iter()
            .any(|f| f.for_each.as_deref() == Some("endpoint"));

        let endpoint_contexts = if needs_endpoints {
            spec.parse_endpoints().await?
        } else {
            Vec::new()
        };

        for file in &self.manifest.files {
            // Handle per-endpoint generation if specified
            if file.for_each.as_deref() == Some("endpoint") {
                // Convert base_context to Context for operation processing
                let context = if let serde_json::Value::Object(obj) = base_context {
                    Context::from_value(serde_json::Value::Object(obj.clone()))
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
                } else {
                    Context::new()
                };

                self.process_operation_file(
                    file,
                    &context,
                    output_path,
                    &endpoint_contexts,
                    &template_opts,
                )
                .await?;
            } else {
                self.process_single_file(file, base_context, output_path)
                    .await?;
            }
        }

        Ok(())
    }

    /// Process a single template file
    async fn process_single_file(
        &self,
        file: &crate::manifest::TemplateFile,
        base_context: &serde_json::Value,
        output_path: &Path,
    ) -> Result<()> {
        let dest_path = output_path.join(&file.destination);

        // Create parent directories if they don't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Create file-specific context
        let file_context = self.create_file_context(base_context, file).await?;

        // Convert file_context to Context
        let tera_context = if let serde_json::Value::Object(obj) = file_context {
            Context::from_value(serde_json::Value::Object(obj))
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            Context::new()
        };

        // Log the template context for debugging
        log::debug!(
            "Rendering template: {} with context: {:#?}",
            file.source,
            tera_context
        );

        // Render the template
        let rendered = match self.tera.render(&file.source, &tera_context) {
            Ok(r) => r,
            Err(e) => {
                // Convert tera::Context to a serializable Map before serializing
                let context_map: std::collections::HashMap<String, serde_json::Value> =
                    tera_context
                        .into_json()
                        .as_object()
                        .map(|obj| obj.clone().into_iter().collect())
                        .unwrap_or_default();
                let context_json = serde_json::to_string_pretty(&context_map)
                    .unwrap_or_else(|_| "Failed to serialize context".to_string());
                log::error!(
                    "Failed to render template {}: {}\nTemplate context: {}",
                    file.source,
                    e,
                    context_json
                );
                return Err(crate::error::Error::Io(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Template rendering failed for {}: {}", file.source, e),
                )));
            }
        };

        // Write the output file
        fs::write(&dest_path, rendered).await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to write file {}: {}", dest_path.display(), e),
            )
        })?;

        Ok(())
    }

    /// Process a template file for each operation
    async fn process_operation_file(
        &self,
        file: &crate::manifest::TemplateFile,
        base_context: &Context,
        output_path: &Path,
        endpoint_contexts: &[crate::openapi::EndpointContext],
        template_opts: &Option<TemplateOptions>,
    ) -> Result<()> {
        // Create schemas directory
        let schemas_dir = output_path.join("schemas");
        fs::create_dir_all(&schemas_dir).await?;

        for ctx in endpoint_contexts {
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
                let mut context = base_context.clone();

                context.insert("endpoint", &ctx.endpoint);
                context.insert("endpoint_cap", &ctx.endpoint_cap);
                context.insert("endpoint_raw", &ctx.endpoint_raw);
                context.insert("fn_name", &ctx.fn_name);
                context.insert("summary", &ctx.summary);
                context.insert("description", &ctx.description);
                context.insert("parameters", &ctx.parameters);
                context.insert("properties", &ctx.properties);
                context.insert("tags", &ctx.tags);
                context.insert(
                    "parameters_type",
                    &format!("{}{}", ctx.endpoint_cap, "Parameters"),
                );
                context.insert(
                    "properties_type",
                    &format!("{}{}", ctx.endpoint_cap, "Properties"),
                );

                log::debug!("Processing template for operation: {}", ctx.endpoint);

                // Generate schema file
                let schema_path = schemas_dir.join(format!("{}.json", ctx.endpoint));
                // TODO: In the future, we should recursively resolve all schema references
                // For now, we're just using the envelope_properties as-is
                let schema_json = serde_json::to_string_pretty(&ctx.envelope_properties)?;
                fs::write(&schema_path, schema_json).await.map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "Failed to write schema file {}: {}",
                            schema_path.display(),
                            e
                        ),
                    )
                })?;

                // Generate the output path with sanitized endpoint name
                let output_file = file.destination
                    .replace("{{endpoint}}", &ctx.endpoint_fs)
                    .replace("{endpoint}", &ctx.endpoint_fs);
                let output_path = output_path.join(&output_file);

                // Create parent directories if they don't exist
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent).await?;
                }

                // Render the template
                let rendered = self
                    .tera
                    .render(file.source.as_str(), &context)
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("Failed to render template {}: {}", file.source, e),
                        )
                    })?;

                // Write the file
                fs::write(&output_path, rendered).await.map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to write file {}: {}", output_path.display(), e),
                    )
                })?;
            }
        }
        Ok(())
    }

    /// Create a context for a single template file
    async fn create_file_context(
        &self,
        base_context: &serde_json::Value,
        file: &crate::manifest::TemplateFile,
    ) -> Result<serde_json::Value> {
        // Start with the file context if it exists
        let mut context = if let JsonValue::Object(file_ctx) = &file.context {
            file_ctx.clone()
        } else {
            serde_json::Map::new()
        };

        // Add base context values if they don't exist in file context
        if let serde_json::Value::Object(base_map) = base_context {
            for (k, v) in base_map {
                // Only add if not already in the file context
                if !context.contains_key(k) {
                    context.insert(k.clone(), v.clone());
                }
            }
        }

        Ok(serde_json::Value::Object(context))
    }

    /// Execute post-generation hooks
    async fn execute_post_generation_hooks(&self, output_path: &Path) -> Result<()> {
        for hook in &self.manifest.hooks.post_generate {
            match hook.as_str() {
                "cargo_fmt" => {
                    if let Ok(mut cmd) = std::process::Command::new("cargo")
                        .args(["fmt", "--"])
                        .current_dir(output_path)
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

/// Validates that all required context variables are present
fn validate_context(
    template: &str,
    context: &Map<String, JsonValue>,
    required_vars: &[&str],
) -> crate::Result<()> {
    let mut missing = Vec::new();

    for var in required_vars {
        if !context.contains_key(*var) {
            missing.push(var.to_string());
        }
    }

    if !missing.is_empty() {
        return Err(crate::Error::template(format!(
            "Missing required context variables for template '{}': {}",
            template,
            missing.join(", ")
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::openapi::OpenAPISpec;
    use crate::template::Template;
    use serde_json::{Map, json};
    use tempfile::TempDir;

    #[test]
    fn test_validate_context() {
        let mut context = Map::new();
        context.insert("foo".to_string(), json!("bar"));
        context.insert("baz".to_string(), json!(42));

        // Test with no required variables
        assert!(validate_context("test_template", &context, &[]).is_ok());

        // Test with existing variables
        assert!(validate_context("test_template", &context, &["foo"]).is_ok());
        assert!(validate_context("test_template", &context, &["baz"]).is_ok());
        assert!(validate_context("test_template", &context, &["foo", "baz"]).is_ok());

        // Test with missing variables
        let result = validate_context("test_template", &context, &["missing"]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing required context variables")
        );

        // Test with some missing variables
        let result = validate_context("test_template", &context, &["foo", "missing"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

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
