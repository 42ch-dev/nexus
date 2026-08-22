//! User capability directory scan (V1.172 P0, DR-10; AR-35).
//!
//! Scans `~/.nexus42/capabilities/<name>/` (see
//! `nexus_home_layout::user_capabilities_dir`) for capability descriptors and
//! produces the entries appended after builtins by the registry constructors
//! (AR-36). Fail-safe by contract: a missing directory is an empty outcome,
//! a bad descriptor is a per-entry skip — never a top-level error, never a
//! panic, never a boot failure (AC-V172-2).

use crate::capability::user_capability::{UserCapability, UserCapabilityDescriptor};
use crate::capability::Capability;
use std::collections::HashSet;
use std::path::Path;

/// A capability directory skipped during the scan, with the named reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedCapability {
    /// The directory (or declared) name the scan was processing.
    pub name: String,
    /// Human-readable reason for the skip (also logged at `warn!`).
    pub reason: String,
}

/// Result of [`scan_user_capabilities`].
///
/// Admitted entries are appended after builtins by the registry constructors
/// (AR-36); skipped entries carry their reasons and were already logged.
#[derive(Default)]
pub struct ScanOutcome {
    /// Admitted user capabilities in scan order (first-in-wins for
    /// duplicate declared names).
    pub admitted: Vec<Box<dyn Capability>>,
    /// Skipped capability directories with named reasons — never a scan error.
    pub skipped: Vec<SkippedCapability>,
}

/// Scan `dir` (`~/.nexus42/capabilities/<name>/`) for capability descriptors
/// (AR-35).
///
/// - Missing or unreadable `dir` → empty outcome (user capabilities are
///   optional; `ModuleCache::warm_dir` missing-dir precedent).
/// - `_`- and `.`-prefixed directories are skipped silently (user-preset
///   scanner precedent, `nexus-home-layout` `list_user_preset_ids`).
/// - A directory is admitted only when `<name>/capability.json` parses and
///   validates AND the directory name equals the descriptor's `name`.
/// - Parse/validation/read failures are per-capability skips with a named
///   reason — never a top-level error, never a panic.
/// - Duplicate declared names: first-in-scan-order wins, the rest skipped
///   (defensive: with the dir-name == descriptor-name rule, two distinct dirs
///   cannot both pass; retained as the AR-36 collision guard for future
///   admission changes).
/// - No admission gates at P0 (collision/hash/clamp are P1, AR-43); the
///   declared name is stored as-is.
#[must_use]
pub fn scan_user_capabilities(dir: &Path) -> ScanOutcome {
    let mut outcome = ScanOutcome::default();
    let mut admitted_names: HashSet<String> = HashSet::new();

    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return outcome,
        Err(e) => {
            // Non-missing read failures are still boot-safe: warn and treat
            // the directory as empty.
            tracing::warn!(
                error = %e,
                path = %dir.display(),
                "cannot read user capabilities directory; treating as empty"
            );
            return outcome;
        }
    };

    for entry in read.flatten() {
        let path = entry.path();
        let Ok(dir_name) = entry.file_name().into_string() else {
            continue;
        };
        if !path.is_dir() {
            continue;
        }
        // Skip system-prefixed and hidden dirs (preset scanner precedent).
        if dir_name.starts_with('_') || dir_name.starts_with('.') {
            continue;
        }

        let descriptor_path = path.join("capability.json");
        let descriptor = match read_descriptor(&descriptor_path) {
            Ok(d) => d,
            Err(reason) => {
                skip(&mut outcome, &dir_name, reason);
                continue;
            }
        };

        if descriptor.name != dir_name {
            skip(
                &mut outcome,
                &dir_name,
                format!(
                    "directory name '{dir_name}' does not match descriptor name '{}'",
                    descriptor.name
                ),
            );
            continue;
        }

        if !admitted_names.insert(descriptor.name.clone()) {
            skip(
                &mut outcome,
                &dir_name,
                format!("duplicate user capability name '{}'", descriptor.name),
            );
            continue;
        }

        outcome
            .admitted
            .push(Box::new(UserCapability::new(&descriptor)));
    }

    outcome
}

/// Read + validate `<name>/capability.json`; returns a named reason on
/// failure (read error, parse error, or AR-34 validation failure).
fn read_descriptor(path: &Path) -> Result<UserCapabilityDescriptor, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let descriptor: UserCapabilityDescriptor =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid capability.json: {e}"))?;
    descriptor
        .validate()
        .map_err(|e| format!("invalid capability.json: {e}"))?;
    Ok(descriptor)
}

/// Record a skip and log it at `warn!` (AR-35: all skips are logged; the
/// daemon never fails boot on a bad user capability).
fn skip(outcome: &mut ScanOutcome, name: &str, reason: String) {
    tracing::warn!(
        capability = %name,
        reason = %reason,
        "skipping user capability"
    );
    outcome.skipped.push(SkippedCapability {
        name: name.to_string(),
        reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn descriptor_json(name: &str) -> String {
        format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }}
            }}"#
        )
    }

    /// Write a valid `<name>/capability.json` trio. `manifest.json` +
    /// `<module-id>.wasm` are not scanned at P0 (AR-35) but are written to
    /// mirror the real install layout.
    fn write_capability_dir(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("capability.json"), descriptor_json(name)).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            r#"{ "module_id": "basic-combat" }"#,
        )
        .unwrap();
        std::fs::write(dir.join("basic-combat.wasm"), b"\0asm").unwrap();
    }

    #[test]
    fn scan_valid_trio_admits_with_declared_name() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "sync.pull");
        let outcome = scan_user_capabilities(tmp.path());
        assert_eq!(outcome.admitted.len(), 1, "one admitted");
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        let cap = outcome.admitted[0].as_ref();
        assert_eq!(cap.name(), "sync.pull");
        assert_eq!(cap.input_schema(), r#"{"type":"object"}"#);
        assert_eq!(cap.output_schema(), r#"{"type":"object"}"#);
    }

    #[test]
    fn scan_empty_dir_returns_empty_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = scan_user_capabilities(tmp.path());
        assert!(outcome.admitted.is_empty());
        assert!(outcome.skipped.is_empty());
    }

    #[test]
    fn scan_missing_dir_returns_empty_outcome_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let outcome = scan_user_capabilities(&missing);
        assert!(outcome.admitted.is_empty());
        assert!(outcome.skipped.is_empty());
    }

    #[test]
    fn scan_invalid_descriptor_json_is_skipped_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("broken.cap");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("capability.json"), "{ not json").unwrap();
        let outcome = scan_user_capabilities(tmp.path());
        assert!(outcome.admitted.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].name, "broken.cap");
        assert!(
            outcome.skipped[0]
                .reason
                .contains("invalid capability.json"),
            "named reason, got: {:?}",
            outcome.skipped[0].reason
        );
    }

    #[test]
    fn scan_validation_failure_is_skipped_with_reason() {
        // "BadName" parses (String field) but fails AR-34 validation (uppercase).
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("BadName");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("capability.json"), descriptor_json("BadName")).unwrap();
        let outcome = scan_user_capabilities(tmp.path());
        assert!(outcome.admitted.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert!(
            outcome.skipped[0]
                .reason
                .contains("invalid capability.json"),
            "named reason, got: {:?}",
            outcome.skipped[0].reason
        );
    }

    #[test]
    fn scan_dir_name_mismatch_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("declared.name");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("capability.json"), descriptor_json("other.name")).unwrap();
        let outcome = scan_user_capabilities(tmp.path());
        assert!(outcome.admitted.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert!(
            outcome.skipped[0].reason.contains("does not match"),
            "named reason: {:?}",
            outcome.skipped[0].reason
        );
    }

    #[test]
    fn scan_skips_underscore_and_dot_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "_system.cap");
        write_capability_dir(tmp.path(), ".hidden.cap");
        write_capability_dir(tmp.path(), "visible.cap");
        let outcome = scan_user_capabilities(tmp.path());
        let names: Vec<&str> = outcome.admitted.iter().map(|c| c.name()).collect();
        assert_eq!(names, vec!["visible.cap"]);
        assert!(outcome.skipped.is_empty());
    }
}
