//! Git adapter: repository/path validation and `git` CLI execution.

pub mod client;
pub mod runner;

pub use client::{
    relative_to_root, validate_path_argument, validate_path_arguments, validate_repo_path,
};
pub use runner::Git;
