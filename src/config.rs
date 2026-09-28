//! Startup configuration parsed from the environment and the command line.
//!
//! The CLI accepts
//! `--repo <path>`, `--repo-path <path>` and the `--repo-path=<path>` form;
//! unknown arguments are ignored so MCP clients can pass extra flags.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::constants::error_templates;
use crate::error::GitError;

/// Forge providers understood by the PR tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeProviderName {
    GitHub,
    GitLab,
    Forgejo,
    Gitea,
    Bitbucket,
}

impl ForgeProviderName {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            "forgejo" => Some(Self::Forgejo),
            "gitea" => Some(Self::Gitea),
            "bitbucket" => Some(Self::Bitbucket),
            _ => None,
        }
    }

    /// Wire name of the provider.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Forgejo => "forgejo",
            Self::Gitea => "gitea",
            Self::Bitbucket => "bitbucket",
        }
    }
}

/// Parsed server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Server-level default repository path (`GIT_REPO_PATH` / `--repo-path`).
    pub default_repo_path: Option<PathBuf>,
    /// `no_verify` accepted on commits and pushes.
    pub allow_no_verify: bool,
    /// `force` accepted on pushes.
    pub allow_force_push: bool,
    /// `git-flow` hooks and filters may be executed.
    pub allow_flow_hooks: bool,
    /// `GitButler` awareness tools enabled.
    pub allow_but: bool,
    /// Jujutsu awareness tools enabled.
    pub allow_jj: bool,
    /// `but` executable override.
    pub but_binary: String,
    /// `jj` executable override.
    pub jj_binary: String,
    /// Tangled awareness tools enabled.
    pub allow_tangled: bool,
    /// Entire awareness tools enabled.
    pub allow_entire: bool,
    /// `entire` executable override.
    pub entire_binary: String,
    /// GitHub API token fallback for PR operations.
    pub github_token: Option<String>,
    /// GitLab API token fallback for PR operations.
    pub gitlab_token: Option<String>,
    /// Forgejo API token fallback for PR operations.
    pub forgejo_token: Option<String>,
    /// Bitbucket API token fallback for PR operations.
    pub bitbucket_token: Option<String>,
    /// Explicit forge provider override for self-hosted instances.
    pub forge_provider: Option<ForgeProviderName>,
    /// Default signing key for commits and tags.
    pub signing_key: Option<String>,
    /// Signing format (`openpgp` | `ssh` | `x509`).
    pub signing_format: Option<String>,
    /// Sign commits produced by this server.
    pub auto_sign_commits: bool,
    /// Sign tags produced by this server.
    pub auto_sign_tags: bool,
}

impl Config {
    /// Reads configuration from the process environment and CLI arguments.
    #[must_use]
    pub fn from_env_and_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let cli_repo_path = parse_cli_repo_path(args);
        let raw_repo_path = env_string("GIT_REPO_PATH").or(cli_repo_path);
        let default_repo_path = raw_repo_path.map(|path| absolute_path(Path::new(&path)));

        Self {
            default_repo_path,
            allow_no_verify: env_bool("GIT_ALLOW_NO_VERIFY"),
            allow_force_push: env_bool("GIT_ALLOW_FORCE_PUSH"),
            allow_flow_hooks: env_bool("GIT_ALLOW_FLOW_HOOKS"),
            allow_but: env_bool("GIT_ALLOW_BUT"),
            allow_jj: env_bool("GIT_ALLOW_JJ"),
            but_binary: env_string("BUT_BINARY").unwrap_or_else(|| "but".to_owned()),
            jj_binary: env_string("JJ_BINARY").unwrap_or_else(|| "jj".to_owned()),
            allow_tangled: env_bool("GIT_ALLOW_TANGLED"),
            allow_entire: env_bool("GIT_ALLOW_ENTIRE"),
            entire_binary: env_string("ENTIRE_BINARY").unwrap_or_else(|| "entire".to_owned()),
            github_token: env_string("GITHUB_TOKEN"),
            gitlab_token: env_string("GITLAB_TOKEN"),
            forgejo_token: env_string("FORGEJO_TOKEN"),
            bitbucket_token: env_string("BITBUCKET_TOKEN"),
            forge_provider: env_string("GIT_FORGE_PROVIDER")
                .as_deref()
                .and_then(ForgeProviderName::parse),
            signing_key: env_string("GIT_SIGNING_KEY"),
            signing_format: env_string("GIT_SIGNING_FORMAT"),
            auto_sign_commits: env_bool("GIT_AUTO_SIGN_COMMITS"),
            auto_sign_tags: env_bool("GIT_AUTO_SIGN_TAGS"),
        }
    }

    /// Resolves the repository path for a request, falling back to the server
    /// default, or fails with the shared "no repository path" message.
    pub fn resolve_repo_path(&self, repo_path: Option<&str>) -> Result<PathBuf, GitError> {
        match repo_path.or_else(|| self.default_repo_path.as_deref().and_then(Path::to_str)) {
            Some(path) => Ok(PathBuf::from(path)),
            None => Err(GitError::classified(error_templates::NO_REPO_PATH)),
        }
    }

    /// Tokens configured for every supported forge.
    #[must_use]
    pub fn forge_token(&self, provider: ForgeProviderName) -> Option<&str> {
        match provider {
            ForgeProviderName::GitHub => self.github_token.as_deref(),
            ForgeProviderName::GitLab => self.gitlab_token.as_deref(),
            ForgeProviderName::Forgejo | ForgeProviderName::Gitea => self.forgejo_token.as_deref(),
            ForgeProviderName::Bitbucket => self.bitbucket_token.as_deref(),
        }
    }

    /// Startup banner lines describing enabled non-default behaviour.
    #[must_use]
    pub fn startup_notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(path) = &self.default_repo_path {
            notes.push(format!("default repository path: {}", path.display()));
        }
        if self.allow_no_verify {
            notes.push("hook bypass enabled (GIT_ALLOW_NO_VERIFY=true)".to_owned());
        }
        if self.allow_force_push {
            notes.push("force push enabled (GIT_ALLOW_FORCE_PUSH=true)".to_owned());
        }
        if self.allow_flow_hooks {
            notes.push("git_flow hooks/filters enabled (GIT_ALLOW_FLOW_HOOKS=true)".to_owned());
        }
        if self.auto_sign_commits {
            notes.push("auto-signing commits (GIT_AUTO_SIGN_COMMITS=true)".to_owned());
        }
        if self.auto_sign_tags {
            notes.push("auto-signing tags (GIT_AUTO_SIGN_TAGS=true)".to_owned());
        }
        if self.signing_key.is_some() {
            notes.push("signing key configured".to_owned());
        }
        notes
    }
}

/// Process-wide configuration, initialized once from the environment.
pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| Config::from_env_and_args(env::args().skip(1)))
}

/// Resolves a repository path against the server default.
pub fn resolve_repo_path(repo_path: Option<&str>) -> Result<PathBuf, GitError> {
    config().resolve_repo_path(repo_path)
}

/// Parses a boolean env value, accepting true/1/yes (case-insensitive).
#[must_use]
pub fn env_bool(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(value.to_lowercase().as_str(), "true" | "1" | "yes"),
        Err(_) => false,
    }
}

/// Reads a non-empty env value.
#[must_use]
pub fn env_string(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Lexically absolutizes a path without resolving symlinks.
#[must_use]
pub fn absolute_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Extracts `--repo` / `--repo-path` from CLI arguments.
///
/// Supports both `--repo-path /path` and `--repo-path=/path`. A missing or
/// dash-prefixed value is a hard error.
#[must_use]
pub fn parse_cli_repo_path<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();

    for (index, arg) in args.iter().enumerate() {
        if arg == "--repo" || arg == "--repo-path" {
            let value = args.get(index + 1).map_or("", String::as_str);
            if value.is_empty() || value.starts_with('-') {
                return Some(invalid_value(value));
            }
            return Some(value.to_owned());
        }

        if let Some(value) = arg
            .strip_prefix("--repo-path=")
            .or_else(|| arg.strip_prefix("--repo="))
        {
            if value.is_empty() || value.starts_with('-') {
                return Some(invalid_value(value));
            }
            return Some(value.to_owned());
        }
    }

    None
}

/// Sentinel used to surface an invalid CLI value through the `Option` channel;
/// `from_env_and_args` turns it into the configured path, which then fails
/// validation with a clear message.
fn invalid_value(value: &str) -> String {
    format!("--repo/--repo-path requires a non-empty value. Received: \"{value}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_cli_forms() {
        assert_eq!(
            parse_cli_repo_path(["--repo-path", "/tmp/repo"]),
            Some("/tmp/repo".to_owned())
        );
        assert_eq!(
            parse_cli_repo_path(["--repo", "/tmp/repo"]),
            Some("/tmp/repo".to_owned())
        );
        assert_eq!(
            parse_cli_repo_path(["--repo-path=/tmp/repo"]),
            Some("/tmp/repo".to_owned())
        );
        assert_eq!(parse_cli_repo_path(["--unknown", "x"]), None);
    }

    #[test]
    fn rejects_empty_or_dash_values() {
        for args in [
            vec!["--repo-path"],
            vec!["--repo-path", ""],
            vec!["--repo-path", "--other"],
            vec!["--repo-path="],
        ] {
            let parsed = parse_cli_repo_path(args).expect("value is reported");
            assert!(parsed.starts_with("--repo/--repo-path requires"));
        }
    }

    #[test]
    fn resolves_repo_path_from_default() {
        let config = Config::from_env_and_args(Vec::<String>::new());
        // No default configured in the test environment unless GIT_REPO_PATH is set.
        if config.default_repo_path.is_none() {
            let error = config.resolve_repo_path(None).expect_err("no default");
            assert_eq!(error.message(), error_templates::NO_REPO_PATH);
        }
        assert_eq!(
            config.resolve_repo_path(Some("/tmp/explicit")).unwrap(),
            PathBuf::from("/tmp/explicit")
        );
    }
}
