//! Git domain services. Each module owns one tool group's logic and never
//! imports MCP transport types beyond tool results.

pub mod advanced;
pub mod branch;
pub mod context;
pub mod inspect;
pub mod remote;
pub mod workspace;
pub mod write;
