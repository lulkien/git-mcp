//! MCP tool handlers. Handlers validate input, delegate to a service, and
//! render the response; they contain no Git logic.

pub mod analytics;
pub mod docs;
pub mod external;
pub mod grouped;
pub mod lfs;
pub mod rewrite;
pub mod workspace;

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::Value;

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};

/// Builds a successful tool result carrying both rendered text and structured
/// content.
#[must_use]
pub fn ok_result(text: String, structured: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(structured);
    result
}

/// Wraps a plain string service result as `{ "output": ... }`.
pub fn output_result(output: &str, format: ResponseFormat) -> Result<CallToolResult, GitError> {
    let text = render_content(&to_value(output)?, format)?;
    Ok(ok_result(text, serde_json::json!({ "output": output })))
}
