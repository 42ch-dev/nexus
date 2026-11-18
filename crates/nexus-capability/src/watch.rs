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

use crate::scan::{self, dir_file_digest, ScanOutcome};
use crate::user_capability::UserCapability;
use crate::Capability;
use crate::{CapabilityRegistry, CapabilityRuntimeDeps};
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

/// Build the watcher's initial baseline from the BOOT scan's admitted
/// outcome.
///
/// The baseline must represent exactly what the registry serves — never a
/// fresh read of the scan dir, which can include a complete trio written
/// between the boot scan and the digest computation (Greptile P1, V1.176
/// PR wave) or an EDIT to an already-admitted trio landing in the same
/// window (Greptile P1, V1.176 PR wave 2). Such a trio/edit would land in
/// the baseline but NOT in the registry, so the first poll would match
/// the baseline and skip the rebuild — the capability stays stale until
/// another fs change.
///
/// The baseline is therefore built from the admission-time per-file stat
/// snapshots captured INSIDE the scan (`ScanOutcome::admitted_stats`):
/// "baseline == registry content" holds by construction, and any post-scan
/// edit/write/deletion diverges from the baseline no matter where in the
/// boot→first-poll window it lands.
///
/// The tree shape matches [`scan_dir_digest`] (dir name → {file name →
/// [`size`, `mtime_ns`]}) so the first poll's `Value` equality comparison
/// is meaningful. A dir the scan did NOT admit (skipped trio, stray dir)
/// is deliberately absent: if it becomes a complete capability later, the
/// first poll sees the change and rescans instead of absorbing it. An
/// admitted capability without a captured snapshot (only possible for
/// hand-constructed outcomes) is likewise absent — fail-safe: the first
/// poll sees it on disk and rescans.
#[must_use]
pub fn digest_from_admitted(outcome: &ScanOutcome) -> DigestPoll {
    let mut tree = serde_json::Map::new();
    for stats in &outcome.admitted_stats {
        tree.insert(
            stats.name.clone(),
            serde_json::Value::Object(stats.files.clone()),
        );
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
    scan_dir: &Path,
    mirror: &[UserCapability],
) -> (CapabilityRegistry, ScanOutcome) {
    #[cfg(feature = "wasm")]
    {
        rebuild_registry_inner(
            CapabilityRegistry::with_runtime_deps(deps),
            scan_dir,
            mirror,
            None,
        )
    }
    #[cfg(not(feature = "wasm"))]
    {
        rebuild_registry_inner(
            CapabilityRegistry::with_runtime_deps(deps),
            scan_dir,
            mirror,
            (),
        )
    }
}

/// Engine boot arm of [`rebuild_registry_with_merge`] (feature `wasm`).
///
/// Rebuilds the builtins through
/// [`CapabilityRegistry::with_runtime_deps_and_wasm`] and forwards the
/// daemon-wide executor handles into the scan (AR-37).
#[must_use]
#[cfg(feature = "wasm")]
pub fn rebuild_registry_with_merge_with_executor(
    deps: &CapabilityRuntimeDeps,
    engine: &std::sync::Arc<nexus_wasm_host::WasmEngine>,
    module_cache: &std::sync::Arc<nexus_wasm_host::ModuleCache>,
    scan_dir: &Path,
    mirror: &[UserCapability],
) -> (CapabilityRegistry, ScanOutcome) {
    rebuild_registry_inner(
        CapabilityRegistry::with_runtime_deps_and_wasm(
            deps,
            std::sync::Arc::clone(engine),
            std::sync::Arc::clone(module_cache),
        ),
        scan_dir,
        mirror,
        Some((engine, module_cache)),
    )
}

#[must_use]
fn rebuild_registry_inner(
    mut reg: CapabilityRegistry,
    scan_dir: &Path,
    mirror: &[UserCapability],
    #[cfg(feature = "wasm")] executor: Option<(
        &std::sync::Arc<nexus_wasm_host::WasmEngine>,
        &std::sync::Arc<nexus_wasm_host::ModuleCache>,
    )>,
    #[cfg(not(feature = "wasm"))] _executor: (),
) -> (CapabilityRegistry, ScanOutcome) {
    #[cfg(not(feature = "wasm"))]
    let _ = _executor;
    // Collision ownership derives from THE DECLARATION (C3 contract), never
    // from constructed handler objects: the watcher rebuild gates against
    // the preset catalog's canonical names, so a builtin declared without a
    // handler object still cannot be claimed by a user capability.
    let builtin_names: HashSet<&str> = nexus_preset::capability_catalog::builtin_names().collect();
    #[cfg(feature = "wasm")]
    let scan = match executor {
        Some((engine, module_cache)) => scan::scan_user_capabilities_with_executor(
            scan_dir,
            &builtin_names,
            Some(engine),
            Some(module_cache),
        ),
        None => scan::scan_user_capabilities(scan_dir, &builtin_names),
    };
    #[cfg(not(feature = "wasm"))]
    let scan = scan::scan_user_capabilities(scan_dir, &builtin_names);
    let merged = merge_user_caps(&scan, mirror);
    let outcome = ScanOutcome {
        admitted: merged.clone(),
        skipped: scan.skipped,
        // W-3 (qc3 S-4): a scan interrupted by a per-entry read error is
        // transient — the merge keeps last-good for every name it did not
        // re-admit, so an incomplete scan never reads as deletions.
        transient: scan.transient,
        // V1.176 PR wave 2: admission-time stats for the entries THIS scan
        // admitted (carried last-good entries have no fresh stats — theirs
        // were captured at their own admission). The boot site derives the
        // watcher baseline only from the boot outcome, where admitted ==
        // scan-admitted, so the baseline is complete by construction.
        admitted_stats: scan.admitted_stats,
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
/// `WatcherGuard` (`nexus-orchestration::boot`) (abort-on-drop).
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
    use crate::test_support::write_capability_dir;
    use crate::CapabilityOrigin;
    use crate::CapabilityRegistryHolder;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_deps() -> CapabilityRuntimeDeps {
        CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
            workspace_executor: None,
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

    /// Input schemas that differ BETWEEN generations of the same capability
    /// name, so the catalog surface itself discriminates which row a
    /// generation resolves (the shared writer's fixed schemas would make two
    /// generations indistinguishable).
    const GEN_ONE_SCHEMA: &str = r#"{"type":"object","generation":1}"#;
    const GEN_TWO_SCHEMA: &str = r#"{"type":"object","generation":2}"#;

    /// Write a hash-consistent trio whose descriptor declares `schema` — the
    /// generation-discriminating fixture used by the reservation test.
    fn write_trio_with_schema(root: &Path, name: &str, schema: &str, wasm: &[u8]) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let sha = crate::test_support::write_module_pair_with_bytes(&dir, wasm);
        let descriptor = serde_json::json!({
            "name": name,
            "inputSchema": schema,
            "outputSchema": r#"{"type":"object"}"#,
            "wasm": { "moduleId": "basic-combat", "wasmSha256": sha },
        });
        std::fs::write(
            dir.join("capability.json"),
            serde_json::to_string(&descriptor).unwrap(),
        )
        .unwrap();
    }

    /// User-capability names a registry CARRIES (origin-filtered, sorted) —
    /// the declaration set, not the executable lookup: a user capability
    /// admitted without a binding is carried (and reserved) even though
    /// `get`/`iter` do not yield it (C3 contract).
    fn user_cap_names(registry: &CapabilityRegistry) -> Vec<String> {
        let mut names: Vec<String> = registry
            .capabilities
            .iter()
            .map(std::convert::AsRef::as_ref)
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
        let (_, boot_outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        let mirror = boot_outcome.admitted;

        // Break demo.pull's trio (invalid descriptor) and delete gone.cap.
        std::fs::remove_dir_all(tmp.path().join("gone.cap")).unwrap();
        std::fs::write(tmp.path().join("demo.pull/capability.json"), "{ not json").unwrap();

        let (_, hot_outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &mirror);
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
        let (_, boot_outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        let mirror = boot_outcome.admitted;
        // Rebuild unchanged: the name is admitted again, and the merge must
        // NOT duplicate it (new entry wins, mirror copy dropped).
        let (reg, hot_outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &mirror);
        assert_eq!(
            hot_outcome.skipped,
            [] as [crate::scan::SkippedCapability; 0]
        );
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
        let (_, boot_outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        let mirror = boot_outcome.admitted;
        assert_eq!(mirror.len(), 2, "both names admitted at boot");

        // The interrupted hot scan re-admits only demo.pull and is marked
        // transient — gone.cap was never observed, so it must NOT be
        // dropped as a deletion.
        let scan = ScanOutcome {
            admitted: vec![mirror[0].clone()],
            skipped: Vec::new(),
            transient: true,
            admitted_stats: Vec::new(),
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
            admitted_stats: Vec::new(),
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
        let (hot_reg, hot_outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);

        assert_eq!(
            boot_outcome.skipped,
            [] as [crate::scan::SkippedCapability; 0]
        );
        assert_eq!(
            hot_outcome.skipped,
            [] as [crate::scan::SkippedCapability; 0]
        );
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
        let (reg, outcome) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        let mirror = outcome.admitted.clone();
        assert!(!mirror.is_empty());

        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        // Dispatch-through-snapshot: clone the Arc exactly as the spine does,
        // then break the trio on disk and swap.
        let held = holder.get().expect("registry present before swap");
        std::fs::write(tmp.path().join("demo.pull/capability.json"), "{ not json").unwrap();
        let (reg2, outcome2) = rebuild_registry_with_merge(&deps, tmp.path(), &mirror);
        assert_eq!(
            outcome2.skipped.len(),
            1,
            "one named skip for the broken trio"
        );
        holder.swap(std::sync::Arc::new(reg2));

        // The pre-swap snapshot still carries last-good for the name and the
        // invocation completes honestly against the held snapshot's own row
        // (engine-less arm → the AR-44 stub contract; the executable lookup
        // misses because the row has no binding in this build — C3 contract).
        assert!(
            held.get("demo.pull").is_none() && held.is_reserved_only("demo.pull"),
            "the snapshot keeps the name: describable, reserved, not executable"
        );
        let held_row = held
            .capabilities
            .iter()
            .map(std::convert::AsRef::as_ref)
            .find(|cap| cap.name() == "demo.pull")
            .expect("snapshot keeps the row");
        let err = held_row.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, crate::CapabilityError::WorkerUnavailable),
            "call completes honestly against the held snapshot: {err:?}"
        );
        // And the swapped registry carries last-good for the same name.
        let live = holder.get().expect("swapped registry present");
        assert!(
            live.is_reserved_only("demo.pull"),
            "last-good admission survives the failed replacement (PL-9)"
        );
    }

    // -------------------------------------------------------------------------
    // Unbound compute name stays reserved across hot swaps (C3, corrected)
    // -------------------------------------------------------------------------

    /// C3 missing-edge case, in the CORRECTED contract shape (I-000544):
    /// `get`/`iter` are real executable lookups, so a row without a binding
    /// is NOT yielded; the name survives in `reserved_names` (the full
    /// projection) and the metadata surface; workflow dispatch of the name
    /// refuses `WorkerUnavailable` BEFORE any effect.
    ///
    /// 1. Hot-admit a compute-capable user capability without a binding:
    ///    executable lookup misses, the name joins the live reservation,
    ///    known metadata still resolves, and the stored row's invocation is
    ///    the typed worker refusal (what dispatch produces, via
    ///    `is_reserved_only`, before any effect). The builtin compute name is
    ///    reserved the same way.
    /// 2. A hot swap that REPLACES the entry keeps the name reserved; a
    ///    retained pre-swap generation still resolves only its OWN row
    ///    through the metadata surface — never the swapped-in entry.
    /// 3. A hot swap that DROPS the row leaves the name reserved: metadata
    ///    gone, reservation intact.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // one generation-sequence narrative; splitting hides the swap story
    async fn unbound_compute_name_remains_reserved_after_hot_swap() {
        use nexus_preset::capability_catalog::CapabilityCatalog;

        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps();

        // Boot with an empty scan dir: no user name is reserved yet.
        let (reg0, outcome0) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        assert!(
            outcome0.admitted.is_empty(),
            "boot admits nothing from an empty dir"
        );
        let holder = CapabilityRegistryHolder::with_registry(std::sync::Arc::new(reg0));
        assert!(!holder.reserved_names().any(|n| n == "demo.compute"));

        // 1. Hot-admit a compute-capable user capability with no binding.
        write_trio_with_schema(
            tmp.path(),
            "demo.compute",
            GEN_ONE_SCHEMA,
            b"gen one module bytes",
        );
        let (reg1, outcome1) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        assert_eq!(
            outcome1.admitted.len(),
            1,
            "the unbound compute capability admits (metadata gates are binding-independent)"
        );
        holder.swap(std::sync::Arc::new(reg1));
        assert!(
            holder.reserved_names().any(|n| n == "demo.compute"),
            "the hot-admitted user name joins the actual live reservation"
        );
        assert!(
            holder.reserved_names().any(|n| n == "narrative.compute"),
            "the builtin compute name is reserved too, also without a binding here"
        );

        // CORRECTED shape: the executable lookup misses the unbound row, and
        // the name exists only as a reservation — the dispatch discriminator
        // (a workflow dispatch of this name returns WorkerUnavailable BEFORE
        // any effect; enforced at the orchestration dispatch sites).
        let gen1 = holder.get().expect("live generation with the admitted row");
        assert!(
            gen1.get("demo.compute").is_none(),
            "get is runnable-only: an unbound row is not yielded"
        );
        assert!(
            gen1.is_reserved_only("demo.compute"),
            "the name is known-but-unbound: dispatch refuses WorkerUnavailable before any effect"
        );
        assert!(
            gen1.iter().all(|cap| cap.name() != "demo.compute"),
            "iter is runnable-only too"
        );

        // Metadata does not shrink: known metadata resolves WITHOUT the
        // executor (the CapabilityCatalog reads the descriptor surface).
        assert_eq!(
            CapabilityCatalog::input_schema(&gen1, "demo.compute"),
            Some(GEN_ONE_SCHEMA),
            "known metadata resolves without constructing an executor"
        );

        // Effect evidence at the row level: the stored row's invocation is
        // the typed worker refusal — exactly the refusal workflow dispatch
        // produces for this name, before any effect.
        let stored_row = gen1
            .capabilities
            .iter()
            .map(std::convert::AsRef::as_ref)
            .find(|cap| cap.name() == "demo.compute")
            .expect("the reserved-only row is still carried");
        let err = stored_row.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, crate::CapabilityError::WorkerUnavailable),
            "the unbound invocation refuses with WorkerUnavailable, got {err:?}"
        );

        // 2. Replace the entry and hot-swap it in; retain the pre-swap
        //    generation exactly as an in-flight dispatch would hold it.
        let gen1_retained = holder.get().expect("generation before the replacement");
        write_trio_with_schema(
            tmp.path(),
            "demo.compute",
            GEN_TWO_SCHEMA,
            b"gen two module bytes",
        );
        let (reg2, _) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        holder.swap(std::sync::Arc::new(reg2));
        assert!(
            holder.reserved_names().any(|n| n == "demo.compute"),
            "the replacement keeps the name reserved"
        );
        let gen2 = holder.get().expect("swapped-in generation");
        assert_eq!(
            CapabilityCatalog::input_schema(&gen2, "demo.compute"),
            Some(GEN_TWO_SCHEMA),
            "the swapped-in metadata is the new generation's"
        );
        // Namespace evidence: the retained generation's metadata surface
        // resolves only its OWN row — the swapped-in entry is unreachable
        // through it.
        assert_eq!(
            CapabilityCatalog::input_schema(&gen1_retained, "demo.compute"),
            Some(GEN_ONE_SCHEMA),
            "a retained old generation cannot dispatch through the swapped-in entry"
        );

        // 3. Drop the row (dir deleted) and hot-swap the rebuild.
        std::fs::remove_dir_all(tmp.path().join("demo.compute")).unwrap();
        let (reg3, outcome3) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        assert!(
            outcome3.admitted.iter().all(|c| c.name() != "demo.compute"),
            "the dropped dir admits nothing"
        );
        holder.swap(std::sync::Arc::new(reg3));
        let live = holder.get().expect("live generation after the drop");
        assert!(
            live.get("demo.compute").is_none(),
            "the live generation no longer serves the dropped row"
        );
        assert_eq!(
            CapabilityCatalog::input_schema(&live, "demo.compute"),
            None,
            "the dropped row's metadata is gone"
        );
        assert!(
            holder.reserved_names().any(|n| n == "demo.compute"),
            "the compute name remains reserved after the hot swap dropped its row"
        );
    }

    /// INVARIANT PIN, not a regression test for the order inside `swap` (C3
    /// fix round 2 relabel): this case drives the two half-steps manually in
    /// the safe order, so changing only the composition inside `swap` cannot
    /// make it fail. What it pins is the half-step SEMANTICS the safe
    /// composition relies on: `reserve_generation` alone joins the live
    /// reservation before publication, and the intermediate state (name
    /// reserved, row not yet live) is observable. Discrimination against a
    /// bad `swap` order is carried by the publication-path invariant —
    /// `publish_refuses_an_unreserved_generation` pins the check, and the
    /// existing `unbound_compute_name_remains_reserved_after_hot_swap` trips
    /// it on every swap under a reverted order (mutation-verified).
    #[test]
    fn swap_reservation_precedes_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps();
        let (reg0, outcome0) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        assert!(
            outcome0.admitted.is_empty(),
            "boot admits nothing from an empty dir"
        );
        let holder = CapabilityRegistryHolder::with_registry(std::sync::Arc::new(reg0));
        assert!(!holder.reserved_names().any(|n| n == "demo.compute"));

        write_capability_dir(tmp.path(), "demo.compute");
        let (reg1, _) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);

        // Half-step 1: reserve only — the name joins the live reservation.
        holder.reserve_generation(&reg1);
        assert!(
            holder.reserved_names().any(|n| n == "demo.compute"),
            "the incoming name is reserved BEFORE the generation is published"
        );
        assert!(
            holder
                .get()
                .expect("previous generation still live")
                .get("demo.compute")
                .is_none(),
            "publication has not happened while the reservation is already in place"
        );

        // Half-step 2: publish — the row is carried (reserved-only: the
        // fixture admits without a binding) and the reservation already holds.
        holder.publish_generation(std::sync::Arc::new(reg1));
        let live = holder.get().expect("published generation");
        assert!(
            live.is_reserved_only("demo.compute"),
            "the published row is carried and describable (no binding in this build)"
        );
        assert!(holder.reserved_names().any(|n| n == "demo.compute"));
    }

    /// INVARIANT PIN (C3 fix round 2 relabel): the publication path itself
    /// enforces the ordering invariant — publishing a generation whose names
    /// are NOT yet reserved fires the debug invariant. This case calls the
    /// publish half directly, so it passes regardless of the order inside
    /// `swap`; what it pins is that the CHECK EXISTS and fires. The order
    /// discrimination is carried by `unbound_compute_name_remains_reserved_
    /// after_hot_swap`, which runs full swaps and trips this invariant under
    /// a reverted order (mutation-verified).
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "reserved before publication")]
    fn publish_refuses_an_unreserved_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps();
        let (reg0, _) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        let holder = CapabilityRegistryHolder::with_registry(std::sync::Arc::new(reg0));
        write_capability_dir(tmp.path(), "demo.compute");
        let (reg1, _) = rebuild_registry_with_merge(&deps, tmp.path(), &[]);
        // Publish WITHOUT reserving first — the invariant must fire.
        holder.publish_generation(std::sync::Arc::new(reg1));
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
        let (reg, outcome) = rebuild_registry_with_merge(deps, scan_dir, &mirror);
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
        // the baseline from admission-time stat snapshots captured inside
        // the scan (`digest_from_admitted`); this helper's scan dirs admit
        // every written trio, so `scan_dir_digest` and the scan-derived
        // digest agree here.
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
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, &scan_dir, &[]);
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
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, &scan_dir, &[]);
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
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, &scan_dir, &[]);
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
        let baseline = digest_from_admitted(&boot_outcome);
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

    /// Rewrite `<root>/<name>/` with a NEW consistent trio whose wasm
    /// differs in size from the fixture's constant bytes — the AR-39 sha
    /// pairing still passes (manifest + descriptor are rewritten to
    /// match), but the digest diverges from any pre-edit baseline.
    fn rewrite_capability_dir(root: &Path, name: &str) {
        let dir = root.join(name);
        let sha = crate::test_support::write_module_pair_with_bytes(
            &dir,
            b"fake module bytes, now longer",
        );
        std::fs::write(
            dir.join("capability.json"),
            crate::test_support::descriptor_json(name, &sha),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn baseline_from_admission_stats_detects_edit_between_scan_and_digest() {
        // Greptile P1 (V1.176 PR wave 2): an EDIT to an admitted trio
        // between the boot scan and the baseline derivation must NOT be
        // absorbed into the baseline. The baseline is built from
        // admission-time stat snapshots captured INSIDE the scan, so the
        // first poll sees the edited disk state and rescans — the stale
        // pre-edit capability is never served past the first poll.
        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        write_capability_dir(&scan_dir, "alpha.cap");
        let deps = test_deps();
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, &scan_dir, &[]);
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        let mirror: std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>> =
            std::sync::Arc::new(std::sync::Mutex::new(boot_outcome.admitted.clone()));
        assert_eq!(
            mirror.lock().unwrap().len(),
            1,
            "alpha.cap admitted at boot"
        );

        // The between-scan-and-digest EDIT: the trio is rewritten with new
        // content (a longer wasm + matching shas) AFTER the boot scan
        // admitted it but BEFORE the baseline is derived. The
        // admission-time baseline must reflect the PRE-edit state, so the
        // first poll diverges.
        rewrite_capability_dir(&scan_dir, "alpha.cap");
        let baseline = digest_from_admitted(&boot_outcome);
        let DigestPoll::Tree(baseline_tree) = &baseline else {
            panic!("expected a Tree baseline from the admitted outcome");
        };
        let wasm_entry = baseline_tree
            .as_object()
            .expect("tree is an object")
            .get("alpha.cap")
            .expect("admitted cap in baseline")
            .as_object()
            .expect("cap leaf is an object")
            .get("basic-combat.wasm")
            .expect("wasm in the baseline leaf");
        let wasm_size = wasm_entry.as_array().expect("wasm entry is an array")[0]
            .as_u64()
            .expect("wasm size is a number");
        assert_eq!(
            wasm_size,
            b"fake module bytes".len() as u64,
            "baseline carries the PRE-edit wasm size (admission-time ground truth)"
        );
        let DigestPoll::Tree(disk) = scan_dir_digest(&scan_dir) else {
            panic!("expected a Tree digest for the present dir");
        };
        assert_ne!(
            baseline_tree, &disk,
            "the edited disk state diverges from the admission-time baseline"
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
            "first poll diverges from the admission-time baseline → exactly one rescan"
        );
        // The rebuilt registry serves the EDITED content — the stale
        // pre-edit capability is not served past the first poll.
        let new_sha = crate::test_support::sha256_hex(b"fake module bytes, now longer");
        let live_mirror = mirror.lock().unwrap();
        assert_eq!(
            live_mirror[0].wasm_sha256(),
            new_sha,
            "stale pre-edit capability is not served after the first poll"
        );
    }

    #[tokio::test]
    async fn admission_time_baseline_unchanged_scan_produces_no_rescans() {
        // V1.176 PR wave 2: with the baseline derived from admission-time
        // stats, an UNCHANGED scan dir keeps the no-op contract (AR-91 #6
        // / AR-95 #5): the first poll matches the baseline and no rebuild
        // fires.
        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        write_capability_dir(&scan_dir, "alpha.cap");
        let deps = test_deps();
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, &scan_dir, &[]);
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(reg));
        let mirror: std::sync::Arc<std::sync::Mutex<Vec<UserCapability>>> =
            std::sync::Arc::new(std::sync::Mutex::new(boot_outcome.admitted.clone()));
        let baseline = digest_from_admitted(&boot_outcome);
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
        assert_eq!(scans, 0, "unchanged scan dir → no rescan (no-op preserved)");
        assert_eq!(rescans.load(Ordering::Relaxed), 0, "no rebuild fired");
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
        let (reg, boot_outcome) = rebuild_registry_with_merge(&deps, &scan_dir, &[]);
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
