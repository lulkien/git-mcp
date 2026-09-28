//! GitButler (`but`) detection.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::config;
use crate::git::external::probe_binary;

/// Result of probing for the GitButler CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ButCheckResult {
    /// True when GitButler awareness is enabled on this server.
    pub enabled: bool,
    /// True when the `but` binary responded.
    pub available: bool,
    /// Version reported by `but --version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Guidance for the agent.
    pub guidance: String,
}

/// Detects whether GitButler's `but` CLI is available and returns guidance.
///
/// GitButler layers a virtual-branch workspace over Git, so plain-`git` writes
/// can fight its branch layout: when `but` is present, agents should prefer it
/// and run `but teardown` before using plain Git tools.
pub async fn check_but() -> ButCheckResult {
    let server_config = config();

    if !server_config.allow_but {
        return ButCheckResult {
            enabled: false,
            available: false,
            version: None,
            guidance: "GitButler awareness is disabled. Set GIT_ALLOW_BUT=true to enable detection of the `but` CLI.".to_owned(),
        };
    }

    let probe = probe_binary(&server_config.but_binary, &["--version"]).await;

    if !probe.available {
        return ButCheckResult {
            enabled: true,
            available: false,
            version: None,
            guidance: "The `but` CLI is not installed. Use git-mcp tools for Git operations. \
Install GitButler (https://gitbutler.com) to enable GitButler-managed workflows."
                .to_owned(),
        };
    }

    ButCheckResult {
        enabled: true,
        available: true,
        version: probe.version,
        guidance: "GitButler (`but`) is available. Prefer the `but` CLI or `but mcp` server for version control \
in GitButler-managed repositories. If you must use git-mcp tools on a GitButler-managed repo, \
run `but teardown` first so plain-Git writes do not conflict with GitButler virtual branches."
            .to_owned(),
    }
}
