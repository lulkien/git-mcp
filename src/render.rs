//! Response rendering in markdown or JSON.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::GitError;

/// Output format requested by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    /// Human-readable text (default).
    #[default]
    Markdown,
    /// Pretty-printed JSON.
    Json,
}

/// Renders a payload for the `content[].text` field of a tool result.
///
/// `markdown` passes strings through untouched and pretty-prints objects and
/// arrays; `json` always pretty-prints.
pub fn render_content(content: &Value, format: ResponseFormat) -> Result<String, GitError> {
    match format {
        ResponseFormat::Json => pretty(content),
        ResponseFormat::Markdown => match content {
            Value::String(text) => Ok(text.clone()),
            Value::Object(_) | Value::Array(_) => pretty(content),
            other => Err(GitError::classified(format!(
                "Unsupported content type for markdown format: {}",
                type_name(other)
            ))),
        },
    }
}

/// Renders either pre-built markdown or the JSON form of a payload.
pub fn render_markdown_data(
    markdown: &str,
    data: &Value,
    format: ResponseFormat,
) -> Result<String, GitError> {
    match format {
        ResponseFormat::Markdown => Ok(markdown.to_owned()),
        ResponseFormat::Json => pretty(data),
    }
}

fn pretty(value: &Value) -> Result<String, GitError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| GitError::classified(format!("Failed to render content as JSON: {error}")))
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Null | Value::Array(_) | Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn markdown_passes_strings_through() {
        let rendered = render_content(&json!("text"), ResponseFormat::Markdown).unwrap();
        assert_eq!(rendered, "text");
    }

    #[test]
    fn markdown_pretty_prints_objects() {
        let rendered = render_content(&json!({ "a": 1 }), ResponseFormat::Markdown).unwrap();
        assert_eq!(rendered, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn json_format_pretty_prints_scalars() {
        let rendered = render_content(&json!(5), ResponseFormat::Json).unwrap();
        assert_eq!(rendered, "5");
    }

    #[test]
    fn markdown_rejects_scalars() {
        let error = render_content(&json!(5), ResponseFormat::Markdown).expect_err("unsupported");
        assert!(error.message().contains("Unsupported content type"));
    }

    #[test]
    fn markdown_data_prefers_markdown() {
        let rendered =
            render_markdown_data("# hi", &json!({ "a": 1 }), ResponseFormat::Markdown).unwrap();
        assert_eq!(rendered, "# hi");
        let rendered =
            render_markdown_data("# hi", &json!({ "a": 1 }), ResponseFormat::Json).unwrap();
        assert!(rendered.contains("\"a\""));
    }
}
