//! Repository and path validation.
//!
//! Every repository path a tool receives is validated (exists, is a directory)
//! and every repository-relative path argument is proven to stay inside the
//! repository — including through symlinks.

use std::path::{Component, Path, PathBuf};

use crate::error::GitError;

/// Resolves a repository path to an absolute path and proves it is an existing
/// directory.
pub fn validate_repo_path(repo_path: impl AsRef<Path>) -> Result<PathBuf, GitError> {
    let requested = repo_path.as_ref();
    let resolved = crate::config::absolute_path(requested);

    match std::fs::metadata(&resolved) {
        Err(_) => Err(GitError::invalid_input(format!(
            "Repository path does not exist: {}",
            requested.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(GitError::invalid_input(format!(
            "Repository path is not a directory: {}",
            requested.display()
        ))),
        Ok(_) => Ok(resolved),
    }
}

/// Normalizes a repository-relative path argument and proves it does not escape
/// the repository root, following symlinks when the target already exists.
///
/// Returns the normalized, repository-relative path.
pub fn validate_path_argument(
    repo_path: impl AsRef<Path>,
    candidate_path: &str,
) -> Result<String, GitError> {
    let normalized = posix_normalize(&candidate_path.replace('\\', "/"));

    if Path::new(&normalized).is_absolute() || normalized == ".." || normalized.starts_with("../") {
        return Err(escapes(candidate_path));
    }

    let repo_root = validate_repo_path(repo_path)?;
    let resolved = repo_root.join(&normalized);

    if escapes_root(&repo_root, &resolved) {
        return Err(escapes(candidate_path));
    }

    // Symlink-aware containment: a link like `repo/link -> /etc/passwd` is
    // lexically inside the repository but points outside it.
    if resolved.exists() && symlink_escapes(&repo_root, &resolved) {
        return Err(GitError::invalid_input(format!(
            "Path argument escapes repository root via symlink: {candidate_path}"
        )));
    }

    Ok(normalized)
}

/// Validates a list of path arguments.
pub fn validate_path_arguments(
    repo_path: impl AsRef<Path>,
    candidate_paths: &[String],
) -> Result<Vec<String>, GitError> {
    let repo_path = repo_path.as_ref();
    candidate_paths
        .iter()
        .map(|candidate| validate_path_argument(repo_path, candidate))
        .collect()
}

/// Expresses `candidate` as a path relative to `root`, or `None` when it is not
/// inside `root`.
#[must_use]
pub fn relative_to_root(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let root: Vec<Component<'_>> = root.components().collect();
    let candidate: Vec<Component<'_>> = candidate.components().collect();

    if candidate.len() < root.len() || candidate[..root.len()] != root[..] {
        return None;
    }

    Some(candidate[root.len()..].iter().collect())
}

fn escapes(candidate_path: &str) -> GitError {
    GitError::invalid_input(format!(
        "Path argument escapes repository root: {candidate_path}"
    ))
}

fn escapes_root(repo_root: &Path, resolved: &Path) -> bool {
    relative_to_root(repo_root, resolved).is_none_or(|relative| {
        relative
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    })
}

fn symlink_escapes(repo_root: &Path, resolved: &Path) -> bool {
    let Ok(real_root) = std::fs::canonicalize(repo_root) else {
        return true;
    };
    let Ok(real_target) = std::fs::canonicalize(resolved) else {
        return true;
    };
    relative_to_root(&real_root, &real_target).is_none()
}

/// Lexical POSIX normalization matching `path.posix.normalize`.
#[must_use]
pub fn posix_normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if stack.last().is_some_and(|last| *last != "..") {
                    stack.pop();
                } else if !absolute {
                    stack.push("..");
                }
            }
            other => stack.push(other),
        }
    }

    let joined = stack.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_posix() {
        assert_eq!(posix_normalize("a/b/../c"), "a/c");
        assert_eq!(posix_normalize("a/../../b"), "../b");
        assert_eq!(posix_normalize("/a/./b//c"), "/a/b/c");
        assert_eq!(posix_normalize(""), ".");
        assert_eq!(posix_normalize("./a/"), "a");
    }

    #[test]
    fn rejects_escaping_paths() {
        let repo = tempfile::tempdir().unwrap();

        for candidate in ["../x", "..", "/etc/passwd", "a/../../x"] {
            let error = validate_path_argument(repo.path(), candidate).expect_err("should reject");
            assert_eq!(error.kind(), crate::error::GitErrorKind::InvalidInput);
            assert!(error.message().contains("escapes repository root"));
        }
    }

    #[test]
    fn rejects_symlink_escapes() {
        let repo = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), repo.path().join("link"))
            .unwrap();

        let error = validate_path_argument(repo.path(), "link").expect_err("should reject");
        assert!(error.message().contains("via symlink"));
    }

    #[test]
    fn accepts_paths_inside_the_repository() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("file.txt"), "x").unwrap();

        assert_eq!(
            validate_path_argument(repo.path(), "./file.txt").unwrap(),
            "file.txt"
        );
    }

    #[test]
    fn rejects_missing_and_non_directory_repo_paths() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let error = validate_repo_path(file.path()).expect_err("not a directory");
        assert!(error.message().contains("is not a directory"));

        let error = validate_repo_path("/definitely/not/here").expect_err("missing");
        assert!(error.message().contains("does not exist"));
        assert_eq!(error.kind(), crate::error::GitErrorKind::InvalidInput);
    }
}
