//! OpenAPI specification parsing and utilities.
//!
//! This module provides functionality for loading and querying OpenAPI specifications.
//! It supports loading from files and provides convenient accessors for common fields.
//!
//! # Examples
//!
//! ```no_run
//! use mcpgen_core::openapi::OpenAPISpec;
//! use mcpgen_core::error::Result;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! // Load an OpenAPI spec from a file
//! let spec = OpenAPISpec::from_file("openapi.json").await?;
//!
//! // Access common fields
//! if let Some(title) = spec.title() {
//!     println!("API Title: {}", title);
//! }
//! if let Some(version) = spec.version() {
//!     println!("API Version: {}", version);
//! }
//! # Ok(())
//! # }
//! ```

use crate::Error;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::path::Path;
use tokio::fs;

/// Represents an OpenAPI specification
#[derive(Debug)]
pub struct OpenAPISpec {
    /// The raw JSON value of the OpenAPI spec
    pub json: JsonValue,
}

/// Info about a single OpenAPI parameter
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParameterInfo {
    /// Name of the parameter as defined in the OpenAPI spec
    pub name: String,
    /// Corresponding Rust type for the parameter
    pub rust_type: String,
    /// Optional description of the parameter
    pub description: Option<String>,
    /// Optional example value for the parameter
    pub example: Option<JsonValue>,
}

/// Info about a single response property
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyInfo {
    /// Name of the property as defined in the OpenAPI schema
    pub name: String,
    /// Corresponding Rust type for the property
    pub rust_type: String,
    /// Optional title metadata for the property
    pub title: Option<String>,
    /// Optional description of the property
    pub description: Option<String>,
    /// Optional example value for the property
    pub example: Option<JsonValue>,
}

/// Parsed endpoint context for template rendering
#[derive(Debug, Serialize, Deserialize)]
pub struct EndpointContext {
    /// Identifier for the endpoint (path with slashes replaced by '_')
    pub endpoint: String,
    /// Uppercase form of the endpoint for type names
    pub endpoint_cap: String,
    /// Name of the generated function for the endpoint
    pub fn_name: String,
    /// Name of the generated parameters struct (e.g., 'users_params')
    pub parameters_type: String,
    /// Name of the generated properties struct
    pub properties_type: String,
    /// Name of the generated response struct
    pub response_type: String,
    /// Raw JSON object representing the response schema properties
    pub envelope_properties: JsonValue,
    /// Typed response property information
    pub properties: Vec<PropertyInfo>,
    /// Names of properties to pass into handler functions
    pub properties_for_handler: Vec<String>,
    /// Typed list of parameters for the endpoint
    pub parameters: Vec<ParameterInfo>,
    /// Summary of the endpoint
    pub summary: String,
    /// Description of the endpoint
    pub description: String,
    /// Tags associated with the endpoint
    pub tags: Vec<String>,
    /// Schema reference for the properties
    pub properties_schema: JsonMap<String, JsonValue>,
    /// Schema reference for the response
    pub response_schema: JsonValue,
    /// Name of the spec file (if loaded from a file)
    pub spec_file_name: Option<String>,
    /// Valid fields for the endpoint
    pub valid_fields: Vec<String>,
}

impl OpenAPISpec {
    /// Create a new OpenAPISpec from a file (supports both YAML and JSON)
    pub async fn from_file<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).await?;

        // Try to parse as JSON first
        if let Ok(json) = serde_json::from_str(&content) {
            return Ok(Self { json });
        }

        // If JSON parsing fails, try YAML
        if let Ok(json) = serde_yaml::from_str(&content) {
            return Ok(Self { json });
        }

        // If both parsers fail, return an error
        Err(crate::Error::openapi(format!(
            "Failed to parse OpenAPI spec at {}: invalid JSON or YAML",
            path.display()
        )))
    }

    /// Get a reference to the raw JSON value
    pub fn as_json(&self) -> &JsonValue {
        &self.json
    }

    /// Get the title of the API
    pub fn title(&self) -> Option<&str> {
        self.json.get("info")?.get("title")?.as_str()
    }

    /// Get the version of the API
    pub fn version(&self) -> Option<&str> {
        self.json.get("info")?.get("version")?.as_str()
    }

    /// Get the base path of the API
    pub fn base_path(&self) -> Option<&str> {
        self.json
            .get("servers")?
            .as_array()?
            .first()?
            .get("url")?
            .as_str()
    }

    /// Parse all endpoints into structured contexts for template rendering
    pub async fn parse_endpoints(&self) -> crate::Result<Vec<EndpointContext>> {
        let mut contexts = Vec::new();
        // Expect 'paths' object
        let paths = self
            .json
            .get("paths")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| Error::openapi("Missing 'paths' object"))?;
        for (path, item) in paths {
            // Only handle GET operations for now
            if item.get("get").is_none() {
                continue;
            }
            let endpoint = path.trim_start_matches('/').replace('/', "_");
            // Extract metadata
            let (summary, description, tags) = OpenAPISpec::extract_operation_metadata(item);
            // Typed parameters and properties
            let param_infos = self.extract_parameter_info(item);
            let (props_json, spec_file) = self.extract_properties_json_value(item, path)?;
            let property_infos = OpenAPISpec::extract_property_info(&props_json);
            // Derive properties_schema as JSON map for template
            let properties_schema = match props_json.as_object() {
                Some(map) => map.clone(),
                None => JsonMap::default(),
            };
            // Build schema reference
            let response_schema =
                OpenAPISpec::build_response_schema(&format!("{}Response", endpoint));
            // Assemble context
            let ctx = EndpointContext {
                endpoint: endpoint.clone(),
                endpoint_cap: endpoint.to_uppercase(),
                fn_name: endpoint.clone(),
                parameters_type: format!("{}Params", endpoint),
                properties_type: format!("{}Properties", endpoint),
                response_type: format!("{}Response", endpoint),
                envelope_properties: props_json.clone(),
                properties: property_infos.clone(),
                properties_for_handler: property_infos.iter().map(|p| p.name.clone()).collect(),
                parameters: param_infos.clone(),
                summary,
                description,
                tags,
                properties_schema,
                response_schema,
                spec_file_name: spec_file,
                valid_fields: Vec::new(),
            };
            contexts.push(ctx);
        }
        Ok(contexts)
    }

    fn extract_operation_metadata(path_item: &JsonValue) -> (String, String, Vec<String>) {
        let mut summary = String::new();
        let mut description = String::new();
        let mut tags: Vec<String> = Vec::new();
        if let Some(get_item) = path_item.get("get") {
            if let Some(obj) = get_item.as_object() {
                let raw_summary = obj
                    .get("summary")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("")
                    .to_string();
                summary = OpenAPISpec::sanitize_markdown(&raw_summary);
                let raw_description = obj
                    .get("description")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("")
                    .to_string();
                description = OpenAPISpec::sanitize_markdown(&raw_description);
                tags = obj
                    .get("tags")
                    .and_then(JsonValue::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(JsonValue::as_str)
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();
            }
        }
        (summary, description, tags)
    }

    fn extract_parameters_for_handler(&self, path_item: &JsonValue) -> Vec<JsonValue> {
        let mut parameters: Vec<JsonValue> = Vec::new();
        if let Some(get_item) = path_item.get("get").and_then(JsonValue::as_object) {
            if let Some(seq) = get_item.get("parameters").and_then(JsonValue::as_array) {
                // Path parameters first
                for param in seq
                    .iter()
                    .filter(|p| p.get("in").and_then(JsonValue::as_str) == Some("path"))
                {
                    if let Some(ref_str) = param.get("$ref").and_then(JsonValue::as_str) {
                        if let Some(resolved) = self.json.pointer(&ref_str[1..]) {
                            parameters.push(resolved.clone());
                        }
                    } else {
                        parameters.push(param.clone());
                    }
                }
                // Query parameters next
                for param in seq
                    .iter()
                    .filter(|p| p.get("in").and_then(JsonValue::as_str) == Some("query"))
                {
                    if let Some(ref_str) = param.get("$ref").and_then(JsonValue::as_str) {
                        if let Some(resolved) = self.json.pointer(&ref_str[1..]) {
                            parameters.push(resolved.clone());
                        }
                    } else {
                        parameters.push(param.clone());
                    }
                }
            }
        }
        parameters
    }

    fn build_response_schema(properties_type: &str) -> JsonValue {
        serde_json::json!({ "$ref": format!("#/components/schemas/{}", properties_type) })
    }

    fn extract_properties_json_value(
        &self,
        path_item: &JsonValue,
        endpoint: &str,
    ) -> crate::Result<(JsonValue, Option<String>)> {
        let get_item = path_item
            .get("get")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| {
                Error::openapi(format!("No GET operation for endpoint '{}'", endpoint))
            })?;
        let response = get_item
            .get("responses")
            .and_then(JsonValue::as_object)
            .and_then(|m| m.get("200"))
            .and_then(JsonValue::as_object)
            .ok_or_else(|| {
                Error::openapi(format!("No 200 response for endpoint '{}'", endpoint))
            })?;
        let content = response
            .get("content")
            .and_then(JsonValue::as_object)
            .and_then(|m| m.get("application/json"))
            .and_then(JsonValue::as_object)
            .ok_or_else(|| {
                Error::openapi(format!("No application/json content for '{}'", endpoint))
            })?;
        let schema = content
            .get("schema")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| Error::openapi(format!("No schema in content for '{}'", endpoint)))?;
        let ref_str = schema
            .get("$ref")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| Error::openapi(format!("No $ref in schema for '{}'", endpoint)))?;
        let key = "#/components/schemas/";
        if !ref_str.starts_with(key) {
            return Err(Error::openapi(format!(
                "Unexpected schema ref '{}'",
                ref_str
            )));
        }
        let name = &ref_str[key.len()..];
        let schemas = self
            .json
            .get("components")
            .and_then(JsonValue::as_object)
            .and_then(|m| m.get("schemas"))
            .and_then(JsonValue::as_object)
            .ok_or_else(|| Error::openapi("No components.schemas section"))?;
        let def = schemas
            .get(name)
            .cloned()
            .ok_or_else(|| Error::openapi(format!("Schema '{}' not found", name)))?;
        let props = def.get("properties").cloned().unwrap_or(JsonValue::Null);
        Ok((props, None))
    }

    fn extract_row_properties(properties_json: &JsonValue) -> Vec<JsonValue> {
        if let Some(data) = properties_json.get("data").and_then(JsonValue::as_object) {
            if let Some(props) = data.get("properties").and_then(JsonValue::as_object) {
                return props
                    .iter()
                    .map(|(k, v)| json!({"name": k, "schema": v}))
                    .collect();
            }
        }
        if let Some(props) = properties_json.as_object() {
            return props
                .iter()
                .map(|(k, v)| json!({"name": k, "schema": v}))
                .collect();
        }
        Vec::new()
    }

    /// Extract typed parameter info for a handler
    fn extract_parameter_info(&self, path_item: &JsonValue) -> Vec<ParameterInfo> {
        self.extract_parameters_for_handler(path_item)
            .into_iter()
            .map(|param| {
                let name = param
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let description = param
                    .get("description")
                    .and_then(JsonValue::as_str)
                    .map(String::from);
                let example = param.get("example").cloned();
                ParameterInfo {
                    name: name.clone(),
                    rust_type: "String".to_string(),
                    description,
                    example,
                }
            })
            .collect()
    }

    /// Extract typed property info from properties JSON
    fn extract_property_info(properties_json: &JsonValue) -> Vec<PropertyInfo> {
        OpenAPISpec::extract_row_properties(properties_json)
            .into_iter()
            .map(|prop| {
                let name = prop
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let schema = prop.get("schema");
                let title = schema
                    .and_then(|s| s.get("title"))
                    .and_then(JsonValue::as_str)
                    .map(String::from);
                let description = schema
                    .and_then(|s| s.get("description"))
                    .and_then(JsonValue::as_str)
                    .map(String::from);
                let example = schema.and_then(|s| s.get("example")).cloned();
                PropertyInfo {
                    name: name.clone(),
                    rust_type: "String".to_string(),
                    title,
                    description,
                    example,
                }
            })
            .collect()
    }

    /// Sanitizes Markdown for Rust doc comments and Swagger UI.
    fn sanitize_markdown(input: &str) -> String {
        use regex::Regex;
        // Regex for problematic Unicode (e.g., smart quotes, em-dash)
        let unicode_re = Regex::new(r"[\u2018\u2019\u201C\u201D\u2014]").unwrap();
        // Regex to collapse any whitespace sequence into a single space
        let ws_re = Regex::new(r"\s+").unwrap();
        input
            .lines()
            .map(|line| {
                let mut line = line.replace('\t', " ");
                // Remove problematic Unicode
                line = unicode_re
                    .replace_all(&line, |caps: &regex::Captures| match &caps[0] {
                        "\u{2018}" | "\u{2019}" => "'",
                        "\u{201C}" | "\u{201D}" => "\"",
                        "\u{2014}" => "-",
                        _ => "",
                    })
                    .to_string();
                // Trim edges and collapse inner whitespace
                let mut trimmed = ws_re.replace_all(&line.trim(), " ").to_string();
                // Remove spaces around hyphens
                trimmed = trimmed
                    .replace(" - ", "-")
                    .replace("- ", "-")
                    .replace(" -", "-");
                // Escape backslashes and quotes
                let mut safe = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
                // Escape braces and brackets
                safe = safe
                    .replace("{", "&#123;")
                    .replace("}", "&#125;")
                    .replace("[", "&#91;")
                    .replace("]", "&#93;");
                safe
            })
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_from_file() -> crate::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("openapi_async.json");
        let json_content = r#"
        {
            "openapi": "3.0.0",
            "info": {
                "title": "Test API Async",
                "version": "2.0.0"
            },
            "servers": [
                {
                    "url": "https://api.example.com/v2"
                }
            ]
        }
        "#;
        tokio::fs::write(&file_path, json_content).await?;

        let spec = OpenAPISpec::from_file(&file_path).await?;
        assert_eq!(spec.title(), Some("Test API Async"));
        assert_eq!(spec.version(), Some("2.0.0"));
        assert_eq!(spec.base_path(), Some("https://api.example.com/v2"));

        Ok(())
    }

    #[test]
    fn test_extract_operation_metadata() {
        let path_item =
            json!({"get": {"summary": "sum", "description": "desc", "tags": ["a","b"]}});
        let (sum, desc, tags) = OpenAPISpec::extract_operation_metadata(&path_item);
        assert_eq!(sum, "sum");
        assert_eq!(desc, "desc");
        assert_eq!(tags, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_extract_parameters_for_handler() {
        let spec = OpenAPISpec { json: json!({}) };
        let path_item = json!({"get": {"parameters": [{"name": "p", "in": "query"}]}});
        let params = spec.extract_parameters_for_handler(&path_item);
        assert_eq!(params, vec![json!({"name": "p", "in": "query"})]);
    }

    #[test]
    fn test_build_response_schema() {
        let schema = OpenAPISpec::build_response_schema("X");
        assert_eq!(schema, json!({"$ref": "#/components/schemas/X"}));
    }

    #[test]
    fn test_extract_properties_json_value() {
        let json = json!({
            "components": { "schemas": { "T": { "properties": { "a": {"type":"string"} } } } },
            "paths": {}
        });
        let spec = OpenAPISpec { json };
        let path_item = json!({"get": {"responses": {"200": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/T"}}}}}}});
        let (props, file) = spec
            .extract_properties_json_value(&path_item, "/x")
            .unwrap();
        assert_eq!(file, None);
        assert_eq!(props, json!({"a": {"type":"string"}}));
    }

    #[test]
    fn test_extract_row_properties() {
        let props = json!({"data": {"properties": {"k": 1, "m": 2}}});
        let rows = OpenAPISpec::extract_row_properties(&props);
        let names: Vec<_> = rows
            .iter()
            .filter_map(|r| r.get("name").and_then(JsonValue::as_str))
            .collect();
        assert_eq!(names, vec!["k", "m"]);
    }

    #[test]
    fn test_extract_row_properties_direct() {
        let props = json!({"x": {"type": "string"}, "y": {"type": "integer"}});
        let rows = OpenAPISpec::extract_row_properties(&props);
        let mut names: Vec<_> = rows
            .iter()
            .filter_map(|r| r.get("name").and_then(JsonValue::as_str))
            .collect();
        names.sort();
        assert_eq!(names, vec!["x", "y"]);
    }

    #[test]
    fn test_sanitize_markdown_basic() {
        let raw = "Line one\n\nLine two";
        assert_eq!(OpenAPISpec::sanitize_markdown(raw), "Line one Line two");
    }

    #[test]
    fn test_sanitize_markdown_escape_and_unicode() {
        let raw = "\"hi\" {x} “quote” — dash";
        let out = OpenAPISpec::sanitize_markdown(raw);
        // Check escapes
        assert!(out.contains("\\\"hi\\\""));
        assert!(out.contains("&#123;x&#125;"));
        // Unicode replaced
        assert!(!out.contains("“"));
        assert!(!out.contains("—"));
    }

    #[test]
    fn test_extract_operation_metadata_trims_and_sanitizes() {
        let path_item = json!({"get": {"summary": " sum \n next", "description": " desc-\tline", "tags": ["t"]}});
        let (s, d, tags) = OpenAPISpec::extract_operation_metadata(&path_item);
        assert_eq!(s, "sum next");
        assert_eq!(d, "desc-line");
        assert_eq!(tags, vec!["t".to_string()]);
    }

    #[test]
    fn test_extract_parameters_ordering() {
        let spec = OpenAPISpec { json: json!({}) };
        let path_item = json!({"get": {"parameters": [
            {"name": "q", "in": "query"},
            {"name": "p", "in": "path"}
        ]}});
        let names: Vec<String> = spec
            .extract_parameters_for_handler(&path_item)
            .into_iter()
            .filter_map(|p| p.get("name").and_then(JsonValue::as_str).map(String::from))
            .collect();
        assert_eq!(names, vec!["p".to_string(), "q".to_string()]);
    }
}
