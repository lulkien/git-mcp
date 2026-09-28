//! Tool-level tests for the standalone workspace tools, against a real
//! `git` repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use git_mcp_rs::tools::workspace::{
    bisect, cherry_pick, merge, rebase, stash, submodule, tag, worktree,
};
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

fn structured(result: &CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("structured content present")
}

fn commit_file(repo: &Path, name: &str, message: &str) {
    std::fs::write(repo.join(name), "x\n").expect("write");
    git(repo, &["add", name]);
    git(repo, &["commit", "-m", message]);
}

// ---------------------------------------------------------------------------
// git_stash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stash_saves_lists_and_pops() {
    let (_dir, repo) = fixture_repo();
    std::fs::write(repo.join("README.md"), "# fixture\nchanged\n").expect("modify");

    let save: stash::StashArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "save",
        "message": "was here",
    }));
    let result = stash::run(&save).await.expect("save succeeds");
    assert!(!text(&result).is_empty());

    let list: stash::StashArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = stash::run(&list).await.expect("list succeeds");
    assert!(text(&result).contains("was here"), "{}", text(&result));

    let pop: stash::StashArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "pop",
    }));
    stash::run(&pop).await.expect("pop succeeds");
    assert!(
        std::fs::read_to_string(repo.join("README.md"))
            .expect("read")
            .contains("changed")
    );
}

#[tokio::test]
async fn stash_lists_an_empty_stack_with_a_placeholder() {
    let (_dir, repo) = fixture_repo();
    let list: stash::StashArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = stash::run(&list).await.expect("list succeeds");
    assert_eq!(structured(&result)["output"], "No stashes.");
}

// ---------------------------------------------------------------------------
// git_rebase
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rebase_requires_an_upstream() {
    let (_dir, repo) = fixture_repo();
    let start: rebase::RebaseArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
    }));
    let error = rebase::run(&start).await.expect_err("upstream required");
    assert_eq!(error.message(), "upstream is required for rebase start.");
}

#[tokio::test]
async fn rebase_replays_feature_onto_main() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "feature"]);
    commit_file(&repo, "feature.txt", "feat: feature work");
    git(&repo, &["checkout", "main"]);
    commit_file(&repo, "main.txt", "feat: main work");
    git(&repo, &["checkout", "feature"]);

    let start: rebase::RebaseArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
        "upstream": "main",
    }));
    let result = rebase::run(&start).await.expect("rebase succeeds");
    assert!(!text(&result).is_empty());

    // The feature commit now sits on top of main's commit.
    let output = Command::new("git")
        .args(["log", "--pretty=%s"])
        .current_dir(&repo)
        .output()
        .expect("git log runs");
    let subjects = String::from_utf8_lossy(&output.stdout);
    let subjects: Vec<&str> = subjects.lines().collect();
    assert_eq!(subjects[0], "feat: feature work");
    assert_eq!(subjects[1], "feat: main work");
}

// ---------------------------------------------------------------------------
// git_cherry_pick
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cherry_pick_requires_refs() {
    let (_dir, repo) = fixture_repo();
    let start: cherry_pick::CherryPickArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
    }));
    let error = cherry_pick::run(&start).await.expect_err("refs required");
    assert_eq!(error.message(), "refs is required for cherry_pick start.");
}

#[tokio::test]
async fn cherry_pick_applies_a_commit_from_another_branch() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "feature"]);
    git(&repo, &["checkout", "main"]);
    commit_file(&repo, "main.txt", "feat: main work");
    git(&repo, &["checkout", "feature"]);

    let start: cherry_pick::CherryPickArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
        "refs": ["main"],
    }));
    let result = cherry_pick::run(&start)
        .await
        .expect("cherry-pick succeeds");
    assert!(!text(&result).is_empty());
    assert!(repo.join("main.txt").exists());
}

// ---------------------------------------------------------------------------
// git_merge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn merge_requires_refs() {
    let (_dir, repo) = fixture_repo();
    let start: merge::MergeArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
    }));
    let error = merge::run(&start).await.expect_err("refs required");
    assert_eq!(error.message(), "refs is required for merge start.");
}

#[tokio::test]
async fn merge_creates_a_merge_commit() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["checkout", "-b", "feature"]);
    commit_file(&repo, "feature.txt", "feat: feature work");
    git(&repo, &["checkout", "main"]);

    let start: merge::MergeArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
        "refs": ["feature"],
        "no_ff": true,
    }));
    let result = merge::run(&start).await.expect("merge succeeds");
    assert!(!text(&result).is_empty());

    let output = Command::new("git")
        .args(["log", "--merges", "--pretty=%s"])
        .current_dir(&repo)
        .output()
        .expect("git log runs");
    assert!(!String::from_utf8_lossy(&output.stdout).trim().is_empty());
}

// ---------------------------------------------------------------------------
// git_bisect
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bisect_start_requires_good_and_bad() {
    let (_dir, repo) = fixture_repo();
    let start: bisect::BisectArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
        "good_ref": "HEAD",
    }));
    let error = bisect::run(&start).await.expect_err("bad ref required");
    assert_eq!(
        error.message(),
        "goodRef and badRef are required for bisect start."
    );
}

#[tokio::test]
async fn bisect_runs_a_session_and_resets() {
    let (_dir, repo) = fixture_repo();
    commit_file(&repo, "a.txt", "feat: a");
    commit_file(&repo, "b.txt", "feat: b");

    let start: bisect::BisectArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "start",
        "good_ref": "HEAD~2",
        "bad_ref": "HEAD",
    }));
    let result = bisect::run(&start).await.expect("bisect start succeeds");
    assert!(
        text(&result).contains("Bisect started between good=HEAD~2 and bad=HEAD."),
        "{}",
        text(&result)
    );

    let reset: bisect::BisectArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "reset",
    }));
    bisect::run(&reset).await.expect("bisect reset succeeds");
}

#[tokio::test]
async fn bisect_run_rejects_shell_metacharacters() {
    let (_dir, repo) = fixture_repo();
    let run: bisect::BisectArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "run",
        "command_args": ["sh", "-c", "true && rm -rf /"],
    }));
    let error = bisect::run(&run)
        .await
        .expect_err("metacharacters rejected");
    assert!(error.message().contains("shell metacharacters"));
}

// ---------------------------------------------------------------------------
// git_tag
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tag_creates_lists_and_deletes() {
    let (_dir, repo) = fixture_repo();

    let create: tag::TagArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "create",
        "name": "v0.1.0",
    }));
    let result = tag::run(&create).await.expect("create succeeds");
    assert_eq!(structured(&result)["output"], "Created tag v0.1.0.");

    let list: tag::TagArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = tag::run(&list).await.expect("list succeeds");
    assert_eq!(text(&result), "v0.1.0");

    let delete: tag::TagArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "delete",
        "name": "v0.1.0",
    }));
    let result = tag::run(&delete).await.expect("delete succeeds");
    assert_eq!(structured(&result)["output"], "Deleted tag v0.1.0.");

    let list: tag::TagArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = tag::run(&list).await.expect("list succeeds");
    assert_eq!(text(&result), "No tags.");
}

// ---------------------------------------------------------------------------
// git_worktree
// ---------------------------------------------------------------------------

#[tokio::test]
async fn worktree_add_requires_a_branch_unless_detached() {
    let (_dir, repo) = fixture_repo();
    let add: worktree::WorktreeArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "add",
        "path": "/tmp/whatever",
    }));
    let error = worktree::run(&add).await.expect_err("branch required");
    assert_eq!(
        error.message(),
        "branch is required for worktree add unless detached=true."
    );
}

#[tokio::test]
async fn worktree_adds_lists_and_removes() {
    let (_dir, repo) = fixture_repo();
    git(&repo, &["branch", "wt-branch"]);
    let linked = tempfile::tempdir().expect("tempdir");
    let linked_path = linked.path().join("linked");
    let linked_str = linked_path.to_str().unwrap();

    let add: worktree::WorktreeArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "add",
        "path": linked_str,
        "branch": "wt-branch",
    }));
    let result = worktree::run(&add).await.expect("add succeeds");
    // git prints its own checkout summary, so the fallback is not used here.
    assert!(!text(&result).is_empty());
    assert!(
        linked_path.join("README.md").exists(),
        "worktree checked out at {}",
        linked_path.display()
    );

    let list: worktree::WorktreeArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = worktree::run(&list).await.expect("list succeeds");
    assert!(text(&result).contains("wt-branch"), "{}", text(&result));

    let remove: worktree::WorktreeArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "remove",
        "path": linked_str,
        "force": true,
    }));
    let result = worktree::run(&remove).await.expect("remove succeeds");
    assert_eq!(
        structured(&result)["output"],
        format!("Removed worktree {linked_str}.")
    );
}

// ---------------------------------------------------------------------------
// git_submodule
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submodule_lists_with_a_placeholder() {
    let (_dir, repo) = fixture_repo();
    let list: submodule::SubmoduleArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "list",
    }));
    let result = submodule::run(&list).await.expect("list succeeds");
    assert_eq!(structured(&result)["output"], "No submodules.");
}

#[tokio::test]
async fn submodule_add_requires_url_and_path() {
    let (_dir, repo) = fixture_repo();
    let add: submodule::SubmoduleArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "add",
        "url": "https://example.com/x.git",
    }));
    let error = submodule::run(&add).await.expect_err("path required");
    assert_eq!(
        error.message(),
        "url and path are required for submodule add."
    );
}

#[tokio::test]
async fn submodule_set_branch_requires_branch_and_path() {
    let (_dir, repo) = fixture_repo();
    let call: submodule::SubmoduleArgs = args(json!({
        "repo_path": repo_str(&repo),
        "action": "set_branch",
    }));
    let error = submodule::run(&call).await.expect_err("branch required");
    assert_eq!(
        error.message(),
        "branch and path are required for submodule set_branch."
    );
}
