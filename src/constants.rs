//! Shared constants.

/// Server name reported during MCP initialization.
pub const SERVER_NAME: &str = "git-mcp-server";

/// Server version, taken from `Cargo.toml`.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum length of a rendered tool response before it is truncated.
pub const CHARACTER_LIMIT: usize = 25_000;

/// Diff directories skipped when `filtered` diffing is requested.
pub const EXCLUDED_DIFF_DIRECTORIES: &[&str] = &["node_modules/", ".yarn/", ".astro/", "dist/"];

/// Diff file extensions skipped when `filtered` diffing is requested.
pub const EXCLUDED_DIFF_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "bmp", "tiff", "mp4", "mp3", "wav", "ogg",
    "pdf", "woff", "woff2", "ttf", "eot", "zip", "tar", "gz",
];

/// State file name used by multi-step workflows (stored under `.git/`).
pub const WORKFLOW_STATE_FILENAME: &str = "gitworkflow.state.json";

/// Error message templates shared by tools.
pub mod error_templates {
    pub const NO_REPO_PATH: &str = "No repository path provided. Pass repo_path in the tool request, \
or configure a server default via GIT_REPO_PATH environment variable or --repo / --repo-path CLI argument.";
    pub const HOOK_BYPASS_DISABLED: &str = "no_verify is disabled on this server. Set GIT_ALLOW_NO_VERIFY=true to permit bypassing git hooks.";
    pub const FORCE_PUSH_DISABLED: &str =
        "force is disabled on this server. Set GIT_ALLOW_FORCE_PUSH=true to permit force push.";
    pub const FLOW_HOOKS_DISABLED: &str = "git_flow hooks and filters are disabled on this server. Set GIT_ALLOW_FLOW_HOOKS=true to permit execution.";
}
