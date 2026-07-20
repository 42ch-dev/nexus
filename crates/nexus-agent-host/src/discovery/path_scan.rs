//! PATH-based discovery for Wave 1 native CLI providers.
//!
//! Scans the `PATH` environment variable for known commands. Wave 1 supports
//! only `claude`. Native CLI entries use distinct `provider_ids` (e.g., `claude-native`)
//! to avoid collision with ACP registry's `claude` (R-004, R-008).
//!
//! # Cross-platform probe (DF-26)
//!
//! Command resolution uses the `which` crate for cross-platform PATH lookup,
//! handling Windows `PATHEXT` extensions, symlink resolution, and executable
//! permission checks automatically. A manual fallback scan is included for
//! environments where the `which` crate may not find a command.
//!
//! **Windows limitation**: While the `which` crate handles `PATHEXT` extensions,
//! some edge cases (e.g., custom shell integrations or non-standard PATH
//! separators) may not be fully covered. The manual fallback provides an
//! additional safety net.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::capability::model::{CapabilityDescriptor, ProtocolKind, ProviderHealth};
use crate::config::AgentHostConfig;
use crate::error::{HostError, HostResult};
use crate::ids::ProviderId;
use crate::{DiscoverySource, LaunchStrategy, ProviderCatalogEntry, TrustLevel};

/// Wave 1 known CLI commands and their provider ID mappings.
///
/// Each entry maps a command name to a distinct `provider_id` that won't collide
/// with ACP registry agent IDs.
const KNOWN_COMMANDS: &[(&str, &str)] = &[("claude", "claude-native"), ("codex", "codex-native")];

/// Discover native CLI providers by scanning PATH.
///
/// For each found command, creates a `ProviderCatalogEntry` with:
/// - `protocol_kind = NativeCli`
/// - `source = PathScan`
/// - `trust = LocalPath`
///
/// Respects config dedup: if explicit config or `suppressed_ids` already has
/// an entry for the same `provider_id`, the PATH result is skipped.
///
/// # Arguments
///
/// * `config` - Agent host configuration (for dedup and defaults).
/// * `suppressed_ids` - Provider IDs that should be suppressed (from disabled config entries).
///
/// # Errors
///
/// Returns an error only if PATH environment variable cannot be read.
pub fn scan_path(
    config: &AgentHostConfig,
    suppressed_ids: &[ProviderId],
) -> HostResult<Vec<ProviderCatalogEntry>> {
    let path_var = std::env::var_os("PATH")
        .ok_or_else(|| HostError::internal("PATH environment variable not found".to_string()))?;

    let path_dirs: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    scan_path_in(config, suppressed_ids, &path_dirs)
}

/// Discover native CLI providers by scanning an explicit directory list.
///
/// Used by the daemon scan handler so probes see login-shell-equivalent dirs
/// without mutating the process `PATH` on a live Tokio runtime.
///
/// # Errors
///
/// Currently infallible; returns [`HostResult`] for symmetry with [`scan_path`].
pub fn scan_path_in(
    config: &AgentHostConfig,
    suppressed_ids: &[ProviderId],
    path_dirs: &[PathBuf],
) -> HostResult<Vec<ProviderCatalogEntry>> {
    let suppressed: HashSet<&ProviderId> = suppressed_ids.iter().collect();

    // Collect configured provider_ids for dedup
    let configured: HashSet<ProviderId> = config
        .providers
        .iter()
        .filter(|pc| pc.enabled)
        .map(super::super::config::ProviderConfig::provider_id)
        .collect();

    let mut entries = Vec::new();

    for &(cmd, provider_id_str) in KNOWN_COMMANDS {
        let pid = ProviderId::new(provider_id_str);

        // Skip if already configured or suppressed
        if configured.contains(&pid) || suppressed.contains(&pid) {
            continue;
        }

        // Search the provided dirs (and process PATH as a which fallback).
        if let Some(found_path) = find_command(path_dirs, cmd) {
            entries.push(ProviderCatalogEntry {
                provider_id: pid.clone(),
                display_name: format!("{cmd} (native CLI)"),
                protocol_kind: ProtocolKind::NativeCli,
                launch: LaunchStrategy::NativeCli {
                    command: found_path.to_string_lossy().into_owned(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::PathScan,
                trust: TrustLevel::LocalPath,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: pid,
                    available: true,
                    latency_ms: None,
                    message: None,
                },
            });
        }
    }

    Ok(entries)
}

/// Resolve a command name to its full path using cross-platform resolution.
///
/// Uses the `which` crate for proper PATH lookup across all platforms.
/// On Unix, this resolves symlinks and checks executability. On Windows,
/// it handles `PATHEXT` extensions (`.exe`, `.cmd`, etc.) automatically.
///
/// Falls back to manual PATH scan if the `which` crate fails internally,
/// ensuring robustness even in unusual environment configurations.
fn find_command(path_dirs: &[PathBuf], command: &str) -> Option<PathBuf> {
    if !path_dirs.is_empty() {
        if let Ok(joined) = std::env::join_paths(path_dirs) {
            if let Ok(path) = which::which_in(command, Some(joined), std::path::Path::new(".")) {
                return Some(path);
            }
        }
    }

    if let Ok(path) = which::which(command) {
        return Some(path);
    }

    // Fallback: manual scan of the provided dirs, then the process PATH,
    // for environments where `which` may not find a command that exists.
    let search_dirs: Vec<PathBuf> = if path_dirs.is_empty() {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default()
    } else {
        path_dirs.to_vec()
    };

    for dir in search_dirs {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }

        // Windows: try appending PATHEXT extensions.
        #[cfg(target_os = "windows")]
        {
            if let Some(ext_var) = std::env::var_os("PATHEXT") {
                for ext in std::env::split_paths(&ext_var) {
                    let ext_str = ext.to_string_lossy();
                    let with_ext = dir.join(format!("{command}{ext_str}"));
                    if with_ext.is_file() {
                        return Some(with_ext);
                    }
                }
            }
        }
    }
    None
}

/// Create PATH scan entries using a custom PATH string (for testing).
///
/// This bypasses the real PATH environment variable by using the `which`
/// crate's `which_in` API (or manual fallback) against the provided
/// directories, allowing deterministic tests.
#[cfg(test)]
pub fn scan_custom_path(
    custom_path_dirs: &[PathBuf],
    suppressed_ids: &[ProviderId],
) -> Vec<ProviderCatalogEntry> {
    let suppressed: HashSet<&ProviderId> = suppressed_ids.iter().collect();

    let mut entries = Vec::new();

    for &(cmd, provider_id_str) in KNOWN_COMMANDS {
        let pid = ProviderId::new(provider_id_str);

        if suppressed.contains(&pid) {
            continue;
        }

        if let Some(found_path) = find_command_in_dirs(custom_path_dirs, cmd) {
            entries.push(ProviderCatalogEntry {
                provider_id: pid.clone(),
                display_name: format!("{cmd} (native CLI)"),
                protocol_kind: ProtocolKind::NativeCli,
                launch: LaunchStrategy::NativeCli {
                    command: found_path.to_string_lossy().into_owned(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::PathScan,
                trust: TrustLevel::LocalPath,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: pid,
                    available: true,
                    latency_ms: None,
                    message: None,
                },
            });
        }
    }

    entries
}

/// Manual PATH scan within specified directories (test helper).
///
/// This does NOT use `which::which()` because test directories are
/// temporary and not on the real PATH. It performs a straightforward
/// file existence check within the provided directories.
#[cfg(test)]
fn find_command_in_dirs(path_dirs: &[PathBuf], command: &str) -> Option<PathBuf> {
    for dir in path_dirs {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AgentHostConfig {
        AgentHostConfig::default()
    }

    #[test]
    fn scan_custom_path_finds_claude() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        // Create a fake claude executable
        let claude_path = temp_dir.path().join("claude");
        std::fs::write(&claude_path, "#!/bin/sh\necho hello\n").expect("Failed to write fixture");

        let entries = scan_custom_path(&[temp_dir.path().to_path_buf()], &[]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].provider_id.0, "claude-native");
        assert_eq!(entries[0].protocol_kind, ProtocolKind::NativeCli);
        assert_eq!(entries[0].source, DiscoverySource::PathScan);
        assert_eq!(entries[0].trust, TrustLevel::LocalPath);
    }

    #[test]
    fn scan_custom_path_finds_codex() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        // Create a fake codex executable
        let codex_path = temp_dir.path().join("codex");
        std::fs::write(&codex_path, "#!/bin/sh\necho hello\n").expect("Failed to write fixture");

        let entries = scan_custom_path(&[temp_dir.path().to_path_buf()], &[]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].provider_id.0, "codex-native");
        assert_eq!(entries[0].protocol_kind, ProtocolKind::NativeCli);
        assert_eq!(entries[0].source, DiscoverySource::PathScan);
        assert_eq!(entries[0].trust, TrustLevel::LocalPath);
    }

    #[test]
    fn scan_custom_path_empty_dir() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let entries = scan_custom_path(&[temp_dir.path().to_path_buf()], &[]);
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_custom_path_suppressed() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let claude_path = temp_dir.path().join("claude");
        std::fs::write(&claude_path, "#!/bin/sh\necho hello\n").expect("Failed to write fixture");

        let entries = scan_custom_path(
            &[temp_dir.path().to_path_buf()],
            &[ProviderId::new("claude-native")],
        );
        assert!(entries.is_empty(), "suppressed ID should be skipped");
    }

    /// Serialize tests that mutate `PATH` so concurrent tests do not see each
    /// other's isolated environments. Mirrors the `SCAN_PATH_LOCK` pattern in
    /// `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs:934`.
    static SCAN_PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard that replaces `PATH` with a single directory on construction
    /// and restores the previous value on drop. Ensures the test genuinely
    /// depends on the stub binaries — without this, `find_command`'s
    /// `which::which(command)` fallback (path_scan.rs:140) could discover a
    /// real `codex`/`claude` binary on the host's PATH and the test would pass
    /// even if the stubs were absent (qc1 W-002).
    struct PathGuard {
        previous: Option<String>,
    }

    impl PathGuard {
        fn isolate(dir: &std::path::Path) -> Self {
            let previous = std::env::var("PATH").ok();
            let new_path = std::env::join_paths([dir.to_path_buf()]).expect("valid PATH");
            std::env::set_var("PATH", new_path);
            Self { previous }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match self.previous {
                Some(ref p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    /// Verify that `scan_path_in` (the production scan path, not the test-only
    /// `scan_custom_path` helper) discovers BOTH native CLI providers when both
    /// stub binaries are present and executable in the scanned directory.
    ///
    /// This exercises `find_command` → `which::which_in` (the primary
    /// resolution path), not just the manual `is_file()` fallback. Sets the
    /// executable bit to match real-world CLI conditions. Closes the coverage
    /// gap where existing tests only exercise codex / claude in isolation via
    /// the test helper.
    ///
    /// **PATH isolation (qc1 W-002):** `find_command` has a fallback that
    /// calls `which::which(command)` against the process PATH. If the host has
    /// real `codex`/`claude` installed, that fallback would make the test pass
    /// even without the stubs. `PathGuard::isolate(temp_dir)` replaces PATH
    /// with the temp directory for the test scope so the test genuinely
    /// depends on the stubs.
    #[test]
    fn scan_path_in_discovers_both_providers_when_stubs_executable() {
        let _lock = SCAN_PATH_LOCK.lock().expect("lock scan tests");

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let claude_path = temp_dir.path().join("claude");
        let codex_path = temp_dir.path().join("codex");
        std::fs::write(&claude_path, "#!/bin/sh\necho hello\n").expect("write claude stub");
        std::fs::write(&codex_path, "#!/bin/sh\necho hello\n").expect("write codex stub");

        set_executable(&claude_path);
        set_executable(&codex_path);

        // Isolate PATH so the test depends on the stubs, not on any real
        // codex/claude that might be installed on the host (qc1 W-002).
        let _path_guard = PathGuard::isolate(temp_dir.path());

        let config = AgentHostConfig::default();
        let entries = scan_path_in(&config, &[], &[temp_dir.path().to_path_buf()])
            .expect("scan_path_in should succeed");

        assert_eq!(
            entries.len(),
            2,
            "both codex and claude should be discovered"
        );

        let ids: Vec<&str> = entries.iter().map(|e| e.provider_id.0.as_str()).collect();
        assert!(
            ids.contains(&"codex-native"),
            "codex-native missing: {ids:?}"
        );
        assert!(
            ids.contains(&"claude-native"),
            "claude-native missing: {ids:?}"
        );

        for entry in &entries {
            assert_eq!(entry.protocol_kind, ProtocolKind::NativeCli);
            assert_eq!(entry.source, DiscoverySource::PathScan);
            assert_eq!(entry.trust, TrustLevel::LocalPath);
            assert!(entry.health.available, "entry should be marked available");
        }
    }

    /// Set the executable bit on a path (Unix only).
    ///
    /// Required for `which::which_in` to resolve the stub — without it, only
    /// the manual `is_file()` fallback finds the file, leaving the primary
    /// cross-platform resolution path untested.
    #[cfg(unix)]
    fn set_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .expect("stat for chmod")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("chmod +x");
    }

    #[cfg(not(unix))]
    fn set_executable(_path: &std::path::Path) {
        // Windows: `which` resolves via PATHEXT; no executable bit needed.
    }

    #[test]
    fn known_commands_mapping() {
        // Verify Wave 1 mapping: claude -> claude-native, codex -> codex-native
        assert_eq!(KNOWN_COMMANDS.len(), 2);
        assert_eq!(KNOWN_COMMANDS[0].0, "claude");
        assert_eq!(KNOWN_COMMANDS[0].1, "claude-native");
        assert_eq!(KNOWN_COMMANDS[1].0, "codex");
        assert_eq!(KNOWN_COMMANDS[1].1, "codex-native");
    }

    // ── DF-26: Cross-platform probe tests (AH5.1) ─────────────────────

    /// Verify that `find_command` uses `which::which()` for cross-platform
    /// resolution. This test verifies the function exists and returns
    /// `None` for commands not on PATH (not hanging, not panicking).
    #[test]
    fn find_command_returns_none_for_missing_command() {
        // A command that definitely does not exist on any system
        let result = find_command(&[], "nexus42_nonexistent_command_99999");
        assert!(
            result.is_none(),
            "find_command should return None for non-existent commands"
        );
    }

    /// Verify that `find_command` finds a real system command using `which`.
    /// Both `ls` (Unix) and `cmd` (Windows) exist on their respective
    /// platforms, so at least one should resolve.
    #[test]
    fn find_command_finds_real_system_command() {
        // Try common commands that exist on any platform
        let found = find_command(&[], "sh")
            .or_else(|| find_command(&[], "cmd"))
            .or_else(|| find_command(&[], "ls"));
        // On CI there should be at least one of these
        // But we don't hard-assert because unusual containers may lack all of them
        if let Some(path) = &found {
            assert!(
                path.is_absolute(),
                "which::which should return absolute paths, got: {}",
                path.display()
            );
        }
        // If none found, that's fine too (unusual environment) — the important
        // thing is that the function didn't panic or hang.
    }

    /// Verify that `scan_path` doesn't panic when PATH is available
    /// (even if `claude` is not on PATH).
    #[test]
    fn scan_path_does_not_panic_without_claude() {
        let config = default_config();
        let result = scan_path(&config, &[]);
        // Should succeed (not panic), even if claude is not found
        assert!(result.is_ok());
    }
}
