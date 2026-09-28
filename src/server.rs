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
use crate::tools::analytics;
use crate::tools::external;
use crate::tools::grouped::{branches, commits, context, history, remotes, status, workspace};
use crate::tools::ok_result;
use crate::tools::rewrite;
use crate::tools::workspace::{
    bisect, cherry_pick, merge, rebase, stash, submodule, tag, worktree,
};

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

    /// Commit-area tool.
    #[tool(
        name = "git_commits",
        description = "Commit-area tool. Use action=add|restore|commit|reset|revert|undo|nuke|wip|unstage|amend.",
        annotations(
            title = "Git Commit Tools",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_commits(
        &self,
        Parameters(args): Parameters<commits::CommitArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(commits::run(&args).await)
    }

    /// Branch tool.
    #[tool(
        name = "git_branches",
        description = "Branch tool. Use action=list|create|delete|rename|checkout|set_upstream|recent for branch workflows.",
        annotations(
            title = "Git Branch Tools",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_branches(
        &self,
        Parameters(args): Parameters<branches::BranchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(branches::run(&args).await)
    }

    /// Remote tool.
    #[tool(
        name = "git_remotes",
        description = "Remote tool. Use action=list|manage|fetch|pull|push for network/transport operations.",
        annotations(
            title = "Git Remote Tools",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn git_remotes(
        &self,
        Parameters(args): Parameters<remotes::RemoteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(remotes::run(&args).await)
    }

    /// Workspace tool.
    #[tool(
        name = "git_workspace",
        description = "Workspace tool for stash/rebase/cherry-pick/merge/bisect/tag/worktree/submodule actions plus stash_all shortcut.",
        annotations(
            title = "Git Workspace Tools",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_workspace(
        &self,
        Parameters(args): Parameters<workspace::WorkspaceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(workspace::run(&args).await)
    }

    /// Context/config tool.
    #[tool(
        name = "git_context",
        description = "Context/config tool. Use action=summary|search|get_config|set_config|aliases for repo context operations.",
        annotations(
            title = "Git Context Tools",
            read_only_hint = false,
            idempotent_hint = true,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_context(
        &self,
        Parameters(args): Parameters<context::ContextArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(context::run(&args).await)
    }

    /// Stash tool.
    #[tool(
        name = "git_stash",
        description = "Stash, list, apply, pop, or drop stashes.",
        annotations(
            title = "Git Stash",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_stash(
        &self,
        Parameters(args): Parameters<stash::StashArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(stash::run(&args).await)
    }

    /// Rebase tool.
    #[tool(
        name = "git_rebase",
        description = "Rebase: start, continue, abort, or skip.",
        annotations(
            title = "Git Rebase",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_rebase(
        &self,
        Parameters(args): Parameters<rebase::RebaseArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(rebase::run(&args).await)
    }

    /// Cherry-pick tool.
    #[tool(
        name = "git_cherry_pick",
        description = "Cherry-pick: start, continue, or abort.",
        annotations(
            title = "Git Cherry-Pick",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_cherry_pick(
        &self,
        Parameters(args): Parameters<cherry_pick::CherryPickArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(cherry_pick::run(&args).await)
    }

    /// Merge tool.
    #[tool(
        name = "git_merge",
        description = "Merge: start, continue, or abort.",
        annotations(
            title = "Git Merge",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_merge(
        &self,
        Parameters(args): Parameters<merge::MergeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(merge::run(&args).await)
    }

    /// Bisect tool.
    #[tool(
        name = "git_bisect",
        description = "Bisect: start, good, bad, skip, run, or reset.",
        annotations(
            title = "Git Bisect",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_bisect(
        &self,
        Parameters(args): Parameters<bisect::BisectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(bisect::run(&args).await)
    }

    /// Tag tool.
    #[tool(
        name = "git_tag",
        description = "Tag: list, create, or delete.",
        annotations(
            title = "Git Tag",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_tag(
        &self,
        Parameters(args): Parameters<tag::TagArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(tag::run(&args).await)
    }

    /// Worktree tool.
    #[tool(
        name = "git_worktree",
        description = "Worktree: add, list, remove, lock, unlock, prune, or repair.",
        annotations(
            title = "Git Worktree",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_worktree(
        &self,
        Parameters(args): Parameters<worktree::WorktreeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(worktree::run(&args).await)
    }

    /// Submodule tool.
    #[tool(
        name = "git_submodule",
        description = "Submodule: add, list, update, sync, or set_branch.",
        annotations(
            title = "Git Submodule",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_submodule(
        &self,
        Parameters(args): Parameters<submodule::SubmoduleArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(submodule::run(&args).await)
    }

    /// Check whether the GitButler CLI is available.
    #[tool(
        name = "git_but_check",
        description = "Detects whether the GitButler `but` CLI is available and returns guidance on whether to prefer `but mcp` / `but` commands over git-mcp tools. When `but` is present, agents should prefer GitButler for version control and run `but teardown` before using git-mcp tools on a GitButler-managed repository.",
        annotations(
            title = "Check GitButler Availability",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_but_check(
        &self,
        Parameters(args): Parameters<external::ButCheckArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(external::run_but(&args).await)
    }

    /// Check whether Jujutsu is available and whether the repo is jj-managed.
    #[tool(
        name = "git_jj_check",
        description = "Detect whether the Jujutsu `jj` CLI is available and whether the repository is jj-managed (has a .jj/ directory). When jj-managed, agents should prefer the `jj` CLI for all version control operations, since git-mcp tools operate on the underlying .git and will not reflect jj's change model.",
        annotations(
            title = "Check Jujutsu Availability",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_jj_check(
        &self,
        Parameters(args): Parameters<external::RepoCheckArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(external::run_jj(&args).await)
    }

    /// Check whether the repository is hosted on Tangled.
    #[tool(
        name = "git_tangled_check",
        description = "Detect whether the repository origin remote points at a Tangled host (tangled.org or a self-hosted knot). Tangled is a decentralized Git host on the AT Protocol — git transport works normally via git-mcp tools, but there is no PR/MR surface.",
        annotations(
            title = "Check Tangled Hosting",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_tangled_check(
        &self,
        Parameters(args): Parameters<external::RepoCheckArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(external::run_tangled(&args).await)
    }

    /// Check whether the Entire CLI is available and whether the repo is managed.
    #[tool(
        name = "git_entire_check",
        description = "Detect whether the Entire CLI is available and whether the repository is Entire-managed (has a .entire/ directory). When managed, agents should use the `entire` CLI for session, checkpoint, and attribution queries; git-mcp tools do not expose Entire's context layer.",
        annotations(
            title = "Check Entire Availability",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_entire_check(
        &self,
        Parameters(args): Parameters<external::RepoCheckArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(external::run_entire(&args).await)
    }

    /// History rewrite tool.
    #[tool(
        name = "git_rewrite",
        description = "Rewrite commit history. Use action=reword|squash|rewrite-messages|backup|restore. History rewriting changes commit hashes and requires force-push; create a backup first.",
        annotations(
            title = "Git History Rewrite",
            read_only_hint = false,
            idempotent_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_rewrite(
        &self,
        Parameters(args): Parameters<rewrite::RewriteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(rewrite::run(&args).await)
    }

    /// Repository analytics tool.
    #[tool(
        name = "git_analytics",
        description = "Read-only repository analytics computed from local git history. Use action=contributors|churn|activity|summary|file-stats.",
        annotations(
            title = "Git Repository Analytics",
            read_only_hint = true,
            idempotent_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_analytics(
        &self,
        Parameters(args): Parameters<analytics::AnalyticsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        into_tool_result(analytics::run(&args).await)
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
