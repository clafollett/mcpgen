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
    TemplateOptions, config::Config, error::Result, manifest::TemplateManifest,
    openapi::OpenAPISpec, template_kind::Template,
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
        // Run post-generation hooks if any
        for hook in &self.manifest.hooks.post_generate {
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
    use crate::template_kind::Template;
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
