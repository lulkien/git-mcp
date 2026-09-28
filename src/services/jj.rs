//! Jujutsu (`jj`) detection.

use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::config;
use crate::git::external::{has_marker_dir, probe_binary};

/// Result of probing for the Jujutsu CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JjCheckResult {
    /// True when Jujutsu awareness is enabled on this server.
    pub enabled: bool,
    /// True when the `jj` binary responded.
    pub available: bool,
    /// Version reported by `jj --version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// True when the repository has a `.jj/` directory.
    pub managed: bool,
    /// Guidance for the agent.
    pub guidance: String,
}

/// Detects whether `jj` is available and whether the repository is jj-managed.
///
/// Jujutsu keeps its own change model (working-copy commit, bookmarks,
/// operation log) over the underlying `.git`, so plain-Git tools cannot see it.
pub async fn check_jj(repo_path: &Path) -> JjCheckResult {
    let server_config = config();

    if !server_config.allow_jj {
        return JjCheckResult {
            enabled: false,
            available: false,
            version: None,
            managed: false,
            guidance: "Jujutsu awareness is disabled. Set GIT_ALLOW_JJ=true to enable detection of the `jj` CLI.".to_owned(),
        };
    }

    let probe = probe_binary(&server_config.jj_binary, &["--version"]).await;
    let managed = has_marker_dir(repo_path, ".jj");

    if !probe.available {
        return JjCheckResult {
            enabled: true,
            available: false,
            version: None,
            managed,
            guidance: if managed {
                "This repository is jj-managed (has a .jj/ directory) but the `jj` CLI is not installed. \
Install Jujutsu (https://jj-vcs.dev) before making VCS changes."
                    .to_owned()
            } else {
                "The `jj` CLI is not installed. Use git-mcp tools for Git operations.".to_owned()
            },
        };
    }

    JjCheckResult {
        enabled: true,
        available: true,
        version: probe.version,
        managed,
        guidance: if managed {
            "This repository is managed by Jujutsu (`jj`). Prefer the `jj` CLI for all version control \
operations — git-mcp tools operate on the underlying .git and will not reflect jj's \
change model (working-copy commit, bookmarks, operation log)."
                .to_owned()
        } else {
            "Jujutsu (`jj`) is installed but this repository is not jj-managed. Use git-mcp tools for \
Git operations."
                .to_owned()
        },
    }
}
