//! Capability hot-reload watch support (V1.176 P1, RN-2; AR-91/AR-92).
//!
//! The daemon watches `~/.nexus42/capabilities/` with a **poll + digest**
//! task (AR-91): every [`USER_CAP_WATCH_INTERVAL`] the scan directory's
//! structural digest (`serde_json::Value` over dir names + file names,
//! sizes, and mtimes — the same trick the V1.175 child-side catalog watch
//! uses, AR-79/F-11) is compared for `Value` equality; a change triggers a
//! **rebuild-and-swap** of a fresh [`CapabilityRegistry`] on the SAME
//! scan/admission path as boot (AR-92 #3), merged with the watcher's
//! last-admitted mirror (AR-92 #5) and swapped into the shared
//! [`CapabilityRegistryHolder`].
//!
//! Zero new dependencies (AR-91 #5 / AR-98): `std::fs` + `serde_json` +
//! `tokio::time` only — fs-notify was rejected and recorded at lock time.

use crate::capability::scan::{self, ScanOutcome};
use crate::capability::user_capability::UserCapability;
use crate::capability::{Capability, CapabilityRegistry, CapabilityRuntimeDeps};
use std::collections::HashSet;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

/// Daemon-side poll interval for the user-capability scan directory
/// (AR-91 #1).
///
/// A `const Duration`, not configurable this iteration. 1 s keeps the total
/// author-visible budget in the 2-second class once the MCP child's 2 s leg
/// is added (AR-93).
pub const USER_CAP_WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// Build the structural digest of `dir` (AR-91 #2).
///
/// One directory level (capability dir names) plus each file's names, sizes,
/// and mtimes, as a `serde_json::Value` tree. Change detection is `Value`
/// equality against the previous tick — the same trick the V1.175 child
/// watch (AR-79, F-11) uses over the tools body; no hash-collision
/// reasoning, no new hashing dependency.
///
/// - Missing or unreadable scan dir → `None` — a stable "no dir" digest:
///   no rescans, no skip spam (F-14 missing-dir contract, AR-91 #2/#6).
/// - `_`- and `.`-prefixed dirs are skipped silently, matching the scan's
///   convention.
/// - Entries are sorted so the tree is byte-stable across ticks.
/// - Metadata (size + ns-precision mtime) suffices: a completed write
///   always changes size or mtime on mainstream filesystems; a content
///   change with identical (name, size, mtime) is out of contract
///   (AR-91 #3).
#[must_use]
pub fn scan_dir_digest(dir: &Path) -> Option<serde_json::Value> {
    // Missing (the boot missing-dir contract) or unreadable → the scan
    // would also treat it as empty; keep the digest stable instead of
    // re-scanning every tick.
    let Ok(read) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut entries: Vec<std::fs::DirEntry> = read.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut tree = serde_json::Map::new();
    for entry in entries {
        // Non-UTF-8 names cannot be capability names (AR-34 charset) — the
        // scan skips them too.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('_') || name.starts_with('.') {
            continue;
        }
        let cap_dir = entry.path();
        if !cap_dir.is_dir() {
            continue;
        }
        let mut files = serde_json::Map::new();
        // A per-capability-dir read error omits the dir this tick; the
        // digest then changes and the next tick re-scans (the merge carries
        // the last good entry — AR-92 #5).
        if let Ok(file_read) = std::fs::read_dir(&cap_dir) {
            let mut file_entries: Vec<std::fs::DirEntry> = file_read.flatten().collect();
            file_entries.sort_by_key(std::fs::DirEntry::file_name);
            for file in file_entries {
                let Ok(file_name) = file.file_name().into_string() else {
                    continue;
                };
                let Ok(meta) = file.metadata() else {
                    continue;
                };
                let mtime_ns = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |t| t.as_nanos());
                files.insert(file_name, serde_json::json!([meta.len(), mtime_ns]));
            }
        }
        tree.insert(name, serde_json::Value::Object(files));
    }
    Some(serde_json::Value::Object(tree))
}

/// Rebuild a fresh registry for hot reload (AR-92 #3/#5).
///
/// Builtins come from the SAME boot constructor family
/// (`with_runtime_deps[_and_wasm]`), and the user arm runs the SAME
/// `scan_user_capabilities` — one admission implementation, two callers
/// (boot + the watcher). The watcher's last-good merge
/// ([`merge_user_caps`]) runs between the scan and the append so a failed
/// hot admission keeps the previous admission for that name (PL-9) and a
/// deleted directory drops the name (AR-94).
///
/// The returned `ScanOutcome` carries the **merged** user-cap set (this
/// scan's admissions + carried last-good entries) so the boot-site aggregate
/// log shape (`log_scan_outcome`) reports what the rebuilt registry serves.
#[must_use]
pub fn rebuild_registry_with_merge(
    deps: &CapabilityRuntimeDeps,
    engine: Option<&std::sync::Arc<nexus_wasm_host::WasmEngine>>,
    module_cache: Option<&std::sync::Arc<nexus_wasm_host::ModuleCache>>,
    scan_dir: &Path,
    mirror: &[UserCapability],
) -> (CapabilityRegistry, ScanOutcome) {
    let mut reg = match (engine, module_cache) {
        (Some(engine), Some(cache)) => CapabilityRegistry::with_runtime_deps_and_wasm(
            deps,
            std::sync::Arc::clone(engine),
            std::sync::Arc::clone(cache),
        ),
        _ => CapabilityRegistry::with_runtime_deps(deps),
    };
    let builtin_names: HashSet<&str> = reg.capabilities.iter().map(|c| c.name()).collect();
    let scan = scan::scan_user_capabilities(scan_dir, &builtin_names, engine, module_cache);
    let merged = merge_user_caps(&scan, mirror);
    let outcome = ScanOutcome {
        admitted: merged.clone(),
        skipped: scan.skipped,
    };
    for cap in &merged {
        reg.capabilities
            .push(Box::new(cap.clone()) as Box<dyn Capability>);
    }
    reg.build_index();
    (reg, outcome)
}

/// Merge rule (AR-92 #5), user-cap arm only (builtins are rebuilt fresh by
/// the constructors):
///
/// - admitted by this scan → new entry wins;
/// - skipped by this scan (dir present, trio failed) → **carry the
///   last-good entry** from `mirror` (the skip is already `warn!`-logged by
///   the scanner, boot vocabulary — PL-9);
/// - absent from the scan (dir deleted) → dropped (removal, AR-94).
///
/// Output order: admitted entries in scan order, then carried entries in
/// mirror order — deterministic across ticks (mirror order is itself
/// deterministic).
#[must_use]
pub fn merge_user_caps(scan: &ScanOutcome, mirror: &[UserCapability]) -> Vec<UserCapability> {
    let mut merged: Vec<UserCapability> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::with_capacity(scan.admitted.len());
    for cap in &scan.admitted {
        seen.insert(cap.name());
        merged.push(cap.clone());
    }
    let skipped_names: HashSet<&str> = scan.skipped.iter().map(|s| s.name.as_str()).collect();
    for cap in mirror {
        if !seen.contains(cap.name()) && skipped_names.contains(cap.name()) {
            merged.push(cap.clone());
        }
    }
    merged
}

/// Core watch loop, generic over the digest poll and the rescan action.
///
/// Mirrors the AR-79 child-side `catalog_watch_loop_inner` seam (F-11) so
/// the baseline / no-op semantics (AR-91 #6, AR-95 #5) are unit-testable
/// without a real scan dir or daemon. The first tick establishes the digest
/// WITHOUT re-scanning (boot already scanned); only a change between
/// established digests re-scans. An unchanged digest produces no scan (and
/// therefore no swap and no skip spam).
///
/// Returns the number of rescans executed. `should_stop` lets tests
/// terminate the otherwise-infinite loop deterministically.
pub async fn watch_loop_inner<PF, SF, P, S>(
    interval: Duration,
    mut poll: P,
    mut rescan: S,
    mut should_stop: impl FnMut() -> bool,
) -> usize
where
    P: FnMut() -> PF,
    PF: Future<Output = Option<serde_json::Value>> + Send,
    S: FnMut() -> SF,
    SF: Future<Output = ()> + Send,
{
    // Outer `Option`: `None` = no baseline yet; `Some(None)` = baseline
    // established with a missing scan dir (stable null digest).
    let mut last_digest: Option<Option<serde_json::Value>> = None;
    let mut rescans = 0usize;
    loop {
        if should_stop() {
            return rescans;
        }
        tokio::time::sleep(interval).await;
        let digest = poll().await;
        if last_digest.as_ref().is_some_and(|prev| prev == &digest) {
            continue;
        }
        // Baseline tick: establish the digest without scanning (boot already
        // scanned). Only a change between established digests rescans.
        if last_digest.is_some() {
            rescan().await;
            rescans += 1;
        }
        last_digest = Some(digest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityOrigin, CapabilityRegistryHolder};
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

    /// Write an admitted `<name>/capability.json` trio (AR-35 layout): a
    /// hash-consistent `manifest.json` + `<module-id>.wasm` pair so the
    /// AR-43 admission gates pass inside the scan.
    fn write_capability_dir(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let wasm = b"fake module bytes";
        let sha = sha256_hex(wasm);
        let descriptor = format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#
        );
        std::fs::write(dir.join("capability.json"), descriptor).unwrap();
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
        std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
    }

    fn test_deps() -> CapabilityRuntimeDeps {
        CapabilityRuntimeDeps {
            pool: None,
            worker_provider: None,
            daemon_tool_dispatch: None,
            cdn_config: None,
        }
    }

    /// (`name`, `wasm_sha256`) pairs of an outcome's admitted user
    /// capabilities — the AR-95 #1 machine-checked equivalence key.
    fn admitted_pairs(outcome: &ScanOutcome) -> Vec<(String, String)> {
        outcome
            .admitted
            .iter()
            .map(|c| (c.name().to_string(), c.wasm_sha256().to_string()))
            .collect()
    }

    /// User-capability names a registry serves (origin-filtered), sorted.
    fn user_cap_names(registry: &CapabilityRegistry) -> Vec<String> {
        let mut names: Vec<String> = registry
            .iter()
            .filter(|c| c.origin() == CapabilityOrigin::User)
            .map(|c| c.name().to_string())
            .collect();
        names.sort();
        names
    }

    // -----------------------------------------------------------------
    // Digest (AR-91 #2)
    // -----------------------------------------------------------------

    #[test]
    fn digest_missing_dir_is_stable_none() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert_eq!(scan_dir_digest(&missing), None);
    }

    #[test]
    fn digest_is_deterministic_and_tracks_size_and_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        write_capability_dir(tmp.path(), "zeta.cap");
        let first = scan_dir_digest(tmp.path()).expect("digest present");
        // Identical content → identical digest (deterministic ordering).
        assert_eq!(scan_dir_digest(tmp.path()).unwrap(), first);

        // A file change (different size) → changed digest.
        let wasm_path = tmp.path().join("demo.pull/basic-combat.wasm");
        std::fs::write(&wasm_path, b"fake module bytes, now longer").unwrap();
        let changed = scan_dir_digest(tmp.path()).expect("digest present");
        assert_ne!(changed, first, "file size change must change the digest");
    }

    #[test]
    fn digest_skips_underscore_and_dot_dirs_like_the_scan() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "_system.cap");
        write_capability_dir(tmp.path(), ".hidden.cap");
        write_capability_dir(tmp.path(), "visible.cap");
        let digest = scan_dir_digest(tmp.path()).expect("digest present");
        let tree = digest.as_object().expect("object tree");
        assert_eq!(
            tree.keys().collect::<Vec<_>>(),
            vec!["visible.cap"],
            "only the visible capability dir is digested"
        );
    }

    // -----------------------------------------------------------------
    // Merge rule (AR-92 #5)
    // -----------------------------------------------------------------

    #[test]
    fn merge_carries_last_good_on_skipped_and_drops_absent() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        write_capability_dir(tmp.path(), "gone.cap");
        let deps = test_deps();
        let (_, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, tmp.path(), &[]);
        let mirror = boot_outcome.admitted;

        // Break demo.pull's trio (invalid descriptor) and delete gone.cap.
        std::fs::remove_dir_all(tmp.path().join("gone.cap")).unwrap();
        std::fs::write(tmp.path().join("demo.pull/capability.json"), "{ not json").unwrap();

        let (_, hot_outcome) = rebuild_registry_with_merge(&deps, None, None, tmp.path(), &mirror);
        // demo.pull skipped with the boot vocabulary reason, gone.cap dropped.
        assert_eq!(hot_outcome.skipped.len(), 1);
        assert_eq!(hot_outcome.skipped[0].name, "demo.pull");
        assert!(
            hot_outcome.skipped[0]
                .reason
                .contains("invalid capability.json"),
            "named reason: {:?}",
            hot_outcome.skipped[0].reason
        );
        let names: Vec<String> = hot_outcome
            .admitted
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["demo.pull".to_string()],
            "last-good carried, absent dropped"
        );
    }

    #[test]
    fn merge_admitted_wins_over_mirror() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = test_deps();
        let (_, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, tmp.path(), &[]);
        let mirror = boot_outcome.admitted;
        // Rebuild unchanged: the name is admitted again, and the merge must
        // NOT duplicate it (new entry wins, mirror copy dropped).
        let (reg, hot_outcome) =
            rebuild_registry_with_merge(&deps, None, None, tmp.path(), &mirror);
        assert!(hot_outcome.skipped.is_empty());
        assert_eq!(hot_outcome.admitted.len(), 1, "no duplication");
        assert_eq!(user_cap_names(&reg), vec!["demo.pull".to_string()]);
    }

    // -----------------------------------------------------------------
    // Boot equivalence (AR-95 #1)
    // -----------------------------------------------------------------

    #[test]
    fn hot_rebuild_equals_boot_constructor_for_identical_dir() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "alpha.cap");
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = test_deps();

        // Boot path: the boot constructor family.
        let (boot_reg, boot_outcome) =
            CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, tmp.path());
        // Hot path with an EMPTY prior mirror (nothing to carry/drop).
        let (hot_reg, hot_outcome) =
            rebuild_registry_with_merge(&deps, None, None, tmp.path(), &[]);

        assert!(boot_outcome.skipped.is_empty());
        assert!(hot_outcome.skipped.is_empty());
        assert_eq!(
            admitted_pairs(&hot_outcome),
            admitted_pairs(&boot_outcome),
            "identical dir content + empty mirror ⇒ identical user-cap set (name + wasm_sha256)"
        );
        assert_eq!(
            user_cap_names(&hot_reg),
            user_cap_names(&boot_reg),
            "registry-level user set matches the boot constructor"
        );
    }

    // -------------------------------------------------------------------------
    // Mid-write keeps last-good (AR-95 #2)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn mid_write_keeps_last_good_and_snapshot_dispatch_survives_swap() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = test_deps();
        let (reg, outcome) = rebuild_registry_with_merge(&deps, None, None, tmp.path(), &[]);
        let mirror = outcome.admitted.clone();
        assert!(!mirror.is_empty());

        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        // Dispatch-through-snapshot: clone the Arc exactly as the spine does,
        // then break the trio on disk and swap.
        let held = holder.get().expect("registry present before swap");
        std::fs::write(tmp.path().join("demo.pull/capability.json"), "{ not json").unwrap();
        let (reg2, outcome2) = rebuild_registry_with_merge(&deps, None, None, tmp.path(), &mirror);
        assert_eq!(
            outcome2.skipped.len(),
            1,
            "one named skip for the broken trio"
        );
        holder.swap(std::sync::Arc::new(reg2));

        // The pre-swap snapshot still resolves and the call finishes against
        // last-good (engine-less arm → the AR-44 stub contract).
        let held_cap = held.get("demo.pull").expect("snapshot keeps the name");
        let err = held_cap.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, crate::capability::CapabilityError::WorkerUnavailable),
            "call completes honestly against the held snapshot: {err:?}"
        );
        // And the swapped registry carries last-good for the same name.
        let live = holder.get().expect("swapped registry present");
        assert!(
            live.get("demo.pull").is_some(),
            "last-good admission survives the failed replacement (PL-9)"
        );
    }

    // -------------------------------------------------------------------------
    // No-op stability (AR-95 #5)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn unchanged_digest_and_missing_dir_produce_no_rescans() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let tree = Some(serde_json::json!({"a.cap": {"capability.json": [12, 34]}}));
        let tick = AtomicUsize::new(0);
        let mut rescans = 0;
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async { tree.clone() }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 3,
        )
        .await;
        assert_eq!(tick.load(Ordering::Relaxed), 3, "three ticks observed");
        assert_eq!(scans, 0, "baseline tick only; no digest change → no scan");
        assert_eq!(rescans, 0, "no rescan action fired");

        // Missing dir: stable null digest → no rescans, no skip spam.
        tick.store(0, Ordering::Relaxed);
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async { None }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 3,
        )
        .await;
        assert_eq!(scans, 0, "missing dir digest stays None → never rescans");
        assert_eq!(rescans, 0, "no skip spam from a missing dir");
    }

    #[tokio::test]
    async fn digest_change_triggers_exactly_one_rescan() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let baseline = serde_json::json!({"a.cap": {}});
        let changed = serde_json::json!({"a.cap": {}, "b.cap": {}});
        let tick = AtomicUsize::new(0);
        let mut rescans = 0;
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async {
                    // Two baseline-identical ticks, then a change, then a
                    // no-op tick — a shared-only capture (AtomicUsize) so
                    // the future has no escaping mutable borrow and stays
                    // Send.
                    Some(if tick.load(Ordering::Relaxed) <= 2 {
                        baseline.clone()
                    } else {
                        changed.clone()
                    })
                }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 4,
        )
        .await;
        assert_eq!(scans, 1, "baseline, no-op, one change rescan, no-op");
        assert_eq!(rescans, 1);
    }
}
