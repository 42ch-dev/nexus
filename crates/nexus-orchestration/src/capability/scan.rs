//! User capability directory scan (V1.172 P0, DR-10; AR-35).
//!
//! Scans `~/.nexus42/capabilities/<name>/` (see
//! `nexus_home_layout::user_capabilities_dir`) for capability descriptors and
//! produces the entries appended after builtins by the registry constructors
//! (AR-36). The admitted entries are the concrete [`UserCapability`] type
//! (AR-92 #4) so the hot-reload watcher can mirror them. Fail-safe by
//! contract: a missing directory is an empty outcome and a bad descriptor is
//! a per-entry skip — never a top-level error, never a panic, never a boot
//! failure (AC-V172-2).

use crate::capability::admission::admit;
use crate::capability::user_capability::{UserCapability, UserCapabilityDescriptor};
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
///
/// `admitted` carries the concrete [`UserCapability`] type (V1.176 P1,
/// AR-92 #4) so the hot-reload watcher can keep a last-admitted mirror and
/// merge last-good entries across rebuilds; the registry append seam boxes
/// them at the boundary.
#[derive(Default)]
pub struct ScanOutcome {
    /// Admitted user capabilities in scan order (first-in-wins for
    /// duplicate declared names).
    pub admitted: Vec<UserCapability>,
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
/// - Entries are processed in **sorted directory-name order** for a
///   deterministic scan (catalog order stable across boots; `read_dir`
///   order is filesystem-dependent).
/// - Per-entry `read_dir` errors and non-UTF-8 directory names are logged at
///   `warn!`; the non-UTF-8 case also gets a skip record (AR-35
///   all-skips-logged).
/// - Duplicate declared names: first-in-sorted-order wins, the rest skipped
///   (defensive: with the dir-name == descriptor-name rule, two distinct dirs
///   cannot both match — the guard is retained for future admission changes
///   and is checked before the mismatch guard so its skip reason stays
///   reachable, M1 QC wave).
/// - Admission (P1 T4, AR-43) runs per candidate **before** it is emitted:
///   `builtin_names` (the registry's builtin name set) drives gate 1
///   (collision → skip, builtin wins — AR-36/AR-43); gates 2–3 (module file
///   presence + `wasm_sha256` pairing, AR-38/AR-39) fail closed per
///   candidate; gate 4 clamps sandbox overrides and never rejects. A
///   rejected candidate is `ScanOutcome.skipped` with its named
///   `AdmissionError` reason — never a panic, never a boot failure, never a
///   half-registered entry (AC-V172-2).
///
/// `engine`/`module_cache` are the daemon-wide executor handles (AR-37) —
/// `Some`/`Some` on the engine boot arm so every admitted capability can run
/// its module, `None`/`None` on the engine-less arm (AR-44) so `run()` returns
/// `WorkerUnavailable`.
#[must_use]
#[allow(clippy::implicit_hasher)] // same builder-agnostic set contract as `admit`
pub fn scan_user_capabilities(
    dir: &Path,
    builtin_names: &HashSet<&str>,
    engine: Option<&std::sync::Arc<nexus_wasm_host::WasmEngine>>,
    module_cache: Option<&std::sync::Arc<nexus_wasm_host::ModuleCache>>,
) -> ScanOutcome {
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

    // Collect entries first so processing order is deterministic: sort by
    // directory name (S-003/M2 — `read_dir` order is not guaranteed).
    let mut entries: Vec<std::fs::DirEntry> = Vec::new();
    for entry in read {
        match entry {
            Ok(e) => entries.push(e),
            Err(e) => {
                // Per-entry I/O errors (e.g. an unreadable subdir) are
                // boot-safe skips — log them (AR-35 all-skips-logged); no
                // capability name is available for a skip record.
                tracing::warn!(
                    error = %e,
                    dir = %dir.display(),
                    "cannot read user capabilities directory entry; skipping"
                );
            }
        }
    }
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let dir_name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(bad) => {
                // Non-UTF-8 names cannot be capability names (AR-34 charset) —
                // skip with a warn + record (S-002, AR-35 all-skips-logged).
                let lossy = bad.to_string_lossy();
                skip(
                    &mut outcome,
                    &lossy,
                    "directory name is not valid UTF-8".to_string(),
                );
                continue;
            }
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

        // Duplicate declared names (AR-36): first-in-sorted-order wins, the
        // rest skipped + logged. Checked BEFORE the dir-name mismatch check
        // (M1 QC wave): the mismatch guard fires on a dir whose descriptor
        // name differs from its dir name; the duplicate guard must fire when
        // a later dir declares a name already admitted — otherwise it would
        // be structurally unreachable and its skip reason never exercised.
        // `contains` (not `insert`) here: a mismatched entry must not reserve
        // its declared name for later entries.
        if admitted_names.contains(&descriptor.name) {
            skip(
                &mut outcome,
                &dir_name,
                format!("duplicate user capability name '{}'", descriptor.name),
            );
            continue;
        }

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

        // AR-43 (P1 T4): admission gates run before the capability is
        // emitted — collision (builtin wins, gate 1), module-file presence
        // (gate 2), `wasm_sha256` pairing (gate 3, AR-39) — fail-closed per
        // candidate; a rejected candidate is a named skip (never a panic,
        // never a boot failure, never half-registered — AC-V172-2). The
        // admitted descriptor carries the clamped sandbox (gate 4, AR-38).
        // The name is reserved only on admission — a rejected candidate must
        // not reserve its declared name for later entries.
        let descriptor = match admit(&descriptor, &path, builtin_names) {
            Ok(admitted) => admitted.descriptor,
            Err(e) => {
                skip(&mut outcome, &dir_name, format!("admission failed: {e}"));
                continue;
            }
        };
        admitted_names.insert(descriptor.name.clone());

        // AR-37: the registered capability carries its own dir (source of
        // manifest.json + <module-id>.wasm) and the daemon-wide engine/cache.
        // The engine arm passes Some/Some so run() executes the module; the
        // engine-less arm passes None/None (AR-44) so run() returns
        // WorkerUnavailable.
        outcome.admitted.push(UserCapability::new(
            &descriptor,
            path,
            engine.cloned(),
            module_cache.cloned(),
        ));
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
///
/// `pub(crate)`: reachable to sibling modules (the registry append path);
/// builtin collisions are recorded through this helper via the scan's
/// admission gate 1 (AR-43) since P1 T4.
pub(crate) fn skip(outcome: &mut ScanOutcome, name: &str, reason: String) {
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
    use crate::capability::Capability;
    use sha2::Digest;
    use std::fmt::Write as _;

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = sha2::Sha256::digest(bytes);
        let mut hex = String::with_capacity(64);
        for b in digest {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    }

    /// The builtin-name set the registry passes into the scan (a small,
    /// realistic subset used by scan tests; `sync.pull` collides).
    fn builtins() -> HashSet<&'static str> {
        HashSet::from(["sync.pull", "narrative.compute"])
    }

    /// 64 lowercase hex chars (valid per AR-34 format rules) for descriptors
    /// whose skip fires BEFORE admission (validation / mismatch / duplicate
    /// guards) — the module pair's real hash is never consulted.
    const FAKE_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// Write `<dir>/manifest.json` + `<dir>/basic-combat.wasm` with a **real**
    /// matching sha (the AR-39 pairing admission verifies). Returns the sha.
    fn write_module_pair(dir: &Path) -> String {
        let wasm = b"fake module bytes";
        let sha = sha256_hex(wasm);
        std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
        let manifest = format!(
            r#"{{
                "module_id": "basic-combat",
                "name": "Basic Combat",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": "{sha}"
            }}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        sha
    }

    fn descriptor_json(name: &str, sha: &str) -> String {
        format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#
        )
    }

    /// Write an admitted `<name>/capability.json` trio (AR-35 layout): a
    /// hash-consistent `manifest.json` + `<module-id>.wasm` pair so the
    /// AR-43 admission gates pass (P1 T4 wires admission into the scan).
    fn write_capability_dir(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let sha = write_module_pair(&dir);
        std::fs::write(dir.join("capability.json"), descriptor_json(name, &sha)).unwrap();
    }

    #[test]
    fn scan_valid_trio_admits_with_declared_name() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        assert_eq!(outcome.admitted.len(), 1, "one admitted");
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        let cap = &outcome.admitted[0];
        assert_eq!(cap.name(), "demo.pull");
        assert_eq!(cap.input_schema(), r#"{"type":"object"}"#);
        assert_eq!(cap.output_schema(), r#"{"type":"object"}"#);
    }

    #[test]
    fn scan_empty_dir_returns_empty_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        assert!(outcome.admitted.is_empty());
        assert!(outcome.skipped.is_empty());
    }

    #[test]
    fn scan_missing_dir_returns_empty_outcome_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let outcome = scan_user_capabilities(&missing, &builtins(), None, None);
        assert!(outcome.admitted.is_empty());
        assert!(outcome.skipped.is_empty());
    }

    #[test]
    fn scan_invalid_descriptor_json_is_skipped_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("broken.cap");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("capability.json"), "{ not json").unwrap();
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
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
        std::fs::write(
            dir.join("capability.json"),
            descriptor_json("BadName", FAKE_SHA),
        )
        .unwrap();
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
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
        std::fs::write(
            dir.join("capability.json"),
            descriptor_json("other.name", FAKE_SHA),
        )
        .unwrap();
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        assert!(outcome.admitted.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert!(
            outcome.skipped[0].reason.contains("does not match"),
            "named reason: {:?}",
            outcome.skipped[0].reason
        );
    }

    /// AR-43 (P1 T4): a scan dir with a valid descriptor AND a colliding
    /// (builtin-named) descriptor — the valid one registers, the colliding
    /// one is skipped with a named `NameCollision` reason, the catalog lacks
    /// the colliding name, and nothing panics (AC-V172-2 skip-and-log).
    #[test]
    fn scan_mixed_dir_admits_valid_and_skips_colliding() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "sync.pull"); // builtin name (gate 1)
        write_capability_dir(tmp.path(), "demo.pull"); // valid, admitted
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        let names: Vec<&str> = outcome.admitted.iter().map(Capability::name).collect();
        assert_eq!(
            names,
            vec!["demo.pull"],
            "valid descriptor registers; colliding name absent"
        );
        assert_eq!(outcome.skipped.len(), 1, "one named skip");
        assert_eq!(outcome.skipped[0].name, "sync.pull");
        assert!(
            outcome.skipped[0].reason.contains("NameCollision")
                || outcome.skipped[0]
                    .reason
                    .contains("collides with a builtin"),
            "named NameCollision reason, got: {:?}",
            outcome.skipped[0].reason
        );
    }

    #[test]
    fn scan_skips_underscore_and_dot_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "_system.cap");
        write_capability_dir(tmp.path(), ".hidden.cap");
        write_capability_dir(tmp.path(), "visible.cap");
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        let names: Vec<&str> = outcome.admitted.iter().map(Capability::name).collect();
        assert_eq!(names, vec!["visible.cap"]);
        assert!(outcome.skipped.is_empty());
    }

    /// M1 (QC wave): two dirs cannot both declare the same descriptor name
    /// (dir-name == descriptor-name makes this structurally impossible), but
    /// the duplicate guard must still be exercised: first-in-sorted-order
    /// wins, the second is skipped with a named reason. `a.dup` sorts before
    /// `b.dup`, so `a.dup` must win.
    #[test]
    fn scan_duplicate_declared_name_first_sorted_wins_second_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Same declared name in two dirs — `a.dup` sorts before `b.dup`.
        write_capability_dir(tmp.path(), "a.dup");
        write_capability_dir(tmp.path(), "b.dup");
        // Rewrite b.dup's descriptor to declare the same name as a.dup.
        let b_dir = tmp.path().join("b.dup");
        std::fs::write(
            b_dir.join("capability.json"),
            descriptor_json("a.dup", FAKE_SHA),
        )
        .unwrap();
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        let names: Vec<&str> = outcome.admitted.iter().map(Capability::name).collect();
        assert_eq!(
            names,
            vec!["a.dup"],
            "first in sorted order wins the duplicate name"
        );
        assert_eq!(outcome.skipped.len(), 1, "second duplicate skipped");
        assert_eq!(outcome.skipped[0].name, "b.dup");
        assert!(
            outcome.skipped[0]
                .reason
                .contains("duplicate user capability name"),
            "named reason: {:?}",
            outcome.skipped[0].reason
        );
    }

    // S-003/M2: scan order must be deterministic (sorted by directory name),
    // not filesystem `read_dir` order. The admitted order equals sorted dir
    // names regardless of creation order.
    #[test]
    fn scan_order_is_sorted_by_directory_name() {
        let tmp = tempfile::tempdir().unwrap();
        // Create out of alphabetical order — the scan must still admit in
        // sorted order.
        write_capability_dir(tmp.path(), "zeta.cap");
        write_capability_dir(tmp.path(), "alpha.cap");
        write_capability_dir(tmp.path(), "mike.cap");
        let outcome = scan_user_capabilities(tmp.path(), &builtins(), None, None);
        let names: Vec<&str> = outcome.admitted.iter().map(Capability::name).collect();
        assert_eq!(
            names,
            vec!["alpha.cap", "mike.cap", "zeta.cap"],
            "admitted in deterministic sorted order"
        );
        assert!(outcome.skipped.is_empty());
    }
}
