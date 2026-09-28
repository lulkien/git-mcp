//! Tool-level tests against a real `git` repository.
//!
//! These run the actual `git` binary so the parsing of `git status`,
//! `git log` and `git diff` output is exercised.

use std::path::{Path, PathBuf};
use std::process::Command;

use git_mcp_rs::render::ResponseFormat;
use git_mcp_rs::tools::grouped::{history, status};
use serde_json::Value;

/// Creates a repository with one commit and returns its path.
fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();

    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "test@example.com"]);
    git(&path, &["config", "user.name", "Test User"]);
    std::fs::write(path.join("README.md"), "# fixture\n").expect("write readme");
    git(&path, &["add", "README.md"]);
    git(&path, &["commit", "-m", "feat: initial commit"]);

    (dir, path)
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo_path_str(repo: &Path) -> String {
    repo.to_str().expect("utf-8 path").to_owned()
}

fn structured(result: &rmcp::model::CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("structured content present")
}

fn text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| match block {
            rmcp::model::ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .expect("text content present")
}

fn status_args(
    repo: &Path,
    action: status::StatusAction,
    format: ResponseFormat,
) -> status::StatusArgs {
    status::StatusArgs {
        repo_path: Some(repo_path_str(repo)),
        action,
        mode: git_mcp_rs::services::inspect::DiffMode::Unstaged,
        from_ref: None,
        to_ref: None,
        filtered: false,
        base_branch: "main".to_owned(),
        response_format: format,
    }
}

#[tokio::test]
async fn git_status_reports_branch_and_changed_files() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("README.md"), "# fixture\nchanged\n").expect("modify");
    std::fs::write(repo.join("untracked.txt"), "new\n").expect("untracked");

    let args = status_args(&repo, status::StatusAction::Status, ResponseFormat::Json);
    let result = status::run(&args).await.expect("status succeeds");

    assert_ne!(result.is_error, Some(true));
    let value = structured(&result);
    assert_eq!(value["status"]["current"], "main");
    assert_eq!(value["status"]["isClean"], false);

    let files = value["status"]["files"].as_array().expect("files array");
    assert_eq!(files.len(), 2, "one modified and one untracked file");
    let paths: Vec<&str> = files
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert!(paths.contains(&"README.md"));
    assert!(paths.contains(&"untracked.txt"));
}

#[tokio::test]
async fn git_status_clean_repository_reports_clean() {
    let (_dir, repo) = fixture_repo();
    let args = status_args(&repo, status::StatusAction::Status, ResponseFormat::Json);
    let result = status::run(&args).await.expect("status succeeds");

    assert_eq!(structured(&result)["status"]["isClean"], true);
}

#[tokio::test]
async fn git_history_log_returns_commits() {
    let (_dir, repo) = fixture_repo();
    let args = history::HistoryArgs {
        repo_path: Some(repo_path_str(&repo)),
        action: history::HistoryAction::Log,
        limit: 10,
        offset: 0,
        first_parent: false,
        no_merges: false,
        all_branches: false,
        simplify_merges: false,
        order: history::LogOrder::Default,
        revision_range: None,
        pathspecs: None,
        author: None,
        grep: None,
        since: None,
        until: None,
        file_path: None,
        ref_name: None,
        response_format: ResponseFormat::Json,
    };

    let result = history::run(&args).await.expect("log succeeds");
    let commits = structured(&result)["commits"]
        .as_array()
        .expect("commits array")
        .clone();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["subject"], "feat: initial commit");
    assert_eq!(commits[0]["authorName"], "Test User");
    assert_eq!(commits[0]["authorEmail"], "test@example.com");
    assert!(commits[0]["dateIso"].as_str().unwrap().contains('T'));
}

#[tokio::test]
async fn git_history_show_and_reflog_return_output() {
    let (_dir, repo) = fixture_repo();
    let base = history::HistoryArgs {
        repo_path: Some(repo_path_str(&repo)),
        action: history::HistoryAction::Show,
        limit: 5,
        offset: 0,
        first_parent: false,
        no_merges: false,
        all_branches: false,
        simplify_merges: false,
        order: history::LogOrder::Default,
        revision_range: None,
        pathspecs: None,
        author: None,
        grep: None,
        since: None,
        until: None,
        file_path: None,
        ref_name: Some("HEAD".to_owned()),
        response_format: ResponseFormat::Markdown,
    };

    let result = history::run(&base).await.expect("show succeeds");
    assert!(text(&result).contains("initial commit"));

    let reflog = history::HistoryArgs {
        action: history::HistoryAction::Reflog,
        ..base
    };
    let result = history::run(&reflog).await.expect("reflog succeeds");
    assert!(text(&result).contains("commit"));
}

#[tokio::test]
async fn git_status_diff_summarizes_changes() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("README.md"), "# fixture\nline two\nline three\n").expect("modify");

    let args = status_args(&repo, status::StatusAction::Diff, ResponseFormat::Json);
    let result = status::run(&args).await.expect("diff succeeds");

    let summary = &structured(&result)["summary"];
    assert_eq!(summary["filesChanged"], 1);
    assert_eq!(summary["insertions"], 2);
    assert_eq!(summary["deletions"], 0);
}

#[tokio::test]
async fn git_status_diff_main_uses_merge_base() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "feature"]);
    std::fs::write(repo.join("feature.txt"), "x\n").expect("write");
    git(&repo, &["add", "feature.txt"]);
    git(&repo, &["commit", "-m", "feat: add feature"]);

    let args = status_args(&repo, status::StatusAction::DiffMain, ResponseFormat::Json);
    let result = status::run(&args).await.expect("diff_main succeeds");

    let value = structured(&result);
    assert_eq!(value["base_branch"], "main");
    assert_eq!(value["summary"]["filesChanged"], 1);
    assert!(
        value["output"].as_str().unwrap().contains("feature.txt"),
        "diff mentions the new file"
    );
}

#[tokio::test]
async fn missing_repository_is_rejected_with_a_clear_message() {
    let args = status::StatusArgs {
        repo_path: Some("/definitely/not/a/repo".to_owned()),
        ..status_args(
            Path::new("/tmp"),
            status::StatusAction::Status,
            ResponseFormat::Json,
        )
    };

    let error = status::run(&args)
        .await
        .expect_err("the repository path is validated before git runs");
    assert_eq!(error.kind(), git_mcp_rs::GitErrorKind::InvalidInput);
    assert!(
        error.message().contains("Repository path does not exist"),
        "{}",
        error.message()
    );
}

#[tokio::test]
async fn escaping_path_arguments_are_rejected() {
    let (_dir, repo) = fixture_repo();
    let args = history::HistoryArgs {
        repo_path: Some(repo_path_str(&repo)),
        action: history::HistoryAction::Blame,
        limit: 5,
        offset: 0,
        first_parent: false,
        no_merges: false,
        all_branches: false,
        simplify_merges: false,
        order: history::LogOrder::Default,
        revision_range: None,
        pathspecs: None,
        author: None,
        grep: None,
        since: None,
        until: None,
        file_path: Some("../../etc/passwd".to_owned()),
        ref_name: None,
        response_format: ResponseFormat::Json,
    };

    let error = history::run(&args).await.expect_err("escape rejected");
    assert_eq!(error.kind(), git_mcp_rs::GitErrorKind::InvalidInput);
    assert!(error.message().contains("escapes repository root"));
}
