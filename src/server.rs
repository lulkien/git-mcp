//! MCP server wiring: tool registration and handler dispatch.

use rmcp::ErrorData;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::schemars::JsonSchema;
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::json;

use crate::constants::{SERVER_NAME, SERVER_VERSION};
use crate::error::into_tool_result;
use crate::tools::grouped::{history, status};
use crate::tools::ok_result;

/// The Git MCP server.
#[derive(Clone)]
pub struct GitMcp {
    tool_router: ToolRouter<Self>,
}

impl Default for GitMcp {
    fn default() -> Self {
        Self::new()
    }
}

/// Arguments accepted by `git_ping`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PingArgs {
    /// Message echoed back to the caller.
    #[serde(default = "default_pong")]
    pub message: String,
}

fn default_pong() -> String {
    "pong".to_owned()
}

#[tool_router]
impl GitMcp {
    /// Builds a server with every tool registered.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Verifies the server is running.
    #[tool(
        name = "git_ping",
        description = "Returns a simple response to verify the server is running.",
        annotations(
            title = "Git MCP Ping",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_ping(
        &self,
        Parameters(args): Parameters<PingArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(ok_result(
            format!("git-mcp-server: {}", args.message),
            json!({ "ok": true, "message": args.message }),
        ))
    }

    /// Status and diff tool.
    #[tool(
        name = "git_status",
        description = "Status and diff tool. Use action=status|diff|diff_main to inspect working tree and branch deltas.",
        annotations(
            title = "Git Status Tools",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_status(
        &self,
        Parameters(args): Parameters<status::StatusArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(status::run(&args).await)
    }

    /// History tool.
    #[tool(
        name = "git_history",
        description = "History tool. Use action=log|show|reflog|blame|lg|who to inspect commits, refs, and contributors.",
        annotations(
            title = "Git History Tools",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_history(
        &self,
        Parameters(args): Parameters<history::HistoryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(history::run(&args).await)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GitMcp {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new(SERVER_NAME, SERVER_VERSION);
        info
    }
}
