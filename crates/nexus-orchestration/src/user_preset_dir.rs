//! User preset directory scanner.
//!
//! Discovers and loads user-installed presets from `~/.nexus42/presets/<name>/`.
//! Each subdirectory is expected to contain a `preset.yaml` in the same format
//! as embedded/system presets. Any directory whose name starts with `_` (system)
//! or `.` (hidden) is silently skipped.
//!
//! Design: V1.9 WS-A — third-party preset loading.

use crate::capability::CapabilityRegistry;
use crate::preset::loader::{load_preset_from_str, LoadedPreset, PresetLoadError};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A discovered user preset with its directory origin.
#[derive(Debug, Clone)]
pub struct UserPresetEntry {
    /// The preset's ID (same as the directory name).
    pub id: String,
    /// The bundle directory on disk (e.g. `~/.nexus42/presets/my-strategy/`).
    pub bundle_dir: PathBuf,
    /// The fully loaded preset.
    pub loaded: LoadedPreset,
}

/// Result of scanning the user preset directory.
#[derive(Debug, Default)]
pub struct UserPresetScanResult {
    /// Successfully loaded user presets.
    pub presets: Vec<UserPresetEntry>,
    /// Warnings for presets that were skipped (corrupted, invalid YAML, etc.).
    pub warnings: Vec<UserPresetWarning>,
}

/// A warning produced during user preset scanning.
#[derive(Debug, Clone)]
pub struct UserPresetWarning {
    /// Directory name that produced the warning.
    pub dir_name: String,
    /// Human-readable warning message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Scan the user preset directory and load all discovered presets.
///
/// Location: `<nexus_home>/presets/<name>/`
///
/// For each subdirectory:
/// - If it contains `preset.yaml`, attempt to load and validate it.
/// - If loading fails, log a warning and skip.
/// - If the directory doesn't exist at all, return empty results (no error).
///
/// Directories starting with `_` (system prefix) or `.` (hidden) are silently
/// skipped to avoid overlap with system presets and hidden files.
///
/// The `nexus_home` parameter is typically `$HOME/.nexus42`.
pub fn scan_user_presets(nexus_home: &Path, caps: &CapabilityRegistry) -> UserPresetScanResult {
    let user_dir = nexus_home.join("presets");

    // Missing directory = no user presets (not an error).
    if !user_dir.exists() {
        tracing::debug!(?user_dir, "user preset directory does not exist — skipping");
        return UserPresetScanResult::default();
    }

    let mut result = UserPresetScanResult::default();

    // Read directory entries and filter (skip _, . prefixed, non-dirs, missing preset.yaml).
    let entries = match std::fs::read_dir(&user_dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(
                ?user_dir,
                error = %e,
                "failed to read user preset directory — skipping"
            );
            return UserPresetScanResult::default();
        }
    };

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

        // Skip system-prefixed directories (starting with `_`).
        if dir_name.starts_with('_') {
            continue;
        }

        // Skip hidden directories (starting with `.`).
        if dir_name.starts_with('.') {
            continue;
        }

        // Skip directories without preset.yaml.
        if !path.join("preset.yaml").exists() {
            result.warnings.push(UserPresetWarning {
                dir_name: dir_name.clone(),
                message: "missing preset.yaml".to_string(),
            });
            continue;
        }

        match load_user_preset_from_dir(&path, &dir_name, caps) {
            Ok(entry) => {
                tracing::info!(
                    id = %entry.id,
                    "registered user preset"
                );
                result.presets.push(entry);
            }
            Err(warning) => {
                tracing::warn!(
                    dir_name = %warning.dir_name,
                    message = %warning.message,
                    "skipping corrupted user preset"
                );
                result.warnings.push(warning);
            }
        }
    }

    result
}

/// Load a single user preset from a bundle directory.
///
/// Reads `preset.yaml`, validates it, and returns a [`UserPresetEntry`].
///
/// # Errors
/// Returns [`UserPresetWarning`] if the preset directory is missing, YAML parsing fails, or validation fails.
pub fn load_user_preset_from_dir(
    bundle_dir: &Path,
    dir_name: &str,
    caps: &CapabilityRegistry,
) -> Result<UserPresetEntry, UserPresetWarning> {
    let preset_yaml_path = bundle_dir.join("preset.yaml");

    let yaml = match std::fs::read_to_string(&preset_yaml_path) {
        Ok(content) => content,
        Err(e) => {
            return Err(UserPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("failed to read preset.yaml: {e}"),
            });
        }
    };

    let loaded = match load_preset_from_str(&yaml, caps) {
        Ok(preset) => preset,
        Err(PresetLoadError::YamlParse(e)) => {
            return Err(UserPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("YAML parse error: {e}"),
            });
        }
        Err(PresetLoadError::Validation { problems, .. }) => {
            return Err(UserPresetWarning {
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
            return Err(UserPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("invalid hook operation: {e}"),
            });
        }
        Err(PresetLoadError::NotFound { preset_id }) => {
            return Err(UserPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("preset not found: {preset_id}"),
            });
        }
        Err(
            e @ (PresetLoadError::YamlSizeExceeded { .. }
            | PresetLoadError::YamlDepthExceeded { .. }),
        ) => {
            return Err(UserPresetWarning {
                dir_name: dir_name.to_string(),
                message: format!("{e}"),
            });
        }
    };

    Ok(UserPresetEntry {
        id: dir_name.to_string(),
        bundle_dir: bundle_dir.to_path_buf(),
        loaded,
    })
}

/// Return the user preset IDs from a scan result.
#[must_use]
pub fn list_user_preset_ids(result: &UserPresetScanResult) -> Vec<String> {
    result.presets.iter().map(|e| e.id.clone()).collect()
}

/// Find a user preset entry by ID.
#[must_use]
pub fn find_user_preset<'a>(
    result: &'a UserPresetScanResult,
    id: &str,
) -> Option<&'a UserPresetEntry> {
    result.presets.iter().find(|e| e.id == id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    /// Minimal valid YAML for testing.
    fn minimal_yaml() -> &'static str {
        r"
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"
    }

    #[test]
    fn missing_directory_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scan_loads_valid_user_presets() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        // Create my-strategy/ with valid preset.yaml
        let strategy_dir = base.join("my-strategy");
        fs::create_dir_all(&strategy_dir).unwrap();
        fs::write(strategy_dir.join("preset.yaml"), minimal_yaml()).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert_eq!(result.presets.len(), 1);
        assert_eq!(result.presets[0].id, "my-strategy");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scan_skips_system_prefixed_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        // Create _system/maintenance/ (should be skipped by user scanner)
        let system_dir = base.join("_system");
        fs::create_dir_all(&system_dir).unwrap();
        fs::write(system_dir.join("preset.yaml"), "bogus").unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scan_skips_hidden_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        // Create .hidden/ (should be skipped)
        let hidden_dir = base.join(".hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("preset.yaml"), "bogus").unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scan_skips_corrupted_presets_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        // Create broken/ with invalid YAML
        let broken_dir = base.join("broken");
        fs::create_dir_all(&broken_dir).unwrap();
        fs::write(broken_dir.join("preset.yaml"), "not valid yaml: [").unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].dir_name, "broken");
        assert!(result.warnings[0].message.contains("YAML parse error"));
    }

    #[test]
    fn scan_skips_directory_without_preset_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        // Create empty/ with no preset.yaml
        let empty_dir = base.join("empty");
        fs::create_dir_all(&empty_dir).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert!(result.presets.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].dir_name, "empty");
        assert!(result.warnings[0].message.contains("missing preset.yaml"));
    }

    #[test]
    fn scan_loads_multiple_user_presets() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        // Create strategy-a/
        let dir_a = base.join("strategy-a");
        fs::create_dir_all(&dir_a).unwrap();
        fs::write(dir_a.join("preset.yaml"), minimal_yaml()).unwrap();

        // Create strategy-b/ with a different preset
        let yaml_b = r"
preset:
  id: strategy-b
  version: 1
  kind: creator
  description: test b
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let dir_b = base.join("strategy-b");
        fs::create_dir_all(&dir_b).unwrap();
        fs::write(dir_b.join("preset.yaml"), yaml_b).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert_eq!(result.presets.len(), 2);
        let ids: Vec<&str> = result.presets.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"strategy-a"));
        assert!(ids.contains(&"strategy-b"));
    }

    #[test]
    fn list_user_preset_ids_returns_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        let strategy_dir = base.join("my-strategy");
        fs::create_dir_all(&strategy_dir).unwrap();
        fs::write(strategy_dir.join("preset.yaml"), minimal_yaml()).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);
        let ids = list_user_preset_ids(&result);

        assert_eq!(ids, vec!["my-strategy".to_string()]);
    }

    #[test]
    fn find_user_preset_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();
        let base = nexus_home.join("presets");

        let strategy_dir = base.join("my-strategy");
        fs::create_dir_all(&strategy_dir).unwrap();
        fs::write(strategy_dir.join("preset.yaml"), minimal_yaml()).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let result = scan_user_presets(&nexus_home, &caps);

        assert!(find_user_preset(&result, "my-strategy").is_some());
        assert!(find_user_preset(&result, "nonexistent").is_none());
    }

    #[test]
    fn user_preset_loaded_via_load_preset() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path().to_path_buf();

        let bundle_dir = nexus_home.join("presets").join("test-strat");
        fs::create_dir_all(&bundle_dir).unwrap();
        fs::write(bundle_dir.join("preset.yaml"), minimal_yaml()).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_user_preset_from_dir(&bundle_dir, "test-strat", &caps).unwrap();
        assert_eq!(loaded.id, "test-strat");
        assert_eq!(loaded.loaded.id, "test-strategy");
    }
}
