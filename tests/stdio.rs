//! End-to-end MCP protocol test: spawns the real binary and speaks JSON-RPC
//! over stdio, the way an MCP client does.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(30);

struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Server {
    fn spawn() -> Self {
        Self::spawn_with(&[])
    }

    fn spawn_with(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_git-mcp-rs"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("server binary starts");

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout: ChildStdout = child.stdout.take().expect("stdout piped");
        let (sender, lines) = channel();

        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        Self {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("write request");
        self.stdin.flush().expect("flush request");
    }

    fn recv(&self) -> Value {
        match self.lines.recv_timeout(TIMEOUT) {
            Ok(line) => serde_json::from_str(&line).expect("valid JSON-RPC message"),
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for a response"),
            Err(RecvTimeoutError::Disconnected) => panic!("server closed the stream"),
        }
    }

    /// Reads responses until one carries the requested id.
    fn recv_id(&self, id: u64) -> Value {
        loop {
            let message = self.recv();
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    fn request(&mut self, id: u64, method: &str, params: &Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        self.recv_id(id)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn initialize(server: &mut Server) -> Value {
    let response = server.request(
        1,
        "initialize",
        &json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "git-mcp-test", "version": "0.0.0" }
        }),
    );
    server.send(&json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
    }));
    response
}

#[test]
fn initializes_and_advertises_tool_capability() {
    let mut server = Server::spawn();
    let response = initialize(&mut server);

    assert_eq!(response["result"]["serverInfo"]["name"], "git-mcp-server");
    assert!(response["result"]["serverInfo"]["version"].is_string());
    assert!(
        response["result"]["capabilities"]["tools"].is_object(),
        "tools capability advertised: {response}"
    );
}

#[test]
fn lists_the_registered_tools() {
    let mut server = Server::spawn();
    initialize(&mut server);

    let response = server.request(2, "tools/list", &json!({}));
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools array")
        .clone();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();

    for expected in [
        "git_ping",
        "git_status",
        "git_history",
        "git_commits",
        "git_branches",
        "git_remotes",
        "git_workspace",
        "git_context",
        "git_stash",
        "git_rebase",
        "git_cherry_pick",
        "git_merge",
        "git_bisect",
        "git_tag",
        "git_worktree",
        "git_submodule",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected} in {names:?}"
        );
    }

    let history = tools
        .iter()
        .find(|tool| tool["name"] == "git_history")
        .expect("git_history present");
    assert!(
        history["description"]
            .as_str()
            .unwrap()
            .contains("action=log"),
        "description preserved"
    );
    let properties = &history["inputSchema"]["properties"];
    assert!(properties["repo_path"].is_object());
    assert!(properties["action"].is_object());
    assert!(
        properties["ref"].is_object(),
        "the wire parameter stays `ref` even though the Rust field is renamed"
    );
    assert!(properties["response_format"].is_object());
    assert_eq!(
        history["annotations"]["readOnlyHint"], true,
        "annotations preserved"
    );
}

#[test]
fn calls_git_ping() {
    let mut server = Server::spawn();
    initialize(&mut server);

    let response = server.request(
        2,
        "tools/call",
        &json!({ "name": "git_ping", "arguments": { "message": "hello" } }),
    );

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["content"][0]["text"],
        "git-mcp-server: hello"
    );
    assert_eq!(response["result"]["structuredContent"]["ok"], true);
    assert_eq!(response["result"]["structuredContent"]["message"], "hello");
}

#[test]
fn calls_git_status_against_a_real_repository() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test User"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(repo)
            .status()
            .expect("git runs");
        assert!(status.success());
    }
    std::fs::write(repo.join("a.txt"), "hello\n").expect("write file");

    let mut server = Server::spawn();
    initialize(&mut server);

    let response = server.request(
        2,
        "tools/call",
        &json!({
            "name": "git_status",
            "arguments": {
                "repo_path": repo.to_str().unwrap(),
                "action": "status",
                "response_format": "json"
            }
        }),
    );

    assert_eq!(response["result"]["isError"], false, "{response}");
    let status = &response["result"]["structuredContent"]["status"];
    assert_eq!(status["isClean"], false);
    assert_eq!(status["files"][0]["path"], "a.txt");
    assert_eq!(
        response["result"]["content"][0]["type"], "text",
        "json format still returns text content"
    );
}

#[test]
fn reports_a_tool_level_error_for_a_missing_repository() {
    let mut server = Server::spawn();
    initialize(&mut server);

    let response = server.request(
        2,
        "tools/call",
        &json!({
            "name": "git_status",
            "arguments": { "repo_path": "/definitely/not/a/repo" }
        }),
    );

    assert_eq!(response["result"]["isError"], true, "{response}");
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("Error (invalid_input):"), "{text}");
    assert!(text.contains("Repository path does not exist"), "{text}");
    assert_eq!(response["result"]["structuredContent"]["success"], false);
    assert_eq!(
        response["result"]["structuredContent"]["error"]["kind"],
        "invalid_input"
    );
}

#[test]
fn server_level_repo_default_from_cli_argument() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    let init = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo)
        .status()
        .expect("git runs");
    assert!(init.success());
    std::fs::write(repo.join("a.txt"), "hello\n").expect("write file");

    // `--repo-path` is how MCP clients configure the server-level default.
    let mut server = Server::spawn_with(&["--repo-path", repo.to_str().unwrap()]);
    initialize(&mut server);

    let response = server.request(
        2,
        "tools/call",
        &json!({
            "name": "git_status",
            "arguments": { "action": "status", "response_format": "json" }
        }),
    );

    assert_eq!(response["result"]["isError"], false, "{response}");
    assert_eq!(
        response["result"]["structuredContent"]["status"]["files"][0]["path"],
        "a.txt"
    );
}
