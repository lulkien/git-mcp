//! Entire CLI detection.

use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::config;
use crate::git::external::{has_marker_dir, probe_binary};

/// Result of probing for the Entire CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntireCheckResult {
    /// True when Entire awareness is enabled on this server.
    pub enabled: bool,
    /// True when the `entire` binary responded.
    pub available: bool,
    /// Version reported by `entire --version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// True when the repository has a `.entire/` directory.
    pub managed: bool,
    /// Guidance for the agent.
    pub guidance: String,
}

/// Detects whether the Entire CLI is available and whether the repository is
/// Entire-managed.
///
/// Entire records the context behind agent work (checkpoints, sessions,
/// prompts); git-mcp cannot expose that layer.
pub async fn check_entire(repo_path: &Path) -> EntireCheckResult {
    let server_config = config();

    if !server_config.allow_entire {
        return EntireCheckResult {
            enabled: false,
            available: false,
            version: None,
            managed: false,
            guidance:
                "Entire awareness is disabled. Set GIT_ALLOW_ENTIRE=true to enable detection."
                    .to_owned(),
        };
    }

    let probe = probe_binary(&server_config.entire_binary, &["--version"]).await;
    let managed = has_marker_dir(repo_path, ".entire");

    if !probe.available {
        return EntireCheckResult {
            enabled: true,
            available: false,
            version: None,
            managed,
            guidance: if managed {
                "This repository is Entire-managed (has a .entire/ directory) but the `entire` CLI is not \
installed. Install Entire (https://entire.io) to query session and checkpoint context."
                    .to_owned()
            } else {
                "The `entire` CLI is not installed. Use git-mcp tools for Git operations."
                    .to_owned()
            },
        };
    }

    EntireCheckResult {
        enabled: true,
        available: true,
        version: probe.version,
        managed,
        guidance: if managed {
            "This repository is managed by Entire. Use the `entire` CLI for session, checkpoint, and \
attribution queries (entire why, entire blame, entire search, entire recap). git-mcp tools \
operate on the git repo and do not expose Entire's context layer."
                .to_owned()
        } else {
            "Entire (`entire`) is installed but this repository is not Entire-managed. Use git-mcp tools \
for Git operations."
                .to_owned()
        },
    }
}
