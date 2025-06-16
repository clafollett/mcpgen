//! Tool registry for managing MCP tool metadata and validation

use crate::error::{ClientError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Information about an MCP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool name (snake_case format)
    pub name: String,
    /// Tool description
    pub description: Option<String>,
    /// Input schema for parameters (JSON Schema)
    pub input_schema: Option<serde_json::Value>,
}

/// Registry for managing tool metadata and validation
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    /// Map of tool name to tool information
    tools: HashMap<String, ToolInfo>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Add a tool to the registry
    pub fn add_tool(&mut self, tool: ToolInfo) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Get tool information by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.get(name)
    }

    /// Check if a tool exists in the registry
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all tool names
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Validate tool parameters against the schema
    pub fn validate_parameters(&self, tool_name: &str, params: &serde_json::Value) -> Result<()> {
        // Check if tool exists
        if !self.has_tool(tool_name) {
            return Err(ClientError::Validation(format!(
                "Unknown tool: '{}'. Available tools: {:?}",
                tool_name,
                self.tool_names()
            )));
        }

        let tool_info = self.get_tool(tool_name).unwrap(); // Safe because we checked above

        // If no schema is defined, accept any parameters
        let Some(schema) = &tool_info.input_schema else {
            return Ok(());
        };

        // Validate basic structure
        self.validate_basic_structure(schema, params, tool_name)?;

        // Validate object parameters if present
        if let Some(params_obj) = params.as_object() {
            self.check_required_fields(schema, params_obj)?;
            self.validate_parameter_types(schema, params_obj)?;
        }

        Ok(())
    }

    /// Validate basic parameter structure (object vs non-object)
    fn validate_basic_structure(
        &self,
        schema: &serde_json::Value,
        params: &serde_json::Value,
        tool_name: &str,
    ) -> Result<()> {
        // Basic type checking - parameters should be an object if schema expects one
        if schema.get("type") == Some(&serde_json::Value::String("object".to_string()))
            && !params.is_object()
            && !params.is_null()
        {
            return Err(ClientError::Validation(format!(
                "Tool '{}' expects object parameters, got: {}",
                tool_name, params
            )));
        }

        // If parameters are null or empty, check for required fields
        if params.is_null() || (params.is_object() && params.as_object().unwrap().is_empty()) {
            if let Some(required) = schema.get("required") {
                if let Some(required_array) = required.as_array() {
                    if !required_array.is_empty() {
                        let required_fields: Vec<String> = required_array
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        return Err(ClientError::Validation(format!(
                            "required parameter '{}' is missing",
                            required_fields.first().unwrap_or(&"unknown".to_string())
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check that all required fields are present
    fn check_required_fields(
        &self,
        schema: &serde_json::Value,
        params_obj: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<()> {
        if let Some(required) = schema.get("required") {
            if let Some(required_array) = required.as_array() {
                for required_field in required_array {
                    if let Some(field_name) = required_field.as_str() {
                        if !params_obj.contains_key(field_name) {
                            return Err(ClientError::Validation(format!(
                                "required parameter '{}' is missing",
                                field_name
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate parameter types against schema
    fn validate_parameter_types(
        &self,
        schema: &serde_json::Value,
        params_obj: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<()> {
        if let Some(properties) = schema.get("properties") {
            if let Some(properties_obj) = properties.as_object() {
                for (param_name, param_value) in params_obj {
                    // Check if parameter is known
                    if !properties_obj.contains_key(param_name) {
                        return Err(ClientError::Validation(format!(
                            "unknown parameter '{}'",
                            param_name
                        )));
                    }

                    // Validate parameter type
                    self.validate_single_parameter_type(
                        param_name,
                        param_value,
                        properties_obj.get(param_name).unwrap(),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Validate a single parameter's type against its schema
    fn validate_single_parameter_type(
        &self,
        param_name: &str,
        param_value: &serde_json::Value,
        param_schema: &serde_json::Value,
    ) -> Result<()> {
        if let Some(expected_type) = param_schema.get("type") {
            if let Some(type_str) = expected_type.as_str() {
                let actual_type = self.get_json_value_type(param_value);

                // Special case: "number" type accepts both integer and number
                let type_matches = match type_str {
                    "number" => actual_type == "number" || actual_type == "integer",
                    _ => type_str == actual_type,
                };

                if !type_matches {
                    return Err(ClientError::Validation(format!(
                        "parameter '{}' should be a {}, got {}",
                        param_name,
                        if type_str == "integer" {
                            "number"
                        } else {
                            type_str
                        },
                        actual_type
                    )));
                }
            }
        }
        Ok(())
    }

    /// Get the JSON Schema type name for a JSON value
    fn get_json_value_type(&self, value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "integer"
                } else {
                    "number"
                }
            }
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }

    /// Update the registry with tools from list_tools response
    pub fn update_from_rmcp_tools(&mut self, rmcp_tools: Vec<rmcp::model::Tool>) {
        self.tools.clear();

        for rmcp_tool in rmcp_tools {
            let tool_info = ToolInfo {
                name: rmcp_tool.name.to_string(),
                description: rmcp_tool.description.map(|d| d.to_string()),
                input_schema: Some(serde_json::Value::Object((*rmcp_tool.input_schema).clone())),
            };
            self.add_tool(tool_info);
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_registry_basic() {
        let mut registry = ToolRegistry::new();

        let tool = ToolInfo {
            name: "get_pet_by_id".to_string(),
            description: Some("Get a pet by ID".to_string()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"}
                },
                "required": ["id"]
            })),
        };

        registry.add_tool(tool);

        assert!(registry.has_tool("get_pet_by_id"));
        assert!(!registry.has_tool("nonexistent_tool"));
        assert_eq!(registry.tool_names(), vec!["get_pet_by_id"]);
    }

    #[test]
    fn test_parameter_validation() {
        let mut registry = ToolRegistry::new();

        let tool = ToolInfo {
            name: "get_pet_by_id".to_string(),
            description: Some("Get a pet by ID".to_string()),
            input_schema: Some(json!({"type": "object"})),
        };

        registry.add_tool(tool);

        // Valid object parameters
        assert!(
            registry
                .validate_parameters("get_pet_by_id", &json!({"id": 123}))
                .is_ok()
        );

        // Valid null parameters
        assert!(
            registry
                .validate_parameters("get_pet_by_id", &json!(null))
                .is_ok()
        );

        // Invalid array parameters when object expected
        assert!(
            registry
                .validate_parameters("get_pet_by_id", &json!([1, 2, 3]))
                .is_err()
        );

        // Unknown tool
        assert!(
            registry
                .validate_parameters("unknown_tool", &json!({}))
                .is_err()
        );
    }
}
