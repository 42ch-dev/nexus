//! PATH-based discovery for Wave 1 native CLI providers.
//!
//! Scans the `PATH` environment variable for known commands. Wave 1 supports
//! only `claude`. Native CLI entries use distinct `provider_ids` (e.g., `claude-native`)
//! to avoid collision with ACP registry's `claude` (R-004, R-008).

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
const KNOWN_COMMANDS: &[(&str, &str)] = &[("claude", "claude-native")];

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

        // Search PATH for the command
        if let Some(found_path) = find_command(&path_dirs, cmd) {
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

/// Search PATH directories for a command.
///
/// On Windows, also tries appending each extension from the `PATHEXT`
/// environment variable (e.g. `.exe`, `.cmd`) if the bare name is not found.
fn find_command(path_dirs: &[PathBuf], command: &str) -> Option<PathBuf> {
    for dir in path_dirs {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }

        // Windows: try appending PATHEXT extensions when the bare name misses.
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
/// This bypasses the real PATH environment variable, allowing deterministic tests.
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

        if let Some(found_path) = find_command(custom_path_dirs, cmd) {
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

    #[test]
    fn known_commands_mapping() {
        // Verify Wave 1 mapping: claude -> claude-native
        assert_eq!(KNOWN_COMMANDS.len(), 1);
        assert_eq!(KNOWN_COMMANDS[0].0, "claude");
        assert_eq!(KNOWN_COMMANDS[0].1, "claude-native");
    }
}
