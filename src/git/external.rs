//! Probes for external VCS tooling (`but`, `jj`, `entire`).

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

/// How long an external binary probe may take before it is treated as absent.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Outcome of probing an external binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalProbe {
    /// True when the binary ran successfully.
    pub available: bool,
    /// Version string reported by the binary.
    pub version: Option<String>,
    /// Failure detail when the binary could not be probed.
    pub error: Option<String>,
}

/// Probes whether an external binary is installed and returns its version.
///
/// Absence is a normal outcome, not an error, so this never fails.
pub async fn probe_binary(binary: &str, version_args: &[&str]) -> ExternalProbe {
    let args: Vec<&str> = if version_args.is_empty() {
        vec!["--version"]
    } else {
        version_args.to_vec()
    };

    let probe = tokio::time::timeout(PROBE_TIMEOUT, Command::new(binary).args(args).output()).await;

    match probe {
        Ok(Ok(output)) if output.status.success() => ExternalProbe {
            available: true,
            version: Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            error: None,
        },
        Ok(Ok(output)) => ExternalProbe {
            available: false,
            version: None,
            error: Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        },
        Ok(Err(error)) => ExternalProbe {
            available: false,
            version: None,
            error: Some(error.to_string()),
        },
        Err(_) => ExternalProbe {
            available: false,
            version: None,
            error: Some(format!("{binary} did not respond within 5 seconds")),
        },
    }
}

/// True when the repository carries a marker directory for an external VCS.
#[must_use]
pub fn has_marker_dir(repo_path: &Path, marker: &str) -> bool {
    repo_path.join(marker).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detects_an_installed_binary() {
        let probe = probe_binary("sh", &["-c", "echo v1"]).await;
        assert!(probe.available);
        assert_eq!(probe.version.as_deref(), Some("v1"));
    }

    #[tokio::test]
    async fn treats_a_missing_binary_as_unavailable() {
        let probe = probe_binary("definitely-not-installed-binary", &[]).await;
        assert!(!probe.available);
        assert!(probe.version.is_none());
        assert!(probe.error.is_some());
    }

    #[test]
    fn detects_marker_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".jj")).unwrap();
        assert!(has_marker_dir(dir.path(), ".jj"));
        assert!(!has_marker_dir(dir.path(), ".entire"));
    }
}
