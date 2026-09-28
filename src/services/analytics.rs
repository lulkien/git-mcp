//! Repository analytics computed from local history.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::GitError;
use crate::git::Git;

/// Per-author commit and line statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContributorStats {
    /// Author name.
    pub name: String,
    /// Author email.
    pub email: String,
    /// Number of commits authored.
    pub commits: u64,
    /// Lines added across those commits.
    pub additions: u64,
    /// Lines removed across those commits.
    pub deletions: u64,
    /// Earliest commit date, ISO-8601 strict.
    pub first_activity: String,
    /// Latest commit date, ISO-8601 strict.
    pub last_activity: String,
}

/// Churn statistics for one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChurnEntry {
    /// Repository-relative path.
    pub file: String,
    /// Number of commits touching the file.
    pub commits: u64,
    /// Lines added.
    pub additions: u64,
    /// Lines removed.
    pub deletions: u64,
}

/// Commit count for one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ActivityEntry {
    /// Day, formatted `YYYY-MM-DD`.
    pub period: String,
    /// Commits authored that day.
    pub commits: u64,
}

/// File size entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileSizeEntry {
    /// Repository-relative path.
    pub path: String,
    /// Blob size in bytes.
    pub bytes: u64,
}

/// File count for one extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileStatEntry {
    /// File extension, or `(none)` for extension-less files.
    pub extension: String,
    /// Number of tracked files with that extension.
    pub count: u64,
}

/// Repository-level summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    /// Number of local branches.
    pub branch_count: u64,
    /// Number of tags.
    pub tag_count: u64,
    /// Number of commits reachable from HEAD.
    pub total_commits: u64,
    /// The ten most active contributors.
    pub top_contributors: Vec<ContributorStats>,
    /// Oldest commit date.
    pub oldest_commit: String,
    /// Newest commit date.
    pub newest_commit: String,
}

/// Tracked-file statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileStats {
    /// File counts grouped by extension, most common first.
    pub by_extension: Vec<FileStatEntry>,
    /// The twenty largest tracked files.
    pub largest_files: Vec<FileSizeEntry>,
    /// Paths touched by the ten most recent commits.
    pub recently_modified: Vec<String>,
}

/// One commit parsed from `git log --numstat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Commit hash.
    pub hash: String,
    /// Author name.
    pub author_name: String,
    /// Author email.
    pub author_email: String,
    /// Author date, ISO-8601 strict.
    pub date_iso: String,
    /// Lines added by the commit.
    pub additions: u64,
    /// Lines removed by the commit.
    pub deletions: u64,
}

/// Parses `git log --pretty=format:%H\t%an\t%ae\t%aI --numstat` output.
///
/// Header lines carry four tab-separated fields; the `--numstat` lines that
/// follow each header carry three and are folded into the commit above them.
#[must_use]
pub fn parse_log_entries(output: &str) -> Vec<LogEntry> {
    let mut entries: Vec<LogEntry> = Vec::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();

        if fields.len() >= 4 {
            entries.push(LogEntry {
                hash: fields[0].to_owned(),
                author_name: fields[1].to_owned(),
                author_email: fields[2].to_owned(),
                date_iso: fields[3].to_owned(),
                additions: 0,
                deletions: 0,
            });
            continue;
        }

        if fields.len() == 3
            && let Some(entry) = entries.last_mut()
        {
            entry.additions += fields[0].trim().parse::<u64>().unwrap_or(0);
            entry.deletions += fields[1].trim().parse::<u64>().unwrap_or(0);
        }
    }

    entries
}

async fn get_log_entries(repo_path: &Path, limit: usize) -> Result<Vec<LogEntry>, GitError> {
    let git = Git::open(repo_path)?;
    let output = git
        .raw(&[
            "log",
            "--pretty=format:%H\t%an\t%ae\t%aI",
            "--numstat",
            "--no-renames",
            "-n",
            &limit.to_string(),
        ])
        .await?;
    Ok(parse_log_entries(&output))
}

/// Aggregates commit and line counts per author, most commits first.
pub async fn get_contributors(repo_path: &Path) -> Result<Vec<ContributorStats>, GitError> {
    let entries = get_log_entries(repo_path, 1000).await?;
    let mut by_author: BTreeMap<String, ContributorStats> = BTreeMap::new();

    for entry in entries {
        let key = format!("{} <{}>", entry.author_name, entry.author_email);
        match by_author.get_mut(&key) {
            Some(existing) => {
                existing.commits += 1;
                existing.additions += entry.additions;
                existing.deletions += entry.deletions;
                if entry.date_iso < existing.first_activity {
                    existing.first_activity.clone_from(&entry.date_iso);
                }
                if entry.date_iso > existing.last_activity {
                    existing.last_activity.clone_from(&entry.date_iso);
                }
            }
            None => {
                by_author.insert(
                    key,
                    ContributorStats {
                        name: entry.author_name,
                        email: entry.author_email,
                        commits: 1,
                        additions: entry.additions,
                        deletions: entry.deletions,
                        first_activity: entry.date_iso.clone(),
                        last_activity: entry.date_iso,
                    },
                );
            }
        }
    }

    let mut contributors: Vec<ContributorStats> = by_author.into_values().collect();
    contributors.sort_by_key(|contributor| Reverse(contributor.commits));
    Ok(contributors)
}

/// Per-file churn across history, most-touched first, limited to 50 files.
pub async fn get_churn(repo_path: &Path) -> Result<Vec<ChurnEntry>, GitError> {
    let git = Git::open(repo_path)?;
    let output = git
        .raw(&["log", "--pretty=format:", "--numstat", "--no-renames"])
        .await?;

    let mut by_file: BTreeMap<String, ChurnEntry> = BTreeMap::new();

    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [additions, deletions, file] = fields[..] else {
            continue;
        };
        if file.is_empty() || additions == "-" || deletions == "-" {
            continue;
        }
        let churn = by_file
            .entry(file.to_owned())
            .or_insert_with(|| ChurnEntry {
                file: file.to_owned(),
                commits: 0,
                additions: 0,
                deletions: 0,
            });
        churn.commits += 1;
        churn.additions += additions.parse::<u64>().unwrap_or(0);
        churn.deletions += deletions.parse::<u64>().unwrap_or(0);
    }

    let mut churn: Vec<ChurnEntry> = by_file.into_values().collect();
    churn.sort_by_key(|entry| Reverse(entry.commits));
    churn.truncate(50);
    Ok(churn)
}

/// Commits per day, oldest first.
pub async fn get_activity(repo_path: &Path) -> Result<Vec<ActivityEntry>, GitError> {
    let git = Git::open(repo_path)?;
    let output = git
        .raw(&["log", "--pretty=format:%ad", "--date=short"])
        .await?;

    let mut by_period: BTreeMap<String, u64> = BTreeMap::new();
    for date in output.lines().filter(|line| !line.is_empty()) {
        *by_period.entry(date.to_owned()).or_insert(0) += 1;
    }

    Ok(by_period
        .into_iter()
        .map(|(period, commits)| ActivityEntry { period, commits })
        .collect())
}

/// Repository-level counts, activity window and top contributors.
pub async fn get_repo_summary(repo_path: &Path) -> Result<RepoSummary, GitError> {
    let git = Git::open(repo_path)?;

    let (branches, tags, total, oldest, newest) = tokio::join!(
        git.raw(&["branch", "--list"]),
        git.raw(&["tag", "--list"]),
        git.raw(&["rev-list", "--count", "HEAD"]),
        git.raw(&["log", "--pretty=format:%aI", "--reverse"]),
        git.raw(&["log", "-1", "--pretty=format:%aI"]),
    );

    let branch_count = count_lines(&branches?);
    let tag_count = count_lines(&tags?);
    let total_commits = total?.trim().parse::<u64>().unwrap_or(0);
    let oldest_commit = oldest?.lines().next().unwrap_or_default().trim().to_owned();
    let newest_commit = newest?.trim().to_owned();

    let mut top_contributors = get_contributors(repo_path).await?;
    top_contributors.truncate(10);

    Ok(RepoSummary {
        branch_count,
        tag_count,
        total_commits,
        top_contributors,
        oldest_commit,
        newest_commit,
    })
}

/// Counts non-empty lines in command output.
#[must_use]
pub fn count_lines(output: &str) -> u64 {
    output.lines().filter(|line| !line.is_empty()).count() as u64
}

/// Tracked-file statistics derived from the index and recent history.
pub async fn get_file_stats(repo_path: &Path) -> Result<FileStats, GitError> {
    let git = Git::open(repo_path)?;
    let mut by_extension: HashMap<String, u64> = HashMap::new();
    let mut largest: Vec<FileSizeEntry> = Vec::new();

    // One call: `git ls-tree -r -l HEAD` returns path and blob size for every
    // tracked file, avoiding a per-file `cat-file` loop.
    let tree = git.raw(&["ls-tree", "-r", "-l", "HEAD"]).await?;
    for line in tree.lines().filter(|line| !line.is_empty()) {
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let fields: Vec<&str> = meta.split_whitespace().collect();
        let bytes = fields
            .get(3)
            .and_then(|size| size.parse::<u64>().ok())
            .unwrap_or(0);
        let extension = match path.rsplit_once('.') {
            Some((_, extension)) if !extension.is_empty() => extension.to_lowercase(),
            _ => "(none)".to_owned(),
        };
        *by_extension.entry(extension).or_insert(0) += 1;
        largest.push(FileSizeEntry {
            path: path.to_owned(),
            bytes,
        });
    }

    largest.sort_by_key(|entry| Reverse(entry.bytes));
    largest.truncate(20);

    let mut by_extension: Vec<FileStatEntry> = by_extension
        .into_iter()
        .map(|(extension, count)| FileStatEntry { extension, count })
        .collect();
    by_extension.sort_by_key(|entry| Reverse(entry.count));

    let names = git
        .raw(&[
            "log",
            "-10",
            "--pretty=format:",
            "--name-only",
            "--no-renames",
        ])
        .await?;
    let mut seen = HashSet::new();
    let recently_modified: Vec<String> = names
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|line| seen.insert((*line).to_owned()))
        .map(str::to_owned)
        .collect();

    Ok(FileStats {
        by_extension,
        largest_files: largest,
        recently_modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headers_and_folds_numstat_lines() {
        let output = "abc\tJane\tjane@example.com\t2024-01-01T00:00:00+00:00\n\
5\t2\tsrc/a.rs\n\
-\t-\tlogo.png\n\
def\tJohn\tjohn@example.com\t2024-01-02T00:00:00+00:00\n\
1\t1\tsrc/b.rs\n";
        let entries = parse_log_entries(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].hash, "abc");
        assert_eq!(entries[0].additions, 5);
        assert_eq!(entries[0].deletions, 2);
        assert_eq!(entries[1].hash, "def");
        assert_eq!(entries[1].additions, 1);
        assert_eq!(entries[1].deletions, 1);
    }

    #[test]
    fn counts_non_empty_lines() {
        assert_eq!(count_lines("main\nfeature\n\n"), 2);
        assert_eq!(count_lines(""), 0);
    }
}
