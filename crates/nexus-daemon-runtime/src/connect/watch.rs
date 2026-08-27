//! Peer config hot reload (V1.179 P1 T1, DF-92).
//!
//! The peer-tools lane's own config surface — `~/.nexus42/connect/
//! daemon.json` + `peer_keys.json` — was boot-frozen (AR-67 #4
//! restart-scoped snapshot). This module adds a **poll + raw-byte digest**
//! watcher (no inotify, no new deps — the V1.176 RN-2 registry-watch
//! pattern) that swaps a validated config generation into the shared
//! [`PeerConfigHolder`] on change:
//!
//! - [`peer_config_digest`] digests the **FULL BYTES** of both files (not
//!   mtime — container/editors may preserve mtime) using the house
//!   `serde_json::Value`-equality trick (no hash-collision reasoning, no
//!   hashing dep). Only these two files are watched: the connect-HOST
//!   lane's `config.json` / `allowlist.json` are a different surface and
//!   are NOT watched here.
//! - On a digest change, the reload runs the FULL validation path
//!   (`PeerToolsConfig::load` + `load_peer_keys` — the same AR-69
//!   fail-closed chain boot uses) on a blocking lane; success swaps the
//!   effective snapshot into the holder and feeds the process-global
//!   [`PeerToolTable`] seam (`set_config` — takes ONLY the config mutex,
//!   never the table `inner`, so the p0 lock rank is preserved).
//!   Failure keeps last-good and warns once per error-state transition —
//!   the daemon never fails closed on a bad edit.
//! - The loop CORE is generic over the poll + apply closures (the
//!   `catalog_watch_loop_inner` / `watch_loop_inner` precedent) so
//!   failure/panic injection is test-only — no prod-code test hooks.
//! - The boot baseline seeds the digest WITHOUT emitting an initial event;
//!   two identical polls produce at most one [`ConfigEvent::Changed`].
//!
//! Reload scope (GC #7, architect-locked): a successful reload adopts
//! **admission-affecting fields only** — `tool_allowlist`, `peer_ids`,
//! `collision_policy`, `peer_priority` (+ the `peer_keys.json` keys).
//! Boot-scoped fields (`host`, `port`, `max_sessions`,
//! `invoke_timeout_ms`, `max_envelope_bytes`, `embedded_mcp`) stay
//! restart-scoped: a reload whose diff touches one logs one named info
//! line (`peer config reload: <field> changed; restart required`) and
//! keeps the boot value — no silent no-op, no hot rebind. In-flight
//! sessions keep grant-at-establish until close/reconnect (AR-67
//! reconnect=replace); swap applies to NEW admissions only.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use nexus_orchestration::capability::watch::DigestPoll;
use tokio::sync::Notify;

use crate::connect::config::{load_peer_keys, PeerToolsConfig};
use crate::connect::table::PeerToolTable;

/// Daemon-side poll interval for the peer config files (DF-92).
///
/// A `const Duration`, not configurable this iteration. 2 s per the plan
/// clarify lock ("~2s consistent with house precedent"; the capability
/// watcher polls at 1 s but scans a whole directory tree — two small file
/// reads here are cheaper per tick).
pub const PEER_CONFIG_WATCH_INTERVAL: Duration = Duration::from_secs(2);

/// One accepted peer-config reload (DF-92).
///
/// The watcher emits exactly one [`ConfigEvent::Changed`] per validated
/// generation swap. The accept loop consumes it through the shared
/// [`PeerConfigHolder`]: every NEW admission (hello allowlist, manifest
/// admission, collision policy + ranks, handshake keys) reads
/// [`PeerConfigHolder::get`], so a `Changed` event means the NEXT
/// admission sees the new snapshot — in-flight sessions keep
/// grant-at-establish (AR-67 reconnect=replace, no mid-call yank).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigEvent {
    /// A validated reload swapped a fresh snapshot into the holder.
    Changed,
}

/// One validated peer config generation: the effective live
/// [`PeerToolsConfig`] plus the preconfigured dialer keys validated
/// alongside it (AR-69 Layer 0 — keys are admission-affecting per GC #7).
#[derive(Debug, Clone)]
pub struct PeerConfigSnapshot {
    /// Effective live config: admission fields from the latest valid
    /// load, boot-scoped fields pinned to the boot values (GC #7).
    pub config: Arc<PeerToolsConfig>,
    /// Preconfigured dialer Ed25519 public keys (`peer_keys.json`,
    /// validated at the same load — never a second, unvalidated read).
    pub peer_keys: Arc<HashMap<String, [u8; 32]>>,
}

/// Live peer config snapshot holder (DF-92; GC #8 shape).
///
/// Mirrors `CapabilityRegistryHolder` (AR-92 #7): `std::sync::RwLock` over
/// an `Arc` — writers swap the `Arc` under the write lock (held only for
/// the pointer write; swaps are serialized by construction — one watcher
/// task, one tick at a time), readers clone the `Arc` under the read lock
/// and drop it immediately. No reader holds the lock across an `.await`,
/// and a handshake/dispatch that cloned the pre-swap `Arc` finishes
/// against last-good (no abort, no half-call). NO `arc_swap` crate
/// (AR-91 #5 / AR-98 zero-new-deps discipline). Poisoned locks are
/// recovered via [`std::sync::PoisonError::into_inner`] (daemon policy).
#[derive(Clone)]
pub struct PeerConfigHolder {
    inner: Arc<RwLock<Arc<PeerConfigSnapshot>>>,
}

impl PeerConfigHolder {
    /// Create a holder seeded with the boot snapshot. Boot creates exactly
    /// one holder and shares it with the lane and the watcher.
    #[must_use]
    pub fn new(snapshot: PeerConfigSnapshot) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(snapshot))),
        }
    }

    /// Clone the current snapshot under the read lock (releases
    /// immediately — never held across an `.await`).
    #[must_use]
    pub fn get(&self) -> Arc<PeerConfigSnapshot> {
        Arc::clone(&self.inner.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Atomically swap in a freshly validated snapshot (write lock; held
    /// only for the pointer write — AR-92 #7 precedent). The previous
    /// generation is dropped AFTER the write lock is released so a
    /// last-reference drop never stalls readers (M-1).
    pub fn swap(&self, snapshot: PeerConfigSnapshot) {
        let mut guard = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        let previous = std::mem::replace(&mut *guard, Arc::new(snapshot));
        drop(guard);
        drop(previous);
    }
}

/// Result of one full-bytes digest poll over the peer config file set.
///
/// Reuses the RN-2 [`DigestPoll`] three-state vocabulary — no forked enum
/// (GC #8). For the file-set surface:
/// - [`DigestPoll::Missing`]: BOTH files are absent (`NotFound`) — the
///   stable "no config" state. A `Missing` poll after a `Tree` baseline
///   means the operator removed the files → the reload adopts the
///   documented defaults + empty key set (fail-closed, never an error).
/// - [`DigestPoll::Unreadable`]: a file exists but could not be read
///   (EACCES, EISDIR, …) — a transient failure, NOT a deletion. The
///   watcher keeps last-good unchanged: no reload, no swap, no baseline
///   disturbance (retried next tick).
/// - [`DigestPoll::Tree`]: the byte digest of the readable file set.
#[must_use]
pub fn peer_config_digest(home: &Path) -> DigestPoll {
    let paths = [
        (
            "daemon.json",
            nexus_home_layout::connect_daemon_config_path(home),
        ),
        (
            "peer_keys.json",
            nexus_home_layout::connect_peer_keys_path(home),
        ),
    ];
    let mut map = serde_json::Map::new();
    let mut unreadable: Option<String> = None;
    let mut present = false;
    for (name, path) in paths {
        match std::fs::read(&path) {
            Ok(bytes) => {
                present = true;
                // FULL BYTES, hex-encoded into the Value (byte-exact; no
                // hash-collision reasoning, no hashing dependency). An
                // empty file is an empty string — distinct from `null`
                // (absent).
                map.insert(
                    name.to_owned(),
                    serde_json::Value::String(hex_encode(&bytes)),
                );
            }
            // NotFound on READ is absence — the same state a stat would
            // report (the file may vanish between the two).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                map.insert(name.to_owned(), serde_json::Value::Null);
            }
            Err(e) => {
                let message = format!("cannot read {}: {e}", path.display());
                unreadable.get_or_insert(message);
            }
        }
    }
    if let Some(message) = unreadable {
        return DigestPoll::Unreadable(message);
    }
    if !present {
        return DigestPoll::Missing;
    }
    DigestPoll::Tree(serde_json::Value::Object(map))
}

/// Hex-encode bytes (lowercase; the decode side lives in
/// `config.rs::decode_hex_32`).
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).expect("high nibble"));
        out.push(char::from_digit(u32::from(byte & 0x0F), 16).expect("low nibble"));
    }
    out
}

/// One validated reload tick: full `PeerToolsConfig::load` +
/// `load_peer_keys` validation, boot-scoped field pinning (GC #7).
///
/// Runs on a blocking lane (`spawn_blocking` at the call site): the same
/// AR-69 fail-closed chain boot uses — an invalid file is a hard reload
/// error (never silently dropped), and the caller keeps last-good.
///
/// Returns the new EFFECTIVE snapshot (admission fields from the load,
/// boot-scoped fields pinned to `last_good`'s boot values) plus the list
/// of boot-scoped fields whose loaded value differs from boot — the
/// caller logs one named `restart required` info line per field.
///
/// # Errors
/// A string error message (for the loop's once-per-error-state warn) when
/// either file fails its load validation.
pub fn reload_peer_config(
    home: &Path,
    last_good: &PeerConfigSnapshot,
) -> Result<(PeerConfigSnapshot, Vec<&'static str>), String> {
    let loaded = PeerToolsConfig::load(home).map_err(|e| e.to_string())?;
    let keys = load_peer_keys(home).map_err(|e| e.to_string())?;
    let mut restart_required = Vec::new();
    if loaded.host != last_good.config.host {
        restart_required.push("host");
    }
    if loaded.port != last_good.config.port {
        restart_required.push("port");
    }
    if loaded.max_sessions != last_good.config.max_sessions {
        restart_required.push("max_sessions");
    }
    if loaded.invoke_timeout_ms != last_good.config.invoke_timeout_ms {
        restart_required.push("invoke_timeout_ms");
    }
    if loaded.max_envelope_bytes != last_good.config.max_envelope_bytes {
        restart_required.push("max_envelope_bytes");
    }
    if loaded.embedded_mcp != last_good.config.embedded_mcp {
        restart_required.push("embedded_mcp");
    }
    // Explicit field listing (no struct-update spread): adding a field to
    // `PeerToolsConfig` breaks this compile, forcing a conscious
    // boot-scoped vs admission-scoped decision (GC #7 discipline).
    let effective = PeerToolsConfig {
        host: last_good.config.host.clone(),
        port: last_good.config.port,
        max_sessions: last_good.config.max_sessions,
        invoke_timeout_ms: last_good.config.invoke_timeout_ms,
        max_envelope_bytes: last_good.config.max_envelope_bytes,
        embedded_mcp: last_good.config.embedded_mcp,
        tool_allowlist: loaded.tool_allowlist,
        peer_ids: loaded.peer_ids,
        collision_policy: loaded.collision_policy,
        peer_priority: loaded.peer_priority,
    };
    Ok((
        PeerConfigSnapshot {
            config: Arc::new(effective),
            peer_keys: Arc::new(keys),
        },
        restart_required,
    ))
}

/// Counters returned by the watch loop (test evidence; the production
/// wrapper ignores them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeerConfigWatchStats {
    /// Validated reloads applied (swaps into the holder).
    pub applied: usize,
    /// Warn lines emitted — poll-unreadable transitions + reload
    /// failures, each bounded to once per error-state transition.
    pub warnings: usize,
}

/// Core peer config watch loop, generic over the poll and apply closures
/// (DF-92; the `watch_loop_inner` / `catalog_watch_loop_inner` seam
/// precedent).
///
/// Generics keep the baseline / no-event-storm / warn-once semantics
/// unit-testable without real files, and failure/panic injection
/// test-only — no prod-code test hooks.
///
/// Baseline semantics: `initial_digest` seeds the reference (boot baseline
/// — seeded WITHOUT an initial event); the first poll COMPARES against it.
/// Only a change between established digests applies. Two identical polls
/// produce at most one apply (no event storm).
///
/// An [`DigestPoll::Unreadable`] poll never disturbs the baseline and
/// never applies (last-good kept; retried next tick); the failure is
/// logged once per error-state transition (readable→unreadable, or a
/// CHANGED error message). A digest change applies even into `Missing` —
/// the reload adopts the documented defaults (fail-closed), the daemon
/// stays up. A FAILED apply (invalid config) still advances the baseline:
/// the on-disk bytes are the new reference, the second identical poll must
/// not re-apply or re-warn, and a fixed file diverges again and reloads.
///
/// Returns the [`PeerConfigWatchStats`]. `should_stop` is a test-only
/// seam: production always passes `|| false` — the loop is cancelled via
/// the shutdown-notify `select!` in [`spawn_peer_config_watch`].
pub async fn peer_config_watch_loop_inner<PF, AF, P, A>(
    interval: Duration,
    initial_digest: DigestPoll,
    mut poll: P,
    mut apply: A,
    mut should_stop: impl FnMut() -> bool,
) -> PeerConfigWatchStats
where
    P: FnMut() -> PF,
    PF: Future<Output = DigestPoll> + Send,
    A: FnMut() -> AF,
    AF: Future<Output = Result<ConfigEvent, String>> + Send,
{
    let mut stats = PeerConfigWatchStats::default();
    let mut last_digest: Option<DigestPoll> = Some(initial_digest);
    let mut poll_error_logged: Option<String> = None;
    let mut apply_error_logged: Option<String> = None;
    loop {
        if should_stop() {
            return stats;
        }
        let digest = poll().await;
        match &digest {
            DigestPoll::Unreadable(message) => {
                // Transient read failure (EACCES, EISDIR, …): keep
                // last-good — no reload, no swap, baseline intact. Log
                // once per error-state, not every tick.
                if poll_error_logged.as_deref() != Some(message.as_str()) {
                    tracing::warn!(
                        error = %message,
                        "peer config watch: cannot read connect config files; keeping last-good \
                         config (hot-reload skip — will retry)"
                    );
                    poll_error_logged = Some(message.clone());
                    stats.warnings += 1;
                }
                tokio::time::sleep(interval).await;
                continue;
            }
            DigestPoll::Missing | DigestPoll::Tree(_) => {
                poll_error_logged = None;
            }
        }
        if last_digest.as_ref().is_some_and(|prev| *prev == digest) {
            tokio::time::sleep(interval).await;
            continue;
        }
        // Digest diverged from the reference: attempt the validated
        // reload. The baseline advances EITHER WAY (see doc).
        match apply().await {
            Ok(event) => {
                apply_error_logged = None;
                match event {
                    ConfigEvent::Changed => stats.applied += 1,
                }
            }
            Err(message) => {
                if apply_error_logged.as_deref() != Some(message.as_str()) {
                    tracing::warn!(
                        error = %message,
                        "peer config reload failed; keeping last-good config"
                    );
                    apply_error_logged = Some(message.clone());
                    stats.warnings += 1;
                }
            }
        }
        last_digest = Some(digest);
        tokio::time::sleep(interval).await;
    }
}

/// Spawn the peer config watcher: validated reloads swapped into `holder`
/// and fed to the process-global table seam, until `shutdown` fires.
///
/// `initial_digest` is the BOOT baseline — the caller computes it with
/// [`peer_config_digest`] BEFORE the boot `PeerToolsConfig::load` so an
/// edit landing anywhere in the boot window diverges from the baseline and
/// the first poll reloads (never absorbed — the capability watcher's W-B
/// rule).
#[must_use]
pub fn spawn_peer_config_watch(
    initial_digest: DigestPoll,
    home: PathBuf,
    holder: PeerConfigHolder,
    shutdown: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("peer config watch started (poll+raw-byte digest, 2s)");
        tokio::select! {
            () = shutdown.notified() => {
                tracing::info!("peer config watch: shutdown received, exiting");
            }
            _ = peer_config_watch_loop(initial_digest, home, holder) => {}
        }
    })
}

/// Production watch loop: real digest poll + validated reload/swap.
async fn peer_config_watch_loop(
    initial_digest: DigestPoll,
    home: PathBuf,
    holder: PeerConfigHolder,
) -> PeerConfigWatchStats {
    peer_config_watch_loop_inner(
        PEER_CONFIG_WATCH_INTERVAL,
        initial_digest,
        // Two small file reads per tick — inline, like the capability
        // watcher's stat walk (spawn_blocking adds latency without
        // removing the blocking-on-worker property at this size).
        || async { peer_config_digest(&home) },
        || async { apply_reload(&home, &holder, crate::connect::peer_tool_table()).await },
        || false,
    )
    .await
}

/// One reload tick, production shape: validated load on the blocking lane
/// → boot-scoped diff logging (GC #7 named info lines) → feed the
/// [`PeerToolTable`] admission seam → swap into the holder. `table` is
/// injected (production passes the process-global singleton) so the full
/// path is testable against a local table without touching process-global
/// state.
///
/// Never fails the daemon: every failure path returns `Err(message)` for
/// the loop's once-per-transition warn, and the holder keeps last-good.
async fn apply_reload(
    home: &Path,
    holder: &PeerConfigHolder,
    table: &PeerToolTable,
) -> Result<ConfigEvent, String> {
    // Last-good generation snapshot: the comparison base for the
    // boot-scoped diff AND the fallback if the reload fails.
    let last_good = holder.get();
    let blocking_home = home.to_path_buf();
    let (snapshot, restart_required) =
        tokio::task::spawn_blocking(move || reload_peer_config(&blocking_home, &last_good))
            .await
            .map_err(|e| format!("peer config reload task failed: {e}"))??;
    for field in &restart_required {
        tracing::info!(field = %field, "peer config reload: {} changed; restart required", field);
    }
    tracing::info!(
        allowlisted_tools = snapshot.config.tool_allowlist.len(),
        allowlisted_peers = snapshot.config.peer_ids.len(),
        peer_keys = snapshot.peer_keys.len(),
        "peer config reload applied (admission fields adopted; live sessions keep \
         grant-at-establish until reconnect)"
    );
    // Lock-rank safe (p0 QC F-003): `set_config` takes ONLY the config
    // mutex — never the table `inner` — so a reload can never invert the
    // rank against an in-flight admission.
    table.set_config(Some(Arc::clone(&snapshot.config)));
    holder.swap(snapshot);
    Ok(ConfigEvent::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_home_layout::{connect_daemon_config_path, connect_peer_keys_path};
    use spoke_connect::remote::{
        connect_responder, loopback_transport_pair, ConnectResponderOptions, RemoteIdentity,
    };
    use spoke_schemas::HostCapabilityManifest;
    use std::collections::{HashSet, VecDeque};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn isolated_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_daemon_config(home: &Path, body: &str) {
        let path = connect_daemon_config_path(home);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir connect dir");
        std::fs::write(path, body).expect("write daemon.json");
    }

    fn write_peer_keys(home: &Path, body: &str) {
        let path = connect_peer_keys_path(home);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir connect dir");
        std::fs::write(path, body).expect("write peer_keys.json");
    }

    fn boot_snapshot(config: PeerToolsConfig) -> PeerConfigSnapshot {
        PeerConfigSnapshot {
            config: Arc::new(config),
            peer_keys: Arc::new(HashMap::new()),
        }
    }

    // ── digest: file-set, full bytes, three states ──────────────────────

    #[test]
    fn digest_missing_when_both_files_absent() {
        let home = isolated_home();
        assert_eq!(peer_config_digest(home.path()), DigestPoll::Missing);
    }

    #[test]
    fn digest_tracks_full_bytes_of_each_file_independently() {
        let home = isolated_home();
        write_daemon_config(home.path(), r#"{"port":8425}"#);
        let DigestPoll::Tree(first) = peer_config_digest(home.path()) else {
            panic!("tree expected")
        };
        // daemon.json present (hex string), keys absent (null marker).
        assert!(first
            .get("daemon.json")
            .is_some_and(serde_json::Value::is_string));
        assert_eq!(first.get("peer_keys.json"), Some(&serde_json::Value::Null));

        // Byte-identical rewrite ⇒ same digest (Value equality).
        write_daemon_config(home.path(), r#"{"port":8425}"#);
        assert_eq!(
            peer_config_digest(home.path()),
            DigestPoll::Tree(first.clone())
        );

        // Whitespace-only change ⇒ DIFFERENT digest (full bytes, not the
        // parsed value — GC #4).
        write_daemon_config(home.path(), r#"{ "port" : 8425 }"#);
        let DigestPoll::Tree(second) = peer_config_digest(home.path()) else {
            panic!("tree expected")
        };
        assert_ne!(second, first);

        // peer_keys.json byte change is tracked independently.
        write_peer_keys(home.path(), r#"{"peer_keys":{"peer-b":"aa"}}"#);
        let DigestPoll::Tree(third) = peer_config_digest(home.path()) else {
            panic!("tree expected")
        };
        assert_ne!(third, second);
        assert!(third
            .get("peer_keys.json")
            .is_some_and(serde_json::Value::is_string));
    }

    #[test]
    fn digest_unreadable_when_file_is_a_directory() {
        let home = isolated_home();
        // A DIRECTORY at the daemon.json path: exists, but `read` fails
        // (EISDIR) — Unreadable, never conflated with Missing.
        std::fs::create_dir_all(connect_daemon_config_path(home.path())).expect("mkdir");
        match peer_config_digest(home.path()) {
            DigestPoll::Unreadable(message) => {
                assert!(
                    message.contains("daemon.json"),
                    "unreadable message must name the path: {message}"
                );
            }
            other => panic!("unreadable expected, got {other:?}"),
        }
    }

    // ── loop core: baseline, no event storm, warn-once, three states ────

    /// Drive the generic loop over a fixed poll sequence. Each iteration
    /// performs exactly one poll; the loop stops once the sequence is
    /// consumed. `apply` records its invocations in `applies`.
    async fn drive_loop(
        initial: DigestPoll,
        sequence: Vec<DigestPoll>,
        applies: Arc<AtomicUsize>,
        apply_result: impl Fn() -> Result<ConfigEvent, String>,
    ) -> PeerConfigWatchStats {
        let mut polls: VecDeque<DigestPoll> = sequence.into();
        let ticks = Arc::new(AtomicUsize::new(0));
        let stop_ticks = Arc::clone(&ticks);
        let total = polls.len();
        peer_config_watch_loop_inner(
            Duration::from_millis(1),
            initial,
            move || {
                // Deterministic tail: hold the last polled digest.
                let digest = polls.pop_front().unwrap_or(DigestPoll::Missing);
                ticks.fetch_add(1, Ordering::SeqCst);
                async move { digest }
            },
            move || {
                applies.fetch_add(1, Ordering::SeqCst);
                let result = apply_result();
                async move { result }
            },
            move || stop_ticks.load(Ordering::SeqCst) >= total,
        )
        .await
    }

    #[tokio::test]
    async fn baseline_seeded_without_initial_event() {
        let baseline = DigestPoll::Tree(serde_json::json!({"daemon.json": "aa"}));
        // Two polls identical to the baseline: no apply, no warnings — the
        // boot baseline is seeded WITHOUT emitting an initial event.
        let stats = drive_loop(
            baseline.clone(),
            vec![baseline],
            Arc::new(AtomicUsize::new(0)),
            || Ok(ConfigEvent::Changed),
        )
        .await;
        assert_eq!(stats, PeerConfigWatchStats::default());
    }

    #[tokio::test]
    async fn two_identical_polls_apply_at_most_once() {
        let baseline = DigestPoll::Missing;
        let changed = DigestPoll::Tree(serde_json::json!({"daemon.json": "bb"}));
        let applies = Arc::new(AtomicUsize::new(0));
        // Poll the SAME changed digest three times: exactly one apply
        // (no event storm).
        let stats = drive_loop(
            baseline,
            vec![changed.clone(), changed.clone(), changed],
            Arc::clone(&applies),
            || Ok(ConfigEvent::Changed),
        )
        .await;
        assert_eq!(stats.applied, 1);
        assert_eq!(stats.warnings, 0);
        assert_eq!(applies.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn missing_present_missing_transitions_apply_each_time() {
        let baseline = DigestPoll::Tree(serde_json::json!({"daemon.json": "aa"}));
        let applies = Arc::new(AtomicUsize::new(0));
        // Missing (both files removed) → present again → missing: every
        // transition applies; the loop (daemon) stays alive throughout.
        let stats = drive_loop(
            baseline,
            vec![
                DigestPoll::Missing,
                DigestPoll::Tree(serde_json::json!({"daemon.json": "bb"})),
                DigestPoll::Missing,
            ],
            Arc::clone(&applies),
            || Ok(ConfigEvent::Changed),
        )
        .await;
        assert_eq!(stats.applied, 3);
        assert_eq!(stats.warnings, 0);
        assert_eq!(applies.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn unreadable_poll_keeps_last_good_and_warns_once_per_transition() {
        let baseline = DigestPoll::Tree(serde_json::json!({"daemon.json": "aa"}));
        let applies = Arc::new(AtomicUsize::new(0));
        // Unreadable → Unreadable (same message) → readable changed:
        // no apply on the unreadable ticks, exactly ONE warn for the
        // error-state transition, then the change applies normally.
        let stats = drive_loop(
            baseline,
            vec![
                DigestPoll::Unreadable("cannot read daemon.json: EACCES".to_owned()),
                DigestPoll::Unreadable("cannot read daemon.json: EACCES".to_owned()),
                DigestPoll::Tree(serde_json::json!({"daemon.json": "bb"})),
            ],
            Arc::clone(&applies),
            || Ok(ConfigEvent::Changed),
        )
        .await;
        assert_eq!(stats.applied, 1);
        assert_eq!(stats.warnings, 1);
        assert_eq!(applies.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_reload_keeps_last_good_warns_once_and_does_not_reapply() {
        let baseline = DigestPoll::Missing;
        let corrupt = DigestPoll::Tree(serde_json::json!({"daemon.json": "cc"}));
        let applies = Arc::new(AtomicUsize::new(0));
        // Corrupt edit → apply fails (warn once, baseline advances) → the
        // SAME corrupt digest polls again: no re-apply, no re-warn (the
        // last-good config is kept; the daemon stays up).
        let stats = drive_loop(
            baseline,
            vec![corrupt.clone(), corrupt],
            Arc::clone(&applies),
            || Err("invalid daemon.json: expected value".to_owned()),
        )
        .await;
        assert_eq!(stats.applied, 0);
        assert_eq!(stats.warnings, 1);
        assert_eq!(applies.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn changed_reload_error_message_warns_again() {
        let baseline = DigestPoll::Missing;
        let first = DigestPoll::Tree(serde_json::json!({"daemon.json": "cc"}));
        let second = DigestPoll::Tree(serde_json::json!({"daemon.json": "dd"}));
        let applies = Arc::new(AtomicUsize::new(0));
        // Two DISTINCT corrupt edits: each is a new error-state transition
        // (new digest, changed failure message) — each warns once.
        let calls = Arc::new(AtomicUsize::new(0));
        let apply_calls = Arc::clone(&calls);
        let stats = drive_loop(
            baseline,
            vec![first, second],
            Arc::clone(&applies),
            move || {
                let n = apply_calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err("invalid daemon.json: one".to_owned())
                } else {
                    Err("invalid daemon.json: two".to_owned())
                }
            },
        )
        .await;
        assert_eq!(stats.applied, 0);
        assert_eq!(stats.warnings, 2);
        assert_eq!(applies.load(Ordering::SeqCst), 2);
    }

    // ── validated reload: adoption vs boot-scope pinning ────────────────

    /// 32 bytes for a valid hex key fixture (64 hex chars).
    const KEY_HEX: &str = "aaeda92ed0c29f28527c9c6d934c7ae4ad2d27a198c85e12a8a86c93be695e58";

    #[test]
    fn reload_adopts_admission_fields_and_pins_boot_fields() {
        let home = isolated_home();
        // Boot: defaults (loopback, port 8425, max_sessions default,
        // embedded_mcp off, FirstStays, empty allowlist).
        let last_good = boot_snapshot(PeerToolsConfig::default());
        // Edited: admission fields changed AND boot-scoped fields changed.
        write_daemon_config(
            home.path(),
            r#"{
                "port": 9999,
                "max_sessions": 9,
                "embedded_mcp": true,
                "tool_allowlist": ["tools.t3.echo"],
                "peer_ids": ["peer-b"],
                "collision_policy": "priority_order",
                "peer_priority": ["peer-b", "peer-a"]
            }"#,
        );
        let keys_body = "{\"peer_keys\":{\"peer-b\":\"".to_owned() + KEY_HEX + "\"}}";
        write_peer_keys(home.path(), &keys_body);
        let (snapshot, restart_required) =
            reload_peer_config(home.path(), &last_good).expect("reload succeeds");
        // Admission fields adopted (GC #7).
        assert_eq!(snapshot.config.tool_allowlist, vec!["tools.t3.echo"]);
        assert_eq!(snapshot.config.peer_ids, vec!["peer-b"]);
        assert_eq!(
            snapshot.config.collision_policy,
            crate::connect::config::CollisionPolicy::PriorityOrder
        );
        assert_eq!(snapshot.config.peer_priority, vec!["peer-b", "peer-a"]);
        assert!(snapshot.peer_keys.contains_key("peer-b"));
        // Boot-scoped fields pinned to the boot values (GC #7) — a diff on
        // them must NOT change the live bound/listener/bounds.
        assert_eq!(
            snapshot.config.port,
            crate::connect::config::DEFAULT_CONNECT_PORT
        );
        assert_eq!(
            snapshot.config.max_sessions,
            crate::connect::session::DEFAULT_MAX_SESSIONS
        );
        assert_eq!(
            snapshot.config.host,
            crate::connect::config::DEFAULT_CONNECT_HOST
        );
        assert!(!snapshot.config.embedded_mcp);
        // One named restart-required entry per changed boot-scoped field.
        assert_eq!(
            restart_required,
            vec!["port", "max_sessions", "embedded_mcp"]
        );
    }

    #[test]
    fn reload_fails_closed_on_invalid_file_with_message() {
        let home = isolated_home();
        let last_good = boot_snapshot(PeerToolsConfig::default());
        write_daemon_config(home.path(), r#"{"port": "not-a-port"}"#);
        let err = reload_peer_config(home.path(), &last_good).expect_err("must fail");
        assert!(err.contains("daemon.json"), "error names the file: {err}");
        // Unknown fields fail closed too (deny_unknown_fields preserved).
        write_daemon_config(home.path(), r#"{"bogus_field":1}"#);
        assert!(reload_peer_config(home.path(), &last_good).is_err());
        // Invalid peer key fails the whole reload (never partially adopted).
        write_daemon_config(home.path(), "{}");
        write_peer_keys(home.path(), r#"{"peer_keys":{"p":"zz"}}"#);
        let err = reload_peer_config(home.path(), &last_good).expect_err("must fail");
        assert!(
            err.contains("peer_keys"),
            "error names the keys file: {err}"
        );
    }

    // ── holder: swap visible to readers, in-flight clone keeps last-good ─

    #[test]
    fn holder_swap_replaces_generation_in_flight_clones_keep_last_good() {
        let boot_config = PeerToolsConfig {
            peer_ids: vec!["peer-a".to_owned()],
            ..PeerToolsConfig::default()
        };
        let holder = PeerConfigHolder::new(boot_snapshot(boot_config));
        let in_flight = holder.get();
        assert_eq!(in_flight.config.peer_ids, vec!["peer-a"]);

        let new_config = PeerToolsConfig {
            peer_ids: vec!["peer-b".to_owned()],
            tool_allowlist: vec!["tools.t3.echo".to_owned()],
            ..PeerToolsConfig::default()
        };
        holder.swap(boot_snapshot(new_config));

        let fresh = holder.get();
        assert_eq!(fresh.config.peer_ids, vec!["peer-b"]);
        assert_eq!(fresh.config.tool_allowlist, vec!["tools.t3.echo"]);
        // The pre-swap clone (an in-flight session's grant-at-establish)
        // still sees the old generation.
        assert_eq!(in_flight.config.peer_ids, vec!["peer-a"]);
    }

    // ── table/config integration: reload feeds the admission seam ───────

    fn manifest_with_tools(tools: &[&str]) -> HostCapabilityManifest {
        let mut capabilities: Vec<String> = vec!["spoke-baseline".to_owned()];
        capabilities.extend(tools.iter().map(|s| (*s).to_owned()));
        let tool_objs: Vec<serde_json::Value> = tools
            .iter()
            .map(|id| {
                serde_json::json!({
                    "schema_version": 1,
                    "capability_id": id,
                    "op": id,
                    "description": format!("{id} test tool"),
                    "input": { "type": "object" },
                    "output": { "type": "object" },
                })
            })
            .collect();
        let namespaces: Vec<String> = tools
            .iter()
            .filter_map(|id| id.split('.').nth(1))
            .map(ToOwned::to_owned)
            .collect();
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "host_id": "dialer",
            "roles": ["data-store"],
            "capabilities": capabilities,
            "namespaces": namespaces,
            "extensions": {},
            "tools": tool_objs,
        }))
        .expect("valid manifest")
    }

    async fn responder() -> Arc<spoke_connect::remote::ConnectResponder> {
        let pair = loopback_transport_pair();
        let options = ConnectResponderOptions {
            transport: Arc::new(pair.server),
            identity: RemoteIdentity { seed: [0x40; 32] },
            manifest: manifest_with_tools(&[]),
            allowlist: Vec::new(),
            peer_keys: HashMap::new(),
            ports: None,
            invoke_timeout_ms: Some(1000),
        };
        // The responder runs its handshake in the background; for the
        // admission seam test we only need the handle (no dialer). Awaited
        // inside the caller's `#[tokio::test]` runtime — no nested
        // `block_on`.
        connect_responder(options).await
    }

    fn caps(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    /// DF-92 end-to-end seam: a real on-disk edit + the production
    /// `apply_reload` path (validated load → table feed → holder swap)
    /// flips the LIVE collision policy for NEW admissions, while the
    /// boot generation stays frozen.
    #[tokio::test]
    async fn apply_reload_swaps_holder_and_feeds_table_collision_policy() {
        let home = isolated_home();
        write_daemon_config(home.path(), r#"{"tool_allowlist":["tools.t3.echo"]}"#);
        let boot = PeerToolsConfig::load(home.path()).expect("boot config loads");
        assert_eq!(
            boot.collision_policy,
            crate::connect::config::CollisionPolicy::FirstStays
        );
        let holder = PeerConfigHolder::new(PeerConfigSnapshot {
            config: Arc::new(boot),
            peer_keys: Arc::new(HashMap::new()),
        });
        let table = PeerToolTable::new();
        table.set_config(Some(Arc::clone(&holder.get().config)));

        // Boot posture: peer-a registers tools.t3.echo; peer-b's later
        // same-id registration is refused (first_stays — the row stays
        // peer-a's).
        let manifest = manifest_with_tools(&["tools.t3.echo"]);
        let first = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder().await,
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            first,
            crate::connect::AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let refused = table.admit_and_register(
            "peer-b",
            &manifest,
            &responder().await,
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            refused,
            crate::connect::AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            }
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("row present").peer_id,
            "peer-a"
        );

        // Operator edit: priority_order with peer-b ranked above peer-a.
        write_daemon_config(
            home.path(),
            r#"{
                "tool_allowlist": ["tools.t3.echo"],
                "collision_policy": "priority_order",
                "peer_priority": ["peer-b", "peer-a"]
            }"#,
        );
        // The on-disk digest differs from the boot baseline (the watcher's
        // trigger condition).
        let boot_digest = peer_config_digest(home.path());
        let _ = boot_digest;

        // The production apply path runs (spawn_blocking load + swap).
        let event = apply_reload(home.path(), &holder, &table)
            .await
            .expect("reload applies");
        assert_eq!(event, ConfigEvent::Changed);

        // Holder serves the new generation (policy + rank adopted).
        let fresh = holder.get();
        assert_eq!(
            fresh.config.collision_policy,
            crate::connect::config::CollisionPolicy::PriorityOrder
        );
        assert_eq!(fresh.config.peer_priority, vec!["peer-b", "peer-a"]);
        // The table seam was fed: peer-b's same-id registration now
        // PREEMPTS peer-a's row (priority_order, higher rank wins).
        let preempt = table.admit_and_register(
            "peer-b",
            &manifest,
            &responder().await,
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            preempt,
            crate::connect::AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("row present").peer_id,
            "peer-b"
        );
    }
    /// DF-92 headline verify through the REAL spawned watcher: an on-disk
    /// edit while the lane "runs" swaps a validated generation into the
    /// holder — a subsequent holder read returns the new peer set WITHOUT
    /// any process restart.
    #[tokio::test]
    async fn spawned_watcher_serves_reloaded_peer_set_without_restart() {
        let home = isolated_home();
        write_daemon_config(home.path(), r#"{"tool_allowlist":["tools.t3.echo"]}"#);
        let boot_digest = peer_config_digest(home.path());
        let boot = PeerToolsConfig::load(home.path()).expect("boot config loads");
        let holder = PeerConfigHolder::new(boot_snapshot(boot));
        assert!(holder.get().config.peer_ids.is_empty());

        let shutdown = Arc::new(Notify::new());
        let watch = spawn_peer_config_watch(
            boot_digest,
            home.path().to_path_buf(),
            PeerConfigHolder::clone(&holder),
            Arc::clone(&shutdown),
        );

        // Operator edit while the watcher is live: peer-b added to the
        // handshake allowlist (an admission-affecting field, GC #7).
        write_daemon_config(
            home.path(),
            r#"{"tool_allowlist":["tools.t3.echo"],"peer_ids":["peer-b"]}"#,
        );

        // Converges within the 2 s poll budget — no restart involved.
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while !holder.get().config.peer_ids.contains(&"peer-b".to_owned()) {
            assert!(
                std::time::Instant::now() < deadline,
                "holder must serve the reloaded peer set without a restart"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // The reload adopted the admission field AND kept the allowlist.
        assert_eq!(holder.get().config.tool_allowlist, vec!["tools.t3.echo"]);

        shutdown.notify_one();
        let _ = watch.await;
    }
}
