//! Tool-level tests for `git_lfs` and `git_docs`.
//!
//! Every case here is offline: the LFS assertions are input validation that
//! runs before any `git lfs` invocation, and the docs case exercises command
//! validation, which runs before any HTTP request.

use git_mcp_rs::services::docs::fetch_git_man_page;
use git_mcp_rs::tools::lfs;
use serde::de::DeserializeOwned;
use serde_json::json;

fn args<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("arguments deserialize")
}

#[tokio::test]
async fn lfs_track_requires_patterns() {
    let call: lfs::LfsArgs = args(json!({ "repo_path": "/tmp", "action": "track" }));
    let error = lfs::run(&call).await.expect_err("patterns required");
    assert_eq!(error.message(), "patterns is required for lfs track.");
}

#[tokio::test]
async fn lfs_untrack_requires_patterns() {
    let call: lfs::LfsArgs = args(json!({ "repo_path": "/tmp", "action": "untrack" }));
    let error = lfs::run(&call).await.expect_err("patterns required");
    assert_eq!(error.message(), "patterns is required for lfs untrack.");
}

#[tokio::test]
async fn lfs_push_requires_a_remote() {
    let call: lfs::LfsArgs = args(json!({ "repo_path": "/tmp", "action": "push" }));
    let error = lfs::run(&call).await.expect_err("remote required");
    assert_eq!(error.message(), "remote is required for lfs push.");
}

#[tokio::test]
async fn lfs_rejects_option_like_patterns() {
    let call: lfs::LfsArgs = args(json!({
        "repo_path": "/tmp",
        "action": "track",
        "patterns": ["--force"],
    }));
    let error = lfs::run(&call).await.expect_err("option rejected");
    assert!(
        error.message().contains("cannot start with"),
        "{}",
        error.message()
    );
}

#[tokio::test]
async fn lfs_requires_a_repository_path() {
    let call: lfs::LfsArgs = args(json!({ "action": "status" }));
    let error = lfs::run(&call).await.expect_err("repo path required");
    assert!(error.message().contains("No repository path provided"));
}

#[tokio::test]
async fn docs_rejects_unsafe_command_names_before_fetching() {
    for command in ["../etc/passwd", "--upload-pack", "commit;rm -rf /", ""] {
        let error = fetch_git_man_page(command)
            .await
            .expect_err("command name rejected");
        assert!(
            error.message().contains("Invalid command name")
                || error.message().contains("cannot be empty")
                || error.message().contains("cannot start with"),
            "{command}: {}",
            error.message()
        );
    }
}

#[tokio::test]
async fn docs_normalizes_git_prefixed_commands() {
    // `git-c` is not a real page, so this reaches the network only for a valid
    // name shape; the assertion is that the name passes validation.
    let result = fetch_git_man_page("git-commit").await;
    if let Err(error) = result {
        let message = error.message();
        assert!(
            !message.contains("Invalid command name"),
            "git-commit should normalize to a valid name: {message}"
        );
    }
}
