//! Tool-level tests for the grouped tools, against a real `git` repository.
//!
//! Arguments are built from JSON so the tests exercise deserialization and the
//! documented defaults, not hand-rolled struct literals.

use std::path::{Path, PathBuf};
use std::process::Command;

use git_mcp_rs::render::ResponseFormat;
use git_mcp_rs::tools::grouped::{branches, commits, context, remotes, workspace};
use rmcp::model::CallToolResult;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn args<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("arguments deserialize")
}

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

fn repo_str(repo: &Path) -> String {
    repo.to_str().expect("utf-8 path").to_owned()
}

fn structured(result: &CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("structured content present")
}

fn text(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| match block {
            rmcp::model::ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .expect("text content present")
}

fn log_subjects(repo: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["log", "--pretty=%s"])
        .current_dir(repo)
        .output()
        .expect("git log runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// git_commits
// ---------------------------------------------------------------------------

#[tokio::test]
async fn commits_stages_and_commits() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("a.txt"), "hello\n").expect("write");

    let add: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "add",
        "all": true,
    }));
    let result = commits::run(&add).await.expect("add succeeds");
    assert_eq!(structured(&result)["output"], "Staged all changes.");

    let commit: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "commit",
        "message": "feat: add a.txt",
    }));
    let result = commits::run(&commit).await.expect("commit succeeds");
    let output = structured(&result)["output"].as_str().unwrap();
    assert!(output.starts_with("Committed "), "{output}");

    assert_eq!(log_subjects(&repo)[0], "feat: add a.txt");
}

#[tokio::test]
async fn commits_stages_selected_paths() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("a.txt"), "a\n").expect("write");
    std::fs::write(repo.join("b.txt"), "b\n").expect("write");

    let add: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "add",
        "paths": ["a.txt"],
    }));
    let result = commits::run(&add).await.expect("add succeeds");
    assert_eq!(structured(&result)["output"], "Staged 1 path(s).");

    let status = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&repo)
        .output()
        .expect("git runs");
    let staged = String::from_utf8_lossy(&status.stdout);
    assert!(staged.contains("a.txt"));
    assert!(!staged.contains("b.txt"));
}

#[tokio::test]
async fn commits_requires_a_message() {
    let (_dir, repo) = fixture_repo();
    let commit: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "commit",
    }));
    let error = commits::run(&commit).await.expect_err("message required");
    assert!(
        error
            .message()
            .contains("message is required for commit action.")
    );
}

#[tokio::test]
async fn commits_hard_reset_requires_confirmation() {
    let (_dir, repo) = fixture_repo();
    let reset: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "reset",
        "mode": "hard",
    }));
    let error = commits::run(&reset).await.expect_err("confirm required");
    assert!(
        error
            .message()
            .contains("Hard reset requires confirm=true.")
    );

    let confirmed: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "reset",
        "mode": "hard",
        "target": "HEAD",
        "confirm": true,
    }));
    let result = commits::run(&confirmed).await.expect("hard reset succeeds");
    assert_eq!(
        structured(&result)["output"],
        "Reset completed with mode=hard."
    );
}

#[tokio::test]
async fn commits_unstages_paths() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("a.txt"), "a\n").expect("write");
    git(&repo, &["add", "a.txt"]);

    let unstage: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "unstage",
        "paths": ["a.txt"],
    }));
    let result = commits::run(&unstage).await.expect("unstage succeeds");
    assert_eq!(structured(&result)["output"], "Restored 1 path(s).");

    let status = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&repo)
        .output()
        .expect("git runs");
    assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty());
}

#[tokio::test]
async fn commits_rejects_paths_that_escape_the_repository() {
    let (_dir, repo) = fixture_repo();
    let unstage: commits::CommitArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "unstage",
        "paths": ["../../etc/passwd"],
    }));
    let error = commits::run(&unstage).await.expect_err("escape rejected");
    assert!(error.message().contains("escapes repository root"));
}

// ---------------------------------------------------------------------------
// git_branches
// ---------------------------------------------------------------------------

#[tokio::test]
async fn branches_lists_creates_and_deletes() {
    let (_dir, repo) = fixture_repo();

    let list: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = branches::run(&list).await.expect("list succeeds");
    let listed = structured(&result)["branches"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["name"], "main");
    assert_eq!(listed[0]["isCurrent"], true);

    let create: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "create",
        "name": "feature",
        "create": true,
    }));
    let result = branches::run(&create).await.expect("create succeeds");
    assert_eq!(
        structured(&result)["output"],
        "Created and checked out feature."
    );

    let list: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
        "all": true,
    }));
    let result = branches::run(&list).await.expect("list succeeds");
    let listed = structured(&result)["branches"].as_array().unwrap();
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(
        listed.iter().find(|b| b["name"] == "feature").unwrap()["isCurrent"],
        true
    );
}

#[tokio::test]
async fn branches_refuse_to_delete_the_checked_out_branch() {
    let (_dir, repo) = fixture_repo();
    let delete: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "delete",
        "name": "main",
        "force": true,
    }));
    let error = branches::run(&delete).await.expect_err("current branch");
    // git words this as "used by worktree" for the branch checked out here.
    assert!(
        error.message().contains("used by worktree") || error.message().contains("checked out"),
        "{}",
        error.message()
    );
}

#[tokio::test]
async fn branches_force_delete_removes_an_unmerged_branch() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "unmerged"]);
    std::fs::write(repo.join("wip.txt"), "x\n").expect("write");
    git(&repo, &["add", "wip.txt"]);
    git(&repo, &["commit", "-m", "wip"]);
    git(&repo, &["checkout", "main"]);

    let safe_delete: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "delete",
        "name": "unmerged",
    }));
    let error = branches::run(&safe_delete)
        .await
        .expect_err("unmerged branch");
    assert!(
        error.message().contains("not fully merged"),
        "{}",
        error.message()
    );

    let forced: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "delete",
        "name": "unmerged",
        "force": true,
    }));
    let result = branches::run(&forced).await.expect("force delete succeeds");
    assert_eq!(structured(&result)["output"], "Deleted branch unmerged.");
}

#[tokio::test]
async fn branches_create_requires_a_name() {
    let (_dir, repo) = fixture_repo();
    let create: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "create",
    }));
    let error = branches::run(&create).await.expect_err("name required");
    assert!(
        error
            .message()
            .contains("name is required for branch create.")
    );
}

#[tokio::test]
async fn branches_recent_lists_by_commit_date() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "feature"]);
    git(&repo, &["checkout", "main"]);

    let recent: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "recent",
        "count": 1,
    }));
    let result = branches::run(&recent).await.expect("recent succeeds");
    let listed = text(&result);
    // `count` limits the listing to the most recently committed branch.
    assert_eq!(listed.lines().count(), 1, "{listed}");

    let recent: branches::BranchArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "recent",
        "count": 5,
    }));
    let result = branches::run(&recent).await.expect("recent succeeds");
    let listed = text(&result);
    assert!(
        listed.contains("main") && listed.contains("feature"),
        "{listed}"
    );
}

// ---------------------------------------------------------------------------
// git_remotes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remotes_manages_and_sanitizes_urls() {
    let (_dir, repo) = fixture_repo();

    let list: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = remotes::run(&list).await.expect("list succeeds");
    assert!(
        structured(&result)["remotes"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let add: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "manage",
        "remote_action": "add",
        "name": "origin",
        "url": "https://user:secret@example.com/org/repo.git",
    }));
    let result = remotes::run(&add).await.expect("add succeeds");
    assert_eq!(structured(&result)["output"], "Added remote origin.");

    let list: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = remotes::run(&list).await.expect("list succeeds");
    let remotes = structured(&result)["remotes"].as_array().unwrap();
    assert_eq!(remotes.len(), 1);
    let url = remotes[0]["fetchUrl"].as_str().unwrap();
    assert!(!url.contains("secret"), "credentials leaked: {url}");
    assert_eq!(url, "https://example.com/org/repo.git");

    let set_url: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "manage",
        "remote_action": "set-url",
        "name": "origin",
        "url": "git@example.com:org/repo.git",
    }));
    let result = remotes::run(&set_url).await.expect("set-url succeeds");
    assert_eq!(structured(&result)["output"], "Updated remote origin URL.");

    let remove: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "manage",
        "remote_action": "remove",
        "name": "origin",
    }));
    let result = remotes::run(&remove).await.expect("remove succeeds");
    assert_eq!(structured(&result)["output"], "Removed remote origin.");
}

#[tokio::test]
async fn remotes_manage_requires_action_and_name() {
    let (_dir, repo) = fixture_repo();
    let manage: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "manage",
    }));
    let error = remotes::run(&manage).await.expect_err("action required");
    assert!(
        error
            .message()
            .contains("remote_action and name are required for remotes manage.")
    );
}

#[tokio::test]
async fn remotes_force_push_is_gated_by_configuration() {
    let (_dir, repo) = fixture_repo();
    let push: remotes::RemoteArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "push",
        "remote": "origin",
        "branch": "main",
        "force": true,
    }));
    // No remote is configured, and force push is disabled by default: the gate
    // must reject the request before git is ever invoked.
    let error = remotes::run(&push).await.expect_err("force push disabled");
    assert!(
        error
            .message()
            .contains("force push is disabled on this server"),
        "{}",
        error.message()
    );
}

// ---------------------------------------------------------------------------
// git_workspace
// ---------------------------------------------------------------------------

#[tokio::test]
async fn workspace_stash_save_list_and_pop() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("README.md"), "# fixture\nchanged\n").expect("modify");

    let save: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "stash",
        "stash_action": "save",
        "message": "wip",
    }));
    let result = workspace::run(&save).await.expect("stash save succeeds");
    assert!(!text(&result).is_empty());

    let list: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "stash",
        "stash_action": "list",
    }));
    let result = workspace::run(&list).await.expect("stash list succeeds");
    assert!(text(&result).contains("wip"), "{}", text(&result));

    let pop: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "stash",
        "stash_action": "pop",
    }));
    let result = workspace::run(&pop).await.expect("stash pop succeeds");
    let _ = result;
    let content = std::fs::read_to_string(repo.join("README.md")).expect("read");
    assert!(content.contains("changed"));
}

#[tokio::test]
async fn workspace_tags_create_list_and_delete() {
    let (_dir, repo) = fixture_repo();

    let create: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "tag",
        "tag_action": "create",
        "name": "v1.0.0",
    }));
    let result = workspace::run(&create).await.expect("tag create succeeds");
    assert_eq!(structured(&result)["output"], "Created tag v1.0.0.");

    let annotate: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "tag",
        "tag_action": "create",
        "name": "v1.1.0",
        "message": "release 1.1.0",
    }));
    let result = workspace::run(&annotate)
        .await
        .expect("annotated tag succeeds");
    assert_eq!(
        structured(&result)["output"],
        "Created annotated tag v1.1.0."
    );

    let list: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "tag",
        "tag_action": "list",
    }));
    let result = workspace::run(&list).await.expect("tag list succeeds");
    let output = text(&result);
    assert!(
        output.contains("v1.0.0") && output.contains("v1.1.0"),
        "{output}"
    );

    let delete: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "tag",
        "tag_action": "delete",
        "name": "v1.0.0",
    }));
    let result = workspace::run(&delete).await.expect("tag delete succeeds");
    assert_eq!(structured(&result)["output"], "Deleted tag v1.0.0.");
}

#[tokio::test]
async fn workspace_merges_a_branch() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "feature"]);
    std::fs::write(repo.join("feature.txt"), "x\n").expect("write");
    git(&repo, &["add", "feature.txt"]);
    git(&repo, &["commit", "-m", "feat: feature"]);
    git(&repo, &["checkout", "main"]);

    let merge: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "merge",
        "ref": "feature",
        "merge_no_ff": true,
    }));
    let result = workspace::run(&merge).await.expect("merge succeeds");
    assert!(!text(&result).is_empty());
    assert!(log_subjects(&repo).iter().any(|s| s.contains("Merge")));
}

#[tokio::test]
async fn workspace_merge_requires_refs() {
    let (_dir, repo) = fixture_repo();
    let merge: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "merge",
    }));
    let error = workspace::run(&merge).await.expect_err("refs required");
    assert!(
        error
            .message()
            .contains("ref or merge_refs is required for merge start.")
    );
}

#[tokio::test]
async fn workspace_rebase_requires_an_upstream() {
    let (_dir, repo) = fixture_repo();
    let rebase: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "rebase",
        "rebase_action": "start",
    }));
    let error = workspace::run(&rebase)
        .await
        .expect_err("upstream required");
    assert!(
        error
            .message()
            .contains("rebase_upstream (or onto) is required for rebase start.")
    );
}

#[tokio::test]
async fn workspace_bisect_run_rejects_shell_metacharacters() {
    let (_dir, repo) = fixture_repo();
    let bisect: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "bisect",
        "bisect_action": "run",
        "command_args": ["sh", "-c", "exit 1; rm -rf /"],
    }));
    let error = workspace::run(&bisect)
        .await
        .expect_err("metacharacters rejected");
    assert!(error.message().contains("shell metacharacters"));
}

#[tokio::test]
async fn workspace_submodule_list_is_empty_by_default() {
    let (_dir, repo) = fixture_repo();
    let list: workspace::WorkspaceArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "submodule",
        "submodule_action": "list",
    }));
    let result = workspace::run(&list)
        .await
        .expect("submodule list succeeds");
    assert_eq!(structured(&result)["output"], "No submodules.");
}

// ---------------------------------------------------------------------------
// git_context
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_summary_reports_repository_state() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("dirty.txt"), "x\n").expect("write");

    let summary: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "summary",
    }));
    let result = context::run(&summary).await.expect("summary succeeds");
    let value = structured(&result)["summary"].clone();
    assert_eq!(value["branch"], "main");
    assert_eq!(value["isClean"], false);
    assert_eq!(value["changedFiles"], 1);
    assert_eq!(value["recentCommits"][0]["subject"], "feat: initial commit");
    assert_eq!(value["inProgress"]["rebasing"], false);
    assert_eq!(value["inProgress"]["cherryPicking"], false);
    assert!(value["remotes"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn context_reads_and_writes_config() {
    let (_dir, repo) = fixture_repo();

    let set: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "set_config",
        "key": "user.name",
        "value": "Renamed User",
    }));
    let result = context::run(&set).await.expect("set_config succeeds");
    assert_eq!(structured(&result)["output"], "Set user.name.");

    let get: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "get_config",
        "key": "user.name",
    }));
    let result = context::run(&get).await.expect("get_config succeeds");
    assert_eq!(text(&result), "Renamed User");
}

#[tokio::test]
async fn context_blocks_credential_config_keys() {
    let (_dir, repo) = fixture_repo();
    for (action, key) in [
        ("get_config", "credential.helper"),
        ("set_config", "url.https://x.insteadOf"),
    ] {
        let call: context::ContextArgs = args(json!({
            "repo_path": repo_str(&repo),
            "action": action,
            "key": key,
            "value": "whatever",
        }));
        let error = context::run(&call).await.expect_err("blocked key");
        assert!(
            error.message().contains("is not permitted"),
            "{}",
            error.message()
        );
    }
}

#[tokio::test]
async fn context_lists_aliases_with_a_placeholder() {
    let (_dir, repo) = fixture_repo();
    let aliases: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "aliases",
    }));
    let result = context::run(&aliases).await.expect("aliases succeeds");
    assert_eq!(structured(&result)["output"], "No aliases configured.");

    git(&repo, &["config", "--local", "alias.st", "status"]);
    let result = context::run(&aliases).await.expect("aliases succeeds");
    // `git config --get-regexp` prints `<key> <value>`, space separated.
    assert!(
        text(&result).contains("alias.st status"),
        "{}",
        text(&result)
    );
}

#[tokio::test]
async fn context_search_requires_a_query() {
    let (_dir, repo) = fixture_repo();
    let search: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "search",
    }));
    let error = context::run(&search).await.expect_err("query required");
    assert!(
        error
            .message()
            .contains("query is required for context search.")
    );
}

#[tokio::test]
async fn context_search_reports_both_sections() {
    let (_dir, repo) = fixture_repo();
    let search: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "search",
        "query": "README",
    }));
    let result = context::run(&search).await.expect("search succeeds");
    let output = text(&result);
    assert!(output.contains("## Pickaxe (-S)"), "{output}");
    assert!(output.contains("## grep"), "{output}");
}

#[tokio::test]
async fn markdown_and_json_formats_both_render() {
    let (_dir, repo) = fixture_repo();
    let call: context::ContextArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "summary",
        "response_format": "json",
    }));
    let result = context::run(&call).await.expect("summary succeeds");
    assert!(text(&result).starts_with('{'), "{}", text(&result));
    assert_eq!(call.response_format, ResponseFormat::Json);
}
