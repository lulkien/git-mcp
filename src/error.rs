//! Structured Git error model: classification of `git` failures into kinds,
//! severity mapping, and conversion into MCP tool results.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use serde_json::{Value, json};

/// Kind of Git failure, used for severity mapping and client-side branching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitErrorKind {
    InvalidInput,
    RepositoryState,
    Permission,
    MissingGit,
    GitConflict,
    Network,
    Unsupported,
    Unknown,
}

impl GitErrorKind {
    /// Wire representation of the kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::RepositoryState => "repository_state",
            Self::Permission => "permission",
            Self::MissingGit => "missing_git",
            Self::GitConflict => "git_conflict",
            Self::Network => "network",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for GitErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

static GIT_NOT_FOUND_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(not found|is not recognized|command not found|ENOENT|'git' is not recognized|cannot find|no such file or directory|bad interpreter|does not exist)",
    )
    .expect("valid git-not-found pattern")
});

static PERMISSION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(permission denied|EACCES|EPERM|Access denied|Could not create|Read-only file system|PermissionError|operation not permitted|insufficient permissions)",
    )
    .expect("valid permission pattern")
});

static CONFLICT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(CONFLICT|merge conflict|rebase in progress|cherry-pick in progress|Merge conflict|unmerged paths|not possible to fast-forward|automatic merge failed)",
    )
    .expect("valid conflict pattern")
});

static NETWORK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(network|timed out|unable to access|could not resolve host|proxy|Connection refused|Host unreachable|Network unreachable|failed to connect|connection timeout|remote error)",
    )
    .expect("valid network pattern")
});

/// A Git failure carrying a classified kind and a human-readable message.
#[derive(Debug, Clone)]
pub struct GitError {
    kind: GitErrorKind,
    message: String,
}

impl GitError {
    /// Builds an error with an explicit kind.
    pub fn new(kind: GitErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Builds an error whose kind is derived from its message, the way `git`
    /// failures are classified from stderr.
    pub fn classified(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            kind: classify(&message),
            message,
        }
    }

    /// Builds an error for input this server rejects before reaching `git`:
    /// escaping path arguments, missing required refs, malformed refs.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            kind: GitErrorKind::InvalidInput,
            message: message.into(),
        }
    }

    /// Builds an error for internal failures (rendering, serialization) that
    /// carry no classification.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: GitErrorKind::Unknown,
            message: message.into(),
        }
    }

    /// The classified kind.
    #[must_use]
    pub fn kind(&self) -> GitErrorKind {
        self.kind
    }

    /// The human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Splits the error into its parts.
    #[must_use]
    pub fn into_parts(self) -> (GitErrorKind, String) {
        (self.kind, self.message)
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(error: std::io::Error) -> Self {
        Self::classified(error.to_string())
    }
}

/// Builds a classified error with `format!`-style message construction.
#[macro_export]
macro_rules! git_error {
    ($($arg:tt)*) => {
        $crate::error::GitError::classified(format!($($arg)*))
    };
}

/// Builds an input-validation error with `format!`-style message construction.
#[macro_export]
macro_rules! invalid_input {
    ($($arg:tt)*) => {
        $crate::error::GitError::invalid_input(format!($($arg)*))
    };
}

/// Classifies a raw message into a [`GitErrorKind`] using the same ordered
/// heuristics used for `git` failures.
#[must_use]
pub fn classify(message: &str) -> GitErrorKind {
    if GIT_NOT_FOUND_PATTERN.is_match(message) {
        return GitErrorKind::MissingGit;
    }
    if PERMISSION_PATTERN.is_match(message) {
        return GitErrorKind::Permission;
    }
    if CONFLICT_PATTERN.is_match(message) {
        return GitErrorKind::GitConflict;
    }
    if NETWORK_PATTERN.is_match(message) {
        return GitErrorKind::Network;
    }
    GitErrorKind::Unknown
}

/// Severity reported for a response `kind` string.
#[must_use]
pub fn determine_severity(kind: &str) -> &'static str {
    if kind.is_empty() {
        return "medium";
    }
    if ["security_error", "permission", "path_traversal"].contains(&kind) {
        return "critical";
    }
    if [
        "missing_git",
        "git_conflict",
        "validation_error",
        "not_found",
    ]
    .contains(&kind)
    {
        return "high";
    }
    "medium"
}

/// Builds the MCP tool result for a failure, with `isError: true` so clients can
/// distinguish it from a successful response.
#[must_use]
pub fn build_tool_error(error: &GitError, context: Option<&str>) -> CallToolResult {
    let message = crate::security::redact::redact_error(error.message());
    let prefixed = match context {
        Some(context) => format!("{context}: {message}"),
        None => message,
    };
    let kind = error.kind().as_str();

    let mut result = CallToolResult::error(vec![ContentBlock::text(format!(
        "Error ({kind}): {prefixed}"
    ))]);
    result.structured_content = Some(json!({
        "success": false,
        "error": {
            "message": prefixed,
            "kind": kind,
            "severity": determine_severity(kind),
            "category": "git_error",
            "code": kind,
        }
    }));
    result
}

/// Converts a `Result` into an MCP tool result, mapping `Err` through
/// [`build_tool_error`].
pub fn into_tool_result(
    result: Result<CallToolResult, GitError>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    Ok(result.unwrap_or_else(|error| build_tool_error(&error, None)))
}

/// Renders any serializable payload as a JSON value for `structuredContent`.
pub fn to_value<T: Serialize>(payload: T) -> Result<Value, GitError> {
    serde_json::to_value(payload)
        .map_err(|error| GitError::internal(format!("Failed to render content as JSON: {error}")))
}
