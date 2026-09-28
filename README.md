# git-mcp

> yet another git-mcp — a Git [MCP](https://modelcontextprotocol.io) server, in Rust

Exposes a complete Git workflow to any MCP client: inspect, commit, branch, remote, stash, rebase, merge, worktree, submodule, tags, bisect, history rewriting, LFS, repository analytics, and official documentation lookup. Safe by default, typed end to end, and fast to start.

24 tools covering 100+ actions, one static binary, no Node runtime.

---

## Why this exists

Agents are good at Git commands and bad at Git *safety*. A model that can run arbitrary shell commands will eventually `git reset --hard` over uncommitted work, force-push a shared branch, or pass `--upload-pack` where a ref belongs. git-mcp narrows that surface to a documented set of operations with explicit parameters, refuses the dangerous combinations unless you opt in, and reports every failure as structured data instead of a surprise.

---

## Requirements

- Rust 1.85 or newer (edition 2024)
- `git` available on `PATH` — the server executes the real binary, it does not link a Git library
- Git LFS on `PATH` for the `git_lfs` tool; the `but`, `jj`, `entire` CLIs are optional and only probed

## Build

```bash
cargo build --release        # target/release/git-mcp-rs
cargo install --path .       # or install it onto PATH
```

## Configure your MCP client

`--repo-path` sets the server-wide default repository, so clients do not have to pass `repo_path` on every call.

```json
{
  "mcpServers": {
    "git": {
      "command": "/home/you/.cargo/bin/git-mcp-rs",
      "args": ["--repo-path", "/path/to/your/repo"]
    }
  }
}
```

VS Code (`.vscode/mcp.json`) uses the same shape under a `servers` key, with `"${workspaceFolder}"` as the path.

Every tool also accepts `repo_path` per call, so one server can serve many repositories.

---

## Configuration

Read from the environment at startup. Everything below defaults to off — the server is restrictive until you say otherwise.

| Variable | Default | Effect |
| --- | --- | --- |
| `GIT_REPO_PATH` | unset | Default repository path. Equivalent to `--repo-path`. |
| `GIT_ALLOW_NO_VERIFY` | off | Permits `no_verify=true` on commits and pushes, which passes `--no-verify` and skips Git hooks. |
| `GIT_ALLOW_FORCE_PUSH` | off | Permits `force=true` on push. `force_with_lease=true` always works and is the safer option. |
| `GIT_AUTO_SIGN_COMMITS` | off | Signs every commit this server creates. |
| `GIT_AUTO_SIGN_TAGS` | off | Signs every tag this server creates. |
| `GIT_SIGNING_KEY` | unset | Signing key (GPG key ID or SSH public key path) used when signing is on. |
| `GIT_ALLOW_BUT` | off | Enables the GitButler `but` probe in `git_but_check`. |
| `GIT_ALLOW_JJ` | off | Enables the Jujutsu `jj` probe in `git_jj_check`. |
| `GIT_ALLOW_TANGLED` | off | Enables the Tangled host probe in `git_tangled_check`. |
| `GIT_ALLOW_ENTIRE` | off | Enables the Entire probe in `git_entire_check`. |
| `BUT_BINARY` / `JJ_BINARY` / `ENTIRE_BINARY` | `but` / `jj` / `entire` | Override the probed executable paths. |

Enabled non-default behaviour is announced on stderr at startup, so a misconfigured server is visible in the client's logs rather than silent.

`GIT_ALLOW_FLOW_HOOKS` and `GIT_SIGNING_FORMAT` are parsed but currently inert — see [Status](#status).

---

## Tools

Grouped tools dispatch on an `action` parameter; single-purpose tools cover the rest.

| Tool | Actions | Notes |
| --- | --- | --- |
| `git_ping` | — | Round-trip check that the server is up. |
| `git_status` | `status`, `diff`, `diff_main` | Working tree state; diffs, optionally filtered of dependency directories and binary/asset files; `diff_main` diffs against a base branch from the merge base. |
| `git_history` | `log`, `show`, `reflog`, `blame`, `lg`, `who` | Commit queries with author/grep/date/path filters, ordering and revision ranges. |
| `git_commits` | `add`, `restore`, `commit`, `reset`, `revert`, `undo`, `nuke`, `wip`, `unstage`, `amend` | Staging, committing, resetting. `reset mode=hard` and `nuke` require `confirm=true`. |
| `git_branches` | `list`, `create`, `delete`, `rename`, `checkout`, `set_upstream`, `recent` | Branch lifecycle. `recent` sorts by commit date and honours `count`. |
| `git_remotes` | `list`, `manage`, `fetch`, `pull`, `push` | Transport. Remote URLs are sanitized — credentials and opaque tokens never reach the client. |
| `git_workspace` | `stash`, `stash_all`, `rebase`, `cherry_pick`, `merge`, `bisect`, `tag`, `worktree`, `submodule` | One entry point for the workspace operations that also have dedicated tools, plus `stash_all`. |
| `git_context` | `summary`, `search`, `get_config`, `set_config`, `aliases` | Branch/ahead/behind summary, in-progress operation detection, history search, and git config access with credential keys blocked. |
| `git_stash` | `save`, `list`, `apply`, `pop`, `drop` | |
| `git_rebase` | `start`, `continue`, `abort`, `skip` | `interactive`, `autosquash`, `merges`, `onto`, `upstream`, `branch`. |
| `git_cherry_pick` | `start`, `continue`, `abort` | `mainline`, `record_origin`, `no_commit`, strategy options. |
| `git_merge` | `start`, `continue`, `abort` | `no_ff`, `ff_only`, `squash`, `no_commit`, `log`, strategy, `conflict_style`. |
| `git_bisect` | `start`, `good`, `bad`, `skip`, `run`, `reset` | `run` rejects shell metacharacters in its argv. |
| `git_tag` | `list`, `create`, `delete` | Annotated and signed tags, honouring `GIT_AUTO_SIGN_TAGS`. |
| `git_worktree` | `add`, `list`, `remove`, `lock`, `unlock`, `prune`, `repair` | Linked worktrees. |
| `git_submodule` | `add`, `list`, `update`, `sync`, `set_branch` | Recursive, remote-tracking and shallow updates. |
| `git_rewrite` | `reword`, `squash`, `rewrite-messages`, `backup`, `restore` | History rewriting, gated on `confirm=true`. `backup` creates `rewrite-backup/<name>` first. |
| `git_analytics` | `contributors`, `churn`, `activity`, `summary`, `file-stats` | Read-only, computed from local history. |
| `git_lfs` | `track`, `untrack`, `ls-files`, `status`, `pull`, `push`, `install`, `migrate-import`, `migrate-export` | Git Large File Storage. |
| `git_docs` | `search`, `man` | Searches git-scm.com, or fetches a command's man page. Requires network access. |
| `git_but_check` | — | Reports whether the GitButler `but` CLI is available, and whether to prefer it. |
| `git_jj_check` | — | Reports whether `jj` is available and whether the repo is jj-managed. |
| `git_tangled_check` | — | Reports whether the origin remote is a Tangled host. |
| `git_entire_check` | — | Reports whether the Entire CLI is available and whether the repo is Entire-managed. |

The four `*_check` tools exist because other version-control layers can sit on top of the same `.git` directory. Each returns `enabled`, a detection result, and `guidance` explaining which tool the agent should prefer.

---

## Responses

Every successful call returns both rendered text and structured content:

```json
{
  "content": [{ "type": "text", "text": "…" }],
  "structuredContent": { "output": "…" }
}
```

`response_format` selects the text rendering: `markdown` (default) or `json`. Structured content is always present and always typed — field names are camelCase on the wire.

Failures return `isError: true` plus a classified error, so a client can tell "no such repository" from "merge conflict" without parsing prose:

```json
{
  "isError": true,
  "content": [{ "type": "text", "text": "Error (invalid_input): Repository path does not exist: /nope" }],
  "structuredContent": {
    "success": false,
    "error": {
      "message": "Repository path does not exist: /nope",
      "kind": "invalid_input",
      "severity": "medium",
      "category": "git_error",
      "code": "invalid_input"
    }
  }
}
```

Kinds: `invalid_input`, `repository_state`, `permission`, `missing_git`, `git_conflict`, `network`, `unsupported`, `unknown`. Severity is derived from the kind. Rendered output is truncated at 25 000 characters with an explicit marker.

---

## Safety model

- **Repository paths are validated** — the target must exist and be a directory; `git` is never spawned against a path that is not a repository root.
- **Path arguments cannot escape the repository** — arguments are normalized, then checked lexically *and* through symlinks, so `../../etc/passwd` and a symlink pointing outside the tree are both rejected.
- **Git is never given an option where it expects a value** — refs, branch names, remotes, refspecs, pathspecs, worktree paths, patterns and search phrases are rejected if they are empty, start with `-`, or contain control characters. This closes the injection class that a model-authored `--upload-pack=…` belongs to.
- **Destructive operations are gated** — hard resets, `nuke`, history rewriting and restore-from-backup require `confirm=true`; force push and hook bypass require the matching server flag.
- **History rewriting checks preconditions first** — a dirty worktree, a detached HEAD, or a half-finished merge/rebase/cherry-pick/bisect aborts the operation with an explanatory error.
- **`git bisect run` argv is screened** — arguments containing shell metacharacters are refused, since Git runs that command through a shell.
- **Secrets are redacted** — remote URLs, config values and error messages pass through credential and token redaction before they are returned. Sensitive config keys (`credential.*`, `url.*`, `core.sshCommand`, `http.*.extraHeader`) cannot be read or written at all.
- **Git cannot consume the protocol stream** — every spawned `git` gets `stdin` closed, so it can never read the MCP transport.

None of this replaces branch protection. `force_with_lease` is preferred over `force`, and the server says so in the error when it refuses.

---

## How it works

```
┌──────────────────────────────────────────────┐
│  MCP transport (stdio, rmcp)                 │  src/server.rs, src/main.rs
├──────────────────────────────────────────────┤
│  Tools — validate input, render responses    │  src/tools/*
├──────────────────────────────────────────────┤
│  Services — domain logic, safety rules       │  src/services/*
├──────────────────────────────────────────────┤
│  Git adapter — validation + `git` execution  │  src/git/*
└──────────────────────────────────────────────┘
```

Two deliberate choices shape everything else.

**It shells out to `git`.** Hooks, credential helpers, commit and tag signing, LFS filters, `includeIf` config, worktrees and submodules are all behaviours of the user's Git installation. A library binding would silently diverge from them, so the server runs the real binary and classifies failures from its stderr.

**It uses the official MCP Rust SDK (`rmcp`).** Tool schemas are derived from Rust types with `schemars`, so the wire contract and the handler signature cannot drift apart.

Errors are `GitError` values with a kind and a message; the kind drives severity, the response `code`, and which preconditions a caller can rely on.

---

## Development

```bash
cargo test                       # 141 tests
cargo clippy --all-targets       # pedantic, zero warnings
cargo fmt --check
```

Test layers:

| File | What it covers |
| --- | --- |
| `src/**` unit tests | Parsers and pure logic: status/log/numstat parsing, path normalization and containment, redaction, argument validation, config parsing, HTML stripping. |
| `tests/tools.rs` | `git_status` / `git_history` against a real repository. |
| `tests/grouped.rs` | The grouped tools, with arguments built from JSON so the wire defaults are exercised. |
| `tests/workspace.rs` | The standalone workspace tools against real repositories. |
| `tests/lfs_docs.rs` | LFS and docs input validation (offline). |
| `tests/stdio.rs` | Spawns the real binary and speaks JSON-RPC over stdio: initialize, `tools/list`, `tools/call`, error shapes, and the `--repo-path` default. |

Tests run against real `git` rather than a mock, so parsing is verified against actual output.

---

## Status

Implemented: everything listed under [Tools](#tools).

Not yet implemented:

- `git_pr` — pull request creation and listing across GitHub, GitLab, Forgejo, Gitea and Bitbucket, via provider CLI or REST. `GITHUB_TOKEN`, `GITLAB_TOKEN`, `FORGEJO_TOKEN`, `BITBUCKET_TOKEN` and `GIT_FORGE_PROVIDER` are already parsed for it.
- `git_flow` — Git Flow branching-model automation. `GIT_ALLOW_FLOW_HOOKS` is parsed for it but inert until it lands.
- `git_workflow` — resumable multi-step workflows (snapshot, replay, branch surgery, publish) with state persisted under `.git/`.
- MCP **resources** — the URI-addressable read-only views (`status`, `log`, `branches`, `diff`) that complement the tools.
- `GIT_SIGNING_FORMAT` is likewise parsed but unused.

## License

MIT
