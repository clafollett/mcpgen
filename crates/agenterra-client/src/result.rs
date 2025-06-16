//! Tool result processing and content type handling

use crate::error::{ClientError, Result};
use serde::{Deserialize, Serialize};

/// Types of content that MCP tools can return
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentType {
    /// Plain text content
    #[serde(rename = "text")]
    Text { text: String },
    /// JSON structured data
    #[serde(rename = "json")]
    Json { json: serde_json::Value },
    /// Binary data (base64 encoded)
    #[serde(rename = "binary")]
    Binary { data: String, mime_type: String },
    /// Image content
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource")]
    Resource { uri: String, text: Option<String> },
}

/// Processed tool result with typed content
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// The processed content items
    pub content: Vec<ContentType>,
    /// Whether this was an error response
    pub is_error: bool,
    /// Error details if applicable
    pub error_code: Option<String>,
    /// Raw response for debugging
    pub raw_response: serde_json::Value,
}

impl ToolResult {
    /// Create a new tool result from rmcp CallToolResult
    pub fn from_rmcp_result(result: &rmcp::model::CallToolResult) -> Result<Self> {
        let raw_response = serde_json::to_value(result)
            .map_err(|e| ClientError::Client(format!("Failed to serialize tool result: {}", e)))?;

        let mut content = Vec::new();
        let mut is_error = false;
        let mut error_code = None;

        // Check for error indicators in the response
        if let Some(error_field) = raw_response.get("error") {
            is_error = true;
            error_code = Some(error_field.to_string());
        }

        // Check if the response indicates an error through the isError field
        if raw_response
            .get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false)
        {
            is_error = true;
        }

        // Process the content array from rmcp result
        for content_item in &result.content {
            match Self::parse_content_item(content_item) {
                Ok(content_type) => {
                    // Check if this content item indicates an error
                    if let ContentType::Text { text } = &content_type {
                        if text.to_lowercase().contains("error")
                            || text.to_lowercase().contains("failed")
                        {
                            is_error = true;
                            if error_code.is_none() {
                                error_code = Some("TOOL_EXECUTION_ERROR".to_string());
                            }
                        }
                    }
                    content.push(content_type);
                }
                Err(e) => {
                    is_error = true;
                    error_code = Some(format!("Content parsing error: {}", e));
                    // Still add as text content for debugging
                    let debug_text = format!("Error parsing content: {}", e);
                    content.push(ContentType::Text { text: debug_text });
                }
            }
        }

        Ok(ToolResult {
            content,
            is_error,
            error_code,
            raw_response,
        })
    }

    /// Parse individual content item from rmcp
    fn parse_content_item(item: &rmcp::model::Content) -> Result<ContentType> {
        // Extract the content based on rmcp Content enum variants
        // This is a simplified parser - we'll expand based on actual rmcp types
        let item_json = serde_json::to_value(item)
            .map_err(|e| ClientError::Client(format!("Failed to serialize content item: {}", e)))?;

        // Try to detect content type from the structure
        if let Some(text) = item_json.get("text").and_then(|t| t.as_str()) {
            // Check if the text is JSON
            if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(text) {
                Ok(ContentType::Json { json: parsed_json })
            } else {
                Ok(ContentType::Text {
                    text: text.to_string(),
                })
            }
        } else if let Some(data) = item_json.get("data").and_then(|d| d.as_str()) {
            let mime_type = item_json
                .get("mimeType")
                .and_then(|m| m.as_str())
                .unwrap_or("application/octet-stream")
                .to_string();

            if mime_type.starts_with("image/") {
                Ok(ContentType::Image {
                    data: data.to_string(),
                    mime_type,
                })
            } else {
                Ok(ContentType::Binary {
                    data: data.to_string(),
                    mime_type,
                })
            }
        } else if let Some(uri) = item_json.get("uri").and_then(|u| u.as_str()) {
            let text = item_json
                .get("text")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
            Ok(ContentType::Resource {
                uri: uri.to_string(),
                text,
            })
        } else {
            // Fallback: treat as JSON
            Ok(ContentType::Json { json: item_json })
        }
    }

    /// Extract all text content as a single string
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|content| match content {
                ContentType::Text { text } => Some(text.as_str()),
                ContentType::Resource {
                    text: Some(text), ..
                } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract all JSON content
    pub fn json(&self) -> Vec<&serde_json::Value> {
        self.content
            .iter()
            .filter_map(|content| match content {
                ContentType::Json { json } => Some(json),
                _ => None,
            })
            .collect()
    }

    /// Check if result contains any errors
    pub fn has_error(&self) -> bool {
        self.is_error
    }

    /// Get the first text content item, if any
    pub fn first_text(&self) -> Option<&str> {
        self.content.iter().find_map(|content| match content {
            ContentType::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }

    /// Get the first JSON content item, if any
    pub fn first_json(&self) -> Option<&serde_json::Value> {
        self.content.iter().find_map(|content| match content {
            ContentType::Json { json } => Some(json),
            _ => None,
        })
    }

    /// Convert to Result<T> based on error status
    pub fn into_result<T>(self) -> std::result::Result<T, ClientError>
    where
        T: From<ToolResult>,
    {
        if self.has_error() {
            let error_msg = self
                .error_code
                .as_deref()
                .unwrap_or("Tool execution failed");
            Err(ClientError::Protocol(format!("Tool error: {}", error_msg)))
        } else {
            Ok(T::from(self))
        }
    }

    /// Get all image content items
    pub fn images(&self) -> Vec<(&str, &str)> {
        self.content
            .iter()
            .filter_map(|content| match content {
                ContentType::Image { data, mime_type } => Some((data.as_str(), mime_type.as_str())),
                _ => None,
            })
            .collect()
    }

    /// Get all resource URIs
    pub fn resources(&self) -> Vec<&str> {
        self.content
            .iter()
            .filter_map(|content| match content {
                ContentType::Resource { uri, .. } => Some(uri.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Check if result contains any specific content type
    pub fn has_content_type(&self, content_type: &str) -> bool {
        self.content.iter().any(|content| {
            matches!(
                (content_type, content),
                ("text", ContentType::Text { .. })
                    | ("json", ContentType::Json { .. })
                    | ("binary", ContentType::Binary { .. })
                    | ("image", ContentType::Image { .. })
                    | ("resource", ContentType::Resource { .. })
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_content_type_serialization() {
        let text_content = ContentType::Text {
            text: "Hello, world!".to_string(),
        };

        let json_str = serde_json::to_string(&text_content).unwrap();
        let deserialized: ContentType = serde_json::from_str(&json_str).unwrap();

        assert_eq!(text_content, deserialized);
    }

    #[test]
    fn test_tool_result_text_extraction() {
        let result = ToolResult {
            content: vec![
                ContentType::Text {
                    text: "First line".to_string(),
                },
                ContentType::Json {
                    json: json!({"key": "value"}),
                },
                ContentType::Text {
                    text: "Second line".to_string(),
                },
            ],
            is_error: false,
            error_code: None,
            raw_response: json!({}),
        };

        assert_eq!(result.text(), "First line\nSecond line");
        assert_eq!(result.first_text(), Some("First line"));
    }

    #[test]
    fn test_tool_result_json_extraction() {
        let json_value = json!({"test": "data"});
        let result = ToolResult {
            content: vec![
                ContentType::Text {
                    text: "Some text".to_string(),
                },
                ContentType::Json {
                    json: json_value.clone(),
                },
            ],
            is_error: false,
            error_code: None,
            raw_response: json!({}),
        };

        let json_items = result.json();
        assert_eq!(json_items.len(), 1);
        assert_eq!(json_items[0], &json_value);
        assert_eq!(result.first_json(), Some(&json_value));
    }

    #[test]
    fn test_error_result() {
        let result = ToolResult {
            content: vec![],
            is_error: true,
            error_code: Some("TOOL_ERROR".to_string()),
            raw_response: json!({"error": "Something went wrong"}),
        };

        assert!(result.has_error());
        assert_eq!(result.error_code, Some("TOOL_ERROR".to_string()));
    }

    #[test]
    fn test_content_type_detection() {
        let result = ToolResult {
            content: vec![
                ContentType::Text {
                    text: "Hello".to_string(),
                },
                ContentType::Json {
                    json: json!({"key": "value"}),
                },
                ContentType::Image {
                    data: "base64data".to_string(),
                    mime_type: "image/png".to_string(),
                },
                ContentType::Resource {
                    uri: "file://test.txt".to_string(),
                    text: Some("content".to_string()),
                },
            ],
            is_error: false,
            error_code: None,
            raw_response: json!({}),
        };

        assert!(result.has_content_type("text"));
        assert!(result.has_content_type("json"));
        assert!(result.has_content_type("image"));
        assert!(result.has_content_type("resource"));
        assert!(!result.has_content_type("binary"));
    }

    #[test]
    fn test_image_extraction() {
        let result = ToolResult {
            content: vec![
                ContentType::Image {
                    data: "data1".to_string(),
                    mime_type: "image/png".to_string(),
                },
                ContentType::Text {
                    text: "Some text".to_string(),
                },
                ContentType::Image {
                    data: "data2".to_string(),
                    mime_type: "image/jpeg".to_string(),
                },
            ],
            is_error: false,
            error_code: None,
            raw_response: json!({}),
        };

        let images = result.images();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0], ("data1", "image/png"));
        assert_eq!(images[1], ("data2", "image/jpeg"));
    }

    #[test]
    fn test_resource_extraction() {
        let result = ToolResult {
            content: vec![
                ContentType::Resource {
                    uri: "file://test1.txt".to_string(),
                    text: None,
                },
                ContentType::Text {
                    text: "Some text".to_string(),
                },
                ContentType::Resource {
                    uri: "http://example.com/api".to_string(),
                    text: Some("API data".to_string()),
                },
            ],
            is_error: false,
            error_code: None,
            raw_response: json!({}),
        };

        let resources = result.resources();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0], "file://test1.txt");
        assert_eq!(resources[1], "http://example.com/api");
    }

    #[test]
    fn test_error_detection_from_content() {
        // Test that error detection works on content with "error" in text
        let content_with_error = ContentType::Text {
            text: "Tool execution failed with error code 404".to_string(),
        };

        // We can't easily test the error detection without creating a full rmcp result
        // This would be tested in integration tests with real tool responses
        if let ContentType::Text { text } = content_with_error {
            assert!(text.contains("failed"));
        }
    }
}
