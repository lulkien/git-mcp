//! Git domain services. Each module owns one tool group's logic and never
//! imports MCP transport types beyond tool results.

pub mod advanced;
pub mod analytics;
pub mod branch;
pub mod but;
pub mod context;
pub mod docs;
pub mod entire;
pub mod inspect;
pub mod jj;
pub mod lfs;
pub mod preflight;
pub mod remote;
pub mod rewrite;
pub mod tangled;
pub mod workspace;
pub mod write;
