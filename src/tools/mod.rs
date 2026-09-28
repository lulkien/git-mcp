//! MCP tool handlers. Handlers validate input, delegate to a service, and
//! render the response; they contain no Git logic.

pub mod grouped;

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::Value;

/// Builds a successful tool result carrying both rendered text and structured
/// content.
#[must_use]
pub fn ok_result(text: String, structured: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(structured);
    result
}
