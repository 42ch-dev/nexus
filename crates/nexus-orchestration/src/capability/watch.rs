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
//!
//! # Cost & worker-thread model (qc3 S-1)
//!
//! The poll + rebuild run synchronously on the watcher's tokio task:
//! - Per tick: one small stat walk (`read_dir` ×2 + `metadata()` per file +
//!   a `serde_json` tree over a handful of capability dirs) — author-sized
//!   (one subdir per installed capability), microseconds-to-low-ms locally.
//! - Rebuild (only on a digest change): full re-scan + builtin
//!   re-construction, bounded by the capability count.
//! - `spawn_blocking` deliberately NOT used: the work is small file IO +
//!   serde only; a handoff adds latency without removing the
//!   blocking-on-worker property, and the swap must run on this task anyway.
//! - The loop sleeps AFTER the work (no drift compensation): a
//!   slow/networked fs or a large caps dir stretches the tick; the AR-93
//!   budget includes the rebuild in the daemon leg ("1 s watch + rebuild +
//!   2 s child watch + one HTTP request").

use crate::capability::scan::{self, ScanOutcome};
use crate::capability::user_capability::UserCapability;
use crate::capability::{Capability, CapabilityRegistry, CapabilityRuntimeDeps};
use std::collections::HashSet;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
/// Result of one digest poll of the user-capability scan directory.
///
/// The watcher treats the three states differently (I-1, PL-9):
/// - [`DigestPoll::Missing`]: the scan dir is **absent** (`NotFound`) — the
///   stable "no dir" state (F-14). A `Missing` poll after a `Tree` baseline
///   means the user deleted the dir → rescan, and the merge drops the names
///   (removal contract, AR-94).
/// - [`DigestPoll::Unreadable`]: the scan dir **exists but could not be
///   read** (EACCES, EMFILE, ENOTDIR, …) — a transient failure, NOT a
///   deletion. The watcher keeps last-good unchanged: no rescan, no swap,
///   no baseline disturbance (the poll is retried next tick).
/// - [`DigestPoll::Tree`]: the structural digest of a readable scan dir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestPoll {
    /// Scan directory absent (`NotFound`).
    Missing,
    /// Scan directory present but unreadable, with the io error message
    /// (path + error) for the once-per-error-state log.
    Unreadable(String),
    /// Structural digest of the present, readable scan dir.
    Tree(serde_json::Value),
}

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
/// - Missing dir → [`DigestPoll::Missing`] — a stable "no dir" state: no
///   rescans, no skip spam (F-14 missing-dir contract, AR-91 #2/#6).
/// - Present-but-unreadable dir (EACCES, EMFILE, …) → [`DigestPoll::Unreadable`]
///   — NOT conflated with a deletion (I-1): the watcher keeps last-good
///   instead of treating every mirrored name as absent.
/// - `_`- and `.`-prefixed dirs are skipped silently, matching the scan's
///   convention.
/// - Entries are sorted so the tree is byte-stable across ticks.
/// - Metadata (size + ns-precision mtime) suffices: a completed write
///   always changes size or mtime on mainstream filesystems; a content
///   change with identical (name, size, mtime) is out of contract
///   (AR-91 #3).
#[must_use]
pub fn scan_dir_digest(dir: &Path) -> DigestPoll {
    // A `NotFound` read failure is the honest "no dir" state; ANY other
    // read failure (EACCES, EMFILE, ENOTDIR, …) is a transient unreadable
    // dir and must NOT be treated as a deletion (I-1) — the watcher keeps
    // last-good unchanged.
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return DigestPoll::Missing,
        Err(e) => return DigestPoll::Unreadable(format!("{}: {e}", dir.display())),
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
        let files = dir_file_digest(&cap_dir);
        tree.insert(name, serde_json::Value::Object(files));
    }
    DigestPoll::Tree(serde_json::Value::Object(tree))
}

/// Build the `{file_name: [size, mtime_ns]}` map for one capability dir —
/// the per-dir leaf of the structural digest (AR-91 #2). Shared by
/// [`scan_dir_digest`] and [`digest_from_admitted`] so the watcher's
/// baseline and poll digests use the same file-metadata shape.
///
/// A per-capability-dir read error omits the dir this tick; the digest
/// then changes and the next tick re-scans (the merge carries the last
/// good entry — AR-92 #5).
fn dir_file_digest(cap_dir: &Path) -> serde_json::Map<String, serde_json::Value> {
    let mut files = serde_json::Map::new();
    if let Ok(file_read) = std::fs::read_dir(cap_dir) {
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
    files
}

/// Build the watcher's initial baseline from the BOOT scan's admitted
/// outcome.
///
/// The baseline must represent exactly what the registry serves — never a
/// fresh read of the scan dir, which can include a complete trio written
/// between the boot scan and the digest computation (Greptile P1, V1.176
/// PR wave). Such a trio would land in the baseline but NOT in the
/// registry, so the first poll would match the baseline and skip the
/// rebuild — the capability stays unavailable until another fs change.
///
/// The tree shape matches [`scan_dir_digest`] (dir name → {file name →
/// [`size`, `mtime_ns`]}) so the first poll's `Value` equality comparison
/// is meaningful. A dir the scan did NOT admit (skipped trio, stray dir)
/// is deliberately absent: if it becomes a complete capability later, the
/// first poll sees the change and rescans instead of absorbing it.
#[must_use]
pub fn digest_from_admitted(admitted: &[UserCapability]) -> DigestPoll {
    let mut tree = serde_json::Map::new();
    for cap in admitted {
        let files = dir_file_digest(cap.dir());
        tree.insert(cap.name().to_string(), serde_json::Value::Object(files));
    }
    DigestPoll::Tree(serde_json::Value::Object(tree))
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
        // Engine-less arm (AR-44). qc3 S-5: with `deps.pool` present this
        // rebuilds a fresh `NarrativeCompute` (its own `WasmEngine` +
        // `ModuleCache::warm_embedded`) per reload — production exposure is
        // ~nil because this arm is only taken when the daemon-wide
        // `WasmEngine::new()` already failed at boot, and `with_pool` then
        // fails the same way and degrades to `engine: None` cheaply; tests
        // use `pool: None` → the no-op `NarrativeCompute::new()`.
        _ => CapabilityRegistry::with_runtime_deps(deps),
    };
    let builtin_names: HashSet<&str> = reg.capabilities.iter().map(|c| c.name()).collect();
    let scan = scan::scan_user_capabilities(scan_dir, &builtin_names, engine, module_cache);
    let merged = merge_user_caps(&scan, mirror);
    let outcome = ScanOutcome {
        admitted: merged.clone(),
        skipped: scan.skipped,
        // W-3 (qc3 S-4): a scan interrupted by a per-entry read error is
        // transient — the merge keeps last-good for every name it did not
        // re-admit, so an incomplete scan never reads as deletions.
        transient: scan.transient,
    };
    // Shared append seam (M-3): the same boxing path as the boot
    // constructors' `append_user_caps`, so a change to the append (extra
    // index/logging) lands in one place.
    reg.append_user_cap_entries(&merged);
    (reg, outcome)
}

/// Merge rule (AR-92 #5), user-cap arm only (builtins are rebuilt fresh by
/// the constructors):
///
/// - admitted by this scan → new entry wins;
/// - skipped by this scan (dir present, trio failed) → **carry the
///   last-good entry** from `mirror` (the skip is already `warn!`-logged by
///   the scanner, boot vocabulary — PL-9);
/// - absent from the scan (dir deleted) → dropped (removal, AR-94);
/// - **transient scan** (W-3, qc3 S-4): a per-entry `read_dir` error
///   interrupted the scan — it may have missed names, so it must NOT read
///   as deletions. Every mirrored name the scan did not re-admit is
///   carried (last-good), regardless of skip records.
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
        if seen.contains(cap.name()) {
            continue;
        }
        // A transient scan is incomplete — carry every unmatched last-good
        // entry instead of dropping it as if deleted (W-3).
        if scan.transient || skipped_names.contains(cap.name()) {
            merged.push(cap.clone());
        }
    }
    merged
}

/// Core watch loop, generic over the digest poll and the rescan action.
///
/// Mirrors the AR-79 child-side `catalog_watch_loop_inner` seam (F-11) so
/// the baseline / no-op semantics (AR-91 #6, AR-95 #5) are unit-testable
/// without a real scan dir or daemon. `initial_digest` seeds the baseline
/// from the BOOT scan's digest (W-B): the first poll COMPARES against it,
/// so a change between boot's scan and the first poll is detected instead
/// of being absorbed as the baseline. Only a change between established
/// digests re-scans; an unchanged digest produces no scan (and therefore
/// no swap and no skip spam).
///
/// An [`DigestPoll::Unreadable`] poll never establishes or disturbs the
/// baseline and never rescans (I-1, PL-9): a transiently unreadable scan
/// dir keeps the last-good registry generation unchanged, and the failure
/// is logged once per error-state. [`DigestPoll::Missing`] after a
/// [`DigestPoll::Tree`] baseline rescans — the merge then drops the missing
/// names (deletion contract, AR-94).
///
/// Returns the number of rescans executed. `should_stop` is a test-only
/// seam (qc1 S-3): production always passes `|| false` (boot.rs) — the
/// loop never self-terminates; it is cancelled via the shutdown-notify
/// `select!` in `user_capability_watch_loop` or by dropping the
/// [`WatcherGuard`](crate::boot::WatcherGuard) (abort-on-drop).
pub async fn watch_loop_inner<PF, SF, P, S>(
    interval: Duration,
    initial_digest: DigestPoll,
    mut poll: P,
    mut rescan: S,
    mut should_stop: impl FnMut() -> bool,
) -> usize
where
    P: FnMut() -> PF,
    PF: Future<Output = DigestPoll> + Send,
    S: FnMut() -> SF,
    SF: Future<Output = ()> + Send,
{
    // Seeded baseline (W-B): the boot scan's digest, so the first poll is
    // a comparison, not a baseline establishment. An `Unreadable` poll
    // never disturbs it (I-1); `Missing`/`Tree` after it rescan on change.
    let mut last_digest: Option<DigestPoll> = Some(initial_digest);
    let mut rescans = 0usize;
    let mut unreadable_warned = false;
    loop {
        if should_stop() {
            return rescans;
        }
        let digest = poll().await;
        match &digest {
            DigestPoll::Unreadable(message) => {
                // Transient read failure (EACCES, EMFILE, ENOTDIR, …): keep
                // last-good unchanged — no rescan, no swap, baseline intact
                // (I-1). Log once per error-state, not every tick.
                if !unreadable_warned {
                    tracing::warn!(
                        error = %message,
                        "cannot read user capabilities directory; keeping last-good registry, no rescan (hot-reload skip — will retry)"
                    );
                    unreadable_warned = true;
                }
                tokio::time::sleep(interval).await;
                continue;
            }
            DigestPoll::Missing | DigestPoll::Tree(_) => {
                unreadable_warned = false;
            }
        }
        if last_digest.as_ref().is_some_and(|prev| prev == &digest) {
            tokio::time::sleep(interval).await;
            continue;
        }
        // Any divergence from the seeded baseline rescans — including the
        // first poll when it differs from the boot scan's digest (W-B).
        // `last_digest` is always `Some` after seeding; `Unreadable` polls
        // never reach this point (they `continue` above).
        rescan().await;
        rescans += 1;
        last_digest = Some(digest);
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::test_support::write_capability_dir;
    use crate::capability::{CapabilityOrigin, CapabilityRegistryHolder};
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn digest_missing_dir_is_stable_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert_eq!(scan_dir_digest(&missing), DigestPoll::Missing);
    }

    #[test]
    fn digest_present_but_unreadable_path_is_not_missing() {
        // A path that exists but cannot be read as a directory (here: a
        // regular file → ENOTDIR; EACCES/EMFILE hit the same non-NotFound
        // branch) must NOT be conflated with a missing dir (I-1) — the
        // watcher keeps last-good on `Unreadable`.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("capabilities");
        std::fs::write(&path, b"not a directory").unwrap();
        match scan_dir_digest(&path) {
            DigestPoll::Unreadable(_) => {}
            other => panic!("non-NotFound read failure must be Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn digest_is_deterministic_and_tracks_size_and_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        write_capability_dir(tmp.path(), "zeta.cap");
        let DigestPoll::Tree(first) = scan_dir_digest(tmp.path()) else {
            panic!("expected a Tree digest for the present dir");
        };
        // Identical content → identical digest (deterministic ordering).
        let DigestPoll::Tree(same) = scan_dir_digest(tmp.path()) else {
            panic!("expected a Tree digest for the present dir");
        };
        assert_eq!(same, first);

        // A file change (different size) → changed digest.
        let wasm_path = tmp.path().join("demo.pull/basic-combat.wasm");
        std::fs::write(&wasm_path, b"fake module bytes, now longer").unwrap();
        let DigestPoll::Tree(changed) = scan_dir_digest(tmp.path()) else {
            panic!("expected a Tree digest for the present dir");
        };
        assert_ne!(changed, first, "file size change must change the digest");
    }

    #[test]
    fn digest_skips_underscore_and_dot_dirs_like_the_scan() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "_system.cap");
        write_capability_dir(tmp.path(), ".hidden.cap");
        write_capability_dir(tmp.path(), "visible.cap");
        let DigestPoll::Tree(digest) = scan_dir_digest(tmp.path()) else {
            panic!("expected a Tree digest for the present dir");
        };
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
    #[test]
    fn merge_transient_scan_carries_unmatched_last_good() {
        // W-3 (qc3 S-4): a per-entry `read_dir` error makes the scan
        // outcome transient — the merge keeps last-good for EVERY name the
        // scan did not re-admit, instead of dropping it as if the dir had
        // been deleted (I-1 consistency with the digest's Unreadable case).
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        write_capability_dir(tmp.path(), "gone.cap");
        let deps = test_deps();
        let (_, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, tmp.path(), &[]);
        let mirror = boot_outcome.admitted;
        assert_eq!(mirror.len(), 2, "both names admitted at boot");

        // The interrupted hot scan re-admits only demo.pull and is marked
        // transient — gone.cap was never observed, so it must NOT be
        // dropped as a deletion.
        let scan = ScanOutcome {
            admitted: vec![mirror[0].clone()],
            skipped: Vec::new(),
            transient: true,
        };
        let merged = merge_user_caps(&scan, &mirror);
        let names: Vec<String> = merged.iter().map(|c| c.name().to_string()).collect();
        assert_eq!(
            names,
            vec!["demo.pull".to_string(), "gone.cap".to_string()],
            "transient scan carries every unmatched last-good entry"
        );
        // Control: a NON-transient scan drops the absent name (removal).
        let scan = ScanOutcome {
            admitted: vec![mirror[0].clone()],
            skipped: Vec::new(),
            transient: false,
        };
        let merged = merge_user_caps(&scan, &mirror);
        let names: Vec<String> = merged.iter().map(|c| c.name().to_string()).collect();
        assert_eq!(
            names,
            vec!["demo.pull".to_string()],
            "non-transient scan drops the absent name (removal contract)"
        );
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
    // No-op stability (AR-95 #5) + transient-error safety (I-1) + M-6
    // -------------------------------------------------------------------------

    /// The real hot-reload action (mirror of the boot wiring): rebuild +
    /// merge + swap, updating the last-admitted mirror.
    fn hot_rebuild_test(
        holder: &CapabilityRegistryHolder,
        deps: &CapabilityRuntimeDeps,
        scan_dir: &Path,
        mirror: &std::sync::Mutex<Vec<UserCapability>>,
    ) {
        let mut mirror = mirror
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (reg, outcome) = rebuild_registry_with_merge(deps, None, None, scan_dir, &mirror);
        holder.swap(std::sync::Arc::new(reg));
        *mirror = outcome.admitted;
    }

    /// Drive the real watch loop (real `scan_dir_digest` + real rebuild/swap)
    /// for `max_ticks`, returning the loop's rescan count. `on_poll` runs
    /// just before each digest read (with the tick number and the scan dir)
    /// so a test can mutate the scan dir mid-loop — the baseline and the
    /// subsequent change happen inside ONE loop invocation, which is how
    /// the production watcher behaves.
    #[allow(clippy::too_many_arguments)] // test helper: all eight are the wiring inputs
    async fn run_real_watch(
        scan_dir: &Path,
        holder: &CapabilityRegistryHolder,
        deps: &CapabilityRuntimeDeps,
        mirror: &std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>>,
        rescans: &AtomicUsize,
        ticks: &AtomicUsize,
        max_ticks: usize,
        on_poll: impl Fn(usize, &Path) + Send + Sync,
    ) -> usize {
        let scan_dir = scan_dir.to_path_buf();
        let holder = holder.clone();
        let deps = deps.clone();
        let mirror = std::sync::Arc::clone(mirror);
        let on_poll = std::sync::Arc::new(on_poll);
        // W-B: the baseline is seeded from the boot-state digest the
        // production boot site computes (the state the mirror was built
        // from) — the first poll compares against it. Production derives
        // the baseline from the scan's admitted outcome
        // (`digest_from_admitted`); this helper's scan dirs admit every
        // written trio, so `scan_dir_digest` and the scan-derived digest
        // agree here.
        let boot_digest = scan_dir_digest(&scan_dir);
        watch_loop_inner(
            Duration::from_millis(5),
            boot_digest,
            || {
                ticks.fetch_add(1, Ordering::Relaxed);
                let dir = scan_dir.clone();
                let on_poll = std::sync::Arc::clone(&on_poll);
                async move {
                    on_poll(ticks.load(Ordering::Relaxed), &dir);
                    scan_dir_digest(&dir)
                }
            },
            || {
                let h = holder.clone();
                let d = deps.clone();
                let m = std::sync::Arc::clone(&mirror);
                let dir = scan_dir.clone();
                async move {
                    rescans.fetch_add(1, Ordering::Relaxed);
                    hot_rebuild_test(&h, &d, &dir, &m);
                }
            },
            || ticks.load(Ordering::Relaxed) >= max_ticks,
        )
        .await
    }

    #[tokio::test]
    async fn unchanged_digest_and_missing_dir_produce_no_rescans() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // W-B: the baseline is SEEDED (boot digest), so these legs only
        // assert no-op stability when the first poll equals the seed —
        // exactly the production steady state.
        let tree = DigestPoll::Tree(serde_json::json!({"a.cap": {"capability.json": [12, 34]}}));
        let tick = AtomicUsize::new(0);
        let mut rescans = 0;
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            tree.clone(),
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
        assert_eq!(scans, 0, "unchanged digest → no scan");
        assert_eq!(rescans, 0, "no rescan action fired");

        // Missing dir → stable `Missing` digest → no rescans, no skip spam.
        tick.store(0, Ordering::Relaxed);
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            DigestPoll::Missing,
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async { DigestPoll::Missing }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 3,
        )
        .await;
        assert_eq!(scans, 0, "missing dir digest stays Missing → never rescans");
        assert_eq!(rescans, 0, "no skip spam from a missing dir");
    }

    #[tokio::test]
    async fn digest_change_triggers_exactly_one_rescan() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let baseline = DigestPoll::Tree(serde_json::json!({"a.cap": {}}));
        let changed = DigestPoll::Tree(serde_json::json!({"a.cap": {}, "b.cap": {}}));
        let tick = AtomicUsize::new(0);
        let mut rescans = 0;
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            baseline.clone(),
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async {
                    // Two baseline-identical ticks, then a change, then a
                    // no-op tick — a shared-only capture (AtomicUsize) so
                    // the future has no escaping mutable borrow and stays
                    // Send.
                    if tick.load(Ordering::Relaxed) <= 2 {
                        baseline.clone()
                    } else {
                        changed.clone()
                    }
                }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 4,
        )
        .await;
        assert_eq!(scans, 1, "seed no-op, no-op, one change rescan, no-op");
        assert_eq!(rescans, 1);
    }

    #[tokio::test]
    async fn boot_to_first_poll_change_is_detected() {
        // W-B (qc2 F-001 / qc3 W-1): the watcher is seeded with the BOOT
        // scan's digest as its initial baseline, so an on-disk change
        // between the boot scan and the FIRST poll (a complete trio written
        // in that window) triggers a rebuild — it is NOT absorbed as the
        // baseline.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let boot_state = DigestPoll::Tree(serde_json::json!({"a.cap": {}}));
        let changed_on_disk = DigestPoll::Tree(serde_json::json!({"a.cap": {}, "b.cap": {}}));
        let tick = AtomicUsize::new(0);
        let mut rescans = 0;
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            boot_state.clone(),
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                // The first poll ALREADY diverges from the boot baseline.
                async { changed_on_disk.clone() }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 2,
        )
        .await;
        assert_eq!(
            scans, 1,
            "first poll diverging from the boot baseline rescans (not absorbed)"
        );
        assert_eq!(rescans, 1);
    }

    #[tokio::test]
    async fn unreadable_poll_keeps_last_good_no_rescan() {
        // I-1: a transiently unreadable scan dir must NOT be treated as a
        // deletion. The baseline survives, no rescan fires, and the
        // registry generation (the last-good) is untouched.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let baseline = DigestPoll::Tree(serde_json::json!({"a.cap": {}}));
        let tick = AtomicUsize::new(0);
        let mut rescans = 0;
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            baseline.clone(),
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async {
                    if tick.load(Ordering::Relaxed) == 1 {
                        baseline.clone()
                    } else {
                        DigestPoll::Unreadable("Permission denied (os error 13)".to_string())
                    }
                }
            },
            || {
                rescans += 1;
                async {}
            },
            || tick.load(Ordering::Relaxed) >= 4,
        )
        .await;
        assert_eq!(scans, 0, "unreadable polls never rescan (I-1)");
        assert_eq!(rescans, 0, "last-good preserved — no swap, no wipe");
    }

    #[tokio::test]
    async fn unreadable_scan_dir_keeps_last_good_registry() {
        // I-1 end-to-end leg: after a Tree baseline, a present-but-unreadable
        // scan dir (replaced by a regular file → ENOTDIR, the same
        // non-NotFound branch as EACCES/EMFILE) keeps the last-good registry
        // generation: no rescan, no swap, no name drop, mirror untouched.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        write_capability_dir(&scan_dir, "demo.pull");
        let deps = test_deps();
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, &scan_dir, &[]);
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        let mirror: std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>> =
            std::sync::Arc::new(std::sync::Mutex::new(boot_outcome.admitted));
        let ticks = AtomicUsize::new(0);
        let rescans = AtomicUsize::new(0);

        // On the third poll (after the baseline and one no-op tick) the
        // scan dir is replaced by a present regular file: every later
        // read_dir fails with a non-NotFound error → Unreadable, and the
        // watcher must keep last-good (no rescan, no swap).
        let scans = run_real_watch(
            &scan_dir,
            &holder,
            &deps,
            &mirror,
            &rescans,
            &ticks,
            5,
            |tick, dir| {
                if tick == 3 {
                    std::fs::remove_dir_all(dir).unwrap();
                    std::fs::write(dir, b"transiently unreadable").unwrap();
                }
            },
        )
        .await;
        assert_eq!(scans, 0, "unreadable polls never rescan (I-1)");
        assert_eq!(rescans.load(Ordering::Relaxed), 0, "no swap happened");
        let live = holder.get().expect("live generation present");
        assert_eq!(
            user_cap_names(&live),
            vec!["demo.pull".to_string()],
            "last-good registry still serves the capability"
        );
        assert_eq!(
            mirror.lock().unwrap().len(),
            1,
            "last-admitted mirror untouched"
        );
    }

    #[tokio::test]
    async fn digest_ok_then_rescan_unreadable_keeps_last_good() {
        // Bugbot High (V1.176 PR wave): a successful digest poll followed
        // by a FAILED rescan (top-level non-NotFound `read_dir` error —
        // EACCES/EMFILE race) must NOT read as deletions. The scan marks
        // its outcome transient, so the merge carries every unmatched
        // last-good entry instead of dropping it as if the dir had been
        // deleted.
        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        write_capability_dir(&scan_dir, "demo.pull");
        let deps = test_deps();
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, &scan_dir, &[]);
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        let mirror: std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>> =
            std::sync::Arc::new(std::sync::Mutex::new(boot_outcome.admitted));
        assert_eq!(mirror.lock().unwrap().len(), 1, "one name admitted at boot");

        // The scan dir becomes unreadable (ENOTDIR — the same non-NotFound
        // branch as EACCES/EMFILE) AFTER the digest poll succeeded.
        std::fs::remove_dir_all(&scan_dir).unwrap();
        std::fs::write(&scan_dir, b"transiently unreadable").unwrap();

        // The digest poll already returned a Tree (digest OK) before the
        // failure; the rescan then runs against the unreadable dir. The
        // poll digest diverges from the baseline exactly once, so exactly
        // one rescan fires.
        let baseline =
            DigestPoll::Tree(serde_json::json!({"demo.pull": {"capability.json": [12, 34]}}));
        let changed = DigestPoll::Tree(
            serde_json::json!({"demo.pull": {"capability.json": [12, 34], "extra.txt": [1, 2]}}),
        );
        let tick = AtomicUsize::new(0);
        let rescans = AtomicUsize::new(0);
        let scans = watch_loop_inner(
            Duration::from_millis(1),
            baseline,
            || {
                tick.fetch_add(1, Ordering::Relaxed);
                async { changed.clone() }
            },
            || {
                rescans.fetch_add(1, Ordering::Relaxed);
                let h = holder.clone();
                let d = deps.clone();
                let m = std::sync::Arc::clone(&mirror);
                let dir = scan_dir.clone();
                async move { hot_rebuild_test(&h, &d, &dir, &m) }
            },
            || tick.load(Ordering::Relaxed) >= 2,
        )
        .await;
        assert_eq!(scans, 1, "digest change triggered exactly one rescan");
        assert_eq!(rescans.load(Ordering::Relaxed), 1);
        let live = holder.get().expect("live generation present");
        assert_eq!(
            user_cap_names(&live),
            vec!["demo.pull".to_string()],
            "failed rescan keeps last-good — no wipe"
        );
        assert_eq!(
            mirror.lock().unwrap().len(),
            1,
            "last-admitted mirror preserved across the failed rescan"
        );
    }

    #[tokio::test]
    async fn baseline_derived_from_scan_outcome_detects_between_scan_and_digest_write() {
        // Greptile P1 (V1.176 PR wave): a complete trio written between
        // the boot scan and the boot digest computation must NOT be
        // absorbed into the watcher's baseline. The baseline is derived
        // from what the scan actually ADMITTED, so the first poll sees
        // the new trio and rescans — the capability becomes dispatchable
        // without another fs change.
        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        write_capability_dir(&scan_dir, "alpha.cap");
        let deps = test_deps();
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, &scan_dir, &[]);
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        let mirror: std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>> =
            std::sync::Arc::new(std::sync::Mutex::new(boot_outcome.admitted.clone()));
        assert_eq!(
            mirror.lock().unwrap().len(),
            1,
            "alpha.cap admitted at boot"
        );

        // The between-scan-and-digest write: a complete trio lands on disk
        // AFTER the boot scan admitted alpha.cap but BEFORE the baseline
        // is derived. The scan-outcome-derived baseline must NOT contain
        // it.
        write_capability_dir(&scan_dir, "beta.cap");
        let baseline = digest_from_admitted(&boot_outcome.admitted);
        let DigestPoll::Tree(baseline_tree) = &baseline else {
            panic!("expected a Tree baseline from the admitted outcome");
        };
        assert!(
            !baseline_tree
                .as_object()
                .expect("tree is an object")
                .contains_key("beta.cap"),
            "baseline must not absorb the unscanned trio"
        );

        let ticks = AtomicUsize::new(0);
        let rescans = AtomicUsize::new(0);
        let scans = watch_loop_inner(
            Duration::from_millis(5),
            baseline,
            || {
                ticks.fetch_add(1, Ordering::Relaxed);
                let dir = scan_dir.clone();
                async move { scan_dir_digest(&dir) }
            },
            || {
                rescans.fetch_add(1, Ordering::Relaxed);
                let h = holder.clone();
                let d = deps.clone();
                let m = std::sync::Arc::clone(&mirror);
                let dir = scan_dir.clone();
                async move { hot_rebuild_test(&h, &d, &dir, &m) }
            },
            || ticks.load(Ordering::Relaxed) >= 3,
        )
        .await;
        assert_eq!(
            scans, 1,
            "first poll diverges from the scan-derived baseline → exactly one rescan"
        );
        let live = holder.get().expect("live generation present");
        assert_eq!(
            user_cap_names(&live),
            vec!["alpha.cap".to_string(), "beta.cap".to_string()],
            "the between-scan-and-digest trio is dispatchable after the first poll"
        );
    }

    #[tokio::test]
    async fn deleted_scan_dir_drops_user_cap_names() {
        // I-1 deleted leg: an absent dir after a Tree baseline is the honest
        // removal contract — exactly one rescan, and the merge drops the
        // name from the swapped registry.
        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        write_capability_dir(&scan_dir, "demo.pull");
        let deps = test_deps();
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, None, None, &scan_dir, &[]);
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        let mirror: std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>> =
            std::sync::Arc::new(std::sync::Mutex::new(boot_outcome.admitted));
        let ticks = AtomicUsize::new(0);
        let rescans = AtomicUsize::new(0);

        // The user deletes the scan dir on the third poll (after the
        // baseline and one no-op tick): Missing after Tree → exactly one
        // rescan, and the rebuild-with-merge drops the name (removal
        // contract).
        let scans = run_real_watch(
            &scan_dir,
            &holder,
            &deps,
            &mirror,
            &rescans,
            &ticks,
            5,
            |tick, dir| {
                if tick == 3 {
                    std::fs::remove_dir_all(dir).unwrap();
                }
            },
        )
        .await;
        assert_eq!(scans, 1, "deleted dir → exactly one rescan");
        let live = holder.get().expect("live generation present");
        assert!(
            user_cap_names(&live).is_empty(),
            "absent dir removes the mirrored names (removal contract)"
        );
        assert!(
            mirror.lock().unwrap().is_empty(),
            "mirror drops with the names"
        );
    }
}
