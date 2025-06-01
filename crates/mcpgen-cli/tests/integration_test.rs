//! End-to-end integration tests for MCPGen CLI

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Path to the project root directory
fn project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2) // Go up to workspace root
        .unwrap_or_else(|| manifest_dir.as_path())
        .to_path_buf()
}

/// List all files recursively in a directory
fn list_files_recursively(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                files.extend(list_files_recursively(&path)?);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    /// Test generating a server from OpenAPI v3 spec
    #[test]
    fn test_generate_from_openapi_v3() -> Result<()> {
        let spec_path = project_root().join("tests/fixtures/openapi/petstore.openapi.v3.json");
        test_scaffold_with_spec(&spec_path)
    }

    /// Test generating a server from Swagger v2 spec
    #[test]
    fn test_generate_from_swagger_v2() -> Result<()> {
        let spec_path = project_root().join("tests/fixtures/openapi/petstore.swagger.v2.json");
        test_scaffold_with_spec(&spec_path)
    }

    /// Helper function to test scaffold with a specific spec file
    fn test_scaffold_with_spec(spec_path: &Path) -> Result<()> {
        // Create a project-local directory for scaffolded output
        let output_dir = project_root().join(".mcpgen_scaffold").join("test_output");

        // Clean up any previous test output
        if output_dir.exists() {
            std::fs::remove_dir_all(&output_dir)?;
        }
        std::fs::create_dir_all(&output_dir)?;

        // Get the template directory
        let template_dir = project_root().join("templates/rust-axum");
        assert!(
            template_dir.exists(),
            "Template directory not found at {}",
            template_dir.display()
        );

        // Build the CLI binary
        let status = Command::new("cargo")
            .args(["build", "--package", "mcpgen", "--bin", "mcpgen"])
            .status()?;
        assert!(status.success(), "Failed to build mcpgen CLI");

        // Run the CLI to scaffold the server
        let output = Command::new("cargo")
            .args(["run", "--"])
            .args(["scaffold"])
            .args(["--template-dir", template_dir.to_str().unwrap()])
            .args(["--spec", spec_path.to_str().unwrap()])
            .args(["--output", output_dir.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        // Check if generation was successful
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            panic!(
                "Generation failed with status {}\nstdout:{}\nstderr:{}",
                output.status, stdout, stderr
            );
        }

        // Verify the generated files
        let generated_files = list_files_recursively(&output_dir)?;
        assert!(!generated_files.is_empty(), "No files were generated");

        // Check for expected files
        let expected_files = ["src/main.rs", "src/handlers/mod.rs", "Cargo.toml"];

        for file in expected_files.iter() {
            let path = output_dir.join(file);
            assert!(
                path.exists(),
                "Expected file was not generated: {}",
                path.display()
            );
        }

        Ok(())
    }
}
