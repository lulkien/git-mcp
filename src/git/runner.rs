//! `git` CLI execution for a single validated repository.
//!
//! The server shells out to the real `git` binary rather than linking a Git
//! library: hooks, credential helpers, signing, LFS filters, and git-flow
//! hooks all depend on the configured Git installation, and error
//! classification matches on Git's own stderr.
//!
//! `stdin` is set to null so a Git process can never consume the MCP protocol
//! stream, and every command runs with the repository as its working directory.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use crate::error::GitError;
use crate::git::client::validate_repo_path;

/// A handle to `git` bound to one repository.
#[derive(Debug, Clone)]
pub struct Git {
    repo: PathBuf,
}

impl Git {
    /// Validates the repository path and returns a handle for it.
    pub fn open(repo_path: impl AsRef<Path>) -> Result<Self, GitError> {
        Ok(Self {
            repo: validate_repo_path(repo_path)?,
        })
    }

    /// Repository root this handle operates on.
    #[must_use]
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// Runs `git <args>` and returns its stdout.
    ///
    /// A non-zero exit becomes a [`GitError`] classified from stderr (falling
    /// back to stdout when Git reported nothing on stderr).
    pub async fn raw<S: AsRef<str>>(&self, args: &[S]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args.iter().map(AsRef::as_ref))
            .current_dir(&self.repo)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|error| GitError::classified(error.to_string()))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout.trim().to_owned()
        } else {
            stderr.trim().to_owned()
        };

        Err(GitError::classified(if message.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            message
        }))
    }
}
