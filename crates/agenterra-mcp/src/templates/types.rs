//! Template-specific types for code generation.
//!
//! This module defines specialized types used in template rendering contexts,
//! particularly for handling parameters and properties in language-specific ways.
//! These types extend the basic OpenAPI definitions with template-specific metadata
//! needed for generating idiomatic code in different programming languages.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Parameter kind based on OpenAPI "in" field - language agnostic
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterKind {
    Path,
    Query,
    Header,
    Cookie,
}

/// Language-agnostic parameter info with target language type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateParameterInfo {
    pub name: String,
    pub target_type: String,
    pub description: Option<String>,
    pub example: Option<JsonValue>,
    pub kind: ParameterKind,
}
