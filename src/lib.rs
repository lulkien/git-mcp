//! yet another git-mcp.
//!
//! The crate is layered: tools validate input and render responses, services
//! hold the domain logic, and the Git adapter shells out to the configured
//! `git` binary.

pub mod config;
pub mod constants;
pub mod error;
pub mod git;
pub mod render;
pub mod security;
pub mod server;
pub mod services;
pub mod tools;
pub mod types;

pub use config::{Config, config, resolve_repo_path};
pub use error::{GitError, GitErrorKind, build_tool_error};
pub use render::ResponseFormat;
pub use server::GitMcp;
pub use services::context::ContextSummary;
pub use types::{BranchInfo, CommitInfo, DiffSummary, FileStatus, GitStatusResult, RemoteInfo};
