//! Entry point: announce the active configuration on stderr, then serve MCP
//! over stdio.

use rmcp::ServiceExt;
use rmcp::transport::stdio;

use git_mcp_rs::GitMcp;
use git_mcp_rs::config::config;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    for note in config().startup_notes() {
        eprintln!("[git-mcp] {note}");
    }

    let service = match GitMcp::new().serve(stdio()).await {
        Ok(service) => service,
        Err(error) => {
            eprintln!("Server startup failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    match service.waiting().await {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Server exited with error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
