//! Inspection service: status, diffs, log, reflog, blame.
//!

use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::constants::{CHARACTER_LIMIT, EXCLUDED_DIFF_DIRECTORIES, EXCLUDED_DIFF_EXTENSIONS};
use crate::error::GitError;
use crate::git::{Git, validate_path_argument};
use crate::security::assert_safe_ref;
use crate::types::{CommitInfo, DiffSummary, FileStatus, GitStatusResult};

/// Which working-tree state a diff is taken against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    /// Unstaged changes (default).
    #[default]
    Unstaged,
    /// Staged changes.
    Staged,
    /// Changes between two refs.
    Refs,
}

impl DiffMode {
    /// The file-status column this mode corresponds to.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unstaged => "unstaged",
            Self::Staged => "staged",
            Self::Refs => "refs",
        }
    }
}

/// Options for a `git log` query.
#[derive(Debug, Clone, Default)]
pub struct GitLogOptions {
    /// Maximum number of commits to return.
    pub limit: usize,
    /// Number of commits to skip.
    pub offset: usize,
    /// Author filter.
    pub author: Option<String>,
    /// Message grep filter.
    pub grep: Option<String>,
    /// Lower bound on commit date.
    pub since: Option<String>,
    /// Upper bound on commit date.
    pub until: Option<String>,
    /// Restrict history to a repository-relative path.
    pub file_path: Option<String>,
}

/// Options for a diff query.
#[derive(Debug, Clone)]
pub struct GitDiffOptions {
    /// Diff base.
    pub mode: DiffMode,
    /// Left-hand ref, required when `mode` is [`DiffMode::Refs`].
    pub from_ref: Option<String>,
    /// Right-hand ref, required when `mode` is [`DiffMode::Refs`].
    pub to_ref: Option<String>,
    /// Skip noise directories and binary/resource extensions.
    pub filtered: bool,
}

/// Truncates rendered output to [`CHARACTER_LIMIT`].
#[must_use]
pub fn truncate(text: &str) -> String {
    if text.chars().count() <= CHARACTER_LIMIT {
        return text.to_owned();
    }
    let head: String = text.chars().take(CHARACTER_LIMIT).collect();
    format!("{head}\n\n[truncated to {CHARACTER_LIMIT} characters]")
}

/// Parses one `--pretty=format:%H%x09%an%x09%ae%x09%aI%x09%s` line.
#[must_use]
pub fn parse_commit_log_line(line: &str) -> Option<CommitInfo> {
    let mut fields = line.splitn(5, '\t');
    let hash = fields.next()?;
    let author_name = fields.next()?;
    let author_email = fields.next()?;
    let date_iso = fields.next()?;
    if hash.is_empty() || author_name.is_empty() || author_email.is_empty() || date_iso.is_empty() {
        return None;
    }

    Some(CommitInfo {
        hash: hash.to_owned(),
        author_name: author_name.to_owned(),
        author_email: author_email.to_owned(),
        date_iso: date_iso.to_owned(),
        subject: fields.next().unwrap_or_default().to_owned(),
    })
}

/// Parses `git log` output into commits, dropping blank lines.
#[must_use]
pub fn parse_commit_log(output: &str) -> Vec<CommitInfo> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(parse_commit_log_line)
        .collect()
}

/// True when a path belongs to an excluded directory or binary/resource type.
#[must_use]
pub fn should_exclude_file(file_path: &str) -> bool {
    if EXCLUDED_DIFF_DIRECTORIES
        .iter()
        .any(|prefix| file_path.starts_with(prefix))
    {
        return true;
    }

    let Some((_, extension)) = file_path.rsplit_once('.') else {
        return false;
    };
    if extension.is_empty() || extension.contains('/') {
        return false;
    }

    let extension = extension.to_lowercase();
    EXCLUDED_DIFF_EXTENSIONS.contains(&extension.as_str())
}

/// Builds the `git diff ...` argument list for a mode.
pub fn build_diff_base_args(options: &GitDiffOptions) -> Result<Vec<String>, GitError> {
    match options.mode {
        DiffMode::Staged => Ok(vec!["diff".to_owned(), "--staged".to_owned()]),
        DiffMode::Refs => {
            let (Some(from_ref), Some(to_ref)) = (&options.from_ref, &options.to_ref) else {
                return Err(GitError::invalid_input(
                    "from_ref and to_ref are required when mode='refs'",
                ));
            };
            let from_ref = assert_safe_ref(from_ref, "from_ref")?;
            let to_ref = assert_safe_ref(to_ref, "to_ref")?;
            Ok(vec!["diff".to_owned(), format!("{from_ref}..{to_ref}")])
        }
        DiffMode::Unstaged => Ok(vec!["diff".to_owned()]),
    }
}

/// Builds the `git log` arguments shared by `git_history log` and
/// [`get_log`].
#[must_use]
pub fn build_log_args(options: &GitLogOptions) -> Vec<String> {
    let mut args = vec![
        "log".to_owned(),
        "--date=iso-strict".to_owned(),
        format!("--skip={}", options.offset),
        "-n".to_owned(),
        options.limit.to_string(),
        "--pretty=format:%H%x09%an%x09%ae%x09%aI%x09%s".to_owned(),
    ];

    if let Some(author) = &options.author {
        args.push(format!("--author={author}"));
    }
    if let Some(grep) = &options.grep {
        args.push(format!("--grep={grep}"));
    }
    if let Some(since) = &options.since {
        args.push(format!("--since={since}"));
    }
    if let Some(until) = &options.until {
        args.push(format!("--until={until}"));
    }

    args
}

/// Reads branch, tracking, ahead/behind counters and changed files.
pub async fn get_status(repo_path: &Path) -> Result<GitStatusResult, GitError> {
    let git = Git::open(repo_path)?;
    // `-u` includes untracked files in the report.
    let output = git.raw(&["status", "--porcelain", "-b", "-u"]).await?;
    Ok(parse_status_porcelain(&output))
}

/// Parses `git status --porcelain -b -u` output.
#[must_use]
pub fn parse_status_porcelain(output: &str) -> GitStatusResult {
    let mut branch = String::new();
    let mut tracking = String::new();
    let mut ahead = 0_i64;
    let mut behind = 0_i64;
    let mut files = Vec::new();

    for line in output.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            let (branch_part, counts) = split_counts(header);
            let branch_part = branch_part
                .strip_prefix("No commits yet on ")
                .unwrap_or(&branch_part)
                .to_owned();
            let (name, track) = match branch_part.split_once("...") {
                Some((name, track)) => (name.to_owned(), track.to_owned()),
                None => (branch_part, String::new()),
            };
            if let Some(counts) = counts {
                ahead = parse_counter(counts, "ahead");
                behind = parse_counter(counts, "behind");
            }
            if name == "HEAD (no branch)" {
                branch = String::new();
            } else {
                branch = name;
                tracking = track;
            }
            continue;
        }

        if let Some(rest) = line.get(3..) {
            let mut chars = line.chars();
            let index = chars.next().unwrap_or(' ').to_string();
            let working_tree = chars.next().unwrap_or(' ').to_string();
            let path = match rest.split_once(" -> ") {
                Some((_, to)) => to.to_owned(),
                None => rest.to_owned(),
            };
            files.push(FileStatus {
                path,
                index,
                working_tree,
            });
        }
    }

    GitStatusResult {
        branch: branch.clone(),
        current: branch,
        tracking,
        ahead,
        behind,
        is_clean: files.is_empty(),
        files,
    }
}

fn split_counts(header: &str) -> (String, Option<&str>) {
    match header.rsplit_once(" [") {
        Some((branch, counts)) => (branch.to_owned(), counts.strip_suffix(']').or(Some(counts))),
        None => (header.to_owned(), None),
    }
}

fn parse_counter(counts: &str, label: &str) -> i64 {
    counts
        .split(',')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(label))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

/// Returns the commits matching the query.
pub async fn get_log(
    repo_path: &Path,
    options: &GitLogOptions,
) -> Result<Vec<CommitInfo>, GitError> {
    let mut args = build_log_args(options);
    if let Some(file_path) = &options.file_path {
        let safe = validate_path_argument(repo_path, file_path)?;
        args.push("--".to_owned());
        args.push(safe);
    }

    let git = Git::open(repo_path)?;
    let output = git.raw(&args).await?;
    Ok(parse_commit_log(&output))
}

/// Returns `git show --stat --patch` output for a ref.
pub async fn show_ref(repo_path: &Path, reference: &str) -> Result<String, GitError> {
    let reference = assert_safe_ref(reference, "ref")?;
    let git = Git::open(repo_path)?;
    let output = git.raw(&["show", "--stat", "--patch", &reference]).await?;
    Ok(truncate(&output))
}

/// Counts changed files, insertions and deletions for a diff.
pub async fn get_diff_summary(
    repo_path: &Path,
    options: &GitDiffOptions,
) -> Result<DiffSummary, GitError> {
    let mut args = build_diff_base_args(options)?;
    args.push("--numstat".to_owned());

    let git = Git::open(repo_path)?;
    let output = git.raw(&args).await?;
    Ok(parse_numstat(&output))
}

/// Parses `git diff --numstat` output into aggregate counts.
#[must_use]
pub fn parse_numstat(output: &str) -> DiffSummary {
    let mut files_changed = 0_usize;
    let mut insertions = 0_u64;
    let mut deletions = 0_u64;

    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.splitn(3, '\t');
        let added = fields.next().unwrap_or("-");
        let removed = fields.next().unwrap_or("-");
        if fields.next().is_none() {
            continue;
        }
        files_changed += 1;
        insertions += added.trim().parse::<u64>().unwrap_or(0);
        deletions += removed.trim().parse::<u64>().unwrap_or(0);
    }

    DiffSummary {
        files_changed,
        insertions,
        deletions,
    }
}

/// Returns the diff text, optionally skipping noise paths.
pub async fn get_diff(repo_path: &Path, options: &GitDiffOptions) -> Result<String, GitError> {
    let base_args = build_diff_base_args(options)?;
    let git = Git::open(repo_path)?;

    if !options.filtered {
        let output = git.raw(&base_args).await?;
        return Ok(truncate(&output));
    }

    let mut names_args = base_args.clone();
    names_args.push("--name-only".to_owned());
    let names = git.raw(&names_args).await?;

    let files: Vec<String> = names
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !should_exclude_file(line))
        .map(str::to_owned)
        .collect();

    if files.is_empty() {
        return Ok("No changed files after filtering.".to_owned());
    }

    let mut chunks = Vec::with_capacity(files.len());
    for file_path in files {
        let mut args = base_args.clone();
        args.push("--".to_owned());
        args.push(file_path.clone());
        let diff = git.raw(&args).await?;
        chunks.push(format!("=== {file_path} ===\n{}", diff.trim()));
    }

    Ok(truncate(&chunks.join("\n\n")))
}

/// Returns blame output for a repository-relative file.
pub async fn blame_file(
    repo_path: &Path,
    file_path: &str,
    reference: Option<&str>,
) -> Result<String, GitError> {
    let safe_file_path = validate_path_argument(repo_path, file_path)?;
    let mut args = vec!["blame".to_owned()];
    if let Some(reference) = reference {
        args.push(assert_safe_ref(reference, "ref")?);
    }
    args.push("--".to_owned());
    args.push(safe_file_path);

    let git = Git::open(repo_path)?;
    let output = git.raw(&args).await?;
    Ok(truncate(&output))
}

/// Returns recent reflog entries.
pub async fn get_reflog(repo_path: &Path, limit: usize) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let output = git
        .raw(&["reflog", "--date=iso", "-n", &limit.to_string()])
        .await?;
    Ok(truncate(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG_LINE: &str =
        "abc1234\tJohn Doe\tjohn@example.com\t2024-01-01T00:00:00+00:00\tfeat: init\ttail";

    #[test]
    fn parses_commit_log_lines() {
        let commit = parse_commit_log_line(LOG_LINE).expect("parsed");
        assert_eq!(commit.hash, "abc1234");
        assert_eq!(commit.author_name, "John Doe");
        assert_eq!(commit.author_email, "john@example.com");
        assert_eq!(commit.date_iso, "2024-01-01T00:00:00+00:00");
        assert_eq!(commit.subject, "feat: init\ttail");
    }

    #[test]
    fn rejects_malformed_log_lines() {
        assert!(parse_commit_log_line("abc\tJohn").is_none());
        assert!(parse_commit_log_line("").is_none());
    }

    #[test]
    fn parses_status_headers() {
        let status = parse_status_porcelain("## main...origin/main [ahead 2, behind 1]\n");
        assert_eq!(status.current, "main");
        assert_eq!(status.tracking, "origin/main");
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert!(status.is_clean);

        let status = parse_status_porcelain("## No commits yet on main\n");
        assert_eq!(status.current, "main");

        let status = parse_status_porcelain("## HEAD (no branch)\n");
        assert!(status.current.is_empty());

        let status = parse_status_porcelain("## main\n");
        assert_eq!(status.current, "main");
        assert!(status.tracking.is_empty());
    }

    #[test]
    fn parses_status_files() {
        let status = parse_status_porcelain(
            "## main\n M src/a.rs\n?? new.txt\nR  old.rs -> new.rs\n M renamed\n",
        );
        assert_eq!(status.files.len(), 4);
        assert_eq!(status.files[0].path, "src/a.rs");
        assert_eq!(status.files[0].index, " ");
        assert_eq!(status.files[0].working_tree, "M");
        assert_eq!(status.files[2].path, "new.rs");
        assert!(!status.is_clean);
    }

    #[test]
    fn parses_numstat() {
        let summary = parse_numstat("5\t2\tsrc/a.rs\n-\t-\tlogo.png\n");
        assert_eq!(summary.files_changed, 2);
        assert_eq!(summary.insertions, 5);
        assert_eq!(summary.deletions, 2);
    }

    #[test]
    fn excludes_directories_and_binary_extensions() {
        assert!(should_exclude_file("node_modules/x/y.js"));
        assert!(should_exclude_file("assets/logo.png"));
        assert!(should_exclude_file("dist/index.js"));
        assert!(!should_exclude_file("src/main.rs"));
        assert!(!should_exclude_file("Makefile"));
    }

    #[test]
    fn truncates_long_output() {
        let long = "a".repeat(CHARACTER_LIMIT + 10);
        let truncated = truncate(&long);
        assert!(truncated.contains("[truncated to 25000 characters]"));
        assert!(!truncate("short").contains("truncated"));
    }

    #[test]
    fn requires_refs_for_ref_mode() {
        let options = GitDiffOptions {
            mode: DiffMode::Refs,
            from_ref: None,
            to_ref: None,
            filtered: false,
        };
        let error = build_diff_base_args(&options).expect_err("missing refs");
        assert!(error.message().contains("from_ref and to_ref are required"));
    }

    #[test]
    fn builds_log_args_with_optional_filters() {
        let options = GitLogOptions {
            limit: 10,
            offset: 5,
            author: Some("jane".to_owned()),
            grep: Some("fix".to_owned()),
            since: Some("2024-01-01".to_owned()),
            until: None,
            file_path: None,
        };
        let args = build_log_args(&options);
        assert!(args.contains(&"--skip=5".to_owned()));
        assert!(args.contains(&"--author=jane".to_owned()));
        assert!(args.contains(&"--grep=fix".to_owned()));
        assert!(args.contains(&"--since=2024-01-01".to_owned()));
        assert!(!args.iter().any(|arg| arg.starts_with("--until")));
    }
}
