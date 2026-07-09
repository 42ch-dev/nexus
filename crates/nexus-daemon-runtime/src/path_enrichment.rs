//! Process PATH enrichment for agent CLI discovery.
//!
//! macOS GUI apps (including the Tauri desktop shell) often inherit a minimal
//! PATH such as `/usr/bin:/bin:/usr/sbin:/sbin`. Homebrew and user-local agent
//! CLIs under `/opt/homebrew/bin` or `~/.local/bin` are then invisible to
//! `which::which` during `POST /v1/daemon/agent-host/scan`.
//!
//! This module merges a login-shell-equivalent set of common user bin dirs into
//! the process PATH **once at daemon boot**, before any scan probe runs. No
//! shell-out; no wire/schema change (V1.101 Class B).

use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// Common user / package-manager bin directories that login shells typically
/// include but GUI-launched processes often omit.
///
/// Only directories that currently exist are returned so PATH stays free of
/// dead entries on machines without Homebrew / cargo / etc.
#[must_use]
pub fn login_equivalent_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = dirs::home_dir() {
        for rel in [
            ".local/bin",
            "bin",
            ".cargo/bin",
            ".npm-global/bin",
            ".bun/bin",
            // Version-manager shims (existence-gated; QC B5).
            ".asdf/shims",
            ".local/share/mise/shims",
        ] {
            let candidate = home.join(rel);
            if candidate.is_dir() {
                dirs.push(candidate);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for absolute in [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
        ] {
            let candidate = PathBuf::from(absolute);
            if candidate.is_dir() {
                dirs.push(candidate);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for absolute in [
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/snap/bin",
        ] {
            let candidate = PathBuf::from(absolute);
            if candidate.is_dir() {
                dirs.push(candidate);
            }
        }
    }

    #[cfg(windows)]
    {
        // Windows GUI PATH is usually already complete; keep a light touch.
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            let npm = PathBuf::from(local).join("npm");
            if npm.is_dir() {
                dirs.push(npm);
            }
        }
    }

    dirs
}

/// Merge `extra_dirs` (prepended, de-duplicated) with an existing PATH value.
///
/// Existing entries keep their relative order after any newly prepended dirs.
/// Duplicates (by string equality of the path component) are skipped.
///
/// # Errors
///
/// Returns [`env::JoinPathsError`] when any collected component contains the
/// OS path-separator character. Callers must not treat that as "already
/// enriched" — the merge did not succeed.
pub fn merge_path(
    existing: Option<&OsStr>,
    extra_dirs: impl IntoIterator<Item = PathBuf>,
) -> Result<OsString, env::JoinPathsError> {
    let mut ordered: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for dir in extra_dirs {
        let key = dir.to_string_lossy().into_owned();
        if seen.insert(key) {
            ordered.push(dir);
        }
    }

    if let Some(existing) = existing {
        for dir in env::split_paths(existing) {
            let key = dir.to_string_lossy().into_owned();
            if seen.insert(key) {
                ordered.push(dir);
            }
        }
    }

    env::join_paths(&ordered)
}

/// Enrich the current process `PATH` with [`login_equivalent_bin_dirs`].
///
/// Idempotent: directories already present are not duplicated.
///
/// **Call before starting a Tokio multi-threaded runtime.** On POSIX,
/// `setenv(3)` is not thread-safe against concurrent `getenv(3)`; the
/// `nexus42` binary invokes this from sync `main` before
/// `tokio::runtime::Runtime::new`. Do not call from inside an already-running
/// async runtime (including [`crate::boot::run_daemon`]).
pub fn apply_process_path_enrichment() {
    let existing = env::var_os("PATH");
    let extras = login_equivalent_bin_dirs();
    if extras.is_empty() {
        return;
    }

    let before_count = existing.as_ref().map_or(0, |p| env::split_paths(p).count());
    let enriched = match merge_path(existing.as_deref(), extras) {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "PATH enrichment: join_paths failed; leaving PATH unchanged"
            );
            return;
        }
    };
    let after_count = env::split_paths(&enriched).count();

    // Process-global PATH update before Tokio (and before any agent scan).
    // Concurrent tests that mutate PATH must serialize (see module tests).
    // Workspace forbids `unsafe`, so we use the safe `set_var` call — but only
    // from a single-threaded context (binary `main` before Runtime::new).
    env::set_var("PATH", &enriched);

    if after_count > before_count {
        tracing::info!(
            before = before_count,
            after = after_count,
            added = after_count - before_count,
            "Enriched process PATH with login-equivalent user bin dirs for agent discovery"
        );
    } else {
        tracing::debug!(
            entries = after_count,
            "Process PATH already includes login-equivalent user bin dirs"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static PATH_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn merge_path_prepends_extras_and_dedupes() {
        let existing = OsString::from("/usr/bin:/bin:/opt/homebrew/bin");
        let extras = vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/custom/bin"),
        ];
        let merged = merge_path(Some(existing.as_os_str()), extras).unwrap();
        let parts: Vec<_> = env::split_paths(&merged).collect();
        assert_eq!(parts[0], PathBuf::from("/opt/homebrew/bin"));
        assert_eq!(parts[1], PathBuf::from("/custom/bin"));
        assert_eq!(parts[2], PathBuf::from("/usr/bin"));
        assert_eq!(parts[3], PathBuf::from("/bin"));
        assert_eq!(
            parts
                .iter()
                .filter(|p| p.as_os_str() == "/opt/homebrew/bin")
                .count(),
            1,
            "homebrew bin must appear once"
        );
    }

    #[test]
    fn merge_path_handles_empty_existing() {
        let extras = vec![PathBuf::from("/opt/homebrew/bin")];
        let merged = merge_path(None, extras).unwrap();
        let parts: Vec<_> = env::split_paths(&merged).collect();
        assert_eq!(parts, vec![PathBuf::from("/opt/homebrew/bin")]);
    }

    #[cfg(unix)]
    #[test]
    fn enrichment_makes_binary_resolvable_via_which() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().to_path_buf();
        let binary = bin_dir.join("nexus-path-probe-agent");
        fs::write(&binary, b"#!/bin/sh\necho ok\n").unwrap();
        let mut perms = fs::metadata(&binary).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&binary, perms).unwrap();

        let previous = env::var_os("PATH");
        // Simulate a stripped macOS GUI PATH that cannot see the temp bin.
        env::set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        assert!(
            which::which("nexus-path-probe-agent").is_err(),
            "stripped PATH must not resolve the probe binary"
        );

        let enriched = merge_path(env::var_os("PATH").as_deref(), vec![bin_dir.clone()]).unwrap();
        env::set_var("PATH", &enriched);
        let found = which::which("nexus-path-probe-agent");
        assert!(
            found.is_ok(),
            "enriched PATH must resolve the probe binary: {found:?}"
        );
        assert_eq!(found.unwrap(), binary);

        // Restore.
        match previous {
            Some(p) => env::set_var("PATH", p),
            None => env::remove_var("PATH"),
        }
    }

    #[test]
    fn apply_process_path_enrichment_is_idempotent() {
        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("PATH");
        env::set_var("PATH", "/usr/bin:/bin");
        apply_process_path_enrichment();
        let once = env::var_os("PATH").unwrap();
        apply_process_path_enrichment();
        let twice = env::var_os("PATH").unwrap();
        assert_eq!(once, twice, "second enrichment must not duplicate entries");

        match previous {
            Some(p) => env::set_var("PATH", p),
            None => env::remove_var("PATH"),
        }
    }

    #[test]
    fn login_equivalent_bin_dirs_includes_asdf_and_mise_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let asdf = home.join(".asdf/shims");
        let mise = home.join(".local/share/mise/shims");
        std::fs::create_dir_all(&asdf).unwrap();
        std::fs::create_dir_all(&mise).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        env::set_var("HOME", home);

        let dirs = login_equivalent_bin_dirs();
        assert!(
            dirs.iter().any(|d| d.ends_with(".asdf/shims")),
            "expected asdf shims in {dirs:?}"
        );
        assert!(
            dirs.iter().any(|d| d.ends_with(".local/share/mise/shims")),
            "expected mise shims in {dirs:?}"
        );

        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
    }
}
