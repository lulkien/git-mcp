//! Tangled hosting detection.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::config;
use crate::git::Git;
use crate::services::remote::parse_remote_list;

/// Result of checking whether the origin remote is a Tangled host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TangledCheckResult {
    /// True when Tangled awareness is enabled on this server.
    pub enabled: bool,
    /// True when the repository appears to be hosted on Tangled.
    pub tangled: bool,
    /// Guidance for the agent.
    pub guidance: String,
}

static TANGLED_HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"tangled\.org|\.tangled\.|knot").expect("valid tangled pattern"));

/// Detects whether the origin remote points at a Tangled host.
///
/// Tangled is decentralized Git hosting on the AT Protocol: transport works
/// normally, but pull requests only exist in its web UI — there is no documented
/// REST API or CLI for them.
pub async fn check_tangled(repo_path: &Path) -> Result<TangledCheckResult, crate::error::GitError> {
    if !config().allow_tangled {
        return Ok(TangledCheckResult {
            enabled: false,
            tangled: false,
            guidance:
                "Tangled awareness is disabled. Set GIT_ALLOW_TANGLED=true to enable detection."
                    .to_owned(),
        });
    }

    let git = Git::open(repo_path)?;
    let remotes = parse_remote_list(&git.raw(&["remote", "-v"]).await?);
    let origin = remotes
        .iter()
        .find(|remote| remote.name == "origin")
        .or_else(|| remotes.first());
    let url = origin
        .and_then(|remote| remote.fetch_url.as_deref())
        .unwrap_or_default();

    let tangled = TANGLED_HOST.is_match(url);

    Ok(TangledCheckResult {
        enabled: true,
        tangled,
        guidance: if tangled {
            "This repository is hosted on Tangled (decentralized Git hosting on the AT Protocol). \
Git transport (push/pull/clone) works normally via git-mcp tools. Tangled supports pull \
requests, but they are managed through the web UI — no documented REST API or CLI exists \
for creating/merging them, so use git-mcp for repository operations and the Tangled web \
UI for pull requests."
                .to_owned()
        } else {
            "This repository is not hosted on Tangled. Use git-mcp tools normally.".to_owned()
        },
    })
}
