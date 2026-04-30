//! System preset directory scanner.
//!
//! Discovers and loads system presets from `~/.nexus42/presets/_system/<name>/`.
//! Each subdirectory is expected to contain a `preset.yaml` in the same format
//! as user/embedded presets. Discovered presets are registered with a `_system.`
//! prefix in their identifier (e.g., `_system.maintenance`).
//!
//! Design: v1.6 WS-D — replace hardcoded `_system.maintenance` with
//! configurable directory scanning.

use crate::capability::CapabilityRegistry;
use crate::preset::loader::{load_preset_from_str, LoadedPreset, PresetLoadError};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// System preset directory name under `~/.nexus42/presets/`.
const SYSTEM_PRESET_DIR_NAME: &str = "_system";

/// Prefix applied to all system preset identifiers.
const SYSTEM_PREFIX: &str = "_system.";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A discovered system preset with its directory origin.
#[derive(Debug, Clone)]
pub struct SystemPresetEntry {
    /// The preset's qualified ID (e.g. `_system.maintenance`).
    pub qualified_id: String,
    /// The bundle directory on disk (e.g. `~/.nexus42/presets/_system/maintenance/`).
    pub bundle_dir: PathBuf,
    /// The fully loaded preset.
    pub loaded: LoadedPreset,
}

/// Result of scanning the system preset directory.
#[derive(Debug, Default)]
pub struct SystemPresetScanResult {
    /// Successfully loaded system presets.
    pub presets: Vec<SystemPresetEntry>,
    /// Warnings for presets that were skipped (corrupted, invalid YAML, etc.).
    pub warnings: Vec<SystemPresetWarning>,
}

/// A warning produced during system preset scanning.
#[derive(Debug, Clone)]
pub struct SystemPresetWarning {
    /// Directory name that produced the warning.
    pub dir_name: String,
    /// Human-readable warning message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Scan the system preset directory and load all discovered presets.
///
/// Location: `<nexus_home>/presets/_system/<name>/`
///
/// For each subdirectory:
/// - If it contains `preset.yaml`, attempt to load and validate it.
/// - If loading fails, log a warning and skip (T4: graceful degradation).
/// - If the directory doesn't exist at all, return empty results (no error).
///
/// The `nexus_home` parameter is typically `$HOME/.nexus42`.
pub fn scan_system_presets(nexus_home: &Path, caps: &CapabilityRegistry) -> SystemPresetScanResult {
    let system_dir = system_preset_base_dir(nexus_home);

    // T4: missing directory = no system presets (not an error).
    if !system_dir.exists() {
        tracing::debug!(
            ?system_dir,
            "system preset directory does not exist — skipping"
        );
        return SystemPresetScanResult::default();
    }

    let entries = match std::fs::read_dir(&system_dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(
                ?system_dir,
                error = %e,
                "failed to read system preset directory — skipping"
            );
            return SystemPresetScanResult::default();
        }
    };

    let mut result = SystemPresetScanResult::default();

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process directories.
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Skip hidden directories (starting with `.`).
        if dir_name.starts_with('.') {
            continue;
        }

        match load_system_preset_from_dir(&path, &dir_name, caps) {
            Ok(entry) => {
                tracing::info!(
                    qualified_id = %entry.qualified_id,
                    "registered system preset"
                );
                result.presets.push(entry);
            }
            Err(warning) => {
                tracing::warn!(
                    dir_name = %warning.dir_name,
                    message = %warning.message,
                    "skipping corrupted system preset"
                );
                result.warnings.push(warning);
            }
        }
    }

    result
}

/// Load a single system preset from a bundle directory.
///
/// Reads `preset.yaml`, validates it, and returns a [`SystemPresetEntry`]
/// with the `_system.` prefix applied.
///
/// # Errors
/// Returns [`SystemPresetWarning`] if the preset directory is missing, YAML parsing fails, or validation fails.
pub fn load_system_preset_from_dir(
    bundle_dir: &Path,
    dir_name: &str,
    caps: &CapabilityRegistry,
) -> Result<SystemPresetEntry, SystemPresetWarning> {
    let preset_yaml_path = bundle_dir.join("preset.yaml");

    let yaml = match std::fs::read_to_string(&preset_yaml_path) {
        Ok(content) => content,
        Err(e) => {
            return Err(SystemPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("failed to read preset.yaml: {e}"),
            });
        }
    };

    let loaded = match load_preset_from_str(&yaml, caps) {
        Ok(preset) => preset,
        Err(PresetLoadError::YamlParse(e)) => {
            return Err(SystemPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("YAML parse error: {e}"),
            });
        }
        Err(PresetLoadError::Validation { problems, .. }) => {
            return Err(SystemPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!(
                    "validation failed: {}",
                    problems
                        .iter()
                        .map(|p| format!("{} ({})", p.error, p.path))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            });
        }
        Err(PresetLoadError::InvalidPresetHookOp(e)) => {
            return Err(SystemPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("invalid hook operation: {e}"),
            });
        }
        Err(PresetLoadError::NotFound { preset_id }) => {
            return Err(SystemPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("preset not found: {preset_id}"),
            });
        }
        Err(
            e @ (PresetLoadError::YamlSizeExceeded { .. }
            | PresetLoadError::YamlDepthExceeded { .. }),
        ) => {
            return Err(SystemPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("{e}"),
            });
        }
    };

    // The qualified ID uses `_system.<dir_name>` convention.
    let qualified_id = format!("{SYSTEM_PREFIX}{dir_name}");

    Ok(SystemPresetEntry {
        qualified_id,
        bundle_dir: bundle_dir.to_path_buf(),
        loaded,
    })
}

/// Get the system preset base directory path: `<nexus_home>/presets/_system/`.
#[must_use]
pub fn system_preset_base_dir(nexus_home: &Path) -> PathBuf {
    nexus_home.join("presets").join(SYSTEM_PRESET_DIR_NAME)
}

/// Get the path to a specific system preset's bundle directory.
#[must_use]
pub fn system_preset_bundle_dir(nexus_home: &Path, name: &str) -> PathBuf {
    system_preset_base_dir(nexus_home).join(name)
}

/// Return the qualified system preset IDs from a scan result.
#[must_use]
pub fn list_system_preset_ids(result: &SystemPresetScanResult) -> Vec<String> {
    result
        .presets
        .iter()
        .map(|e| e.qualified_id.clone())
        .collect()
}

/// Find a system preset entry by qualified ID.
#[must_use]
pub fn find_system_preset<'a>(
    result: &'a SystemPresetScanResult,
    qualified_id: &str,
) -> Option<&'a SystemPresetEntry> {
    result
        .presets
        .iter()
        .find(|e| e.qualified_id == qualified_id)
}

// ---------------------------------------------------------------------------
// Embedded fallback (T3)
// ---------------------------------------------------------------------------

/// The embedded `_system.maintenance` preset YAML content.
///
/// This is used as the first-start fallback: if the `_system/maintenance/`
/// directory doesn't exist, auto-create it from this embedded definition.
///
/// The preset uses `kind: capability` enter actions for direct capability
/// invocation (sync.pull → outbox.flush → registry.refresh → end), matching
/// the behavior of the previous hardcoded Rust graph.
pub const EMBEDDED_MAINTENANCE_YAML: &str = r#"
preset:
  id: maintenance
  version: 1
  kind: system
  description: "System maintenance: sync pull, outbox flush, registry refresh"
  requires_capabilities:
    - sync.pull
    - outbox.flush
    - registry.refresh
  initial: sync_pull
  terminal: end
states:
  - id: sync_pull
    description: "Pull remote sync bundles"
    enter:
      - kind: capability
        name: sync.pull
    next: outbox_flush
  - id: outbox_flush
    description: "Flush pending outbox entries"
    enter:
      - kind: capability
        name: outbox.flush
    next: registry_refresh
  - id: registry_refresh
    description: "Refresh capability registry"
    enter:
      - kind: capability
        name: registry.refresh
    next: end
  - id: end
    terminal: true
"#;

/// Auto-create the `_system/maintenance/` directory from embedded content
/// if it doesn't exist (first-start fallback for backward compatibility).
///
/// Returns `true` if the directory was created (first start),
/// `false` if it already existed.
///
/// # Errors
/// Returns an I/O error if filesystem operations fail.
pub fn ensure_maintenance_preset(nexus_home: &Path) -> std::io::Result<bool> {
    let bundle_dir = system_preset_bundle_dir(nexus_home, "maintenance");
    let preset_yaml = bundle_dir.join("preset.yaml");

    if preset_yaml.exists() {
        return Ok(false);
    }

    // Create the directory structure.
    std::fs::create_dir_all(&bundle_dir)?;
    std::fs::write(&preset_yaml, EMBEDDED_MAINTENANCE_YAML)?;

    tracing::info!(
        ?bundle_dir,
        "auto-created _system.maintenance preset from embedded definition"
    );

    Ok(true)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn system_preset_base_dir_layout() {
        // nexus_home is typically ~/.nexus42 (already includes .nexus42)
        let nexus_home = PathBuf::from("/fake/home/.nexus42");
        let dir = system_preset_base_dir(&nexus_home);
        assert_eq!(dir, PathBuf::from("/fake/home/.nexus42/presets/_system"));
    }

    #[test]
    fn system_preset_bundle_dir_layout() {
        let nexus_home = PathBuf::from("/fake/home/.nexus42");
        let dir = system_preset_bundle_dir(&nexus_home, "maintenance");
        assert_eq!(
            dir,
            PathBuf::from("/fake/home/.nexus42/presets/_system/maintenance")
        );
    }

    #[test]
    fn missing_directory_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scan_loads_valid_system_presets() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        // Create _system/maintenance/ with valid preset.yaml
        let maintenance_dir = base.join("maintenance");
        fs::create_dir_all(&maintenance_dir).unwrap();
        fs::write(
            maintenance_dir.join("preset.yaml"),
            EMBEDDED_MAINTENANCE_YAML,
        )
        .unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);

        assert_eq!(result.presets.len(), 1);
        assert_eq!(result.presets[0].qualified_id, "_system.maintenance");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scan_loads_multiple_system_presets() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        // Create _system/maintenance/
        let maintenance_dir = base.join("maintenance");
        fs::create_dir_all(&maintenance_dir).unwrap();
        fs::write(
            maintenance_dir.join("preset.yaml"),
            EMBEDDED_MAINTENANCE_YAML,
        )
        .unwrap();

        // Create _system/health-check/ with a minimal valid preset
        let health_dir = base.join("health-check");
        fs::create_dir_all(&health_dir).unwrap();
        let health_yaml = r#"
preset:
  id: health-check
  version: 1
  kind: system
  description: "Health check system preset"
  requires_capabilities: []
  initial: start
  terminal: done
states:
  - id: start
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;
        fs::write(health_dir.join("preset.yaml"), health_yaml).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);

        assert_eq!(result.presets.len(), 2);
        let ids: Vec<&str> = result
            .presets
            .iter()
            .map(|p| p.qualified_id.as_str())
            .collect();
        assert!(ids.contains(&"_system.maintenance"));
        assert!(ids.contains(&"_system.health-check"));
    }

    #[test]
    fn scan_skips_corrupted_presets_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        // Create _system/broken/ with invalid YAML
        let broken_dir = base.join("broken");
        fs::create_dir_all(&broken_dir).unwrap();
        fs::write(broken_dir.join("preset.yaml"), "not valid yaml: [").unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].dir_name, "broken");
        assert!(result.warnings[0].message.contains("YAML parse error"));
    }

    #[test]
    fn scan_skips_directory_without_preset_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        // Create _system/empty/ with no preset.yaml
        let empty_dir = base.join("empty");
        fs::create_dir_all(&empty_dir).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].dir_name, "empty");
    }

    #[test]
    fn scan_skips_hidden_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        // Create _system/.hidden/ (should be skipped)
        let hidden_dir = base.join(".hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("preset.yaml"), "bogus").unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert!(result.warnings.is_empty()); // Hidden dirs are silently skipped
    }

    #[test]
    fn list_system_preset_ids_returns_qualified_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        let maintenance_dir = base.join("maintenance");
        fs::create_dir_all(&maintenance_dir).unwrap();
        fs::write(
            maintenance_dir.join("preset.yaml"),
            EMBEDDED_MAINTENANCE_YAML,
        )
        .unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);
        let ids = list_system_preset_ids(&result);

        assert_eq!(ids, vec!["_system.maintenance".to_string()]);
    }

    #[test]
    fn find_system_preset_by_qualified_id() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = system_preset_base_dir(&nexus_home);

        let maintenance_dir = base.join("maintenance");
        fs::create_dir_all(&maintenance_dir).unwrap();
        fs::write(
            maintenance_dir.join("preset.yaml"),
            EMBEDDED_MAINTENANCE_YAML,
        )
        .unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_system_presets(&nexus_home, &caps);

        assert!(find_system_preset(&result, "_system.maintenance").is_some());
        assert!(find_system_preset(&result, "_system.nonexistent").is_none());
    }

    #[test]
    fn ensure_maintenance_preset_creates_on_first_start() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();

        // First call: directory doesn't exist, should create.
        let created = ensure_maintenance_preset(&nexus_home).unwrap();
        assert!(created);

        let preset_path = system_preset_bundle_dir(&nexus_home, "maintenance").join("preset.yaml");
        assert!(preset_path.exists());
        let content = fs::read_to_string(&preset_path).unwrap();
        assert!(content.contains("id: maintenance"));
        assert!(content.contains("kind: system"));

        // Second call: directory already exists, should not overwrite.
        let created_again = ensure_maintenance_preset(&nexus_home).unwrap();
        assert!(!created_again);
    }

    #[test]
    fn ensure_maintenance_preset_does_not_overwrite_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let bundle_dir = system_preset_bundle_dir(&nexus_home, "maintenance");
        fs::create_dir_all(&bundle_dir).unwrap();

        let custom_yaml = "preset:\n  id: maintenance\n  version: 2\n  kind: system\n  description: custom\n  requires_capabilities: []\n  initial: a\n  terminal: a\nstates:\n  - id: a\n    terminal: true\n";
        fs::write(bundle_dir.join("preset.yaml"), custom_yaml).unwrap();

        let created = ensure_maintenance_preset(&nexus_home).unwrap();
        assert!(!created);

        // Content should remain the custom version.
        let content = fs::read_to_string(bundle_dir.join("preset.yaml")).unwrap();
        assert!(content.contains("version: 2"));
        assert!(content.contains("custom"));
    }

    #[test]
    fn embedded_maintenance_yaml_is_valid() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_preset_from_str(EMBEDDED_MAINTENANCE_YAML, &caps).unwrap();
        assert_eq!(loaded.id, "maintenance");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.manifest.states.len(), 4);
        assert!(loaded
            .manifest
            .preset
            .requires_capabilities
            .contains(&"sync.pull".to_string()));
        assert!(loaded
            .manifest
            .preset
            .requires_capabilities
            .contains(&"outbox.flush".to_string()));
        assert!(loaded
            .manifest
            .preset
            .requires_capabilities
            .contains(&"registry.refresh".to_string()));
    }
}
