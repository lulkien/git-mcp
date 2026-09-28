//! Remote service: listing, managing, fetching, pulling and pushing.
//!
//! Remote URLs are sanitized before they reach a client so credentials baked
//! into a remote never leak into tool output.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use url::Url;

use crate::config::config;
use crate::constants::error_templates;
use crate::error::GitError;
use crate::git::Git;
use crate::security::{assert_safe_arg, assert_safe_ref};
use crate::types::RemoteInfo;

/// Action selected by `git_remotes action=manage`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteManageAction {
    /// Add a new remote.
    Add,
    /// Remove a remote.
    Remove,
    /// Change an existing remote's URL.
    SetUrl,
}

impl RemoteManageAction {
    /// Wire representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::SetUrl => "set-url",
        }
    }
}

/// Options for `manage_remote`.
#[derive(Debug, Clone)]
pub struct ManageRemoteOptions {
    /// Operation to perform.
    pub action: RemoteManageAction,
    /// Remote name.
    pub name: String,
    /// Remote URL, required by `add` and `set-url`.
    pub url: Option<String>,
}

/// Options for fetching.
#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// Remote to fetch from.
    pub remote: Option<String>,
    /// Branch to fetch.
    pub branch: Option<String>,
    /// Prune deleted remote-tracking refs.
    pub prune: bool,
}

/// Options for pulling.
#[derive(Debug, Clone)]
pub struct PullOptions {
    /// Remote to pull from.
    pub remote: Option<String>,
    /// Branch to pull.
    pub branch: Option<String>,
    /// Rebase instead of merging.
    pub rebase: bool,
}

/// Options for pushing.
#[derive(Debug, Clone)]
pub struct PushOptions {
    /// Remote to push to.
    pub remote: Option<String>,
    /// Branch to push.
    pub branch: Option<String>,
    /// Set the upstream of the pushed branch.
    pub set_upstream: bool,
    /// Force, but only when the remote ref is what we last saw.
    pub force_with_lease: bool,
    /// Hard force push; requires `GIT_ALLOW_FORCE_PUSH=true`.
    pub force: bool,
    /// Bypass pre-push hooks; requires `GIT_ALLOW_NO_VERIFY=true`.
    pub no_verify: bool,
    /// Push all tags.
    pub tags: bool,
}

static SENSITIVE_QUERY_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(token|auth|password|secret|key)").expect("valid sensitive key pattern")
});

static OPAQUE_SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{20,}$").expect("valid segment pattern"));

static SSH_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ssh://([^@]+@)?([^/]+)/(.+)$").expect("valid ssh url pattern"));

static SCP_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^@]+@)?([^:]+):(.+)$").expect("valid scp url pattern"));

/// Renders the remote/branch pair used in status messages.
#[must_use]
pub fn format_remote_branch(remote: Option<&str>, branch: Option<&str>) -> String {
    let remote_label = remote.unwrap_or("tracking remote");
    match branch {
        Some(branch) => format!("{remote_label}/{branch}"),
        None => remote_label.to_owned(),
    }
}

/// Strips credentials and opaque tokens from a remote URL.
#[must_use]
pub fn sanitize_remote_url(url: Option<&str>) -> Option<String> {
    let url = url?;

    // SCP-style remotes (`git@host:org/repo.git`) are not parseable as URLs.
    if url.contains(':') && !url.contains("://") {
        return Some(sanitize_scp_url(url));
    }

    let Ok(mut parsed) = Url::parse(url) else {
        return Some(url.to_owned());
    };

    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);

    let sensitive: Vec<String> = parsed
        .query_pairs()
        .filter(|(key, _)| SENSITIVE_QUERY_KEY.is_match(key))
        .map(|(key, _)| key.into_owned())
        .collect();
    if !sensitive.is_empty() {
        let mut pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(key, value)| {
                let value = if sensitive.contains(&key.to_string()) {
                    "***".to_owned()
                } else {
                    value.into_owned()
                };
                (key.into_owned(), value)
            })
            .collect();
        pairs.sort();
        parsed.query_pairs_mut().clear().extend_pairs(pairs);
    }

    let segments: Vec<String> = parsed
        .path_segments()
        .map(|segments| {
            segments
                .map(|segment| {
                    if OPAQUE_SEGMENT.is_match(segment) {
                        "***".to_owned()
                    } else {
                        segment.to_owned()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if !segments.is_empty() {
        parsed.set_path(&segments.join("/"));
    }

    Some(parsed.to_string())
}

fn sanitize_scp_url(url: &str) -> String {
    if let Some(caps) = SSH_URL.captures(url) {
        let credentials = caps
            .get(1)
            .map_or(String::new(), |m| sanitize_credentials(m.as_str()));
        let host = caps.get(2).map_or("", |m| m.as_str());
        let path = caps.get(3).map_or("", |m| m.as_str());
        return format!("ssh://{credentials}{host}/{path}");
    }

    if let Some(caps) = SCP_URL.captures(url) {
        let credentials = caps
            .get(1)
            .map_or(String::new(), |m| sanitize_credentials(m.as_str()));
        let host = caps.get(2).map_or("", |m| m.as_str());
        let path = caps.get(3).map_or("", |m| m.as_str());
        return format!("{credentials}{host}:{path}");
    }

    url.to_owned()
}

fn sanitize_credentials(credentials: &str) -> String {
    // Drop the password from `user:pass@`.
    match credentials.split_once(':') {
        Some((username, _)) => format!("{username}@"),
        None => credentials.to_owned(),
    }
}

/// Lists configured remotes with sanitized URLs.
pub async fn list_remotes(repo_path: &Path) -> Result<Vec<RemoteInfo>, GitError> {
    let git = Git::open(repo_path)?;
    let output = git.raw(&["remote", "-v"]).await?;
    Ok(parse_remote_list(&output))
}

/// Parses `git remote -v` output.
#[must_use]
pub fn parse_remote_list(output: &str) -> Vec<RemoteInfo> {
    let mut remotes: Vec<RemoteInfo> = Vec::new();

    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let (Some(name), Some(url), Some(kind)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let url = sanitize_remote_url(Some(url));

        if let Some(remote) = remotes.iter_mut().find(|remote| remote.name == name) {
            match kind {
                "(fetch)" => remote.fetch_url = url,
                "(push)" => remote.push_url = url,
                _ => {}
            }
            continue;
        }

        let (fetch_url, push_url) = match kind {
            "(fetch)" => (url, None),
            "(push)" => (None, url),
            _ => (None, None),
        };
        remotes.push(RemoteInfo {
            name: name.to_owned(),
            fetch_url,
            push_url,
        });
    }

    remotes
}

/// Adds, removes or re-points a remote.
pub async fn manage_remote(
    repo_path: &Path,
    options: &ManageRemoteOptions,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let name = assert_safe_arg(&options.name, "remote")?;

    match options.action {
        RemoteManageAction::Add => {
            let Some(url) = &options.url else {
                return Err(GitError::invalid_input("url is required for action='add'"));
            };
            git.raw(&["remote", "add", &name, url]).await?;
            Ok(format!("Added remote {name}."))
        }
        RemoteManageAction::Remove => {
            git.raw(&["remote", "remove", &name]).await?;
            Ok(format!("Removed remote {name}."))
        }
        RemoteManageAction::SetUrl => {
            let Some(url) = &options.url else {
                return Err(GitError::invalid_input(
                    "url is required for action='set-url'",
                ));
            };
            git.raw(&["remote", "set-url", &name, url]).await?;
            Ok(format!("Updated remote {name} URL."))
        }
    }
}

/// Fetches from a remote.
pub async fn fetch_remote(repo_path: &Path, options: &FetchOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    let mut args = vec!["fetch".to_owned()];
    if options.prune {
        args.push("--prune".to_owned());
    }

    let remote = options
        .remote
        .as_deref()
        .map(|remote| assert_safe_arg(remote, "remote"))
        .transpose()?;
    let branch = options
        .branch
        .as_deref()
        .map(|branch| assert_safe_ref(branch, "branch"))
        .transpose()?;

    match (&remote, &branch) {
        (Some(remote), Some(branch)) => {
            args.push(remote.clone());
            args.push(branch.clone());
            git.raw(&args).await?;
            Ok(format!("Fetched {remote}/{branch}."))
        }
        (Some(remote), None) => {
            args.push(remote.clone());
            git.raw(&args).await?;
            Ok(format!("Fetched {remote}."))
        }
        _ => {
            git.raw(&args).await?;
            Ok("Fetched default remote.".to_owned())
        }
    }
}

/// Pulls from a remote.
pub async fn pull_remote(repo_path: &Path, options: &PullOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    let mut args = vec!["pull".to_owned()];
    if options.rebase {
        args.push("--rebase".to_owned());
    }

    // Git only takes a remote/branch pair together.
    if let (Some(remote), Some(branch)) = (&options.remote, &options.branch) {
        args.push(assert_safe_arg(remote, "remote")?);
        args.push(assert_safe_ref(branch, "branch")?);
    }

    git.raw(&args).await?;

    let target = format_remote_branch(options.remote.as_deref(), options.branch.as_deref());
    let suffix = if options.rebase { " with rebase" } else { "" };
    Ok(format!("Pulled {target}{suffix}."))
}

/// Pushes to a remote.
pub async fn push_remote(repo_path: &Path, options: &PushOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let server_config = config();

    if options.no_verify && !server_config.allow_no_verify {
        return Err(GitError::invalid_input(
            error_templates::HOOK_BYPASS_DISABLED,
        ));
    }
    if options.force && !server_config.allow_force_push {
        return Err(GitError::invalid_input(
            "force push is disabled on this server. Set GIT_ALLOW_FORCE_PUSH=true to enable it. \
Consider using force_with_lease instead for a safer alternative.",
        ));
    }

    let mut args = vec!["push".to_owned()];
    for (enabled, flag) in [
        (options.set_upstream, "--set-upstream"),
        (options.force_with_lease, "--force-with-lease"),
        (options.force, "--force"),
        (options.tags, "--tags"),
        (options.no_verify, "--no-verify"),
    ] {
        if enabled {
            args.push(flag.to_owned());
        }
    }

    // `git push <branch>` would read the branch as the remote, so resolve the
    // default remote when only a branch was given.
    let remote = match &options.remote {
        Some(remote) => Some(assert_safe_arg(remote, "remote")?),
        None if options.branch.is_some() => Some("origin".to_owned()),
        None => None,
    };
    let branch = match (&remote, &options.branch) {
        (Some(_), Some(branch)) => Some(assert_safe_ref(branch, "branch")?),
        _ => None,
    };

    if let Some(remote) = remote.clone() {
        args.push(remote);
    }
    if let Some(branch) = branch {
        args.push(branch);
    }

    git.raw(&args).await?;
    Ok(format!(
        "Pushed {}.",
        format_remote_branch(options.remote.as_deref(), options.branch.as_deref())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_in_https_urls() {
        assert_eq!(
            sanitize_remote_url(Some("https://user:secret@github.com/o/r.git")).unwrap(),
            "https://github.com/o/r.git"
        );
    }

    #[test]
    fn redacts_scp_and_ssh_credentials() {
        assert_eq!(
            sanitize_remote_url(Some("user:pass@github.com:o/r.git")).unwrap(),
            "user@github.com:o/r.git"
        );
        assert_eq!(
            sanitize_remote_url(Some("git@github.com:o/r.git")).unwrap(),
            "git@github.com:o/r.git"
        );
        // `ssh://` URLs are parsed as URLs, so the username is dropped entirely.
        assert_eq!(
            sanitize_remote_url(Some("ssh://git@github.com/o/r.git")).unwrap(),
            "ssh://github.com/o/r.git"
        );
    }

    #[test]
    fn redacts_sensitive_query_values_and_opaque_segments() {
        let sanitized =
            sanitize_remote_url(Some("https://host/o/r.git?access_token=abcd1234")).unwrap();
        assert!(!sanitized.contains("abcd1234"), "{sanitized}");
        assert!(sanitized.contains("***"), "{sanitized}");

        // A path segment made of nothing but 20+ token characters is masked;
        // segments with structure (like `r.git`) are left alone.
        let sanitized =
            sanitize_remote_url(Some("https://host/o/abcdefghijklmnopqrstuvwxyz1234")).unwrap();
        assert!(sanitized.ends_with("/o/***"), "{sanitized}");

        let sanitized = sanitize_remote_url(Some("https://host/o/r.git")).unwrap();
        assert_eq!(sanitized, "https://host/o/r.git");
    }

    #[test]
    fn parses_remote_listing() {
        let output = "origin\thttps://github.com/o/r.git (fetch)\norigin\thttps://github.com/o/r.git (push)\nupstream\tgit@github.com:u/r.git (fetch)\nupstream\tgit@github.com:u/r.git (push)\n";
        let remotes = parse_remote_list(output);
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(
            remotes[0].fetch_url.as_deref(),
            Some("https://github.com/o/r.git")
        );
        assert_eq!(remotes[0].push_url, remotes[0].fetch_url);
        assert_eq!(remotes[1].name, "upstream");
    }

    #[test]
    fn formats_remote_branch_labels() {
        assert_eq!(
            format_remote_branch(Some("origin"), Some("main")),
            "origin/main"
        );
        assert_eq!(format_remote_branch(Some("origin"), None), "origin");
        assert_eq!(format_remote_branch(None, None), "tracking remote");
    }
}
