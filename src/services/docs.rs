//! Git documentation lookups against git-scm.com.

use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::constants::CHARACTER_LIMIT;
use crate::error::GitError;
use crate::security::assert_safe_command_name;

/// User agent sent with documentation requests.
const USER_AGENT: &str = "git-mcp-docs/1.0";

/// Request timeout for documentation lookups.
const TIMEOUT: Duration = Duration::from_secs(10);

/// One documentation search hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitDocsSearchResult {
    /// Page title.
    pub title: String,
    /// Absolute URL of the page.
    pub url: String,
    /// Matching excerpt, when the site provides one.
    pub excerpt: String,
}

/// Documentation search response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitDocsSearchResponse {
    /// The query that was searched for.
    pub query: String,
    /// Matching pages.
    pub results: Vec<GitDocsSearchResult>,
}

static SCRIPT_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("valid script pattern"));
static STYLE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("valid style pattern"));
static ANY_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("valid tag pattern"));
static NUMERIC_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#(\d+);").expect("valid entity pattern"));
static WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("valid whitespace pattern"));

static RESULT_LIST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<ul[^>]*>(.*?)</ul>").expect("valid list pattern"));
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<li[^>]*>(.*?)</li>").expect("valid item pattern"));
static ITEM_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#).expect("valid link pattern")
});
static ITEM_EXCERPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<(?:p|span)[^>]*class="[^"]*excerpt[^"]*"[^>]*>(.*?)</(?:p|span)>"#)
        .expect("valid excerpt pattern")
});
static DOCS_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a[^>]+href="(/docs/[^"]+)"[^>]*>(.*?)</a>"#)
        .expect("valid docs link pattern")
});

static MAIN_SECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?is)<(?:article|div)[^>]*(?:id="main"|class="[^"]*(?:sect|man-page|article)[^"]*")[^>]*>"#,
    )
    .expect("valid main section pattern")
});
static SECTION_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)</(?:article|div)>").expect("valid section end pattern"));

/// Strips HTML tags, decodes common entities, and collapses whitespace.
#[must_use]
pub fn strip_html(html: &str) -> String {
    let text = SCRIPT_BLOCK.replace_all(html, " ");
    let text = STYLE_BLOCK.replace_all(&text, " ");
    let text = ANY_TAG.replace_all(&text, " ");

    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    let text = NUMERIC_ENTITY.replace_all(&text, |caps: &regex::Captures<'_>| {
        caps.get(1)
            .and_then(|code| code.as_str().parse::<u32>().ok())
            .and_then(char::from_u32)
            .map_or_else(String::new, |character| character.to_string())
    });

    WHITESPACE.replace_all(&text, " ").trim().to_owned()
}

/// Extracts the content between two markers in an HTML document.
#[must_use]
pub fn extract_between(html: &str, start_marker: &Regex, end_marker: &Regex) -> String {
    let Some(start) = start_marker.find(html) else {
        return html.to_owned();
    };
    let remainder = &html[start.end()..];

    match end_marker.find(remainder) {
        Some(end) => remainder[..end.start()].to_owned(),
        None => remainder.to_owned(),
    }
}

/// Percent-encodes a query the way JavaScript's `encodeURIComponent` does.
#[must_use]
pub fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    encoded
}

async fn fetch_html(url: &str) -> Result<String, GitError> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| GitError::classified(error.to_string()))?;

    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "text/html")
        .send()
        .await
        .map_err(|error| GitError::classified(error.to_string()))?;

    if !response.status().is_success() {
        return Err(GitError::classified(format!(
            "git-scm.com returned HTTP {}",
            response.status().as_u16()
        )));
    }

    response
        .text()
        .await
        .map_err(|error| GitError::classified(error.to_string()))
}

/// Searches git-scm.com using the same endpoint as the site's search box.
pub async fn search_git_docs(query: &str) -> Result<GitDocsSearchResponse, GitError> {
    let url = format!(
        "https://git-scm.com/search/results?search={}&language=en",
        encode_uri_component(query)
    );

    let html = fetch_html(&url).await.map_err(|error| {
        GitError::classified(format!(
            "Failed to fetch git docs search: {}",
            error.message()
        ))
    })?;

    let list_html = RESULT_LIST
        .captures(&html)
        .and_then(|caps| caps.get(1))
        .map_or(html.as_str(), |list| list.as_str());

    let mut results = Vec::new();
    for item in LIST_ITEM.captures_iter(list_html) {
        let Some(item) = item.get(1) else { continue };
        let Some(link) = ITEM_LINK.captures(item.as_str()) else {
            continue;
        };

        let href = link.get(1).map_or("", |m| m.as_str()).trim();
        let title = strip_html(link.get(2).map_or("", |m| m.as_str()))
            .trim()
            .to_owned();
        if title.is_empty() {
            continue;
        }

        let excerpt = ITEM_EXCERPT
            .captures(item.as_str())
            .and_then(|caps| caps.get(1))
            .map(|excerpt| strip_html(excerpt.as_str()).trim().to_owned())
            .unwrap_or_default();

        let full_url = if href.starts_with("http") {
            href.to_owned()
        } else {
            format!("https://git-scm.com{href}")
        };
        results.push(GitDocsSearchResult {
            title,
            url: full_url,
            excerpt,
        });
    }

    // Fallback: plain links to /docs/ pages when the list markup is absent.
    if results.is_empty() {
        let mut seen = Vec::new();
        for link in DOCS_LINK.captures_iter(&html) {
            if results.len() >= 20 {
                break;
            }
            let href = link.get(1).map_or("", |m| m.as_str());
            let title = strip_html(link.get(2).map_or("", |m| m.as_str()))
                .trim()
                .to_owned();
            if title.is_empty() || seen.contains(&href.to_owned()) {
                continue;
            }
            seen.push(href.to_owned());
            results.push(GitDocsSearchResult {
                title,
                url: format!("https://git-scm.com{href}"),
                excerpt: String::new(),
            });
        }
    }

    Ok(GitDocsSearchResponse {
        query: query.to_owned(),
        results,
    })
}

/// Fetches the man page for a git command from git-scm.com.
pub async fn fetch_git_man_page(command: &str) -> Result<String, GitError> {
    let normalized = command.trim().to_lowercase();
    let normalized = normalized
        .strip_prefix("git-")
        .or_else(|| normalized.strip_prefix("git "))
        .unwrap_or(&normalized)
        .to_owned();

    assert_safe_command_name(&normalized)?;

    let url = format!("https://git-scm.com/docs/git-{normalized}");
    let html = fetch_html(&url).await.map_err(|error| {
        if error.message().contains("HTTP 404") {
            GitError::classified(format!(
                "No man page found for \"git {normalized}\". Check the command name or search with action=\"search\"."
            ))
        } else {
            GitError::classified(format!(
                "Failed to fetch git man page: {}",
                error.message()
            ))
        }
    })?;

    let article = extract_between(&html, &MAIN_SECTION, &SECTION_END);
    let content = strip_html(if article.is_empty() { &html } else { &article });

    let full = format!("# git-{normalized}(1)\n\nSource: {url}\n\n{content}");
    Ok(if full.chars().count() > CHARACTER_LIMIT {
        let head: String = full.chars().take(CHARACTER_LIMIT).collect();
        format!("{head}\n\n[...truncated — content exceeded limit]")
    } else {
        full
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_scripts_and_entities() {
        let html = "<html><head><style>p{color:red}</style><script>var x = 1;</script></head>\
<body><p>Hello &amp; welcome&nbsp;to <b>Git</b></p><p>&#8212; done</p></body></html>";
        let text = strip_html(html);
        assert_eq!(text, "Hello & welcome to Git — done");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(strip_html("a\n\n   b\t\tc"), "a b c");
    }

    #[test]
    fn extracts_between_markers() {
        let html = "<div id=\"main\"><p>body</p></div><footer>end</footer>";
        let article = extract_between(html, &MAIN_SECTION, &SECTION_END);
        assert!(article.contains("body"));
        assert!(!article.contains("footer"));
    }

    #[test]
    fn returns_whole_document_without_markers() {
        let html = "<p>no markers</p>";
        assert_eq!(extract_between(html, &MAIN_SECTION, &SECTION_END), html);
    }

    #[test]
    fn encodes_queries_like_encode_uri_component() {
        assert_eq!(
            encode_uri_component("undo last commit"),
            "undo%20last%20commit"
        );
        assert_eq!(encode_uri_component("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(encode_uri_component("50%"), "50%25");
        assert_eq!(encode_uri_component("rebase --onto"), "rebase%20--onto");
    }
}
