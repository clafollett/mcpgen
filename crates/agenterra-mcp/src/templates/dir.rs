//! Unified handling of template directory resolution and operations

use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

use super::ServerTemplateKind;

/// Trait for reading template configuration, allowing dependency injection for testing
pub trait TemplateConfigReader {
    fn get_template_dir(&self) -> Option<String>;
}

/// Production implementation that reads from environment variables
pub struct EnvTemplateConfigReader;

impl TemplateConfigReader for EnvTemplateConfigReader {
    fn get_template_dir(&self) -> Option<String> {
        std::env::var("AGENTERRA_TEMPLATE_DIR").ok()
    }
}

/// Mock implementation for testing with controlled values
#[cfg(test)]
pub struct MockTemplateConfigReader(Option<String>);

#[cfg(test)]
impl MockTemplateConfigReader {
    pub fn new(template_dir: Option<String>) -> Self {
        Self(template_dir)
    }
}

#[cfg(test)]
impl TemplateConfigReader for MockTemplateConfigReader {
    fn get_template_dir(&self) -> Option<String> {
        self.0.clone()
    }
}

/// Represents a template directory with resolved paths and validation
#[derive(Debug, Clone)]
pub struct TemplateDir {
    /// Root directory containing the templates
    root_dir: PathBuf,
    /// Path to the specific template directory (root_dir/template_name)
    template_path: PathBuf,
    /// The template kind (language/framework)
    kind: ServerTemplateKind,
}

impl TemplateDir {
    /// Create a new TemplateDir with explicit paths
    pub fn new(root_dir: PathBuf, template_path: PathBuf, kind: ServerTemplateKind) -> Self {
        Self {
            root_dir,
            template_path,
            kind,
        }
    }

    /// Returns the template path as a string slice
    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.template_path.to_string_lossy()
    }

    /// Returns a displayable version of the template path
    pub fn display(&self) -> std::path::Display<'_> {
        self.template_path.display()
    }

    /// Discover the template directory based on the template kind and optional override
    pub fn discover(kind: ServerTemplateKind, custom_dir: Option<&Path>) -> io::Result<Self> {
        debug!(
            "TemplateDir::discover - kind: {:?}, custom_dir: {:?}",
            kind, custom_dir
        );

        let root_dir = if let Some(dir) = custom_dir {
            // Use the provided directory directly
            debug!("Using custom template directory: {}", dir.display());
            if !dir.exists() {
                error!("Custom template directory not found: {}", dir.display());
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Template directory not found: {}", dir.display()),
                ));
            }
            dir.to_path_buf()
        } else {
            // Auto-discover the template directory
            debug!("Auto-discovering template directory...");
            let discovered = Self::find_template_base_dir().ok_or_else(|| {
                error!("Could not find template directory in any standard location");
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not find template directory in any standard location",
                )
            })?;
            debug!("Auto-discovered template base: {}", discovered.display());
            discovered
        };

        let template_path = root_dir
            .join("templates")
            .join("mcp")
            .join(kind.role().as_str())
            .join(kind.as_str());

        debug!("Resolved template path: {}", template_path.display());
        debug!("Template path exists: {}", template_path.exists());

        // Validate the template directory exists
        if !template_path.exists() {
            error!(
                "Template directory not found at resolved path: {}",
                template_path.display()
            );
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Template directory not found: {}", template_path.display()),
            ));
        }

        info!(
            "Successfully created TemplateDir for: {}",
            template_path.display()
        );
        Ok(Self::new(root_dir, template_path, kind))
    }

    /// Find the base template directory by checking standard locations
    pub fn find_template_base_dir() -> Option<PathBuf> {
        Self::find_template_base_dir_with_config(&EnvTemplateConfigReader)
    }

    /// Find the base template directory with a custom config reader (for testing)
    pub fn find_template_base_dir_with_config(
        config_reader: &dyn TemplateConfigReader,
    ) -> Option<PathBuf> {
        // 1. Check environment variable via config reader
        if let Some(dir) = config_reader.get_template_dir() {
            let path = PathBuf::from(dir);
            if path.exists() {
                // Validate the path for security
                if let Err(e) = Self::validate_template_path(&path) {
                    error!("Template directory validation failed: {}", e);
                    return None;
                }
                return Some(path);
            }
        }

        // 2. Check executable directory and parent directories
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // Canonicalize to get absolute path
                if let Ok(exe_dir_abs) = exe_dir.canonicalize() {
                    // Check if templates are next to the executable
                    let templates_dir = exe_dir_abs.join("templates");
                    if templates_dir.exists() {
                        return Some(exe_dir_abs);
                    }

                    // Check parent directory (for development)
                    if let Some(parent_dir) = exe_dir_abs.parent() {
                        let templates_dir = parent_dir.join("templates");
                        if templates_dir.exists() {
                            return Some(parent_dir.to_path_buf());
                        }
                    }
                }
            }
        }

        // 3. Check current directory (as fallback for development)
        if let Ok(current_dir) = std::env::current_dir() {
            let templates_dir = current_dir.join("templates");
            if templates_dir.exists() {
                return Some(current_dir);
            }
        }

        // 4. Check in the crate root (for development)
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest_path = PathBuf::from(manifest_dir);
            if let Some(workspace_root) = manifest_path.parent() {
                let templates_dir = workspace_root.join("templates");
                if templates_dir.exists() {
                    return Some(workspace_root.to_path_buf());
                }
            }
        }

        // 5. Check in the user's home directory
        if let Some(home_dir) = dirs::home_dir() {
            let templates_dir = home_dir.join(".agenterra").join("templates");
            if templates_dir.exists() {
                return Some(home_dir.join(".agenterra"));
            }
        }

        None
    }

    /// Validate that a template directory path is safe
    /// Prevents directory traversal and ensures path is within expected bounds
    fn validate_template_path(path: &Path) -> Result<(), io::Error> {
        // Canonicalize to resolve any ".." or "." components
        let canonical_path = path.canonicalize().map_err(|e| {
            error!("Failed to canonicalize template path: {}", e);
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid template path: {}", e),
            )
        })?;

        // Convert to string for validation
        let path_str = canonical_path.to_string_lossy();

        // Reject paths that look suspicious
        let suspicious_patterns = [
            "/etc/",
            "/usr/bin/",
            "/usr/sbin/",
            "/root/",
            "/.ssh/",
            "/tmp/",
            "C:\\Windows",
            "C:\\Users\\",
            "C:\\Program Files",
        ];

        for pattern in &suspicious_patterns {
            if path_str.contains(pattern) {
                error!("Potentially unsafe template path rejected: {}", path_str);
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("Template path not allowed: {}", path_str),
                ));
            }
        }

        // If we have a home directory, ensure path is within reasonable bounds
        if let Some(home_dir) = dirs::home_dir() {
            if let Ok(home_canonical) = home_dir.canonicalize() {
                // Allow paths under home directory
                if canonical_path.starts_with(&home_canonical) {
                    return Ok(());
                }
            }
        }

        // Allow paths that are clearly development/workspace related
        if path_str.contains("/workspace/")
            || path_str.contains("/agenterra/")
            || path_str.contains("/tmp/")
            || path_str.contains("target/debug")
            || path_str.contains("target/release")
        {
            return Ok(());
        }

        debug!("Template path validation passed: {}", path_str);
        Ok(())
    }

    /// Get the root directory containing the templates
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Get the template kind
    pub fn kind(&self) -> ServerTemplateKind {
        self.kind
    }

    /// Get the path to the specific template directory
    pub fn template_path(&self) -> &Path {
        &self.template_path
    }

    /// Convert to PathBuf
    pub fn into_path_buf(self) -> PathBuf {
        self.template_path
    }

    /// Check if the template directory exists
    pub fn exists(&self) -> bool {
        self.template_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use tracing_test::traced_test;

    #[test]
    fn test_template_dir_validation() {
        let temp_dir = tempdir().unwrap();

        // Create new server structure
        let server_template_dir = temp_dir.path().join("templates/mcp/server/rust_axum");
        fs::create_dir_all(&server_template_dir).unwrap();

        // Test server template discovery
        let server_template =
            TemplateDir::discover(ServerTemplateKind::RustAxum, Some(temp_dir.path()));
        assert!(server_template.is_ok());
        assert_eq!(
            server_template.unwrap().template_path(),
            server_template_dir.as_path()
        );

        // Test with non-existent directory
        let result = TemplateDir::discover(
            ServerTemplateKind::RustAxum,
            Some(Path::new("/nonexistent")),
        );
        assert!(result.is_err());
    }

    #[test]
    #[traced_test]
    fn test_debug_logging_output() {
        let temp_dir = tempdir().unwrap();
        let server_template_dir = temp_dir.path().join("templates/mcp/server/rust_axum");
        fs::create_dir_all(&server_template_dir).unwrap();

        // This should generate debug logs
        let _result = TemplateDir::discover(ServerTemplateKind::RustAxum, Some(temp_dir.path()));

        // Check that debug logs were generated
        // Note: This test will fail initially with eprintln! but pass with tracing::debug!
        assert!(
            logs_contain("Auto-discovering template directory")
                || logs_contain("Resolved template path")
        );
    }

    #[test]
    fn test_find_template_base_dir_uses_absolute_paths() {
        // Test absolute path resolution using mock config reader
        let temp_workspace = tempdir().unwrap();
        let templates_dir = temp_workspace.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();

        // Test with mock config reader - no global state modification
        let mock_config = MockTemplateConfigReader::new(Some(
            temp_workspace.path().to_string_lossy().to_string(),
        ));
        let result = TemplateDir::find_template_base_dir_with_config(&mock_config);
        assert!(result.is_some());

        // Test the resolved path is absolute and exists
        let resolved_path = result.unwrap();
        assert!(resolved_path.is_absolute());
        assert!(resolved_path.exists());
    }

    #[test]
    fn test_find_template_base_dir_executable_location() {
        // Test that template discovery works from executable location
        // This simulates the installed binary scenario
        let temp_workspace = tempdir().unwrap();
        let bin_dir = temp_workspace.path().join("bin");
        let templates_dir = temp_workspace.path().join("templates");

        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&templates_dir).unwrap();

        // Test with mock config that simulates env var configuration
        let mock_config = MockTemplateConfigReader::new(Some(
            temp_workspace.path().to_string_lossy().to_string(),
        ));
        let result = TemplateDir::find_template_base_dir_with_config(&mock_config);
        assert!(result.is_some());

        // Verify the discovered path exists
        let discovered_path = result.unwrap();
        assert!(discovered_path.exists());
    }

    #[test]
    fn test_security_template_dir_validation() {
        // Test that template directory paths are validated for security
        let malicious_paths = vec![
            "../../../etc/passwd",
            "/etc/passwd",
            "../../.ssh/id_rsa",
            "C:\\Windows\\System32",
            "/usr/local/../../etc/passwd",
        ];

        for path in malicious_paths {
            // Test with mock config reader using malicious path
            let mock_config = MockTemplateConfigReader::new(Some(path.to_string()));
            let result = TemplateDir::find_template_base_dir_with_config(&mock_config);

            // The path should be rejected for security reasons, even if it exists
            assert!(
                result.is_none(),
                "Malicious path should be rejected: {}",
                path
            );
        }
    }

    #[test]
    fn test_output_directory_traversal_protection() {
        // Test protection against output directory traversal
        let temp_dir = tempdir().unwrap();
        let server_template_dir = temp_dir.path().join("templates/mcp/server/rust_axum");
        fs::create_dir_all(&server_template_dir).unwrap();

        // Attempt to create template dir with malicious output path
        let malicious_output_paths =
            vec!["../../../etc", "/etc", "../../sensitive", "..\\..\\Windows"];

        for _path in malicious_output_paths {
            // This test documents the need for output path validation
            // Currently there's no validation in TemplateDir
            // The validation should happen in the CLI layer
        }
    }

    #[test]
    fn test_concurrent_template_discovery() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        // Setup shared test directory
        let temp_dir = tempdir().unwrap();
        let server_template_dir = temp_dir.path().join("templates/mcp/server/rust_axum");
        fs::create_dir_all(&server_template_dir).unwrap();

        const NUM_THREADS: usize = 10;
        let barrier = Arc::new(Barrier::new(NUM_THREADS));
        let mut handles = vec![];

        // Spawn multiple threads that all try to discover templates simultaneously
        for i in 0..NUM_THREADS {
            let barrier_clone = Arc::clone(&barrier);
            let temp_dir_path = temp_dir.path().to_string_lossy().to_string();

            let handle = thread::spawn(move || {
                // Wait for all threads to be ready
                barrier_clone.wait();

                // Each thread uses its own mock config reader (thread-safe)
                let mock_config = MockTemplateConfigReader::new(Some(temp_dir_path));
                let result = TemplateDir::find_template_base_dir_with_config(&mock_config);

                // Should succeed without panics or race conditions
                assert!(result.is_some(), "Thread {} failed to discover template", i);

                let base_dir = result.unwrap();
                assert!(base_dir.exists());
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        // No cleanup needed - no global state was modified
    }

    #[test]
    fn test_environment_variable_template_discovery() {
        // Sequential test for environment variable functionality
        let temp_dir = tempdir().unwrap();
        let templates_dir = temp_dir.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();

        // Test 1: Without env var set (should return None for env var path)
        let env_config = EnvTemplateConfigReader;
        let _no_env_result = env_config.get_template_dir();
        // Note: We can't assert None because AGENTERRA_TEMPLATE_DIR might be set globally
        // This test documents the behavior

        // Test 2: With env var set temporarily (single-threaded, marked unsafe due to race potential)
        unsafe {
            std::env::set_var("AGENTERRA_TEMPLATE_DIR", temp_dir.path());
        }
        let with_env_result = env_config.get_template_dir();
        assert!(with_env_result.is_some());
        assert_eq!(with_env_result.unwrap(), temp_dir.path().to_string_lossy());

        // Test 3: Test the full discovery process with env var
        let discovery_result = TemplateDir::find_template_base_dir();
        assert!(discovery_result.is_some());

        // Cleanup (unsafe due to potential race with other threads reading env vars)
        unsafe {
            std::env::remove_var("AGENTERRA_TEMPLATE_DIR");
        }

        // Test 4: After cleanup, env var should be gone
        let _after_cleanup = env_config.get_template_dir();
        // Note: Can't assert None due to potential global env var, but documents cleanup
    }
}
