//! Advanced service: stash, bisect and tag operations.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::config;
use crate::error::GitError;
use crate::git::{Git, validate_path_argument};
use crate::security::assert_safe_ref;

/// Action selected by the stash operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StashAction {
    /// Save the working tree to a new stash.
    Save,
    /// List stashes (default).
    #[default]
    List,
    /// Apply a stash, keeping it.
    Apply,
    /// Apply a stash and drop it.
    Pop,
    /// Delete a stash.
    Drop,
}

/// Options for a stash operation.
#[derive(Debug, Clone, Default)]
pub struct StashOptions {
    /// Operation to perform.
    pub action: StashAction,
    /// Stash message.
    pub message: Option<String>,
    /// Stash index.
    pub index: Option<usize>,
    /// Include untracked files.
    pub include_untracked: bool,
}

/// Action selected by the bisect operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BisectAction {
    /// Start a bisect session (default).
    #[default]
    Start,
    /// Mark a commit as good.
    Good,
    /// Mark a commit as bad.
    Bad,
    /// Skip the current commit.
    Skip,
    /// Run a command on each step.
    Run,
    /// End the bisect session.
    Reset,
}

/// Options for a bisect operation.
#[derive(Debug, Clone, Default)]
pub struct BisectOptions {
    /// Operation to perform.
    pub action: BisectAction,
    /// Commit to mark.
    pub reference: Option<String>,
    /// Known-good commit.
    pub good_ref: Option<String>,
    /// Known-bad commit.
    pub bad_ref: Option<String>,
    /// Single executable token for `bisect run`.
    pub command: Option<String>,
    /// Full argv for `bisect run`.
    pub command_args: Option<Vec<String>>,
}

/// Action selected by the tag operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TagAction {
    /// List tags (default).
    #[default]
    List,
    /// Create a tag.
    Create,
    /// Delete a tag.
    Delete,
}

/// Options for a tag operation.
#[derive(Debug, Clone, Default)]
pub struct TagOptions {
    /// Operation to perform.
    pub action: TagAction,
    /// Tag name.
    pub name: Option<String>,
    /// Commit the tag points at.
    pub target: Option<String>,
    /// Annotation message.
    pub message: Option<String>,
    /// Sign the tag; defaults to the server auto-sign setting.
    pub sign: Option<bool>,
    /// Signing key, falling back to the configured default.
    pub signing_key: Option<String>,
}

static SHELL_META_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // `git bisect run` executes its argument through a shell, so every argument
    // must be free of shell metacharacters.
    Regex::new(r"[;&|`$<>()\[\]{}]").expect("valid shell metacharacter pattern")
});

static WHITESPACE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s").expect("valid whitespace pattern"));

/// Saves, lists, applies, pops or drops a stash.
pub async fn run_stash(repo_path: &Path, options: &StashOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        StashAction::Save => {
            let mut args = vec!["stash".to_owned(), "push".to_owned()];
            if options.include_untracked {
                args.push("--include-untracked".to_owned());
            }
            if let Some(message) = &options.message {
                args.push("-m".to_owned());
                args.push(message.clone());
            }
            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Stash saved."))
        }
        StashAction::List => {
            let output = git.raw(&["stash", "list"]).await?;
            Ok(fallback(output.trim(), "No stashes."))
        }
        action => {
            let index = options.index.unwrap_or(0);
            let reference = format!("stash@{{{index}}}");
            let verb = match action {
                StashAction::Apply => "apply",
                StashAction::Pop => "pop",
                _ => "drop",
            };
            let output = git.raw(&["stash", verb, &reference]).await?;
            let placeholder = match action {
                StashAction::Apply => format!("Applied {reference}."),
                StashAction::Pop => format!("Popped {reference}."),
                _ => format!("Dropped {reference}."),
            };
            Ok(fallback(output.trim(), &placeholder))
        }
    }
}

/// Drives a `git bisect` session.
pub async fn run_bisect(repo_path: &Path, options: &BisectOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        BisectAction::Start => {
            let (Some(bad_ref), Some(good_ref)) = (&options.bad_ref, &options.good_ref) else {
                return Err(GitError::invalid_input(
                    "goodRef and badRef are required for bisect start.",
                ));
            };
            let bad_ref = assert_safe_ref(bad_ref, "bad_ref")?;
            let good_ref = assert_safe_ref(good_ref, "good_ref")?;

            git.raw(&["bisect", "start"]).await?;
            git.raw(&["bisect", "bad", &bad_ref]).await?;
            git.raw(&["bisect", "good", &good_ref]).await?;
            Ok(format!(
                "Bisect started between good={good_ref} and bad={bad_ref}."
            ))
        }
        BisectAction::Good | BisectAction::Bad | BisectAction::Skip => {
            let op = match options.action {
                BisectAction::Good => "good",
                BisectAction::Bad => "bad",
                _ => "skip",
            };
            let mut args = vec!["bisect".to_owned(), op.to_owned()];
            if let Some(reference) = &options.reference {
                args.push(assert_safe_ref(reference, "ref")?);
            }
            let output = git.raw(&args).await?;
            let placeholder = match options.action {
                BisectAction::Good => "Marked current commit as good.",
                BisectAction::Bad => "Marked current commit as bad.",
                _ => "Skipped current bisect commit.",
            };
            Ok(fallback(output.trim(), placeholder))
        }
        BisectAction::Run => {
            let command_args = resolve_bisect_run_args(options)?;
            let mut args = vec!["bisect".to_owned(), "run".to_owned()];
            args.extend(command_args);
            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Bisect run completed."))
        }
        BisectAction::Reset => {
            let output = git.raw(&["bisect", "reset"]).await?;
            Ok(fallback(output.trim(), "Bisect reset."))
        }
    }
}

/// Validates the argv handed to `git bisect run`.
fn resolve_bisect_run_args(options: &BisectOptions) -> Result<Vec<String>, GitError> {
    let command_args = options.command_args.clone().unwrap_or_else(|| {
        options
            .command
            .as_ref()
            .map(|command| vec![command.clone()])
            .unwrap_or_default()
    });

    if command_args.is_empty() {
        return Err(GitError::invalid_input(
            "command_args (or command) is required for bisect run.",
        ));
    }

    for argument in &command_args {
        if SHELL_META_PATTERN.is_match(argument) {
            return Err(GitError::invalid_input(format!(
                "bisect run argument contains shell metacharacters: {argument}"
            )));
        }
    }

    if let Some(command) = &options.command
        && WHITESPACE_PATTERN.is_match(command)
    {
        return Err(GitError::invalid_input(
            "command must be a single executable token. Use command_args to pass arguments.",
        ));
    }

    Ok(command_args)
}

/// Lists, creates or deletes tags.
pub async fn run_tag(repo_path: &Path, options: &TagOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let server_config = config();

    match options.action {
        TagAction::List => {
            let output = git.raw(&["tag"]).await?;
            let tags: Vec<&str> = output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect();
            Ok(if tags.is_empty() {
                "No tags.".to_owned()
            } else {
                tags.join("\n")
            })
        }
        TagAction::Delete => {
            let Some(name) = &options.name else {
                return Err(GitError::invalid_input(
                    "name is required for delete action.",
                ));
            };
            let name = assert_safe_ref(name, "tag name")?;
            git.raw(&["tag", "-d", &name]).await?;
            Ok(format!("Deleted tag {name}."))
        }
        TagAction::Create => {
            let Some(name) = &options.name else {
                return Err(GitError::invalid_input(
                    "name is required for create action.",
                ));
            };
            let name = assert_safe_ref(name, "tag name")?;
            let target = options
                .target
                .as_deref()
                .map(|target| assert_safe_ref(target, "target"))
                .transpose()?;

            let should_sign = options.sign.unwrap_or(server_config.auto_sign_tags);
            if should_sign {
                let key = options
                    .signing_key
                    .clone()
                    .or_else(|| server_config.signing_key.clone());
                let mut args = vec!["tag".to_owned()];
                match key {
                    Some(key) => {
                        args.push("-u".to_owned());
                        args.push(key);
                    }
                    None => args.push("-s".to_owned()),
                }
                args.push("-m".to_owned());
                args.push(options.message.clone().unwrap_or_else(|| name.clone()));
                args.push(name.clone());
                if let Some(target) = target {
                    args.push(target);
                }
                git.raw(&args).await?;
                return Ok(format!("Created signed tag {name}."));
            }

            if let Some(message) = &options.message {
                let mut args = vec!["tag".to_owned(), "-a".to_owned(), name.clone()];
                args.push("-m".to_owned());
                args.push(message.clone());
                if let Some(target) = target {
                    args.push(target);
                }
                git.raw(&args).await?;
                return Ok(format!("Created annotated tag {name}."));
            }

            let mut args = vec!["tag".to_owned(), name.clone()];
            if let Some(target) = target {
                args.push(target);
            }
            git.raw(&args).await?;
            Ok(format!("Created tag {name}."))
        }
    }
}

/// Returns `value` when non-empty, otherwise the placeholder.
#[must_use]
pub fn fallback(value: &str, placeholder: &str) -> String {
    if value.is_empty() {
        placeholder.to_owned()
    } else {
        value.to_owned()
    }
}

/// Validates a worktree path: absolute or relative, but never an option.
pub fn validate_worktree_path(path: &str, name: &str) -> Result<String, GitError> {
    crate::security::assert_safe_arg(path, name)
}

/// Convenience wrapper for a single validated repository-relative path.
pub fn validate_single_path(repo_path: &Path, path: &str) -> Result<String, GitError> {
    validate_path_argument(repo_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_metacharacters_in_bisect_run() {
        let options = BisectOptions {
            action: BisectAction::Run,
            command_args: Some(vec!["cargo".to_owned(), "test;rm -rf /".to_owned()]),
            ..BisectOptions::default()
        };
        let error = resolve_bisect_run_args(&options).expect_err("metacharacters rejected");
        assert!(error.message().contains("shell metacharacters"));
    }

    #[test]
    fn rejects_multi_token_command() {
        let options = BisectOptions {
            action: BisectAction::Run,
            command: Some("cargo test".to_owned()),
            ..BisectOptions::default()
        };
        let error = resolve_bisect_run_args(&options).expect_err("whitespace rejected");
        assert!(error.message().contains("single executable token"));
    }

    #[test]
    fn requires_argv_for_bisect_run() {
        let options = BisectOptions {
            action: BisectAction::Run,
            ..BisectOptions::default()
        };
        let error = resolve_bisect_run_args(&options).expect_err("argv required");
        assert!(error.message().contains("command_args"));
    }

    #[test]
    fn accepts_a_plain_command() {
        let options = BisectOptions {
            action: BisectAction::Run,
            command: Some("cargo".to_owned()),
            command_args: None,
            ..BisectOptions::default()
        };
        assert_eq!(resolve_bisect_run_args(&options).unwrap(), vec!["cargo"]);
    }

    #[test]
    fn derives_placeholders() {
        assert_eq!(fallback("", "Stash saved."), "Stash saved.");
        assert_eq!(fallback(" out ", "Stash saved."), " out ");
    }
}
