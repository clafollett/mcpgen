//! Template type definitions and discovery for AgentERRA.
//!
//! This module defines the supported template types and provides functionality
//! for discovering template directories in the filesystem. It supports both
//! built-in templates and custom template paths.
//!
//! # Examples
//!
//! ```
//! use agenterra_mcp::ServerTemplateKind;
//! use std::str::FromStr;
//!
//! // Parse a template from a string
//! let template = ServerTemplateKind::from_str("rust_axum").unwrap();
//! assert_eq!(template, ServerTemplateKind::RustAxum);
//! assert_eq!(template.as_str(), "rust_axum");
//!
//! // You can also use the Display trait
//! assert_eq!(template.to_string(), "rust_axum");
//!
//! // The default template is RustAxum
//! assert_eq!(ServerTemplateKind::default(), ServerTemplateKind::RustAxum);
//! ```
//!
//! For template directory discovery, use the `TemplateDir::discover()` method from the
//! `template_dir` module, which handles finding template directories automatically.
//!
//! # Template Discovery
//!
//! The module searches for templates in the following locations:
//! 1. Directory specified by `AGENTERRA_TEMPLATE_DIR` environment variable
//! 2. `templates/` directory in the project root (for development)
//! 3. `~/.agenterra/templates/` in the user's home directory
//! 4. `/usr/local/share/agenterra/templates/` for system-wide installation
//! 5. `./templates/` in the current working directory

// Internal imports (std, crate)
use std::fmt;
use std::str::FromStr;

/// Template role (server or client)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateRole {
    /// Server-side template
    Server,
    /// Client-side template
    Client,
}

impl TemplateRole {
    /// Returns the role as a string slice
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
        }
    }
}

impl fmt::Display for TemplateRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Server-side template kinds for MCP server generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ServerTemplateKind {
    /// Rust with Axum web framework
    #[default]
    RustAxum,
    /// Python with FastAPI
    PythonFastAPI,
    /// TypeScript with Express
    TypeScriptExpress,
    /// Custom template path
    Custom,
}

/// Client-side template kinds for client library generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClientTemplateKind {
    /// Rust with reqwest HTTP client
    #[default]
    RustReqwest,
    /// Python with requests library
    PythonRequests,
    /// TypeScript with axios library
    TypeScriptAxios,
    /// Custom template path
    Custom,
}

impl FromStr for ServerTemplateKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust_axum" => Ok(ServerTemplateKind::RustAxum),
            "python_fastapi" => Ok(ServerTemplateKind::PythonFastAPI),
            "typescript_express" => Ok(ServerTemplateKind::TypeScriptExpress),
            "custom" => Ok(ServerTemplateKind::Custom),
            _ => Err(format!("Unknown server template kind: {}", s)),
        }
    }
}

impl FromStr for ClientTemplateKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust_reqwest" => Ok(ClientTemplateKind::RustReqwest),
            "python_requests" => Ok(ClientTemplateKind::PythonRequests),
            "typescript_axios" => Ok(ClientTemplateKind::TypeScriptAxios),
            "custom" => Ok(ClientTemplateKind::Custom),
            _ => Err(format!("Unknown client template kind: {}", s)),
        }
    }
}

impl ServerTemplateKind {
    /// Returns the template identifier as a string slice
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RustAxum => "rust_axum",
            Self::PythonFastAPI => "python_fastapi",
            Self::TypeScriptExpress => "typescript_express",
            Self::Custom => "custom",
        }
    }

    /// Returns the template role (always server)
    pub fn role(&self) -> TemplateRole {
        TemplateRole::Server
    }

    /// Returns the language/framework name
    pub fn framework(&self) -> &'static str {
        match self {
            Self::RustAxum => "rust",
            Self::PythonFastAPI => "python",
            Self::TypeScriptExpress => "typescript",
            Self::Custom => "custom",
        }
    }

    /// Returns an iterator over all available server template kinds
    pub fn all() -> impl Iterator<Item = Self> {
        use ServerTemplateKind::*;
        [RustAxum, PythonFastAPI, TypeScriptExpress, Custom]
            .iter()
            .copied()
    }
}

impl ClientTemplateKind {
    /// Returns the template identifier as a string slice
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RustReqwest => "rust_reqwest",
            Self::PythonRequests => "python_requests",
            Self::TypeScriptAxios => "typescript_axios",
            Self::Custom => "custom",
        }
    }

    /// Returns the template role (always client)
    pub fn role(&self) -> TemplateRole {
        TemplateRole::Client
    }

    /// Returns the language/framework name
    pub fn framework(&self) -> &'static str {
        match self {
            Self::RustReqwest => "rust",
            Self::PythonRequests => "python",
            Self::TypeScriptAxios => "typescript",
            Self::Custom => "custom",
        }
    }

    /// Returns an iterator over all available client template kinds
    pub fn all() -> impl Iterator<Item = Self> {
        use ClientTemplateKind::*;
        [RustReqwest, PythonRequests, TypeScriptAxios, Custom]
            .iter()
            .copied()
    }
}

impl fmt::Display for ServerTemplateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for ClientTemplateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ServerTemplateKind tests
    #[test]
    fn test_server_as_str() {
        assert_eq!(ServerTemplateKind::RustAxum.as_str(), "rust_axum");
        assert_eq!(ServerTemplateKind::PythonFastAPI.as_str(), "python_fastapi");
        assert_eq!(
            ServerTemplateKind::TypeScriptExpress.as_str(),
            "typescript_express"
        );
        assert_eq!(ServerTemplateKind::Custom.as_str(), "custom");
    }

    #[test]
    fn test_server_display() {
        assert_eq!(format!("{}", ServerTemplateKind::RustAxum), "rust_axum");
        assert_eq!(
            format!("{}", ServerTemplateKind::PythonFastAPI),
            "python_fastapi"
        );
        assert_eq!(
            format!("{}", ServerTemplateKind::TypeScriptExpress),
            "typescript_express"
        );
        assert_eq!(format!("{}", ServerTemplateKind::Custom), "custom");
    }

    #[test]
    fn test_server_from_str() {
        assert_eq!(
            "rust_axum".parse::<ServerTemplateKind>().unwrap(),
            ServerTemplateKind::RustAxum
        );
        assert_eq!(
            "python_fastapi".parse::<ServerTemplateKind>().unwrap(),
            ServerTemplateKind::PythonFastAPI
        );
        assert_eq!(
            "typescript_express".parse::<ServerTemplateKind>().unwrap(),
            ServerTemplateKind::TypeScriptExpress
        );
        assert_eq!(
            "custom".parse::<ServerTemplateKind>().unwrap(),
            ServerTemplateKind::Custom
        );

        // Test case insensitivity
        assert_eq!(
            "RUST_AXUM".parse::<ServerTemplateKind>().unwrap(),
            ServerTemplateKind::RustAxum
        );

        // Test invalid variants
        assert!("invalid".parse::<ServerTemplateKind>().is_err());
        assert!("rust_reqwest".parse::<ServerTemplateKind>().is_err()); // Client template
    }

    #[test]
    fn test_server_default() {
        assert_eq!(ServerTemplateKind::default(), ServerTemplateKind::RustAxum);
    }

    #[test]
    fn test_server_all() {
        let all_kinds: Vec<_> = ServerTemplateKind::all().collect();
        assert_eq!(all_kinds.len(), 4);

        let unique_kinds: HashSet<_> = ServerTemplateKind::all().collect();
        assert_eq!(unique_kinds.len(), 4);

        assert!(unique_kinds.contains(&ServerTemplateKind::RustAxum));
        assert!(unique_kinds.contains(&ServerTemplateKind::PythonFastAPI));
        assert!(unique_kinds.contains(&ServerTemplateKind::TypeScriptExpress));
        assert!(unique_kinds.contains(&ServerTemplateKind::Custom));
    }

    #[test]
    fn test_server_role() {
        assert_eq!(ServerTemplateKind::RustAxum.role(), TemplateRole::Server);
        assert_eq!(
            ServerTemplateKind::PythonFastAPI.role(),
            TemplateRole::Server
        );
        assert_eq!(
            ServerTemplateKind::TypeScriptExpress.role(),
            TemplateRole::Server
        );
        assert_eq!(ServerTemplateKind::Custom.role(), TemplateRole::Server);
    }

    #[test]
    fn test_server_framework() {
        assert_eq!(ServerTemplateKind::RustAxum.framework(), "rust");
        assert_eq!(ServerTemplateKind::PythonFastAPI.framework(), "python");
        assert_eq!(
            ServerTemplateKind::TypeScriptExpress.framework(),
            "typescript"
        );
        assert_eq!(ServerTemplateKind::Custom.framework(), "custom");
    }

    // ClientTemplateKind tests
    #[test]
    fn test_client_as_str() {
        assert_eq!(ClientTemplateKind::RustReqwest.as_str(), "rust_reqwest");
        assert_eq!(
            ClientTemplateKind::PythonRequests.as_str(),
            "python_requests"
        );
        assert_eq!(
            ClientTemplateKind::TypeScriptAxios.as_str(),
            "typescript_axios"
        );
        assert_eq!(ClientTemplateKind::Custom.as_str(), "custom");
    }

    #[test]
    fn test_client_display() {
        assert_eq!(
            format!("{}", ClientTemplateKind::RustReqwest),
            "rust_reqwest"
        );
        assert_eq!(
            format!("{}", ClientTemplateKind::PythonRequests),
            "python_requests"
        );
        assert_eq!(
            format!("{}", ClientTemplateKind::TypeScriptAxios),
            "typescript_axios"
        );
        assert_eq!(format!("{}", ClientTemplateKind::Custom), "custom");
    }

    #[test]
    fn test_client_from_str() {
        assert_eq!(
            "rust_reqwest".parse::<ClientTemplateKind>().unwrap(),
            ClientTemplateKind::RustReqwest
        );
        assert_eq!(
            "python_requests".parse::<ClientTemplateKind>().unwrap(),
            ClientTemplateKind::PythonRequests
        );
        assert_eq!(
            "typescript_axios".parse::<ClientTemplateKind>().unwrap(),
            ClientTemplateKind::TypeScriptAxios
        );
        assert_eq!(
            "custom".parse::<ClientTemplateKind>().unwrap(),
            ClientTemplateKind::Custom
        );

        // Test case insensitivity
        assert_eq!(
            "RUST_REQWEST".parse::<ClientTemplateKind>().unwrap(),
            ClientTemplateKind::RustReqwest
        );

        // Test invalid variants
        assert!("invalid".parse::<ClientTemplateKind>().is_err());
        assert!("rust_axum".parse::<ClientTemplateKind>().is_err()); // Server template
    }

    #[test]
    fn test_client_default() {
        assert_eq!(
            ClientTemplateKind::default(),
            ClientTemplateKind::RustReqwest
        );
    }

    #[test]
    fn test_client_all() {
        let all_kinds: Vec<_> = ClientTemplateKind::all().collect();
        assert_eq!(all_kinds.len(), 4);

        let unique_kinds: HashSet<_> = ClientTemplateKind::all().collect();
        assert_eq!(unique_kinds.len(), 4);

        assert!(unique_kinds.contains(&ClientTemplateKind::RustReqwest));
        assert!(unique_kinds.contains(&ClientTemplateKind::PythonRequests));
        assert!(unique_kinds.contains(&ClientTemplateKind::TypeScriptAxios));
        assert!(unique_kinds.contains(&ClientTemplateKind::Custom));
    }

    #[test]
    fn test_client_role() {
        assert_eq!(ClientTemplateKind::RustReqwest.role(), TemplateRole::Client);
        assert_eq!(
            ClientTemplateKind::PythonRequests.role(),
            TemplateRole::Client
        );
        assert_eq!(
            ClientTemplateKind::TypeScriptAxios.role(),
            TemplateRole::Client
        );
        assert_eq!(ClientTemplateKind::Custom.role(), TemplateRole::Client);
    }

    #[test]
    fn test_client_framework() {
        assert_eq!(ClientTemplateKind::RustReqwest.framework(), "rust");
        assert_eq!(ClientTemplateKind::PythonRequests.framework(), "python");
        assert_eq!(
            ClientTemplateKind::TypeScriptAxios.framework(),
            "typescript"
        );
        assert_eq!(ClientTemplateKind::Custom.framework(), "custom");
    }

    // TemplateRole tests
    #[test]
    fn test_template_role_as_str() {
        assert_eq!(TemplateRole::Server.as_str(), "server");
        assert_eq!(TemplateRole::Client.as_str(), "client");
    }

    #[test]
    fn test_template_role_display() {
        assert_eq!(format!("{}", TemplateRole::Server), "server");
        assert_eq!(format!("{}", TemplateRole::Client), "client");
    }
}
