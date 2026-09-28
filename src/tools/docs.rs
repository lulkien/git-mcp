//! `git_docs` — official Git documentation lookup.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};
use crate::services::docs::{fetch_git_man_page, search_git_docs};
use crate::tools::ok_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocsAction {
    /// Search git-scm.com by keyword.
    Search,
    /// Fetch the man page for one command.
    Man,
}

/// Arguments accepted by `git_docs`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocsArgs {
    /// Search or man-page lookup.
    pub action: DocsAction,
    /// For `search`, the search terms; for `man`, the git command name without
    /// the `git-` prefix (e.g. `commit`, `rebase`, `merge`).
    pub query: String,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Runs the requested documentation lookup.
pub async fn run(args: &DocsArgs) -> Result<CallToolResult, GitError> {
    match args.action {
        DocsAction::Search => {
            let response = search_git_docs(&args.query).await?;
            let payload = json!({
                "query": response.query,
                "results": response.results,
            });

            let text = match args.response_format {
                ResponseFormat::Json => render_content(&payload, ResponseFormat::Json)?,
                ResponseFormat::Markdown => render_search_markdown(&args.query, &payload),
            };
            Ok(ok_result(text, payload))
        }
        DocsAction::Man => {
            let content = fetch_git_man_page(&args.query).await?;
            let text = render_content(&to_value(&content)?, args.response_format)?;
            Ok(ok_result(
                text,
                json!({ "command": args.query, "content": content }),
            ))
        }
    }
}

/// Renders search results as a markdown document.
fn render_search_markdown(query: &str, payload: &Value) -> String {
    let results = payload["results"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();

    if results.is_empty() {
        return format!("No results found for \"{query}\" on git-scm.com.");
    }

    let mut lines = vec![format!("## Git Docs Search: \"{query}\"")];
    lines.push(String::new());
    for result in results {
        let title = result["title"].as_str().unwrap_or_default();
        let url = result["url"].as_str().unwrap_or_default();
        let excerpt = result["excerpt"].as_str().unwrap_or_default();
        lines.push(format!("### [{title}]({url})"));
        if !excerpt.is_empty() {
            lines.push(excerpt.to_owned());
        }
        lines.push(String::new());
    }

    lines.join("\n")
}
