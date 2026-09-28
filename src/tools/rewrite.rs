//! `git_rewrite` — history rewriting with confirm gating.

use std::collections::BTreeMap;
use std::path::Path;

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::rewrite::{
    BackupOptions, RewordOptions, RewriteMessagesOptions, SquashOptions, create_backup,
    restore_backup, reword_commit, rewrite_messages, squash_commits,
};
use crate::tools::output_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RewriteAction {
    /// Rewrite a single commit message.
    Reword,
    /// Collapse the last N commits into one.
    Squash,
    /// Replace messages across a range from an explicit map.
    RewriteMessages,
    /// Create a backup branch at HEAD.
    Backup,
    /// Reset hard to a backup branch.
    Restore,
}

impl RewriteAction {
    /// Wire representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reword => "reword",
            Self::Squash => "squash",
            Self::RewriteMessages => "rewrite-messages",
            Self::Backup => "backup",
            Self::Restore => "restore",
        }
    }

    /// Confirmation message for the actions that rewrite history.
    fn confirm_requirement(self) -> Option<&'static str> {
        match self {
            Self::Backup => None,
            Self::Restore => {
                Some("restore requires confirm=true because it performs a hard reset.")
            }
            Self::Reword => Some("reword requires confirm=true because it rewrites history."),
            Self::Squash => Some("squash requires confirm=true because it rewrites history."),
            Self::RewriteMessages => {
                Some("rewrite-messages requires confirm=true because it rewrites history.")
            }
        }
    }
}

/// Arguments accepted by `git_rewrite`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RewriteArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// History rewrite operation.
    pub action: RewriteAction,
    /// Commit ref to reword (defaults to HEAD).
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// New commit message (reword/squash).
    #[serde(default)]
    pub message: Option<String>,
    /// Number of commits to squash.
    #[serde(default)]
    pub count: Option<u32>,
    /// Commit range to rewrite (e.g. `HEAD~5..HEAD`).
    #[serde(default)]
    pub range: Option<String>,
    /// Map of commit SHA to replacement message (rewrite-messages).
    #[serde(default)]
    pub messages: Option<BTreeMap<String, String>>,
    /// Backup branch name (backup/restore).
    #[serde(default)]
    pub name: Option<String>,
    /// Confirm destructive history rewriting.
    #[serde(default)]
    pub confirm: bool,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

// `ref` is a Rust keyword, so the field is `ref_name` and renamed on the wire.
impl RewriteArgs {
    fn reference(&self) -> Option<&str> {
        self.ref_name.as_deref()
    }
}

/// Performs the requested history rewrite.
pub async fn run(args: &RewriteArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    require_confirm(args)?;
    let output = dispatch(args, &repo_path).await?;
    output_result(&output, args.response_format)
}

fn require_confirm(args: &RewriteArgs) -> Result<(), GitError> {
    if let Some(requirement) = args.action.confirm_requirement()
        && !args.confirm
    {
        return Err(GitError::invalid_input(requirement));
    }
    Ok(())
}

async fn dispatch(args: &RewriteArgs, repo_path: &Path) -> Result<String, GitError> {
    let action = args.action;

    match action {
        RewriteAction::Backup => {
            let name = required(args.name.as_deref(), "name", action)?;
            create_backup(repo_path, &BackupOptions { name }).await
        }
        RewriteAction::Restore => {
            let name = required(args.name.as_deref(), "name", action)?;
            restore_backup(repo_path, &BackupOptions { name }).await
        }
        RewriteAction::Reword => {
            let message = required(args.message.as_deref(), "message", action)?;
            reword_commit(
                repo_path,
                &RewordOptions {
                    reference: args.reference().map(str::to_owned),
                    message,
                },
            )
            .await
        }
        RewriteAction::Squash => {
            let message = required(args.message.as_deref(), "message", action)?;
            let Some(count) = args.count else {
                return Err(missing("count", action));
            };
            squash_commits(repo_path, &SquashOptions { count, message }).await
        }
        RewriteAction::RewriteMessages => {
            let range = required(args.range.as_deref(), "range", action)?;
            let messages = args.messages.clone().unwrap_or_default();
            if messages.is_empty() {
                return Err(GitError::invalid_input(
                    "messages mapping is required for rewrite-messages.",
                ));
            }
            rewrite_messages(repo_path, &RewriteMessagesOptions { range, messages }).await
        }
    }
}

fn required(value: Option<&str>, field: &str, action: RewriteAction) -> Result<String, GitError> {
    match value {
        Some(value) if !value.is_empty() => Ok(value.to_owned()),
        _ => Err(missing(field, action)),
    }
}

fn missing(field: &str, action: RewriteAction) -> GitError {
    GitError::invalid_input(format!("{field} is required for {}.", action.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn parse<T: DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).expect("arguments deserialize")
    }

    #[test]
    fn action_wire_names() {
        assert_eq!(RewriteAction::Reword.as_str(), "reword");
        assert_eq!(RewriteAction::RewriteMessages.as_str(), "rewrite-messages");
        assert_eq!(
            serde_json::to_value(RewriteAction::RewriteMessages).unwrap(),
            json!("rewrite-messages")
        );
    }

    #[test]
    fn backup_needs_no_confirmation_but_others_do() {
        let args: RewriteArgs = parse(json!({ "action": "backup", "name": "b" }));
        assert!(require_confirm(&args).is_ok());

        let args: RewriteArgs = parse(json!({ "action": "squash", "message": "m", "count": 2 }));
        let error = require_confirm(&args).expect_err("confirm required");
        assert_eq!(
            error.message(),
            "squash requires confirm=true because it rewrites history."
        );
    }

    #[test]
    fn parses_the_wire_ref_parameter() {
        let args: RewriteArgs = parse(json!({ "action": "reword", "ref": "HEAD~2" }));
        assert_eq!(args.reference(), Some("HEAD~2"));
    }
}
