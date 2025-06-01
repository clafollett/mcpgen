//! Template type definitions and discovery for MCPGen.
//!
//! This module defines the supported template types and provides functionality
//! for discovering template directories in the filesystem. It supports both
//! built-in templates and custom template paths.
//!
//! # Examples
//!
//! ```
//! use mcpgen_core::template_kind::Template;
//! use std::str::FromStr;
//!
//! // Parse a template from a string
//! let template = Template::from_str("rust-axum").unwrap();
//! assert_eq!(template, Template::RustAxum);
//! assert_eq!(template.as_str(), "rust-axum");
//! 
//! // You can also use the Display trait
//! assert_eq!(template.to_string(), "rust-axum");
//! 
//! // The default template is RustAxum
//! assert_eq!(Template::default(), Template::RustAxum);
//! 
//! // Find the base template directory (synchronous example)
//! if let Some(base_dir) = template.find_template_base_dir() {
//!     println!("Found template base directory: {}", base_dir.display());
//! }
//! ```
//!
//! For async operations like `template_dir()`, you would typically use an async runtime
//! like `tokio` or `async-std` in your application.
//!
//! # Template Discovery
//!
//! The module searches for templates in the following locations:
//! 1. Directory specified by `MCPGEN_TEMPLATE_DIR` environment variable
//! 2. `templates/` directory in the project root (for development)
//! 3. `~/.mcpgen/templates/` in the user's home directory
//! 4. `/usr/local/share/mcpgen/templates/` for system-wide installation
//! 5. `./templates/` in the current working directory

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Supported template types
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Template {
    /// Rust with Axum web framework
    RustAxum,
    /// Python with FastAPI
    PythonFastAPI,
    /// TypeScript with Express
    TypeScriptExpress,
    /// Custom template path
    Custom,
}

impl Template {
    /// Returns the template identifier as a string slice
    pub fn as_str(&self) -> &'static str {
        match self {
            Template::RustAxum => "rust-axum",
            Template::PythonFastAPI => "python-fastapi",
            Template::TypeScriptExpress => "typescript-express",
            Template::Custom => "custom",
        }
    }

    /// Returns the path to the template's directory
    pub async fn template_dir(&self) -> std::io::Result<PathBuf> {
        let base = match self.find_template_base_dir() {
            Some(dir) => dir,
            None => PathBuf::from("templates"),
        };
        Ok(base.join(self.as_str()))
    }

    /// Discovers the base template directory by looking in common locations
    pub fn find_template_base_dir(&self) -> Option<PathBuf> {
        // 1. Check environment variable
        if let Ok(dir) = std::env::var("MCPGEN_TEMPLATE_DIR") {
            let path = PathBuf::from(dir);
            if path.exists() {
                return Some(path);
            }
        }

        // 2. Check for root templates directory (for development)
        let root_dir = PathBuf::from("..");
        let templates_dir = root_dir.join("templates");
        if templates_dir.exists() {
            return Some(root_dir);
        }

        // 3. Check current directory
        let current_dir = PathBuf::from(".");
        let templates_dir = current_dir.join("templates");
        if templates_dir.exists() {
            return Some(current_dir);
        }

        // 4. Check in the crate root (for development)
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest_path = PathBuf::from(manifest_dir);
            let workspace_root = manifest_path.parent()?;
            let templates_dir = workspace_root.join("templates");
            if templates_dir.exists() {
                return Some(workspace_root.to_path_buf());
            }
        }

        // 5. Check in the user's home directory
        if let Some(home_dir) = dirs::home_dir() {
            let templates_dir = home_dir.join(".mcpgen").join("templates");
            if templates_dir.exists() {
                return Some(home_dir.join(".mcpgen"));
            }
        }

        None
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for Template {
    fn default() -> Self {
        Template::RustAxum
    }
}

impl FromStr for Template {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust-axum" => Ok(Template::RustAxum),
            "python-fastapi" => Ok(Template::PythonFastAPI),
            "typescript-express" => Ok(Template::TypeScriptExpress),
            "custom" => Ok(Template::Custom),
            _ => Err(format!("Unknown template: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_parsing() {
        assert_eq!("rust-axum".parse::<Template>().unwrap(), Template::RustAxum);
        assert_eq!(
            "python-fastapi".parse::<Template>().unwrap(),
            Template::PythonFastAPI
        );
        assert_eq!(
            "typescript-express".parse::<Template>().unwrap(),
            Template::TypeScriptExpress
        );
        assert!("invalid".parse::<Template>().is_err());
    }

    #[test]
    fn test_template_display() {
        assert_eq!(Template::RustAxum.to_string(), "rust-axum");
        assert_eq!(Template::PythonFastAPI.to_string(), "python-fastapi");
        assert_eq!(
            Template::TypeScriptExpress.to_string(),
            "typescript-express"
        );
    }

    #[tokio::test]
    async fn test_template_dir() {
        let template = Template::RustAxum;
        let dir = template
            .template_dir()
            .await
            .expect("Failed to get template dir");
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.ends_with("templates/rust-axum") || dir_str.ends_with("templates\\rust-axum"),
            "Unexpected template dir: {}",
            dir_str
        );
    }
}
