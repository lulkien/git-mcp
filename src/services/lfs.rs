//! Git LFS service: tracking, status, transfer, hooks and migration.

use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::GitError;
use crate::git::Git;
use crate::security::assert_safe_arg;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LfsAction {
    /// Track file patterns.
    Track,
    /// Stop tracking file patterns.
    Untrack,
    /// List LFS-tracked files.
    LsFiles,
    /// Show LFS status.
    Status,
    /// Download LFS objects.
    Pull,
    /// Upload LFS objects.
    Push,
    /// Install LFS hooks for the repository.
    Install,
    /// Import existing files into LFS.
    MigrateImport,
    /// Export files out of LFS.
    MigrateExport,
}

/// Options for an LFS operation.
#[derive(Debug, Clone)]
pub struct LfsOptions {
    /// Operation to perform.
    pub action: LfsAction,
    /// File patterns for `track` and `untrack`.
    pub patterns: Option<Vec<String>>,
    /// Remote name for `pull` and `push`.
    pub remote: Option<String>,
    /// Comma-separated include patterns for migrate and pull.
    pub include: Option<String>,
    /// Comma-separated exclude patterns for migrate and pull.
    pub exclude: Option<String>,
    /// Rewrite all refs (`--everything` / `--all`).
    pub everything: bool,
}

/// Runs an LFS operation.
pub async fn run_lfs_action(repo_path: &Path, options: &LfsOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        LfsAction::Install => {
            let output = git.raw(&["lfs", "install"]).await?;
            Ok(fallback(
                output.trim(),
                "Git LFS installed for this repository.",
            ))
        }
        LfsAction::Track | LfsAction::Untrack => {
            let verb = if options.action == LfsAction::Track {
                "track"
            } else {
                "untrack"
            };
            let patterns = required_patterns(options, verb)?;
            let mut args = vec!["lfs".to_owned(), verb.to_owned()];
            args.extend(patterns.iter().cloned());
            let output = git.raw(&args).await?;

            let placeholder = format!(
                "{}: {}",
                if options.action == LfsAction::Track {
                    "Tracking"
                } else {
                    "Untracked"
                },
                patterns.join(", ")
            );
            Ok(fallback(output.trim(), &placeholder))
        }
        LfsAction::LsFiles => {
            let output = git.raw(&["lfs", "ls-files"]).await?;
            Ok(fallback(output.trim(), "No LFS-tracked files found."))
        }
        LfsAction::Status => {
            let output = git.raw(&["lfs", "status"]).await?;
            Ok(fallback(output.trim(), "No LFS status changes."))
        }
        LfsAction::Pull => {
            let mut args = vec!["lfs".to_owned(), "pull".to_owned()];
            if let Some(remote) = &options.remote {
                args.push(assert_safe_arg(remote, "remote")?);
            }
            args.extend(include_exclude_args(options)?);
            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "LFS pull complete."))
        }
        LfsAction::Push => {
            let Some(remote) = &options.remote else {
                return Err(GitError::invalid_input("remote is required for lfs push."));
            };
            let remote = assert_safe_arg(remote, "remote")?;
            let mut args = vec!["lfs".to_owned(), "push".to_owned(), remote.clone()];
            if options.everything {
                args.push("--all".to_owned());
            }
            let output = git.raw(&args).await?;
            Ok(fallback(
                output.trim(),
                &format!("LFS push to {remote} complete."),
            ))
        }
        LfsAction::MigrateImport | LfsAction::MigrateExport => {
            let verb = if options.action == LfsAction::MigrateImport {
                "import"
            } else {
                "export"
            };
            let mut args = vec!["lfs".to_owned(), "migrate".to_owned(), verb.to_owned()];
            if options.everything {
                args.push("--everything".to_owned());
            }
            args.extend(include_exclude_args(options)?);
            let output = git.raw(&args).await?;
            Ok(fallback(
                output.trim(),
                &format!("LFS migrate {verb} complete."),
            ))
        }
    }
}

fn required_patterns(options: &LfsOptions, action: &str) -> Result<Vec<String>, GitError> {
    let patterns = options.patterns.clone().unwrap_or_default();
    if patterns.is_empty() {
        return Err(GitError::invalid_input(format!(
            "patterns is required for lfs {action}."
        )));
    }
    patterns
        .iter()
        .map(|pattern| assert_safe_arg(pattern, "pattern"))
        .collect()
}

fn include_exclude_args(options: &LfsOptions) -> Result<Vec<String>, GitError> {
    let mut args = Vec::new();
    if let Some(include) = &options.include {
        args.push("--include".to_owned());
        args.push(assert_safe_arg(include, "include")?);
    }
    if let Some(exclude) = &options.exclude {
        args.push("--exclude".to_owned());
        args.push(assert_safe_arg(exclude, "exclude")?);
    }
    Ok(args)
}

fn fallback(value: &str, placeholder: &str) -> String {
    if value.is_empty() {
        placeholder.to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(patterns: Option<Vec<String>>) -> LfsOptions {
        LfsOptions {
            action: LfsAction::Track,
            patterns,
            remote: None,
            include: None,
            exclude: None,
            everything: false,
        }
    }

    #[test]
    fn track_requires_patterns() {
        let error = required_patterns(&options(None), "track").expect_err("patterns required");
        assert_eq!(error.message(), "patterns is required for lfs track.");

        let error =
            required_patterns(&options(Some(vec![])), "track").expect_err("patterns required");
        assert!(error.message().contains("patterns is required"));
    }

    #[test]
    fn rejects_option_like_patterns() {
        let error = required_patterns(&options(Some(vec!["--force".to_owned()])), "track")
            .expect_err("option rejected");
        assert!(error.message().contains("cannot start with"));
    }

    #[test]
    fn accepts_glob_patterns() {
        let patterns =
            required_patterns(&options(Some(vec!["*.psd".to_owned()])), "track").unwrap();
        assert_eq!(patterns, vec!["*.psd"]);
    }

    #[test]
    fn builds_include_and_exclude_flags() {
        let mut opts = options(None);
        opts.include = Some("*.zip".to_owned());
        opts.exclude = Some("*.png".to_owned());
        assert_eq!(
            include_exclude_args(&opts).unwrap(),
            vec!["--include", "*.zip", "--exclude", "*.png"]
        );
    }
}
