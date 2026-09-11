//! `DeepSeek Harness` native provider adapter (`deepseek-harness-sdk` 0.2).
//!
//! Implements a native provider for the official `dsh` runtime through the
//! SDK's high-level surface: ONE [`DeepSeekHarness`] per provider session
//! recipe, initialized eagerly inside `launch()` (a managed Ready handle is
//! never returned before initialization — v1.188 P0 T2), driven by
//! `start_session(Some(host_generated_id))` + `Session::run(Input::Text,
//! None)` wrapped in `tokio::time::timeout` (the SDK's inbox-receipt /
//! root-idle waits are unbounded by design; the 0.2 notification observer
//! stays `None` in P0 — P1 owns it). Nexus owns only the `HostEvent`
//! normalization (`map_dsh`); the crate owns the runtime spawn, the stdio
//! JSON-RPC wire parser, argv composition, and the close ladder.
//!
//! # Executable resolution (P0 T2)
//!
//! [`resolve_dsh_executable`] is the single resolution site: an explicit
//! configured command/path wins, then a nonblank parent `DSH_RUNTIME_BIN`,
//! then the bare command `dsh` resolved through PATH. Bare commands resolve
//! via PATH; a relative path containing separators is rejected rather than
//! resolved against an accidental daemon cwd; a valid explicit absolute
//! path is preserved (canonicalized). An invalid explicit or env override
//! fails without fallback to another binary. The canonical absolute
//! executable is handed to the SDK as `Config.dsh_bin` (the SDK itself
//! never default-searches `dsh`, and `Config.env` is not an executable
//! override).
//!
//! # Home/profile behavior (P0 T2)
//!
//! Ordinary argv is exactly `--profile sdk` (the SDK owns argv
//! construction); the sealed deny-all recipe adds exactly one
//! `--patch <owned absolute asset>` pair. Home selection is the SDK's:
//! explicit `Config.dsh_home` → caller env `DSH_HOME` → parent `DSH_HOME`
//! → parent `~/.dsh`, one spawn-environment merge with no process-global
//! mutation. The retired `DSH_CWD`/`DSH_SESSION_ROOT`/`DSH_CORDIS_CONFIG`
//! keys are gone; the SDK strips them from the child environment.
//!
//! # Sealed deny-all recipe (architecture §3.4)
//!
//! An all-false `PromptPermissionScope` selects the sealed recipe BEFORE
//! operation admission: the already-initialized ordinary harness is closed
//! through the retained cleanup owner, an exclusive owner-only child
//! DSH_HOME is provisioned under `<selected home>/nexus/<random id>`
//! (empty-bundles `sdk` manifest, startup-frozen reload, empty profile/home
//! patch layers), and the sealed harness is initialized with the
//! Nexus-owned closed no-tool Cordis asset (`deny_all.cordis.patch.yml`,
//! embedded beside this file) as its only patch — no user/plugin/MCP layer
//! enters that runtime. The prompt is never sent to the ordinary runtime.
//! The selected recipe is retained for the session; once a prompt has been
//! admitted, any scope change (deny_all ↔ None included) is rejected
//! rather than losing the conversation or escalating permissions. `None`
//! preserves the existing ordinary policy — it is not a tool-isolation
//! claim; partially permissive scopes stay `not_supported`.
//!
//! # Lifecycle ownership (P0 T2)
//!
//! Each session retains its harness, selected recipe, sealed-home lease,
//! and ONE cleanup owner/result. `shutdown()` never removes the session
//! before a confirmed close: the close runs as a retained shared task, the
//! caller waits at most `shutdown_ms` (the SDK ladder is 1+6+3s plus an
//! unbounded final kill/wait, so no "5s guaranteed reap" claim exists), a
//! waiter timeout returns cleanup-unconfirmed WITHOUT cancelling the
//! cleanup task or deleting the session/home lease, close errors
//! propagate, and concurrent/retried shutdowns observe the same shared
//! completion. The sealed-home lease is deleted only after a confirmed
//! close; an unconfirmed close retains every byte of evidence. A session
//! is never removed before confirmed close.
//!
//! # Message-level streaming, no cancel (P1 / AR-6)
//!
//! v1.188 P1 maps root `assistant/message` notifications into
//! `MessageDelta` events through a bounded retained producer while
//! `Session::run` is in flight. [`CapabilityDescriptor::dsh_limited`]
//! advertises `streaming: true` only after the actual-dsh timing proof in
//! `tests/dsh_real_runtime.rs`; `cancellation` stays `false` (no cancel
//! RPC). Each turn emits exactly one `OpStarted` before content, then
//! mapped deltas and exactly one terminal.
//!
//! # Timeout / failure behavior (P1 T2)
//!
//! The per-turn `tokio::time::timeout` wraps the whole run coroutine. When
//! it fires, the SDK coroutine is dropped but the runtime may keep the
//! abandoned prompt alive — no cancel RPC exists. The host emits one
//! `OpFailed(timeout)` and transfers the session to retained close; there
//! is no zombie-session reuse on the same handle.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use deepseek_harness_sdk::{Config, DeepSeekHarness, Error as DshError, Input, Notification};
use futures_util::future::{BoxFuture, FutureExt, Shared};
use futures_util::StreamExt;
use tokio::sync::{mpsc, Mutex, RwLock};
#[cfg(unix)]
use super::sealed_fs;

use crate::capability::model::{
    CapabilityDescriptor, HostContentBlock, HostEvent, HostEventStream, ManagedSessionHandle,
    OperationFailedEvent, OperationStartedEvent, PromptPermissionScope, ProtocolKind,
    ProviderDescriptor, ProviderHealth, TextDeltaEvent,
};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::providers::native_cli::map_dsh::{
    classify_error_parts, classify_notification, classify_run_error, finalize_successful_run,
    operation_delivery_overflow_failure, operation_protocol_failure, ClassifyNotificationError,
    DshNotificationClass, RunReconciliation,
};
use deepseek_harness_sdk::RunResult;
use std::time::Instant;
use tokio::sync::Notify;
use crate::ProviderAdapter;

/// The bare command resolved through PATH when no explicit configured
/// command/path and no nonblank parent `DSH_RUNTIME_BIN` exists (P0 T2: the
/// ordinary upstream `dsh` executable — the retired `dsh-jsonrpc-agent`
/// helper name is gone).
const DEFAULT_DSH_COMMAND: &str = "dsh";

/// The environment override honored between an explicit configured command
/// and the PATH fallback.
const DSH_RUNTIME_BIN_ENV: &str = "DSH_RUNTIME_BIN";

/// Subdirectory under the selected DSH_HOME that owns every Nexus-managed
/// child home (architecture §3.4: `<selected home>/nexus/<random id>`).
const SEALED_HOME_SUBDIR: &str = "nexus";

/// The sealed profile's manifest: an empty bundle list composed over an
/// empty root, with startup-frozen patch reload (upstream
/// `loadProfileDirectory` reads `bundles ?? []`; `loadProfile` initializes
/// only an absent manifest; `normalizeShippedProfile` preserves a custom
/// sdk empty tuple).
const SEALED_PROFILE_MANIFEST: &str =
    r#"{"private":true,"dsh":{"profile":{"bundles":[],"patchReload":"startup"}}}"#;

/// An empty patch layer (the sealed profile's own patch file and the sealed
/// home-level patch file): no user/plugin layer exists in the recipe.
const EMPTY_PATCH_LAYER: &str = "[]\n";

/// The embedded, reviewed closed no-tool Cordis asset (architecture §3.4):
/// upstream sdk-minimal's complete service tree minus its three
/// tool-producing entries, with tools native, empty agents and provider
/// retry maxRetries 0. Copied into the owned sealed child home and passed
/// as the single absolute `--patch` overlay.
const DENY_ALL_PATCH: &str = include_str!("deny_all.cordis.patch.yml");

/// File name of the copied no-tools patch inside the sealed child home.
const DENY_ALL_PATCH_FILENAME: &str = "nexus-deny-all.cordis.patch.yml";

/// Nexus-owned delivery bounds (architecture §4.3).
const DSCH_MAX_QUEUED_MESSAGES: usize = 64;
const DSCH_MAX_EVENT_TEXT_BYTES: usize = 256 * 1024;
const DSCH_MAX_PENDING_PAYLOAD_BYTES: usize = 1024 * 1024;
const DSCH_MAX_EMITTED_TEXT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct DeliveryBudget {
    pending: StdMutex<usize>,
    emitted: StdMutex<usize>,
    /// Accepted message deltas this turn (queue depth can shrink while the
    /// consumer drains; overflow is keyed off total accepts, not buffer fill).
    accepted_messages: StdMutex<usize>,
}

impl DeliveryBudget {
    fn try_reserve(&self, bytes: usize) -> bool {
        if bytes > DSCH_MAX_EVENT_TEXT_BYTES {
            return false;
        }
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let mut emitted = self.emitted.lock().unwrap_or_else(|e| e.into_inner());
        let mut accepted_messages = self
            .accepted_messages
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *accepted_messages >= DSCH_MAX_QUEUED_MESSAGES {
            return false;
        }
        if *pending + bytes > DSCH_MAX_PENDING_PAYLOAD_BYTES {
            return false;
        }
        if *emitted + bytes > DSCH_MAX_EMITTED_TEXT_BYTES {
            return false;
        }
        *accepted_messages += 1;
        *pending += bytes;
        *emitted += bytes;
        true
    }

    fn release(&self, bytes: usize) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        *pending = pending.saturating_sub(bytes);
    }
}

/// Monotonic instants for actual-runtime streaming proof (L2).
#[derive(Default, Clone, Debug)]
pub struct DshStreamingRunTiming {
    pub first_callback: Option<Instant>,
    pub run_completed: Option<Instant>,
}

pub static LAST_DSH_RUN_TIMING: StdMutex<Option<DshStreamingRunTiming>> = StdMutex::new(None);

pub fn take_last_dsh_run_timing() -> Option<DshStreamingRunTiming> {
    LAST_DSH_RUN_TIMING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

struct QueuedPayload {
    event: Option<HostEvent>,
    reservation: Option<usize>,
    budget: Arc<DeliveryBudget>,
}

impl QueuedPayload {
    fn into_event(mut self) -> HostEvent {
        if let Some(bytes) = self.reservation.take() {
            self.budget.release(bytes);
        }
        self.event.take().expect("queued payload consumed once")
    }
}

impl Drop for QueuedPayload {
    fn drop(&mut self) {
        if let Some(bytes) = self.reservation.take() {
            self.budget.release(bytes);
        }
    }
}

struct StreamObserver {
    root_session_id: String,
    session_id: HostSessionId,
    op_id: HostOperationId,
    content_tx: mpsc::Sender<QueuedPayload>,
    reconciliation: Arc<StdMutex<RunReconciliation>>,
    classifier_failed: Arc<AtomicBool>,
    delivery_failed: Arc<AtomicBool>,
    budget: Arc<DeliveryBudget>,
    failure_notify: Arc<Notify>,
    run_timing: Arc<StdMutex<DshStreamingRunTiming>>,
}

impl StreamObserver {
    fn signal_failure(&self, classifier: bool, delivery: bool) {
        if classifier {
            self.classifier_failed.store(true, Ordering::Release);
        }
        if delivery {
            self.delivery_failed.store(true, Ordering::Release);
        }
        self.failure_notify.notify_waiters();
    }

    fn observe(&self, notification: &Notification) {
        if self.classifier_failed.load(Ordering::Acquire)
            || self.delivery_failed.load(Ordering::Acquire)
        {
            return;
        }
        match classify_notification(notification, &self.root_session_id) {
            Ok(DshNotificationClass::Ignore) => {}
            Ok(DshNotificationClass::RootText(text)) => {
                let bytes = text.len();
                if !self.budget.try_reserve(bytes) {
                    self.signal_failure(false, true);
                    return;
                }
                {
                    let mut timing = self.run_timing.lock().unwrap_or_else(|e| e.into_inner());
                    if timing.first_callback.is_none() {
                        timing.first_callback = Some(Instant::now());
                    }
                }
                let event = HostEvent::MessageDelta(TextDeltaEvent {
                    session_id: self.session_id.clone(),
                    op_id: self.op_id.clone(),
                    text,
                });
                match self.content_tx.try_send(QueuedPayload {
                    event: Some(event),
                    reservation: Some(bytes),
                    budget: Arc::clone(&self.budget),
                }) {
                    Ok(()) => {
                        self.reconciliation
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .note_root_text_emitted();
                    }
                    Err(_) => self.signal_failure(false, true),
                }
            }
            Err(ClassifyNotificationError::Protocol(_)) => self.signal_failure(true, false),
            Err(ClassifyNotificationError::EventTooLarge(_)) => self.signal_failure(false, true),
        }
    }
}


fn delivery_failure_signaled(
    classifier_failed: &AtomicBool,
    delivery_failed: &AtomicBool,
) -> bool {
    classifier_failed.load(Ordering::Acquire) || delivery_failed.load(Ordering::Acquire)
}

async fn wait_delivery_failure(
    failure_notify: &Notify,
    classifier_failed: &AtomicBool,
    delivery_failed: &AtomicBool,
) {
    loop {
        if delivery_failure_signaled(classifier_failed, delivery_failed) {
            return;
        }
        failure_notify.notified().await;
    }
}

/// Signals the producer when the consumer drops the stream before normal
/// completion (`content_tx` stays open while the observer still holds a
/// sender clone during `Session::run`).
struct StreamConsumerDropGuard {
    stream_finished: Arc<AtomicBool>,
    consumer_dropped: Arc<AtomicBool>,
    consumer_drop_notify: Arc<Notify>,
}

impl StreamConsumerDropGuard {
    fn mark_stream_finished(&self) {
        self.stream_finished.store(true, Ordering::Release);
    }
}

impl Drop for StreamConsumerDropGuard {
    fn drop(&mut self) {
        if !self.stream_finished.load(Ordering::Acquire) {
            // Latch synchronously so a real owner drop before the producer
            // enters `select!` is never lost (Notify alone is not enough).
            self.consumer_dropped.store(true, Ordering::Release);
            self.consumer_drop_notify.notify_waiters();
        }
    }
}

fn consumer_drop_signaled(consumer_dropped: &AtomicBool) -> bool {
    consumer_dropped.load(Ordering::Acquire)
}

async fn wait_consumer_drop(consumer_dropped: &AtomicBool, consumer_drop_notify: &Notify) {
    loop {
        if consumer_drop_signaled(consumer_dropped) {
            return;
        }
        consumer_drop_notify.notified().await;
    }
}

async fn reunify_run_harness(
    run_harness: &Mutex<Option<DeepSeekHarness>>,
    state: &Mutex<ClientState>,
) {
    let harness = run_harness.lock().await.take();
    if let Some(harness) = harness {
        let mut guard = state.lock().await;
        if !guard.closed && guard.harness.is_none() {
            guard.harness = Some(harness);
        }
    }
}

async fn invalidate_session_after_failed_operation(
    state: Arc<Mutex<ClientState>>,
    retained: RetainedLeases,
) {
    let mut guard = state.lock().await;
    if guard.closed {
        return;
    }
    guard.closed = true;
    let cleanup = start_close(
        guard.harness.take(),
        guard.sealed_home.take(),
        Arc::clone(&retained),
    );
    guard.cleanup = Some(cleanup);
}

/// Resolve the dsh runtime executable to a canonical absolute path.
///
/// Selection order (P0 T2): an explicit configured command/path (blank
/// counts as absent, SDK truthiness parity), then a nonblank parent
/// `DSH_RUNTIME_BIN`, then the bare command `dsh` resolved through PATH.
///
/// - Bare command names (no path separator) resolve through PATH.
/// - A relative path containing a separator is rejected — never resolved
///   against an accidental daemon cwd.
/// - An absolute path must name an executable file; it is canonicalized
///   and preserved.
///
/// An invalid explicit or env override is an error WITHOUT fallback to
/// another binary (fail closed). The returned message is safe static
/// category text (no environment dumps); only handshake proves readiness.
pub fn resolve_dsh_executable(explicit: Option<&str>) -> Result<PathBuf, String> {
    let override_value = explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var(DSH_RUNTIME_BIN_ENV)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        });
    match override_value {
        Some(value) => resolve_override(&value),
        None => resolve_bare_command(DEFAULT_DSH_COMMAND).ok_or_else(|| {
            "dsh runtime not found: no configured command, DSH_RUNTIME_BIN unset, \
             and `dsh` is not on PATH"
                .to_string()
        }),
    }
}

/// Resolve one explicit/env override value: absolute path, or bare command
/// name via PATH; a relative path with a separator fails closed.
fn resolve_override(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if path.is_absolute() {
        return canonicalize_executable(path)
            .ok_or_else(|| "configured dsh runtime path is not an executable file".to_string());
    }
    if value.contains('/') || value.contains('\\') {
        return Err(
            "a relative dsh runtime path containing separators is rejected; use an absolute \
             path or a bare command name"
                .to_string(),
        );
    }
    resolve_bare_command(value)
        .ok_or_else(|| "configured dsh runtime command was not found on PATH".to_string())
}

/// Resolve a bare command name through PATH and canonicalize the hit.
fn resolve_bare_command(command: &str) -> Option<PathBuf> {
    let found = which::which(command).ok()?;
    canonicalize_executable(&found)
}

/// Canonicalize an absolute (or PATH-resolved) executable, validating that
/// it is an executable file. Only the initialize handshake proves
/// readiness; this gate keeps bad overrides from ever reaching spawn.
fn canonicalize_executable(path: &Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    if !canonical.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if canonical.metadata().ok()?.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(canonical)
}

/// The recipe a session runs (architecture §3.4). `Ordinary` is the
/// upstream `dsh --profile sdk` composition; `SealedDenyAll` is the
/// Nexus-owned closed no-tool composition in the exclusive child home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Recipe {
    Ordinary,
    SealedDenyAll,
}

/// Map a requested prompt permission scope to the recipe it selects,
/// BEFORE operation admission. `None` preserves the existing ordinary
/// policy (never a tool-isolation claim); the all-false scope selects the
/// sealed deny-all recipe; any partially permissive scope stays
/// `not_supported` — it is rejected, never silently discarded.
fn recipe_for_scope(
    provider_id: &ProviderId,
    scope: Option<PromptPermissionScope>,
) -> HostResult<Recipe> {
    match scope {
        None => Ok(Recipe::Ordinary),
        Some(scope)
            if !scope.allow_read && !scope.allow_write && !scope.allow_destructive =>
        {
            Ok(Recipe::SealedDenyAll)
        }
        Some(_) => Err(HostError::capability_unsupported(
            provider_id.clone(),
            "prompt permission scope",
            "only the all-false deny_all scope is enforceable on the dsh native provider; \
             partially permissive scopes are unsupported",
        )),
    }
}

/// One unconfirmed cleanup result: safe static text, whether the runtime
/// close itself was CONFIRMED (only then is a lease-removal retry safe),
/// and the exact lease path whose removal failed (retained for a later
/// shutdown/reconciliation attempt; fix wave 2: a completed shared
/// cleanup error alone is insufficient — only a confirmed removal clears
/// the path/record).
#[derive(Clone)]
struct CleanupError {
    message: String,
    close_confirmed: bool,
    failed_lease: Option<PathBuf>,
}

/// One retained close operation: a shared future every waiter observes,
/// driven by a dedicated task so a caller-side timeout never cancels the
/// close itself (architecture §3.2). The `Err` payload is safe static
/// text (the raw SDK close error embeds stderr tails and is never
/// exported).
type CleanupWait = Shared<BoxFuture<'static, Result<(), CleanupError>>>;

/// Exact retained lease paths whose removal previously failed (fix wave
/// 2): provider-global, exact paths only — never a broad scan. Only a
/// confirmed removal clears an entry.
type RetainedLeases = Arc<std::sync::Mutex<BTreeSet<PathBuf>>>;

/// Record one exact lease path whose removal failed so a later
/// shutdown/reconciliation attempt can target it.
fn retain_lease_path(retained: &RetainedLeases, path: PathBuf) {
    retained
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(path);
}

/// Retry removal of every retained lease path (exact paths only) through
/// descriptor-relative no-follow deletion rooted at the retained anchor
/// (fix wave 3 — never a path-based `remove_dir_all` on a
/// security-sensitive retained lease); only a confirmed removal clears
/// the record.
fn reconcile_retained_leases(retained: &RetainedLeases) {
    let paths: Vec<PathBuf> = retained
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .cloned()
        .collect();
    for path in paths {
        if remove_lease_at_path(&path).is_ok() {
            retained
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&path);
        }
    }
}

/// Close one harness through the SDK ladder and — ONLY after a confirmed
/// close — delete its sealed-home lease through the lease's PINNED
/// anchor descriptors (fix wave 3: never a path-based removal on the
/// security-sensitive lease). Every failure is an unconfirmed cleanup
/// (`Err` with safe static text): a ladder error retains the lease, and
/// a lease-removal failure is NOT success — the exact path is retained
/// in `retained` for a later reconciliation attempt. The raw SDK close
/// error is never exported: its Display embeds stderr tails that may
/// carry secrets.
async fn close_and_reap(
    mut harness: Option<DeepSeekHarness>,
    sealed_home: Option<SealedLease>,
    retained: &RetainedLeases,
) -> Result<(), CleanupError> {
    let result = match harness.as_mut() {
        Some(harness) => harness.close().await,
        None => Ok(()),
    };
    match result {
        Ok(()) => {
            if let Some(lease) = sealed_home {
                // Confirmed close ends the lease; a removal failure keeps
                // the evidence AND fails the cleanup — the caller must
                // not remove the session record or report health, and the
                // exact path stays retained for a later retry.
                if remove_lease_tree(&lease).is_err() {
                    retain_lease_path(retained, lease.path.clone());
                    return Err(CleanupError {
                        message:
                            "dsh close completed but the sealed home could not be removed; \
                             cleanup is unconfirmed and the lease evidence is retained"
                                .to_string(),
                        close_confirmed: true,
                        failed_lease: Some(lease.path),
                    });
                }
            }
            Ok(())
        }
        Err(_sdk_error) => {
            // The lease is retained for reconciliation; never delete a
            // possibly-live child's files. The path is NOT registered for
            // removal retry: only a confirmed close makes removal safe.
            Err(CleanupError {
                message: "dsh close ladder reported an error; cleanup is unconfirmed and the \
                          session evidence is retained"
                    .to_string(),
                close_confirmed: false,
                failed_lease: sealed_home.map(|lease| lease.path),
            })
        }
    }
}

/// Spawn one retained close owner for an already-owned harness/lease
/// pair (recipe switch and probe closes): the close runs on a dedicated
/// task so a caller-side timeout never cancels it (architecture §3.2).
fn start_close(
    harness: Option<DeepSeekHarness>,
    sealed_home: Option<SealedLease>,
    retained: RetainedLeases,
) -> CleanupWait {
    let cleanup = async move { close_and_reap(harness, sealed_home, &retained).await }
        .boxed()
        .shared();
    // Drive the close to completion independently of any waiter.
    tokio::spawn(cleanup.clone());
    cleanup
}
/// Start the retained FINAL close owner for a session (fix wave: the
/// shutdown close is admitted without waiting behind an active
/// `Session::run`). The spawned task itself acquires the per-session
/// mutex — unbounded, because it IS the retained owner — marks the
/// session closed, takes any pending switch close plus the current
/// harness/lease, resolves the pending close first (a failed switch's
/// ordinary close is the only remaining cleanup and its result
/// propagates), then closes whatever harness the session currently runs.
/// After a confirmed harness close it reconciles the SESSION-associated
/// failed lease (fix wave 2 finding: a sealed-switch start+delete
/// failure retains its exact anchored lease in `ClientState`) — the
/// session record may only be removed after that specific lease's
/// confirmed deletion.
fn start_final_close(state: Arc<Mutex<ClientState>>, retained: RetainedLeases) -> CleanupWait {
    let cleanup = async move {
        let (pending, harness, sealed_home, session_failed_lease) = {
            let mut guard = state.lock().await;
            guard.closed = true;
            (
                guard.cleanup.take(),
                guard.harness.take(),
                guard.sealed_home.take(),
                guard.failed_lease.take(),
            )
        };
        if let Some(pending) = pending {
            // A pending (switch) close must resolve before the final
            // close completes; its failure IS the unconfirmed result.
            pending.await?;
        }
        close_and_reap(harness, sealed_home, &retained).await?;
        if let Some(path) = session_failed_lease {
            // The session-associated lease must be confirmed deleted
            // before the session may be removed; a failure retains the
            // exact path and fails the cleanup.
            if remove_lease_at_path(&path).is_err() {
                retain_lease_path(&retained, path.clone());
                return Err(CleanupError {
                    message: "dsh close completed but the session-associated sealed home \
                              could not be removed; cleanup is unconfirmed and the lease \
                              evidence is retained"
                        .to_string(),
                    close_confirmed: true,
                    failed_lease: Some(path),
                });
            }
        }
        Ok(())
    }
    .boxed()
    .shared();
    tokio::spawn(cleanup.clone());
    cleanup
}

/// Resolve the DSH_HOME the SDK would select for this environment
/// (explicit `dsh_home` is never set on the ordinary path, so the chain is
/// caller env `DSH_HOME` → parent `DSH_HOME` → parent `~/.dsh`). Uses the
/// SDK's own resolution rather than re-implementing it; no process-global
/// mutation.
fn selected_dsh_home(env: &HashMap<String, String>) -> PathBuf {
    let mut parent = HashMap::new();
    if let Some(value) = std::env::var("DSH_HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        parent.insert("DSH_HOME".to_string(), value);
    }
    let probe = Config {
        env: Some(env.clone()),
        ..Config::default()
    };
    probe.resolve_dsh_home(&parent)
}

/// Provision the exclusive owner-only sealed child DSH_HOME under
/// `<selected home>/nexus/<random id>` (architecture §3.4). The selected
/// home's own files are never rewritten — only the `nexus/` subtree is
/// created. Returns the RESOLVED child path (the anchor's pre-existing
/// symlinks resolved exactly once at validation), which the caller holds
/// as a lease until the sealed runtime's close is confirmed.
///
/// Path safety (fix wave 2): the selected home MUST resolve to an
/// absolute path, and provisioning runs through descriptor-relative
/// no-follow operations (`sealed_fs`): the FULL selected-home ancestor
/// chain is validated — the anchor's pre-existing symlinks are resolved
/// exactly once, then every (resolved) component is opened with
/// `openat(O_NOFOLLOW | O_DIRECTORY)` from the root and pinned by
/// descriptor, so a swapped component after validation cannot redirect a
/// write. The unique leaf is created with one exclusive `mkdirat`
/// (existence IS ownership — no check-then-create window) and made
/// owner-only via `fchmod` BEFORE any content lands, and every layout
/// directory/file is created relative to the pinned parent descriptor
/// with exclusive no-follow creates and fsynced. The returned patch path
/// is asserted absolute and inside the leaf. Unsupported targets fail
/// sealed provisioning closed rather than silently weakening it.
fn provision_sealed_home(env: &HashMap<String, String>) -> Result<SealedLease, String> {
    let selected = selected_dsh_home(env);
    let selected = std::path::absolute(&selected)
        .map_err(|_io| "sealed dsh home selection could not be made absolute".to_string())?;
    if !selected.is_absolute() {
        return Err("sealed dsh home selection is not absolute".to_string());
    }
    let child = selected
        .join(SEALED_HOME_SUBDIR)
        .join(uuid::Uuid::new_v4().simple().to_string());
    provision_sealed_home_fd(&child)
}

/// Create one sealed child home at `child` with the exact recipe layout:
/// absolute path, full no-follow ancestor validation, one exclusive
/// owner-only leaf create, the empty-bundles startup-frozen `sdk`
/// manifest, empty profile and home patch layers, and the embedded
/// no-tools patch asset — all via descriptor-relative operations. On any
/// failure the partial directory is removed — a failed provision leaves
/// no lease. (Test-facing wrapper: assertions read through the original,
/// equivalent path.)
#[cfg(test)]
fn provision_sealed_home_at(child: &Path) -> Result<(), String> {
    if !child.is_absolute() {
        return Err("sealed dsh home path is not absolute".to_string());
    }
    provision_sealed_home_fd(child).map(|_| ())
}

/// Resolve the selected-home anchor's nearest EXISTING ancestor exactly
/// once (pre-existing symlinks in the user's own anchor — e.g. macOS
/// `/var` — are legitimate and resolved at validation time), returning
/// the canonical prefix plus the missing tail components. Components
/// BELOW the anchor (`nexus`, the leaf) never pass through this
/// resolution: they are handled exclusively through descriptor-relative
/// operations under the pinned anchor descriptor.
#[cfg(unix)]
fn canonical_anchor(path: &Path) -> Result<(PathBuf, Vec<std::ffi::OsString>), String> {
    let mut missing = Vec::new();
    let mut cursor = path;
    loop {
        match std::fs::canonicalize(cursor) {
            Ok(canonical) => {
                if !canonical.is_dir() {
                    return Err("sealed dsh home anchor is not a directory".to_string());
                }
                missing.reverse();
                return Ok((canonical, missing));
            }
            Err(_) => {
                let Some(name) = cursor.file_name() else {
                    return Err("sealed dsh home anchor has no existing ancestor".to_string());
                };
                missing.push(name.to_os_string());
                let Some(parent) = cursor.parent() else {
                    return Err("sealed dsh home anchor has no existing ancestor".to_string());
                };
                cursor = parent;
            }
        }
    }
}

/// One provisioned sealed child home (the lease): the resolved absolute
/// path PLUS the pinned descriptors of its `nexus` anchor parent and the
/// owned leaf, retained for the WHOLE lease lifetime (fix wave 3). Every
/// later operation — the pre-start revalidation and the eventual
/// deletion — is anchored at these descriptors, so a swapped component
/// can never redirect a write, the SDK handoff, or a removal.
struct SealedLease {
    /// Resolved absolute child path (symlink-free at provisioning).
    path: PathBuf,
    /// Leaf directory name (the random id; no separators).
    leaf_name: String,
    /// Pinned descriptor of the `nexus` anchor parent (deletion root).
    #[cfg(unix)]
    nexus_fd: rustix::fd::OwnedFd,
    /// Pinned descriptor of the owned leaf itself (revalidation root).
    #[cfg(unix)]
    leaf_fd: rustix::fd::OwnedFd,
}

impl SealedLease {
    /// Immediately before the SDK start handoff (fix wave 3): revalidate
    /// through the pinned descriptors that the lexical absolute child
    /// and patch still resolve to the OWNED inode/files (dev+ino
    /// equality). Any swap fails closed.
    #[cfg(unix)]
    fn revalidate_for_start(&self) -> Result<(), String> {
        let lexical = sealed_fs::open_dir_nofollow_absolute(&self.path)?;
        if sealed_fs::inode_of(&lexical)? != sealed_fs::inode_of(&self.leaf_fd)? {
            return Err(
                "the sealed home path was swapped after provisioning; failing closed".to_string(),
            );
        }
        let lexical_patch = sealed_fs::open_file_nofollow_at(&lexical, DENY_ALL_PATCH_FILENAME)?;
        let pinned_patch =
            sealed_fs::open_file_nofollow_at(&self.leaf_fd, DENY_ALL_PATCH_FILENAME)?;
        if sealed_fs::inode_of(&lexical_patch)? != sealed_fs::inode_of(&pinned_patch)? {
            return Err(
                "the sealed patch was swapped after provisioning; failing closed".to_string(),
            );
        }
        Ok(())
    }

    /// Non-unix counterpart: provisioning always fails closed, so a
    /// lease can never exist; unreachable in practice.
    #[cfg(not(unix))]
    fn revalidate_for_start(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Delete one sealed lease anchored at its PINNED descriptors — never a
/// path-based removal that could follow a swapped lexical component
/// (fix wave 3). An already-gone leaf is a confirmed removal.
#[cfg(unix)]
fn remove_lease_tree(lease: &SealedLease) -> Result<(), String> {
    match sealed_fs::remove_tree_at(&lease.nexus_fd, &lease.leaf_name) {
        Ok(()) => Ok(()),
        Err(message) => {
            if lease.path.try_exists().unwrap_or(true) {
                Err(message)
            } else {
                Ok(())
            }
        }
    }
}

/// Non-unix targets never provision sealed leases (provisioning fails
/// closed), so there is never a lease to remove.
#[cfg(not(unix))]
fn remove_lease_tree(_lease: &SealedLease) -> Result<(), String> {
    Ok(())
}

/// Delete one retained lease by its exact retained path for
/// reconciliation (fix wave 3): the path was RESOLVED (symlink-free) at
/// provisioning, so it is walked no-follow WITHOUT any fresh
/// canonicalization — a component swapped to a symlink fails the walk
/// and the lease stays retained rather than deleting a swapped lexical
/// target. An already-gone anchor/leaf is a confirmed removal.
#[cfg(unix)]
fn remove_lease_at_path(lease: &Path) -> Result<(), String> {
    let Some(nexus_dir) = lease.parent() else {
        return Err("retained lease path has no anchor parent".to_string());
    };
    let Some(leaf_name) = lease.file_name().and_then(|name| name.to_str()) else {
        return Err("retained lease path has no leaf name".to_string());
    };
    let nexus_fd = match sealed_fs::open_dir_nofollow_absolute(nexus_dir) {
        Ok(fd) => fd,
        Err(message) => {
            return if nexus_dir.try_exists().unwrap_or(true) {
                Err(message)
            } else {
                Ok(())
            };
        }
    };
    match sealed_fs::remove_tree_at(&nexus_fd, leaf_name) {
        Ok(()) => Ok(()),
        Err(message) => {
            if lease.try_exists().unwrap_or(true) {
                Err(message)
            } else {
                Ok(())
            }
        }
    }
}

/// A sealed start failure (fix wave 3): the error to return, plus the
/// exact anchored lease path when even the immediate anchored deletion
/// failed — the caller (recipe switch) associates that path with the
/// SESSION so the final shutdown must reconcile it before the session
/// record may be removed.
struct SealedStartFailure {
    error: HostError,
    undeleted_lease: Option<PathBuf>,
}

/// Unix implementation: descriptor-relative no-follow provisioning (see
/// [`sealed_fs`]). The full selected-home ancestor chain is validated —
/// the anchor's pre-existing symlinks are resolved exactly once, every
/// (resolved) component is opened no-follow from the root and pinned by
/// descriptor with the non-owner-mutation boundary enforced, and the
/// selected home / `nexus` / leaf must additionally be owned by the
/// effective uid (unsafe owner/mode fails closed). Returns the pinned
/// lease.
#[cfg(unix)]
fn provision_sealed_home_fd(child: &Path) -> Result<SealedLease, String> {
    let Some(nexus_dir) = child.parent() else {
        return Err("sealed dsh home path has no parent".to_string());
    };
    let Some(home_dir) = nexus_dir.parent() else {
        return Err("sealed dsh home path has no selected home".to_string());
    };
    let Some(leaf_name) = child.file_name().and_then(|name| name.to_str()) else {
        return Err("sealed dsh home leaf name is invalid".to_string());
    };
    // Resolve + validate + pin the FULL selected-home ancestor chain;
    // create missing anchor-tail components; then ensure the `nexus`
    // parent (a symlinked one is rejected no-follow) and exclusively
    // create the owner-only leaf. The selected home and everything below
    // it must be owned by the effective uid.
    let (canonical_home, missing) = canonical_anchor(home_dir)?;
    let mut dir_fd = sealed_fs::ensure_dir_nofollow_absolute(&canonical_home)?;
    let mut resolved = canonical_home;
    for component in &missing {
        let name = component
            .to_str()
            .ok_or_else(|| "sealed dsh home path is not valid UTF-8".to_string())?;
        dir_fd = sealed_fs::ensure_dir_at(&dir_fd, name, 0o755)?;
        resolved.push(component);
    }
    sealed_fs::check_dir_boundary(&dir_fd, true)?;
    let nexus_fd = sealed_fs::ensure_dir_at(&dir_fd, SEALED_HOME_SUBDIR, 0o755)?;
    sealed_fs::check_dir_boundary(&nexus_fd, true)?;
    resolved.push(SEALED_HOME_SUBDIR);
    let leaf_fd = sealed_fs::mkdir_exclusive_at(&nexus_fd, leaf_name, 0o700)?;
    sealed_fs::check_dir_boundary(&leaf_fd, true)?;
    sealed_fs::fsync_dir(&nexus_fd)?;
    resolved.push(leaf_name);

    let provision = || -> Result<(), String> {
        let profiles_fd = sealed_fs::mkdir_exclusive_at(&leaf_fd, "profiles", 0o700)?;
        let sdk_fd = sealed_fs::mkdir_exclusive_at(&profiles_fd, "sdk", 0o700)?;
        sealed_fs::write_file_exclusive_at(
            &sdk_fd,
            "package.json",
            format!("{SEALED_PROFILE_MANIFEST}\n").as_bytes(),
        )?;
        sealed_fs::write_file_exclusive_at(&sdk_fd, "cordis.patch.yml", EMPTY_PATCH_LAYER.as_bytes())?;
        sealed_fs::write_file_exclusive_at(&leaf_fd, "cordis.patch.yml", EMPTY_PATCH_LAYER.as_bytes())?;
        sealed_fs::write_file_exclusive_at(&leaf_fd, DENY_ALL_PATCH_FILENAME, DENY_ALL_PATCH.as_bytes())?;
        sealed_fs::fsync_dir(&sdk_fd)?;
        sealed_fs::fsync_dir(&profiles_fd)?;
        sealed_fs::fsync_dir(&leaf_fd)?;
        // Final assertion: the patch handed to the SDK is absolute and
        // remains inside the freshly created leaf.
        let patch = resolved.join(DENY_ALL_PATCH_FILENAME);
        let canonical_patch = std::fs::canonicalize(&patch)
            .map_err(|_io| "sealed dsh patch path could not be canonicalized".to_string())?;
        let canonical_child = std::fs::canonicalize(&resolved)
            .map_err(|_io| "sealed dsh child path could not be canonicalized".to_string())?;
        if !patch.is_absolute() || !canonical_patch.starts_with(&canonical_child) {
            return Err("sealed dsh patch path escaped the child home".to_string());
        }
        Ok(())
    };
    if provision().is_err() {
        // Anchored removal of the partial lease through the pinned
        // descriptors — never a path-based removal.
        let _ = sealed_fs::remove_tree_at(&nexus_fd, leaf_name);
        return Err("failed to provision the sealed dsh home".to_string());
    }
    Ok(SealedLease {
        path: resolved,
        leaf_name: leaf_name.to_string(),
        nexus_fd,
        leaf_fd,
    })
}

/// Unsupported targets fail sealed provisioning CLOSED rather than
/// silently weakening the no-follow contract.
#[cfg(not(unix))]
fn provision_sealed_home_fd(_child: &Path) -> Result<SealedLease, String> {
    Err("sealed deny_all home provisioning is unsupported on this platform: \
         descriptor-relative no-follow filesystem primitives are required"
        .to_string())
}

/// Crate-client-scoped state for a managed dsh session, guarded by the
/// per-session mutex (B-2): only this session's operations contend on it —
/// cancel/shutdown of other sessions never wait on this session's run. The
/// provider-global registry `RwLock` is only for short lookups.
struct ClientState {
    /// Host-generated DSH session id (AR-2): `start_session(Some(id))`
    /// reuses it across executes — the runtime lazily creates the
    /// agent+session pair on the first run and resumes it afterwards.
    dsh_session_id: String,
    /// The initialized harness for the selected recipe (one per provider
    /// session); started inside `launch()` before the Ready handle is
    /// returned. Taken by the retained close owner on switch/shutdown.
    harness: Option<DeepSeekHarness>,
    /// Set by the final close task (or a failed recipe switch) under the
    /// lock so a run that already cloned the state can never start a
    /// fresh harness after the close, and an execute observes the
    /// torn-down session. A failed switch fails closed (STOP, never
    /// fallback).
    closed: bool,
    /// The recipe this session runs (ordinary by default; sealed after a
    /// first deny-all prompt closes the ordinary harness).
    recipe: Recipe,
    /// Set once the first prompt has been admitted: any later scope change
    /// (deny_all ↔ None included) is rejected, never a conversation reset
    /// or a permission escalation (architecture §3.4).
    policy_locked: bool,
    /// The exclusive owner-only child DSH_HOME lease of the sealed recipe
    /// (resolved path + pinned anchor/leaf descriptors), retained until
    /// its runtime close is confirmed.
    sealed_home: Option<SealedLease>,
    /// Session-associated retained lease (fix wave 3): a sealed-switch
    /// start whose immediate anchored deletion failed keeps its exact
    /// anchored lease path HERE (not only in the provider-global set);
    /// the final close must reconcile this specific lease before the
    /// session record may be removed.
    failed_lease: Option<PathBuf>,
    /// A close currently in flight that the FINAL close must resolve
    /// first: the recipe switch stores its ordinary close here (consumed
    /// on confirmed success); the final close task takes and awaits it so
    /// a failed switch never leaves an ownerless close. The final close
    /// itself lives in [`NativeSession::cleanup_slot`], outside this
    /// mutex, so shutdown admission never waits behind an active run.
    cleanup: Option<CleanupWait>,
}

/// Internal state for a managed dsh native session.
struct NativeSession {
    /// Per-session lock around the crate harness and session metadata
    /// (B-2). A run holds it across `Session::run`; shutdown admission
    /// never acquires it on the caller path (fix wave).
    state: Arc<Mutex<ClientState>>,
    /// Shutdown intent, set WITHOUT the per-session mutex so new executes
    /// are rejected immediately even while a run holds the lock (fix
    /// wave: the configured shutdown deadline covers lock acquisition;
    /// admission never waits behind an active `Session::run`).
    closing: Arc<AtomicBool>,
    /// The ONE retained final-close owner/result, behind a synchronous
    /// cell held only for an atomic check-and-insert (never across an
    /// await, never behind the run mutex): concurrent/retried shutdowns
    /// observe the same shared completion.
    cleanup_slot: Arc<std::sync::Mutex<Option<CleanupWait>>>,
    /// Working directory for the runtime, retained from `LaunchSpec::cwd`
    /// (N-1).
    cwd: PathBuf,
    /// The canonical absolute runtime executable resolved at `launch()`;
    /// reused for the sealed switch so a mid-session binary replacement
    /// can never swap the runtime identity under a live session.
    executable: PathBuf,
}
impl std::fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("NativeSession");
        debug.field("cwd", &self.cwd.display());
        debug.field("executable", &self.executable.display());
        debug.field("closing", &self.closing.load(Ordering::Acquire));
        match self.state.try_lock() {
            Ok(state) => {
                debug
                    .field("dsh_session_id", &state.dsh_session_id)
                    .field("recipe", &state.recipe)
                    .field("policy_locked", &state.policy_locked)
                    .field("harness_started", &state.harness.is_some())
                    .field("closed", &state.closed);
            }
            Err(_) => {
                debug.field("client_state", &"<locked>");
            }
        }
        debug.finish()
    }
}

/// `DeepSeek Harness` native provider.
///
/// Spawns the official `dsh` runtime (bring-your-own: explicit configured
/// command/path → nonblank parent `DSH_RUNTIME_BIN` → PATH `dsh`) via the
/// `deepseek-harness-sdk` crate and normalizes each `Session::run` outcome
/// into `HostEvent` items. Multi-turn continuity is the host-generated DSH
/// session id reused via `start_session(Some(id))` (AR-2/AR-5).
pub struct DshNativeProvider {
    /// Provider ID (typically `dsh-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Explicit configured command/path override for the runtime
    /// executable. `None` selects the normal chain (nonblank parent
    /// `DSH_RUNTIME_BIN` → PATH `dsh`). Resolution and validation happen in
    /// [`resolve_dsh_executable`] at launch/probe; the canonical absolute
    /// result is passed to the SDK as `Config.dsh_bin`.
    dsh_bin: Option<String>,
    /// Environment variables to inject into the runtime process (one
    /// immutable snapshot handed to the SDK at harness start; the SDK owns
    /// the spawn merge and DSH_HOME normalization). The dsh runtime argv
    /// is exactly `--profile sdk [--patch <owned asset>]` composed by the
    /// SDK, so any nonempty `native_args` at construction is unsupported
    /// and rejected — never silently dropped, never stored.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
    /// Exact lease paths whose removal previously failed (fix wave 2),
    /// retried by later shutdown/probe reconciliation; only a confirmed
    /// removal clears an entry.
    retained_leases: RetainedLeases,
}

impl DshNativeProvider {
    /// Create a new `DeepSeek Harness` provider with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns `HostError::CapabilityUnsupported` when `native_args` is
    /// nonempty: the SDK composes the exact argv (`--profile sdk`, plus the
    /// owned patch for the sealed recipe) and arbitrary native arguments
    /// are unsupported — rejected at construction, never dropped.
    pub fn new(
        provider_id: ProviderId,
        display_name: String,
        dsh_bin: Option<String>,
        native_args: Vec<String>,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> HostResult<Self> {
        if !native_args.is_empty() {
            return Err(HostError::capability_unsupported(
                provider_id,
                "native args",
                "dsh runtime argv is exactly `--profile sdk` (plus the owned deny-all patch); \
                 native arguments are unsupported",
            ));
        }
        Ok(Self {
            provider_id,
            display_name,
            dsh_bin,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            retained_leases: Arc::new(std::sync::Mutex::new(BTreeSet::new())),
            timeouts,
        })
    }

    /// Create with default configuration: no explicit executable override,
    /// so resolution falls to a nonblank parent `DSH_RUNTIME_BIN` and then
    /// the bare command `dsh` on PATH.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("dsh-native"),
            "DeepSeek Harness (native)".to_string(),
            None,
            Vec::new(),
            HashMap::new(),
            TimeoutConfig::default(),
        )
        .expect("empty native args are always accepted")
    }

    /// Register-time constructor used by daemon boot (P0 T2): `dsh_bin` is
    /// the boot-resolved canonical executable (explicit/env/PATH chain
    /// resolved once at registration), or `None` to resolve at launch.
    #[must_use]
    pub fn with_dsh_bin(dsh_bin: Option<String>) -> Self {
        Self::new(
            ProviderId::new("dsh-native"),
            "DeepSeek Harness (native)".to_string(),
            dsh_bin,
            Vec::new(),
            HashMap::new(),
            TimeoutConfig::default(),
        )
        .expect("empty native args are always accepted")
    }

    /// Compose the SDK launch configuration for one recipe against the
    /// canonical executable. Ordinary argv is exactly `--profile sdk`; the
    /// sealed recipe adds the owned absolute no-tools patch and the
    /// exclusive child `dsh_home`. The configured initialize budget
    /// (default 15s) maps to `initialize_timeout` and the session/prompt
    /// request budget to `request_timeout`; the full-run deadline is the
    /// caller-side `tokio::time::timeout` around `Session::run`.
    fn sdk_config(
        &self,
        cwd: &Path,
        executable: &Path,
        sealed_home: Option<&Path>,
    ) -> Config {
        Config {
            cwd: Some(cwd.to_path_buf()),
            dsh_bin: Some(executable.to_string_lossy().into_owned()),
            env: Some(self.env.clone()),
            initialize_timeout: Some(self.timeouts.initialize_duration()),
            request_timeout: Some(self.timeouts.prompt_duration()),
            dsh_home: sealed_home.map(Path::to_path_buf),
            patches: sealed_home.map_or_else(Vec::new, |home| {
                vec![home.join(DENY_ALL_PATCH_FILENAME)]
            }),
            ..Config::default()
        }
    }

    /// Spawn and initialize one harness, mapping every SDK failure into
    /// the typed launch-class error with safe static diagnostics (P0 T2:
    /// recipe-initialization failure surfaces before any prompt admission;
    /// P2 consumes this class for manager health).
    async fn start_harness(&self, config: Config) -> HostResult<DeepSeekHarness> {
        DeepSeekHarness::start(config).await.map_err(|error| {
            let (_category, message) = classify_error_parts(&error);
            HostError::launch_failed(self.provider_id.clone(), message, None)
        })
    }


    /// Initialize the sealed deny-all harness: provision the exclusive
    /// child home (its pinned lease is retained until confirmed close),
    /// revalidate the anchor through the pinned descriptors IMMEDIATELY
    /// before the SDK start handoff, then start the runtime against it.
    /// A failed start removes the fresh lease through the anchored
    /// pinned descriptors — the SDK ran its close ladder over the failed
    /// boot, so the child is reaped and no runtime can still reference
    /// the files; a removal failure there retains the exact path for
    /// per-session reconciliation (fix wave 3).
    async fn start_sealed_harness(
        &self,
        cwd: &Path,
        executable: &Path,
    ) -> Result<(DeepSeekHarness, SealedLease), SealedStartFailure> {
        let launch_error = |message: String| SealedStartFailure {
            error: HostError::launch_failed(self.provider_id.clone(), message, None),
            undeleted_lease: None,
        };
        let lease = provision_sealed_home(&self.env).map_err(&launch_error)?;
        // The anchor guard must live through the SDK start handoff:
        // revalidate that the lexical child/patch still resolve to the
        // owned inode/files before the runtime is pointed at them.
        if let Err(message) = lease.revalidate_for_start() {
            let undeleted_lease = self.discard_failed_lease(lease);
            return Err(SealedStartFailure {
                error: HostError::launch_failed(self.provider_id.clone(), message, None),
                undeleted_lease,
            });
        }
        match self
            .start_harness(self.sdk_config(cwd, executable, Some(&lease.path)))
            .await
        {
            Ok(harness) => Ok((harness, lease)),
            Err(error) => {
                let undeleted_lease = self.discard_failed_lease(lease);
                Err(SealedStartFailure { error, undeleted_lease })
            }
        }
    }

    /// Delete a fresh lease whose sealed start failed, through the
    /// anchored pinned descriptors. On removal failure the exact path is
    /// retained in the provider reconciliation set AND returned so the
    /// caller can associate it with its own scope (the session).
    #[cfg(unix)]
    fn discard_failed_lease(&self, lease: SealedLease) -> Option<PathBuf> {
        if remove_lease_tree(&lease).is_err() {
            retain_lease_path(&self.retained_leases, lease.path.clone());
            return Some(lease.path);
        }
        None
    }

    /// Non-unix counterpart: sealed leases never exist (provisioning
    /// fails closed); unreachable in practice.
    #[cfg(not(unix))]
    fn discard_failed_lease(&self, _lease: SealedLease) -> Option<PathBuf> {
        None
    }

    /// Probe both recipes without a model call (P0 T2): resolve the
    /// executable, initialize + close the ordinary harness, then
    /// provision, revalidate, initialize, close and delete the sealed child
    /// home. Any failure leaves the provider unavailable with a safe static
    /// reason. Retained ownership (fix waves): each recipe's start +
    /// confirmed close — and the WHOLE sealed recipe from the moment its
    /// lease is provisioned — runs through a RETAINED spawned owner
    /// installed before the first await the outer probe deadline can
    /// cancel, so the probe deadline can drop the wait, never the owner:
    /// a timeout during EITHER recipe's initialization cannot drop the
    /// only path owner (confirmed child termination/close still reaps
    /// the direct child and drives lease removal; no reliance on SDK
    /// `kill_on_drop`).
    /// Availability requires a CONFIRMED close of both recipes — a close
    /// error or a lease-removal failure reports unavailable with the
    /// exact path retained for reconciliation, never healthy with a
    /// leaked lease. Previously retained lease paths get one exact-path
    /// removal retry at the start (reconciliation, no scans).
    async fn probe_recipes(&self) -> HostResult<ProviderHealth> {
        let unavailable = |message: String| ProviderHealth {
            provider_id: self.provider_id.clone(),
            available: false,
            latency_ms: None,
            message: Some(message),
        };
        // Reconcile exact retained lease paths from earlier failed
        // removals; entries that still fail stay retained.
        reconcile_retained_leases(&self.retained_leases);
        // An unresolvable/invalid override is an unavailable health value
        // (the pre-T2 probe contract), not a probe error.
        let executable = match resolve_dsh_executable(self.dsh_bin.as_deref()) {
            Ok(executable) => executable,
            Err(message) => return Ok(unavailable(message)),
        };
        let cwd = std::env::current_dir()
            .map_err(|_io| HostError::internal("probe could not resolve the current directory"))?;

        // The ordinary recipe's start + confirmed close run as ONE
        // retained spawned owner (tri-QC F-001, seat 2), installed before
        // the first await the outer probe deadline can cancel — symmetric
        // with the sealed owner below. A waiter timeout drops the wait,
        // never the only owner of the spawned ordinary child; the owner
        // still drives the start to completion and confirms the close,
        // reaping the direct child.
        let ordinary_config = self.sdk_config(&cwd, &executable, None);
        let ordinary_retained = Arc::clone(&self.retained_leases);
        let ordinary_cleanup = async move {
            match DeepSeekHarness::start(ordinary_config).await {
                Ok(ordinary) => close_and_reap(Some(ordinary), None, &ordinary_retained)
                    .await
                    .map_err(|failure| CleanupError {
                        message: format!(
                            "ordinary dsh recipe close was not confirmed: {}",
                            failure.message
                        ),
                        ..failure
                    }),
                Err(error) => {
                    // The SDK ran its close ladder over the failed boot
                    // (child reaped); only the safe classified category
                    // text leaves this boundary.
                    let (_category, message) = classify_error_parts(&error);
                    Err(CleanupError {
                        message: format!("ordinary dsh recipe failed to initialize: {message}"),
                        close_confirmed: true,
                        failed_lease: None,
                    })
                }
            }
        }
        .boxed()
        .shared();
        tokio::spawn(ordinary_cleanup.clone());
        if let Err(failure) = ordinary_cleanup.await {
            return Ok(unavailable(failure.message));
        }

        // The sealed lease is provisioned synchronously so its exact
        // path (and pinned anchor descriptors) are owned from the first
        // moment; the retained spawned owner then drives revalidation ->
        // start -> confirmed close -> anchored lease removal.
        let lease = match provision_sealed_home(&self.env) {
            Ok(lease) => lease,
            Err(message) => {
                return Ok(unavailable(format!(
                    "sealed dsh recipe failed to initialize: {message}"
                )));
            }
        };
        let config = self.sdk_config(&cwd, &executable, Some(&lease.path));
        let retained = Arc::clone(&self.retained_leases);
        let sealed_cleanup = async move {
            // The anchor guard lives through the SDK start handoff:
            // revalidate the owned inode/files immediately before start.
            if let Err(message) = lease.revalidate_for_start() {
                if remove_lease_tree(&lease).is_err() {
                    retain_lease_path(&retained, lease.path.clone());
                }
                return Err(CleanupError {
                    message: format!("sealed dsh recipe failed to initialize: {message}"),
                    close_confirmed: true,
                    failed_lease: None,
                });
            }
            match DeepSeekHarness::start(config).await {
                Ok(sealed) => close_and_reap(Some(sealed), Some(lease), &retained).await,
                Err(error) => {
                    // The SDK ran its close ladder over the failed boot
                    // (child reaped); remove the fresh lease through the
                    // anchored pinned descriptors, retaining the exact
                    // path if even that removal fails.
                    let (_category, message) = classify_error_parts(&error);
                    if remove_lease_tree(&lease).is_err() {
                        retain_lease_path(&retained, lease.path.clone());
                    }
                    Err(CleanupError {
                        message: format!("sealed dsh recipe failed to initialize: {message}"),
                        close_confirmed: true,
                        failed_lease: None,
                    })
                }
            }
        }
        .boxed()
        .shared();
        tokio::spawn(sealed_cleanup.clone());
        if let Err(failure) = sealed_cleanup.await {
            return Ok(unavailable(failure.message));
        }

        Ok(ProviderHealth {
            provider_id: self.provider_id.clone(),
            available: true,
            latency_ms: None,
            // Static success text (tri-QC F-001, seat 1): this public
            // serialized diagnostic must disclose neither the resolved
            // executable path nor any runtime/caller home path
            // (architecture §3.1).
            message: Some("ordinary and sealed recipes initialized and closed".to_string()),
        })
    }

    /// Build the event stream for one admitted turn.
    ///
    /// The admission (scope validation, recipe selection/switch, policy
    /// lock) already happened in `execute`; one turn = one `Session::run`
    /// on the session's harness, driven lazily on the first stream poll
    /// under the per-session lock (B-2). Exactly one `OpStarted` precedes
    /// the mapped result events (P0 conformance cutover); the run is
    /// wrapped in `tokio::time::timeout(prompt_duration)` because the
    /// SDK's inbox-receipt / root-idle waits are unbounded by design.
    /// Emits at most one terminal event:
    ///
    /// - `Ok(RunResult)` → exactly one `MessageDelta(final_response)` + one
    ///   `OpFinished` (AR-1 dsh table / AR-6).
    /// - `tokio::time::error::Elapsed` → one `OpFailed(timeout)`.
    /// - `Err(DshError)` → one `OpFailed` from `classify_run_error` (AR-7).
    /// - Session closed by `shutdown()` before the run starts → one
    ///   `OpFailed(stream_closed)` (stream-abort backstop).
    #[allow(clippy::too_many_lines)]
    fn build_event_stream(
        &self,
        op_id: HostOperationId,
        session_id: HostSessionId,
        prompt_text: String,
        state: Arc<Mutex<ClientState>>,
        closing: Arc<AtomicBool>,
    ) -> HostEventStream {
        let retained = Arc::clone(&self.retained_leases);
        let run_timeout = self.timeouts.prompt_duration();
        let prompt_ms = self.timeouts.prompt_ms;
        let budget = Arc::new(DeliveryBudget::default());
        let op_id_for_stream = op_id.clone();
        let session_id_for_stream = session_id.clone();

        let (content_tx, content_rx) = mpsc::channel(DSCH_MAX_QUEUED_MESSAGES);
        let (terminal_tx, terminal_rx) = mpsc::channel(1);

        let budget_for_observer = Arc::clone(&budget);
        let consumer_dropped = Arc::new(AtomicBool::new(false));
        let consumer_drop_notify = Arc::new(Notify::new());
        let stream_finished = Arc::new(AtomicBool::new(false));
        let consumer_dropped_for_task = Arc::clone(&consumer_dropped);
        let consumer_drop_notify_for_task = Arc::clone(&consumer_drop_notify);
        tokio::spawn(async move {
            let consumer_dropped = consumer_dropped_for_task;
            let consumer_drop_notify = consumer_drop_notify_for_task;
            let (tail_events, invalidate) = {
                let reconciliation = Arc::new(StdMutex::new(RunReconciliation::new()));
                let classifier_failed = Arc::new(AtomicBool::new(false));
                let delivery_failed = Arc::new(AtomicBool::new(false));
                let failure_notify = Arc::new(Notify::new());
                let run_timing = Arc::new(StdMutex::new(DshStreamingRunTiming::default()));
                {
                    let mut slot = LAST_DSH_RUN_TIMING.lock().unwrap_or_else(|e| e.into_inner());
                    *slot = Some(run_timing.lock().unwrap_or_else(|e| e.into_inner()).clone());
                }

                let run_harness = Arc::new(Mutex::new(None::<DeepSeekHarness>));
                let run_harness_for_run = Arc::clone(&run_harness);

                let run = async {
                    let root_session_id = {
                        let mut guard = state.lock().await;
                        if guard.closed || closing.load(Ordering::Acquire) {
                            return Err(RunError::SessionGone);
                        }
                        let root_session_id = guard.dsh_session_id.clone();
                        let Some(harness) = guard.harness.take() else {
                            return Err(RunError::SessionGone);
                        };
                        *run_harness_for_run.lock().await = Some(harness);
                        root_session_id
                    };

                    let observer = Arc::new(StreamObserver {
                        root_session_id: root_session_id.clone(),
                        session_id: session_id.clone(),
                        op_id: op_id.clone(),
                        content_tx: content_tx.clone(),
                        reconciliation: Arc::clone(&reconciliation),
                        classifier_failed: Arc::clone(&classifier_failed),
                        delivery_failed: Arc::clone(&delivery_failed),
                        budget: Arc::clone(&budget_for_observer),
                        failure_notify: Arc::clone(&failure_notify),
                        run_timing: Arc::clone(&run_timing),
                    });
                    let obs = Arc::clone(&observer);
                    let mut slot = run_harness_for_run.lock().await;
                    let harness = slot.as_mut().ok_or(RunError::SessionGone)?;
                    let session = harness.start_session(Some(root_session_id));
                    session
                        .run(Input::Text(prompt_text), Some(&|n| obs.observe(n)))
                        .await
                        .map_err(RunError::Sdk)
                };

                let run_outcome = if delivery_failure_signaled(
                    &classifier_failed,
                    &delivery_failed,
                ) {
                    RunSelectOutcome::FailureSignal
                } else if consumer_drop_signaled(&consumer_dropped) {
                    RunSelectOutcome::ReceiverClosed
                } else {
                    tokio::select! {
                        biased;
                        _ = wait_delivery_failure(
                            &failure_notify,
                            &classifier_failed,
                            &delivery_failed,
                        ) => RunSelectOutcome::FailureSignal,
                        _ = wait_consumer_drop(&consumer_dropped, &consumer_drop_notify) => {
                            RunSelectOutcome::ReceiverClosed
                        }
                        result = tokio::time::timeout(run_timeout, run) => match result {
                            Ok(inner) => RunSelectOutcome::Run(inner),
                            Err(_elapsed) => RunSelectOutcome::TimedOut,
                        },
                    }
                };

                reunify_run_harness(&run_harness, &state).await;

                match run_outcome {
                    RunSelectOutcome::FailureSignal => {
                        if classifier_failed.load(Ordering::Acquire) {
                            (
                                vec![HostEvent::OpFailed(operation_protocol_failure(
                                    &session_id,
                                    &op_id,
                                ))],
                                true,
                            )
                        } else {
                            (
                                vec![HostEvent::OpFailed(operation_delivery_overflow_failure(
                                    &session_id,
                                    &op_id,
                                ))],
                                true,
                            )
                        }
                    }
                    RunSelectOutcome::ReceiverClosed => (
                        vec![HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "stream_closed".to_string(),
                            error_message: "dsh event stream closed before the turn completed"
                                .to_string(),
                        })],
                        true,
                    ),
                    RunSelectOutcome::TimedOut => (
                        vec![HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "timeout".to_string(),
                            error_message: format!("dsh turn timed out after {prompt_ms}ms"),
                        })],
                        true,
                    ),
                    RunSelectOutcome::Run(Ok(result)) => {
                        if classifier_failed.load(Ordering::Acquire) {
                            (
                                vec![HostEvent::OpFailed(operation_protocol_failure(
                                    &session_id,
                                    &op_id,
                                ))],
                                true,
                            )
                        } else if delivery_failed.load(Ordering::Acquire) {
                            (
                                vec![HostEvent::OpFailed(operation_delivery_overflow_failure(
                                    &session_id,
                                    &op_id,
                                ))],
                                true,
                            )
                        } else {
                            {
                                let mut timing =
                                    run_timing.lock().unwrap_or_else(|e| e.into_inner());
                                timing.run_completed = Some(Instant::now());
                            }
                            let snapshot = reconciliation.lock().unwrap().clone();
                            match finalize_successful_run(
                                &result,
                                &snapshot,
                                &session_id,
                                &op_id,
                            ) {
                                Ok(events) => (events, false),
                                Err(failed) => (vec![HostEvent::OpFailed(failed)], true),
                            }
                        }
                    }
                    RunSelectOutcome::Run(Err(RunError::SessionGone)) => (
                        vec![HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "stream_closed".to_string(),
                            error_message: "dsh session closed before the turn completed"
                                .to_string(),
                        })],
                        true,
                    ),
                    RunSelectOutcome::Run(Err(RunError::Sdk(error))) => (
                        vec![HostEvent::OpFailed(classify_run_error(
                            &error,
                            &session_id,
                            &op_id,
                        ))],
                        true,
                    ),
                }
            };

            if invalidate {
                invalidate_session_after_failed_operation(state, retained).await;
            }

            drop(content_tx);
            for event in tail_events {
                let _ = terminal_tx.send(event).await;
            }
        });

        let stream_finished_for_guard = Arc::clone(&stream_finished);
        let consumer_dropped_for_guard = Arc::clone(&consumer_dropped);
        let consumer_drop_notify_for_guard = Arc::clone(&consumer_drop_notify);
        futures_util::stream::unfold(
            (
                false,
                false,
                content_rx,
                terminal_rx,
                budget,
                VecDeque::<HostEvent>::new(),
                op_id_for_stream.clone(),
                session_id_for_stream.clone(),
                StreamConsumerDropGuard {
                    stream_finished: stream_finished_for_guard,
                    consumer_dropped: consumer_dropped_for_guard,
                    consumer_drop_notify: consumer_drop_notify_for_guard,
                },
            ),
            |(
                started,
                mut finished,
                mut content_rx,
                mut terminal_rx,
                budget,
                mut pending,
                op_id,
                session_id,
                drop_guard,
            )| async move {
                if let Some(event) = pending.pop_front() {
                    return Some((
                        Ok(event),
                        (
                            started,
                            finished,
                            content_rx,
                            terminal_rx,
                            budget,
                            pending,
                            op_id,
                            session_id,
                            drop_guard,
                        ),
                    ));
                }
                if finished {
                    drop_guard.mark_stream_finished();
                    return None;
                }
                if !started {
                    return Some((
                        Ok(HostEvent::OpStarted(OperationStartedEvent {
                            op_id: op_id.clone(),
                            session_id: session_id.clone(),
                        })),
                        (
                            true,
                            finished,
                            content_rx,
                            terminal_rx,
                            budget,
                            pending,
                            op_id,
                            session_id,
                            drop_guard,
                        ),
                    ));
                }

                if let Ok(payload) = content_rx.try_recv() {
                    let event = payload.into_event();
                    return Some((
                        Ok(event),
                        (
                            started,
                            finished,
                            content_rx,
                            terminal_rx,
                            budget,
                            pending,
                            op_id,
                            session_id,
                            drop_guard,
                        ),
                    ));
                }

                match content_rx.recv().await {
                    Some(payload) => {
                        let event = payload.into_event();
                        Some((
                            Ok(event),
                            (
                                started,
                                finished,
                                content_rx,
                                terminal_rx,
                                budget,
                                pending,
                                op_id,
                                session_id,
                                drop_guard,
                            ),
                        ))
                    }
                    None => {
                        if let Some(event) = terminal_rx.recv().await {
                            pending.push_back(event);
                            while let Ok(more) = terminal_rx.try_recv() {
                                pending.push_back(more);
                            }
                            finished = true;
                            let next = pending.pop_front().expect("terminal batch");
                            Some((
                                Ok(next),
                                (
                                    started,
                                    finished,
                                    content_rx,
                                    terminal_rx,
                                    budget,
                                    pending,
                                    op_id,
                                    session_id,
                                drop_guard,
                                ),
                            ))
                        } else {
                            drop_guard.mark_stream_finished();
                            None
                        }
                    }
                }
            },
        )
        .boxed()
    }

}


enum RunSelectOutcome {
    FailureSignal,
    ReceiverClosed,
    TimedOut,
    Run(Result<RunResult, RunError>),
}

/// Failure of the turn's `Session::run` attempt.
enum RunError {
    /// Session closed by `shutdown()` before the turn could run.
    SessionGone,
    /// SDK error from `Session::run`.
    Sdk(DshError),
}

#[async_trait]
impl ProviderAdapter for DshNativeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: self.provider_id.clone(),
            display_name: self.display_name.clone(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: CapabilityDescriptor::dsh_limited(),
        }
    }

    async fn probe(
        &self,
        request: crate::capability::model::ProbeRequest,
    ) -> HostResult<ProviderHealth> {
        // P0 T2: the probe initializes and closes BOTH recipes without a
        // model call (bounded by the caller's probe budget). Resolution
        // failures, recipe-initialization failures and unconfirmed closes
        // all leave the provider unavailable with safe static text.
        match tokio::time::timeout(
            std::time::Duration::from_millis(request.timeout_ms),
            self.probe_recipes(),
        )
        .await
        {
            Ok(result) => result,
            Err(_elapsed) => Err(HostError::timeout(
                "probe",
                format!("dsh recipe probe timed out after {}ms", request.timeout_ms),
            )
            .with_provider(self.provider_id.clone())),
        }
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        // Fail closed on injected MCP servers: the dsh runtime composition
        // carries no MCP layer and Nexus never injects one.
        if !spec.mcp_servers.is_empty() {
            return Err(HostError::capability_unsupported(
                self.provider_id.clone(),
                "mcp servers",
                "the dsh native provider does not accept MCP server injection",
            ));
        }

        // Executable resolution (P0 T2): explicit configured command/path →
        // nonblank parent DSH_RUNTIME_BIN → PATH `dsh`; an invalid override
        // fails here without fallback. Only the initialize handshake below
        // proves readiness.
        let executable = resolve_dsh_executable(self.dsh_bin.as_deref())
            .map_err(|message| HostError::provider_unavailable(self.provider_id.clone(), message))?;

        // Initialize BEFORE returning the managed Ready handle (P0 T2): a
        // failed spawn/initialize is the typed launch-class failure.
        let harness = self
            .start_harness(self.sdk_config(&spec.cwd, &executable, None))
            .await?;

        let host_session_id = HostSessionId::new();

        // Host-generated DSH session id: start_session(Some(id)) reuses it
        // across executes; the runtime lazily creates the agent+session
        // pair on the first run and resumes it afterwards (AR-2/AR-5).
        let dsh_session_id = uuid::Uuid::new_v4().to_string();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    state: Arc::new(Mutex::new(ClientState {
                        dsh_session_id,
                        harness: Some(harness),
                        closed: false,
                        recipe: Recipe::Ordinary,
                        policy_locked: false,
                        sealed_home: None,
                        failed_lease: None,
                        cleanup: None,
                    })),
                    closing: Arc::new(AtomicBool::new(false)),
                    cleanup_slot: Arc::new(std::sync::Mutex::new(None)),
                    cwd: spec.cwd.clone(),
                    executable,
                },
            );
        }

        tracing::info!(
            session_id = %host_session_id,
            provider_id = %self.provider_id,
            cwd = %spec.cwd.display(),
            "dsh session launched (ordinary recipe initialized)"
        );

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::dsh_limited(),
            process_identity: None,
        })
    }

    // One admission plus one turn; the recipe switch (close ordinary,
    // initialize sealed) runs inline before the stream is built so a
    // recipe-initialization failure returns as the typed execute error
    // before prompt admission. Allow the line count here.
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let crate::capability::model::HostOperation::Prompt {
            op_id,
            content,
            permission_scope,
        } = op
        else {
            return Err(HostError::capability_unsupported(
                self.provider_id.clone(),
                "non-prompt operation",
                "Native CLI provider only supports Prompt operations",
            ));
        };

        // Scope → recipe BEFORE operation admission (P0 T2): the all-false
        // scope selects deny_all; partially permissive scopes are
        // unsupported; None preserves the ordinary policy.
        let desired = recipe_for_scope(&self.provider_id, permission_scope)?;

        // Build prompt text from content blocks.
        let prompt_text: String = content
            .iter()
            .map(|block| match block {
                HostContentBlock::Text { text } => text.as_str(),
                HostContentBlock::ResourceLink { uri, .. } => uri.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        if prompt_text.is_empty() {
            return Err(HostError::protocol_error(
                "empty prompt text for native CLI",
                None,
            ));
        }

        // Fetch the session state under a short registry read (B-2), then
        // admit under the per-session lock only.
        let (state, closing, cwd, executable) = {
            let sessions = self.sessions.read().await;
            let native_session = sessions.get(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in native CLI provider",
                    session.session_id
                ))
            })?;
            (
                Arc::clone(&native_session.state),
                Arc::clone(&native_session.closing),
                native_session.cwd.clone(),
                native_session.executable.clone(),
            )
        };

        // Shutdown intent rejects new executes IMMEDIATELY (fix wave):
        // the flag is set without the per-session mutex, so a run in
        // flight can never delay this rejection.
        if closing.load(Ordering::Acquire) {
            return Err(HostError::internal(format!(
                "dsh session {} is closing or closed",
                session.session_id
            )));
        }

        {
            let mut guard = state.lock().await;
            if guard.closed || closing.load(Ordering::Acquire) {
                return Err(HostError::internal(format!(
                    "dsh session {} is closing or closed",
                    session.session_id
                )));
            }
            if guard.policy_locked && guard.recipe != desired {
                // Once a prompt has been admitted the recipe is retained:
                // reject policy changes (deny_all ↔ None included) rather
                // than losing the conversation or escalating permissions.
                return Err(HostError::policy_denied(
                    "the permission scope cannot change after the first admitted prompt; \
                     the session recipe is retained",
                )
                .with_provider(self.provider_id.clone())
                .with_session(session.session_id.clone()));
            }
            if !guard.policy_locked && desired == Recipe::SealedDenyAll {
                // First deny-all prompt: close the already-initialized
                // ordinary harness through the retained cleanup owner and
                // await a CONFIRMED close before initializing the sealed
                // harness. The prompt is never sent to the ordinary
                // runtime; any failure fails closed (never a normal-profile
                // fallback).
                let harness = guard.harness.take();
                let cleanup = start_close(
                    harness,
                    guard.sealed_home.take(),
                    Arc::clone(&self.retained_leases),
                );
                guard.cleanup = Some(cleanup.clone());
                let close_result = tokio::time::timeout(
                    self.timeouts.shutdown_duration(),
                    cleanup,
                )
                .await;
                match close_result {
                    Ok(Ok(())) => {
                        // Consumed confirmed close: clear the record so a
                        // later shutdown owns the sealed harness's close.
                        guard.cleanup = None;
                    }
                    Ok(Err(failure)) => {
                        guard.closed = true;
                        return Err(HostError::cleanup_unconfirmed(failure.message)
                            .with_provider(self.provider_id.clone())
                            .with_session(session.session_id.clone()));
                    }
                    Err(_elapsed) => {
                        guard.closed = true;
                        return Err(HostError::cleanup_unconfirmed(
                            "ordinary dsh harness close was not confirmed within the shutdown \
                             budget; the cleanup owner is retained",
                        )
                        .with_provider(self.provider_id.clone())
                        .with_session(session.session_id.clone()));
                    }
                }
                let (sealed, lease) = match self.start_sealed_harness(&cwd, &executable).await
                {
                    Ok(pair) => pair,
                    Err(failure) => {
                        // Every sealed-home/start failure permanently
                        // closes the session (fix wave): no retryable
                        // half-switched state, no ordinary fallback, and
                        // later None/deny executes are rejected
                        // immediately. When even the immediate anchored
                        // deletion failed, the exact lease is associated
                        // with THIS session (fix wave 3): the final close
                        // must reconcile it before the record may be
                        // removed. The switch cleanup/result state is
                        // preserved for the final close owner.
                        if let Some(path) = failure.undeleted_lease {
                            guard.failed_lease = Some(path);
                        }
                        guard.closed = true;
                        return Err(failure.error.with_session(session.session_id.clone()));
                    }
                };
                guard.harness = Some(sealed);
                guard.sealed_home = Some(lease);
                guard.recipe = Recipe::SealedDenyAll;
                tracing::info!(
                    session_id = %session.session_id,
                    provider_id = %self.provider_id,
                    "dsh session switched to the sealed deny_all recipe"
                );
            }
            // The prompt is admitted: lock the recipe against later scope
            // changes (first admitted prompt only).
            guard.policy_locked = true;
        }

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            op_id = %op_id,
            "dsh turn started"
        );

        Ok(self.build_event_stream(
            op_id,
            session.session_id.clone(),
            prompt_text,
            state,
            closing,
        ))
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()> {
        // AR-6: the SDK exposes no cancel / session-close RPC, and killing
        // the runtime mid-turn would abandon the turn (documented crate
        // non-goal). Honest no-op: the turn runs to completion (or the
        // turn timeout) and the terminal still arrives on the stream.
        tracing::info!(
            session_id = %session.session_id,
            op_id = %op_id,
            provider_id = %self.provider_id,
            "dsh cancel: no-op (the SDK has no cancel RPC; AR-6)",
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // P0 T2 lifecycle ownership (fix wave): close admission happens
        // IMMEDIATELY — the shutdown intent is set and the retained final
        // close owner is established WITHOUT acquiring the per-session
        // mutex a run may be holding, so the configured `shutdown_ms`
        // waiter deadline covers everything the caller waits on. The
        // spawned final-close task itself acquires that mutex unbounded
        // (it is the retained owner), marks the session closed, resolves
        // any pending switch close, then closes the current harness and
        // reaps the lease. A waiter timeout never cancels the cleanup
        // task, never deletes the session record or lease, and
        // concurrent/retried shutdowns observe the SAME shared
        // completion; only a confirmed close removes the session.
        let session_parts = {
            let sessions = self.sessions.read().await;
            sessions
                .get(&session.session_id)
                .map(|native_session| {
                    (
                        Arc::clone(&native_session.state),
                        Arc::clone(&native_session.closing),
                        Arc::clone(&native_session.cleanup_slot),
                    )
                })
        };
        let Some((state, closing, cleanup_slot)) = session_parts else {
            // A previous shutdown already observed the confirmed close and
            // removed the record: this is the retained completed result,
            // never a new reap of an already-closed client.
            return Ok(());
        };
        // Reject new executes immediately, before any waiting.
        closing.store(true, Ordering::Release);
        // Establish (or observe) the ONE final-close owner atomically:
        // this synchronous cell is never held across an await and never
        // behind the run mutex.
        let cleanup = {
            let mut slot = cleanup_slot.lock().unwrap_or_else(|error| {
                // A poisoned cell can only mean a panic between
                // check-and-insert; recover the value rather than
                // abandoning the cleanup contract.
                error.into_inner()
            });
            match &*slot {
                Some(cleanup) => cleanup.clone(),
                None => {
                    let cleanup = start_final_close(state, Arc::clone(&self.retained_leases));
                    *slot = Some(cleanup.clone());
                    cleanup
                }
            }
        };
        match tokio::time::timeout(self.timeouts.shutdown_duration(), cleanup).await {
            Ok(Ok(())) => {
                let mut sessions = self.sessions.write().await;
                sessions.remove(&session.session_id);
                tracing::info!(
                    session_id = %session.session_id,
                    provider_id = %self.provider_id,
                    "dsh session shut down (close confirmed; session record removed)"
                );
                Ok(())
            }
            Ok(Err(failure)) => {
                // The close owner completed with an unconfirmed result.
                // If the CLOSE was confirmed and only the lease removal
                // failed, a retried shutdown reconciles: retry removal of
                // the exact retained path(s) (no scans) and — only when
                // this session's failed path is confirmed gone — remove
                // the record and report success (fix wave 2: a completed
                // shared cleanup error alone is insufficient).
                if let (true, Some(failed_path)) =
                    (failure.close_confirmed, &failure.failed_lease)
                {
                    reconcile_retained_leases(&self.retained_leases);
                    let still_retained = self
                        .retained_leases
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .contains(failed_path);
                    if !still_retained {
                        let mut sessions = self.sessions.write().await;
                        sessions.remove(&session.session_id);
                        tracing::info!(
                            session_id = %session.session_id,
                            provider_id = %self.provider_id,
                            "dsh session shut down (lease removal reconciled; session record removed)"
                        );
                        return Ok(());
                    }
                }
                // Propagate the failure; the session record and lease stay
                // registered and a retry observes this same completion.
                Err(HostError::cleanup_unconfirmed(failure.message)
                    .with_provider(self.provider_id.clone())
                    .with_session(session.session_id.clone()))
            }
            Err(_elapsed) => Err(HostError::cleanup_unconfirmed(format!(
                "dsh session close was not confirmed within {}ms; the cleanup owner, session \
                 record and home lease are retained",
                self.timeouts.shutdown_ms
            ))
            .with_provider(self.provider_id.clone())
            .with_session(session.session_id.clone())),
        }
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::dsh_limited()
    }
}

#[cfg(test)]
mod tests {
    // Lock guards (session registry / crate client) are intentionally held
    // to the end of the visible test scope for readability; the nursery
    // significant_drop_tightening suggestion to drop them earlier is noise
    // here. `PROCESS_ENV_LOCK` (lib.rs test_support) is deliberately held
    // across awaits: it serializes python-fixture spawns and
    // `DSH_RUNTIME_BIN` reads against the env-mutating discovery tests
    // (see its doc comment).
    #![allow(clippy::significant_drop_tightening)]
    #![allow(clippy::await_holding_lock)]

    use super::*;
    use crate::capability::model::{FinishReason, HostOperation, LaunchSpec, McpServerConfig};

    const MOCK_DSH_AGENT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/mock_dsh_agent.py"
    );

    fn launch_spec() -> LaunchSpec {
        LaunchSpec {
            cwd: std::path::PathBuf::from("/tmp"),
            model: None,
            mode: None,
            owner: crate::capability::model::SessionOwner {
                creator_id: "ctr_test".to_string(),
                workspace_root: std::path::PathBuf::from("/tmp"),
                orchestration_run_id: None,
            },
            mcp_servers: vec![],
        }
    }

    /// Hermetic environment for stub-backed providers: the request log and
    /// an isolated DSH_HOME (the SDK creates/resolves DSH_HOME at harness
    /// start; the caller-env route keeps tests away from any real home).
    fn stub_env(req_log: &Path, home: &Path) -> HashMap<String, String> {
        HashMap::from([
            (
                "REQ_LOG".to_string(),
                req_log.to_string_lossy().into_owned(),
            ),
            (
                "DSH_HOME".to_string(),
                home.to_string_lossy().into_owned(),
            ),
        ])
    }

    /// A provider backed by the python SDK-wire fixture as its explicit
    /// executable (empty native args).
    fn stub_provider(provider_id: &str, env: HashMap<String, String>) -> DshNativeProvider {
        DshNativeProvider::new(
            ProviderId::new(provider_id),
            "Test".to_string(),
            Some(MOCK_DSH_AGENT.to_string()),
            Vec::new(),
            env,
            TimeoutConfig::default(),
        )
        .expect("empty native args are accepted")
    }

    fn stub_provider_with_timeouts(
        provider_id: &str,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> DshNativeProvider {
        DshNativeProvider::new(
            ProviderId::new(provider_id),
            "Test".to_string(),
            Some(MOCK_DSH_AGENT.to_string()),
            Vec::new(),
            env,
            timeouts,
        )
        .expect("empty native args are accepted")
    }

    /// Launch under the env lock: launch() now initializes the runtime
    /// eagerly, spawning the python fixture whose `#!/usr/bin/env python3`
    /// shebang resolves python3 through PATH at execve time (see lib.rs
    /// test_support).
    #[allow(clippy::future_not_send)]
    async fn launch_hermetic(provider: &DshNativeProvider) -> ManagedSessionHandle {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        provider.launch(launch_spec()).await.expect("launch")
    }

    /// All REQ_LOG entries as parsed JSON values.
    fn req_log_entries(req_log: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(req_log)
            .expect("REQ_LOG written")
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// The `_spawn` records (one per runtime process the provider started).
    fn spawn_records(entries: &[serde_json::Value]) -> Vec<serde_json::Value> {
        entries
            .iter()
            .filter(|value| value["method"] == "_spawn")
            .cloned()
            .collect()
    }

    /// The ordered wire-method sequence (including `_spawn` records).
    fn method_sequence(entries: &[serde_json::Value]) -> Vec<String> {
        entries
            .iter()
            .filter_map(|value| value["method"].as_str().map(str::to_string))
            .collect()
    }

    fn argv_of(spawn: &serde_json::Value) -> Vec<String> {
        spawn["argv"]
            .as_array()
            .expect("argv array")
            .iter()
            .filter_map(|arg| arg.as_str().map(str::to_string))
            .collect()
    }

    async fn collect_events(stream: HostEventStream) -> Vec<HostEvent> {
        let results: Vec<_> = stream.collect().await;
        results
            .into_iter()
            .map(|r| r.expect("stream item should be Ok"))
            .collect()
    }

    fn terminal_count(events: &[HostEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)))
            .count()
    }

    /// Run one prompt turn to completion and return the collected events.
    // The held `PROCESS_ENV_LOCK` guard makes the future !Send; test-only
    // helper, run on tokio's current-thread test runtime (no Send needed).
    #[allow(clippy::future_not_send)]
    async fn run_turn(
        provider: &DshNativeProvider,
        handle: &ManagedSessionHandle,
        text: &str,
    ) -> Vec<HostEvent> {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let stream = provider
            .execute(
                handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: text.to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await
            .expect("execute");
        collect_events(stream).await
    }

    /// The all-false deny_all scope (architecture §3.4 selection).
    const fn deny_all_scope() -> PromptPermissionScope {
        PromptPermissionScope {
            allow_read: false,
            allow_write: false,
            allow_destructive: false,
        }
    }

    /// Execute one prompt with an explicit permission scope.
    #[allow(clippy::future_not_send)]
    async fn run_turn_scoped(
        provider: &DshNativeProvider,
        handle: &ManagedSessionHandle,
        text: &str,
        scope: Option<PromptPermissionScope>,
    ) -> HostResult<Vec<HostEvent>> {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let stream = provider.execute(
            handle,
            HostOperation::Prompt {
                op_id: HostOperationId::new(),
                content: vec![HostContentBlock::Text {
                    text: text.to_string(),
                }],
                permission_scope: scope,
            },
        )
        .await?;
        Ok(collect_events(stream).await)
    }

    /// Read the provider's stored DSH session id for a launched session
    /// (short registry read, then the per-session lock — same order as the
    /// production paths).
    async fn stored_dsh_session_id(
        provider: &DshNativeProvider,
        session_id: &HostSessionId,
    ) -> String {
        let state = {
            let guard = provider.sessions.read().await;
            guard.get(session_id).map(|ns| Arc::clone(&ns.state))
        };
        state
            .expect("session must be registered")
            .lock()
            .await
            .dsh_session_id
            .clone()
    }

    /// RAII guard replacing `PATH` with a single directory on construction
    /// and restoring the previous value on drop (mirrors the
    /// discovery/path_scan.rs test guard).
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
            match &self.previous {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    /// RAII guard replacing `DSH_RUNTIME_BIN` on construction and restoring
    /// the previous value on drop.
    struct DshRuntimeBinGuard {
        previous: Option<String>,
    }

    impl DshRuntimeBinGuard {
        fn set(value: &Path) -> Self {
            let previous = std::env::var(DSH_RUNTIME_BIN_ENV).ok();
            std::env::set_var(DSH_RUNTIME_BIN_ENV, value);
            Self { previous }
        }
        fn remove() -> Self {
            let previous = std::env::var(DSH_RUNTIME_BIN_ENV).ok();
            std::env::remove_var(DSH_RUNTIME_BIN_ENV);
            Self { previous }
        }
    }

    impl Drop for DshRuntimeBinGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(DSH_RUNTIME_BIN_ENV, value),
                None => std::env::remove_var(DSH_RUNTIME_BIN_ENV),
            }
        }
    }

    /// Write an executable stub file (resolution tests only need the
    /// executable bit, not a working runtime).
    fn write_executable(path: &Path) {
        std::fs::write(path, "#!/bin/sh\necho stub\n").expect("write stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).expect("stat").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).expect("chmod +x");
        }
    }

    #[test]
    fn default_config_descriptor() {
        let provider = DshNativeProvider::default_config();
        let desc = provider.descriptor();

        assert_eq!(desc.provider_id.0, "dsh-native");
        assert_eq!(desc.protocol_kind, ProtocolKind::NativeCli);
        assert!(desc.capabilities.text_prompt);
        assert!(
            desc.capabilities.streaming,
            "dsh-native streams committed messages (v1.188 P1, AR-6)"
        );
        assert!(
            !desc.capabilities.cancellation,
            "dsh-native must not claim cancellation (AR-6)"
        );
        assert!(
            desc.capabilities.session_restore,
            "dsh-native claims session_restore via start_session(Some(id)) reuse (AR-2)"
        );
        assert!(!desc.capabilities.structured_tool_calls);
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_has_no_explicit_runtime_override() {
        // P0 T2: the default provider carries NO explicit override — the
        // resolution chain (nonblank parent DSH_RUNTIME_BIN → PATH `dsh`)
        // decides at launch/probe time. The retired `dsh-jsonrpc-agent`
        // helper name appears nowhere.
        let provider = DshNativeProvider::default_config();
        assert!(provider.dsh_bin.is_none());
        assert!(provider.env.is_empty());
        assert_eq!(DEFAULT_DSH_COMMAND, "dsh");
    }

    #[test]
    fn nonempty_native_args_are_rejected_at_construction() {
        let result = DshNativeProvider::new(
            ProviderId::new("args-dsh"),
            "Args".to_string(),
            None,
            vec!["--verbose".to_string()],
            HashMap::new(),
            TimeoutConfig::default(),
        );
        assert!(
            matches!(result, Err(HostError::CapabilityUnsupported { .. })),
            "unsupported native args must fail closed at construction"
        );
    }

    // ── Executable resolution (P0 T2) ────────────────────────────────────

    #[test]
    fn resolve_explicit_absolute_path_is_canonicalized() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let bin = temp_dir.path().join("dsh-custom");
        write_executable(&bin);
        let resolved =
            resolve_dsh_executable(Some(bin.to_string_lossy().as_ref())).expect("absolute path");
        assert!(resolved.is_absolute());
        assert_eq!(
            resolved,
            std::fs::canonicalize(&bin).expect("canonical"),
            "an explicit absolute path is preserved (canonicalized)"
        );
    }

    #[test]
    fn resolve_relative_path_with_separator_is_rejected() {
        for value in ["./dsh", "bin/dsh", "../dsh", "some\\dsh"] {
            assert!(
                resolve_dsh_executable(Some(value)).is_err(),
                "relative path {value:?} must be rejected, never resolved against a daemon cwd"
            );
        }
    }

    #[test]
    fn resolve_explicit_bare_command_uses_path() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let bin = temp_dir.path().join("dsh-custom");
        write_executable(&bin);
        let _path_guard = PathGuard::isolate(temp_dir.path());
        let _bin_guard = DshRuntimeBinGuard::remove();

        let resolved = resolve_dsh_executable(Some("dsh-custom")).expect("bare command");
        assert_eq!(resolved, std::fs::canonicalize(&bin).expect("canonical"));

        assert!(
            resolve_dsh_executable(Some("definitely-missing-dsh-xyz")).is_err(),
            "a missing bare command fails without fallback"
        );
    }

    #[test]
    fn resolve_invalid_explicit_override_never_falls_back() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let env_bin = temp_dir.path().join("env-dsh");
        write_executable(&env_bin);
        // Even with a VALID DSH_RUNTIME_BIN and a PATH `dsh`, an invalid
        // explicit override fails without fallback to either.
        let path_bin = temp_dir.path().join("dsh");
        write_executable(&path_bin);
        let _path_guard = PathGuard::isolate(temp_dir.path());
        let _bin_guard = DshRuntimeBinGuard::set(&env_bin);

        assert!(
            resolve_dsh_executable(Some("/definitely/missing/dsh")).is_err(),
            "invalid explicit absolute override must fail without fallback"
        );
        assert!(
            resolve_dsh_executable(Some("definitely-missing-dsh-xyz")).is_err(),
            "invalid explicit bare override must fail without fallback"
        );
    }

    #[test]
    fn resolve_env_route_then_path_fallback() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let env_bin = temp_dir.path().join("env-dsh");
        write_executable(&env_bin);

        // Nonblank DSH_RUNTIME_BIN wins over PATH when no explicit command.
        {
            let _path_guard = PathGuard::isolate(temp_dir.path());
            let _bin_guard = DshRuntimeBinGuard::set(&env_bin);
            let resolved = resolve_dsh_executable(None).expect("env route");
            assert_eq!(resolved, std::fs::canonicalize(&env_bin).expect("canonical"));
        }

        // An invalid env override fails closed — no PATH fallback.
        {
            let empty_dir = temp_dir.path().join("empty");
            std::fs::create_dir_all(&empty_dir).expect("empty dir");
            let path_bin = temp_dir.path().join("dsh");
            write_executable(&path_bin);
            let _path_guard = PathGuard::isolate(temp_dir.path());
            let _bin_guard = DshRuntimeBinGuard::set(Path::new("/definitely/missing/dsh"));
            assert!(
                resolve_dsh_executable(None).is_err(),
                "an invalid DSH_RUNTIME_BIN override must fail without PATH fallback"
            );
        }

        // PATH `dsh` is the last resort.
        {
            let path_bin = temp_dir.path().join("dsh");
            let _path_guard = PathGuard::isolate(temp_dir.path());
            let _bin_guard = DshRuntimeBinGuard::remove();
            let resolved = resolve_dsh_executable(None).expect("PATH dsh");
            assert_eq!(resolved, std::fs::canonicalize(&path_bin).expect("canonical"));

            let empty_dir = temp_dir.path().join("nothing-here");
            std::fs::create_dir_all(&empty_dir).expect("empty dir");
            let _empty_path_guard = PathGuard::isolate(&empty_dir);
            assert!(
                resolve_dsh_executable(None).is_err(),
                "no override and no PATH `dsh` must fail"
            );
        }
    }

    #[test]
    fn resolve_blank_values_count_as_absent() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let env_bin = temp_dir.path().join("env-dsh");
        write_executable(&env_bin);
        let _path_guard = PathGuard::isolate(temp_dir.path());
        let _bin_guard = DshRuntimeBinGuard::set(&env_bin);
        // SDK truthiness parity: a blank explicit value counts as absent,
        // so the env route applies.
        let resolved = resolve_dsh_executable(Some("   ")).expect("blank explicit is absent");
        assert_eq!(resolved, std::fs::canonicalize(&env_bin).expect("canonical"));
    }

    // ── Sealed deny_all asset and home provisioning (architecture §3.4) ──

    /// The embedded no-tools asset is exactly upstream sdk-minimal's closed
    /// service allowlist minus its three tool producers, with the Nexus
    /// pins (profile sdk, tools native, agents empty, retry maxRetries 0).
    #[test]
    fn deny_all_asset_is_the_closed_allowlist() {
        let ids: Vec<&str> = DENY_ALL_PATCH
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- id: "))
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        let mut expected = vec![
            "agent",
            "agent-invariant",
            "agent-loop",
            "agent-loop-invariant",
            "deepseek-llm-api-extensions",
            "fs-local",
            "invariants",
            "jobs",
            "llm",
            "llm-deepseek",
            "llm-retry",
            "plugin-package-inventory-deepseek",
            "pty",
            "sandbox",
            "sandbox-policy",
            "sdk-app-startup",
            "sdk-jsonrpc-server",
            "session",
            "session-invariant",
            "session-log-deepseek",
            "session-projection",
            "session-title",
            "sessions",
            "scope-invariant",
            "subprocess",
            "system-prompt",
            "terminal-bash",
            "terminal-pwsh",
            "timer",
            "tools",
        ];
        expected.sort_unstable();
        assert_eq!(
            sorted, expected,
            "the retained service IDs are exactly the architecture §3.4 allowlist"
        );
        for forbidden in ["persistent-bash", "persistent-pwsh", "str-replace-editor"] {
            assert!(
                !ids.contains(&forbidden),
                "tool producer {forbidden} must not be a retained service id"
            );
        }
        // The Nexus pins on the upstream tree.
        assert!(DENY_ALL_PATCH.contains("profile: sdk"), "sdk-app-startup profile pin");
        assert!(DENY_ALL_PATCH.contains("mode: native"), "tools native pin");
        assert!(DENY_ALL_PATCH.contains("agents: []"), "empty agents pin");
        assert!(DENY_ALL_PATCH.contains("maxRetries: 0"), "provider retry pin");
        // Upstream platform conditions survive verbatim.
        assert!(DENY_ALL_PATCH.contains("process.platform === 'win32'"));
        assert!(DENY_ALL_PATCH.contains("process.platform !== 'win32'"));
    }

    /// The COMPLETE serialized asset contract (fix wave, architecture
    /// §3.4): order, package names, injections, configs and conditions are
    /// pinned by the SHA-256 of the embedded bytes — any drift (a renamed
    /// package, a reordered row, a changed injection or condition) fails
    /// this guard, while the allowlist/pin assertions above keep the
    /// intent readable. Update the constant only with a reviewed,
    /// source-grounded asset change.
    #[test]
    fn deny_all_asset_serialized_contract_is_pinned() {
        use sha2::Digest;
        use std::fmt::Write as _;
        let digest = sha2::Sha256::digest(DENY_ALL_PATCH.as_bytes());
        let mut digest_hex = String::with_capacity(64);
        for byte in digest {
            let _ = write!(digest_hex, "{byte:02x}");
        }
        assert_eq!(
            digest_hex, "d24cd62315fd82f27cf2684bb34e4374bb96a253b02d1e4dc5c01fda9ab40398",
            "the sealed deny_all asset drifted from the pinned upstream-derived contract"
        );
    }

    #[test]
    fn provision_sealed_home_layout_and_exclusivity() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let child = temp_dir.path().join("nexus").join("abc123");
        provision_sealed_home_at(&child).expect("fresh child provisions");

        assert_eq!(
            std::fs::read_to_string(child.join("profiles/sdk/package.json"))
                .expect("manifest"),
            format!("{SEALED_PROFILE_MANIFEST}\n"),
            "the sdk manifest is the exact empty-bundles startup-frozen tuple"
        );
        assert_eq!(
            std::fs::read_to_string(child.join("profiles/sdk/cordis.patch.yml"))
                .expect("profile patch"),
            EMPTY_PATCH_LAYER,
            "the profile patch layer is empty"
        );
        assert_eq!(
            std::fs::read_to_string(child.join("cordis.patch.yml")).expect("home patch"),
            EMPTY_PATCH_LAYER,
            "the home patch layer is empty"
        );
        assert_eq!(
            std::fs::read_to_string(child.join(DENY_ALL_PATCH_FILENAME)).expect("patch"),
            DENY_ALL_PATCH,
            "the copied patch is byte-identical to the embedded asset"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&child).expect("stat").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "the child home is owner-only");
            let patch_mode = std::fs::metadata(child.join(DENY_ALL_PATCH_FILENAME))
                .expect("stat patch")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(patch_mode, 0o600, "the sealed patch file is owner-only");
        }

        // Existing path is rejected (exclusivity).
        assert!(
            provision_sealed_home_at(&child).is_err(),
            "an existing child path must be rejected"
        );

        // A symlink at the child path is rejected.
        #[cfg(unix)]
        let link = temp_dir.path().join("nexus").join("link456");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp_dir.path(), &link).expect("symlink");
            assert!(
                provision_sealed_home_at(&link).is_err(),
                "a symlink child path must be rejected"
            );
        }
    }

    #[test]
    fn selected_dsh_home_follows_sdk_precedence() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        // Caller env DSH_HOME wins over the parent value.
        let previous = std::env::var("DSH_HOME").ok();
        std::env::set_var("DSH_HOME", "/parent/dsh-home");
        let caller = HashMap::from([("DSH_HOME".to_string(), "/caller/dsh-home".to_string())]);
        assert_eq!(
            selected_dsh_home(&caller),
            PathBuf::from("/caller/dsh-home"),
            "caller env DSH_HOME outranks the parent value"
        );
        // Parent DSH_HOME is used when the caller env is absent; a blank
        // caller value falls through (SDK parity).
        let blank = HashMap::from([("DSH_HOME".to_string(), "   ".to_string())]);
        assert_eq!(
            selected_dsh_home(&blank),
            PathBuf::from("/parent/dsh-home"),
            "a blank caller DSH_HOME falls through to the parent value"
        );
        match previous {
            Some(value) => std::env::set_var("DSH_HOME", value),
            None => std::env::remove_var("DSH_HOME"),
        }
    }

    // ── Provider behavior over the SDK-wire fixture ─────────────────────

    #[tokio::test]
    async fn non_prompt_operation_is_capability_unsupported() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let provider = stub_provider(
            "test-dsh-nonprompt",
            stub_env(
                &temp_dir.path().join("reqs.jsonl"),
                &temp_dir.path().join("dsh-home"),
            ),
        );
        let handle = launch_hermetic(&provider).await;

        let result = provider
            .execute(
                &handle,
                HostOperation::SetModel {
                    model: "deepseek-v4".to_string(),
                },
            )
            .await;

        assert!(matches!(
            result,
            Err(HostError::CapabilityUnsupported { .. })
        ));
    }

    #[tokio::test]
    async fn empty_prompt_is_protocol_error() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let provider = stub_provider(
            "test-dsh-empty",
            stub_env(
                &temp_dir.path().join("reqs.jsonl"),
                &temp_dir.path().join("dsh-home"),
            ),
        );
        let handle = launch_hermetic(&provider).await;

        let result = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![],
                    permission_scope: None,
                },
            )
            .await;

        assert!(matches!(
            result,
            Err(HostError::ProviderProtocolError { .. })
        ));
    }

    #[tokio::test]
    async fn mcp_injection_is_rejected_at_launch() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let provider = stub_provider(
            "test-dsh-mcp",
            stub_env(
                &temp_dir.path().join("reqs.jsonl"),
                &temp_dir.path().join("dsh-home"),
            ),
        );
        let mut spec = launch_spec();
        spec.mcp_servers = vec![McpServerConfig::Http {
            name: "injected".to_string(),
            url: "http://127.0.0.1:9/".to_string(),
        }];
        let result = provider.launch(spec).await;
        assert!(
            matches!(result, Err(HostError::CapabilityUnsupported { .. })),
            "injected MCP servers must fail closed at launch: {result:?}"
        );
    }

    #[tokio::test]
    async fn execute_unknown_session_is_internal_error() {
        let provider = DshNativeProvider::default_config();
        let handle = ManagedSessionHandle {
            provider_id: ProviderId::new("dsh-native"),
            session_id: HostSessionId::new(),
            capabilities: CapabilityDescriptor::dsh_limited(),
            process_identity: None,
        };

        let result = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await;

        assert!(matches!(result, Err(HostError::InternalHostError { .. })));
    }

    #[tokio::test]
    async fn cancel_is_honest_noop() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let provider = stub_provider(
            "test-dsh-cancel",
            stub_env(
                &temp_dir.path().join("reqs.jsonl"),
                &temp_dir.path().join("dsh-home"),
            ),
        );
        let handle = launch_hermetic(&provider).await;

        provider
            .cancel(&handle, HostOperationId::new())
            .await
            .expect("cancel must be an honest Ok no-op (AR-6)");
    }

    /// Confirmed close: shutdown removes the session record only after the
    /// SDK close completes; a later execute fails and the wire log shows
    /// the shutdown request. The launch happens under the env lock because
    /// it initializes (spawns) the fixture eagerly.
    #[tokio::test]
    async fn shutdown_confirmed_close_removes_session_and_blocks_new_execute() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let provider = stub_provider(
            "test-dsh-shutdown",
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
        );
        let handle = launch_hermetic(&provider).await;

        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider
                .shutdown(handle.clone())
                .await
                .expect("shutdown must succeed on a confirmed close");
        }

        let result = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await;

        assert!(
            matches!(result, Err(HostError::InternalHostError { .. })),
            "execute after a confirmed shutdown must fail"
        );
        let methods = method_sequence(&req_log_entries(&req_log));
        assert!(
            methods.contains(&"shutdown".to_string()),
            "the runtime saw the cooperative shutdown request: {methods:?}"
        );
        // A retried shutdown observes the retained completed result.
        provider
            .shutdown(handle)
            .await
            .expect("shutdown after a confirmed close observes the same completion");
    }

    /// A caller-side close-wait timeout does NOT cancel the retained
    /// cleanup task or remove the session (P0 T2): the first shutdown
    /// returns cleanup-unconfirmed while the SDK close ladder is still in
    /// flight (the fixture delays its shutdown reply), the session record
    /// survives, and a retried shutdown observes the same completion — now
    /// confirmed — and removes the record.
    #[tokio::test]
    async fn shutdown_waiter_timeout_retains_cleanup_owner_and_session() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let mut env = stub_env(&req_log, &temp_dir.path().join("dsh-home"));
        env.insert("SHUTDOWN_DELAY_MS".to_string(), "400".to_string());
        let provider = stub_provider_with_timeouts(
            "test-dsh-slow-close",
            env,
            TimeoutConfig {
                shutdown_ms: 50,
                ..TimeoutConfig::default()
            },
        );
        let handle = launch_hermetic(&provider).await;

        let first = {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider.shutdown(handle.clone()).await
        };
        assert!(
            matches!(first, Err(HostError::CleanupUnconfirmed { .. })),
            "a waiter timeout must be cleanup-unconfirmed, never success: {first:?}"
        );
        // The session record was NOT removed before the confirmed close.
        {
            let sessions = provider.sessions.read().await;
            assert!(
                sessions.contains_key(&handle.session_id),
                "the session record must survive an unconfirmed close"
            );
        }

        // Let the fixture's delayed reply land so the retained cleanup task
        // completes, then retry: the SAME completion is observed.
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider
                .shutdown(handle.clone())
                .await
                .expect("the retained close completes and the retry observes it");
        }
        let sessions = provider.sessions.read().await;
        assert!(
            !sessions.contains_key(&handle.session_id),
            "a confirmed close removes the session record"
        );
    }

    /// End-to-end success arm over the fixture stub: `launch()` initializes
    /// the ordinary runtime with exactly `--profile sdk`; one turn maps to
    /// exactly one `OpStarted`, one `MessageDelta(final_response)` and one
    /// terminal `OpFinished`; the wire `session/prompt` carries the
    /// provider's stored DSH session id.
    #[tokio::test]
    async fn dsh_turn_completes_via_stub() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider("test-dsh-stub", stub_env(&req_log, &dsh_home));
        let handle = launch_hermetic(&provider).await;
        let stored_before = stored_dsh_session_id(&provider, &handle.session_id).await;

        let events = run_turn(&provider, &handle, "hello dsh").await;

        assert_eq!(terminal_count(&events), 1, "exactly one terminal event");
        assert!(
            matches!(events.first(), Some(HostEvent::OpStarted(_))),
            "exactly one OpStarted precedes content: {events:?}"
        );
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(delta) => Some(delta.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["mock dsh reply"], "events: {events:?}");
        let finish_reasons: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::OpFinished(f) => Some(&f.reason),
                _ => None,
            })
            .collect();
        assert_eq!(
            finish_reasons,
            vec![&FinishReason::EndTurn],
            "turn/end kind 'completed' maps to EndTurn"
        );

        let entries = req_log_entries(&req_log);
        // Exactly one runtime was spawned, with exactly the ordinary argv
        // and the caller-env DSH_HOME.
        let spawns = spawn_records(&entries);
        assert_eq!(spawns.len(), 1, "one runtime spawn: {entries:?}");
        assert_eq!(
            argv_of(&spawns[0]),
            vec!["--profile".to_string(), "sdk".to_string()],
            "ordinary argv is exactly --profile sdk"
        );
        assert_eq!(
            spawns[0]["dsh_home"].as_str(),
            Some(dsh_home.to_string_lossy().as_ref()),
            "the child env carries the caller-env DSH_HOME"
        );
        // Initialize precedes the single prompt (eager launch init).
        let methods = method_sequence(&entries);
        assert_eq!(
            methods,
            vec![
                "_spawn".to_string(),
                "initialize".to_string(),
                "session/prompt".to_string()
            ],
            "initialize-at-launch precedes the one wire prompt: {methods:?}"
        );
        // The wire prompt must use the stored session id (multi-turn
        // continuity, AR-2/AR-5).
        let prompts: Vec<&serde_json::Value> = entries
            .iter()
            .filter(|value| value["method"] == "session/prompt")
            .collect();
        assert_eq!(prompts.len(), 1);
        assert_eq!(
            prompts[0]["sessionId"].as_str(),
            Some(stored_before.as_str()),
            "wire session id must match the stored dsh session id"
        );
    }

    /// Timeout arm over the fixture stub (P1 T2): with the stub holding the
    /// turn open, the provider timeout emits exactly one `OpFailed(timeout)`
    /// and transfers the host session to retained close — no retry on the
    /// same handle (no zombie-session reuse).
    #[tokio::test]
    async fn dsh_timeout_arm_fails_turn_and_closes_session() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let timeouts = TimeoutConfig {
            prompt_ms: 2000,
            ..TimeoutConfig::default()
        };
        let mut env = stub_env(&req_log, &req_log_dir.path().join("dsh-home"));
        env.insert("HOLD_TURN".to_string(), "1".to_string());
        let provider = stub_provider_with_timeouts("test-dsh-timeout", env, timeouts);
        let handle = launch_hermetic(&provider).await;

        let events = run_turn(&provider, &handle, "slow turn 1").await;
        assert_eq!(terminal_count(&events), 1);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, HostEvent::OpFailed(f) if f.error_category == "timeout")),
            "timeout arm must emit OpFailed(timeout): {events:?}"
        );

        let retry = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "slow turn 2".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await;
        assert!(
            retry.is_err(),
            "a timed-out turn must close the session before another execute"
        );
        assert!(
            matches!(retry, Err(HostError::InternalHostError { .. })),
            "closed session must reject execute with InternalHostError"
        );

        let methods = method_sequence(&req_log_entries(&req_log));
        assert!(
            methods.iter().any(|m| m == "session/prompt"),
            "at least one session/prompt must reach the stub: {methods:?}"
        );

        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider
                .shutdown(handle)
                .await
                .expect("shutdown with a live harness must succeed");
        }
    }

    /// Stream-abort backstop: a session closed by `shutdown()` before the
    /// stream's first poll emits `OpStarted` then exactly one
    /// `OpFailed(stream_closed)` — no run happens on a torn-down session.
    #[tokio::test]
    async fn dsh_stream_abort_backstop_when_session_closed_before_run() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let provider = stub_provider(
            "test-dsh-backstop",
            stub_env(
                &temp_dir.path().join("reqs.jsonl"),
                &temp_dir.path().join("dsh-home"),
            ),
        );
        let handle = launch_hermetic(&provider).await;

        // Create the stream while the session exists, then close the
        // session before the stream is polled — the run finds the closed
        // state and emits the stream-abort backstop.
        let stream = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await
            .expect("execute");

        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider.shutdown(handle).await.expect("shutdown");
        }

        let events = collect_events(stream).await;
        assert_eq!(terminal_count(&events), 1);
        assert!(
            matches!(events.first(), Some(HostEvent::OpStarted(_))),
            "OpStarted still precedes the backstop: {events:?}"
        );
        assert!(
            events.iter().any(
                |e| matches!(e, HostEvent::OpFailed(f) if f.error_category == "stream_closed")
            ),
            "backstop must emit OpFailed(stream_closed): {events:?}"
        );
    }

    // ── Probe (P0 T2: both recipes, no model call) ──────────────────────

    /// An invalid explicit override probes unavailable even when
    /// `DSH_RUNTIME_BIN` points at a working runtime: explicit overrides
    /// never fall back.
    #[tokio::test]
    async fn probe_unavailable_for_invalid_explicit_override() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let _bin_guard = DshRuntimeBinGuard::set(Path::new(MOCK_DSH_AGENT));
        let provider = DshNativeProvider::new(
            ProviderId::new("nonexistent-dsh-xyz"),
            "Fake".to_string(),
            Some("nonexistent_dsh_runtime_xyz_12345".to_string()),
            Vec::new(),
            HashMap::new(),
            TimeoutConfig::default(),
        )
        .expect("empty native args");

        let health = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 5000 })
            .await
            .expect("probe should return a health value");

        assert!(
            !health.available,
            "an invalid explicit override must be unavailable without env fallback"
        );
    }

    /// Both recipes initialize and close without a model call: the wire log
    /// shows the ordinary spawn (`--profile sdk` exactly), the sealed spawn
    /// (`--profile sdk --patch <owned absolute asset>` inside the exclusive
    /// child home under `<selected home>/nexus/`), and no prompt; the
    /// probe's sealed lease is deleted after the confirmed close.
    /// Tri-QC F-001 (seat 1): the public success message is STATIC and
    /// discloses neither the explicit runtime path nor the caller
    /// DSH_HOME path — it is serialized verbatim into the daemon
    /// provider-catalog response (`GET /v1/daemon/agent-host/providers`).
    #[tokio::test]
    async fn probe_initializes_and_closes_both_recipes() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        let provider = stub_provider("test-dsh-probe", stub_env(&req_log, &dsh_home));

        let health = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 30_000 })
            .await
            .expect("probe should succeed");
        assert!(
            health.available,
            "both recipes initialize and close: {}",
            health.message.clone().unwrap_or_default()
        );

        // The public health diagnostic (copied verbatim into the daemon
        // provider-catalog response) is static and path-free: neither the
        // explicit runtime path nor the caller-selected DSH_HOME leaks.
        let message = health.message.unwrap_or_default();
        assert_eq!(
            message, "ordinary and sealed recipes initialized and closed",
            "the public success diagnostic is static text"
        );
        assert!(
            !message.contains(MOCK_DSH_AGENT),
            "no explicit runtime path disclosure: {message}"
        );
        assert!(
            !message.contains(&dsh_home.to_string_lossy().into_owned()),
            "no caller home path disclosure: {message}"
        );

        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(spawns.len(), 2, "ordinary + sealed spawns: {entries:?}");
        assert_eq!(
            argv_of(&spawns[0]),
            vec!["--profile".to_string(), "sdk".to_string()],
            "ordinary argv is exactly --profile sdk"
        );
        let sealed_argv = argv_of(&spawns[1]);
        assert_eq!(sealed_argv.len(), 4, "sealed argv adds one patch pair");
        assert_eq!(sealed_argv[0..2], ["--profile".to_string(), "sdk".to_string()]);
        assert_eq!(sealed_argv[2], "--patch");
        let patch_path = PathBuf::from(&sealed_argv[3]);
        assert!(patch_path.is_absolute(), "the patch is an absolute path");
        let nexus_root = std::fs::canonicalize(&dsh_home)
            .expect("canonical selected home")
            .join(SEALED_HOME_SUBDIR);
        assert!(
            patch_path.starts_with(&nexus_root),
            "the patch lives inside the owned child home: {patch_path:?}"
        );
        let sealed_home = PathBuf::from(
            spawns[1]["dsh_home"].as_str().expect("sealed dsh_home logged"),
        );
        assert!(
            sealed_home.starts_with(&nexus_root),
            "the sealed child home sits under <selected home>/nexus: {sealed_home:?}"
        );
        let methods = method_sequence(&entries);
        assert!(
            !methods.iter().any(|m| m == "session/prompt"),
            "the probe never sends a prompt: {methods:?}"
        );
        assert!(
            !sealed_home.exists(),
            "the confirmed close ended the probe's sealed lease"
        );
    }

    // ── deny_all recipe admission (architecture §3.4) ───────────────────

    /// First deny-all prompt: the ordinary harness (initialized at launch)
    /// is closed through the retained owner, the sealed harness initializes
    /// with the owned patch inside the exclusive child home, and the prompt
    /// reaches ONLY the sealed runtime. A second identical-scope prompt
    /// reuses the sealed recipe (no third spawn). The child-home lease is
    /// deleted by the confirmed shutdown close.
    #[tokio::test]
    async fn deny_all_first_prompt_switches_to_sealed_recipe() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        let provider = stub_provider("test-dsh-denyall", stub_env(&req_log, &dsh_home));
        let handle = launch_hermetic(&provider).await;

        let events = run_turn_scoped(&provider, &handle, "no tools please", Some(deny_all_scope()))
            .await
            .expect("deny_all turn executes");
        assert!(
            matches!(events.last(), Some(HostEvent::OpFinished(_))),
            "the sealed turn completes: {events:?}"
        );

        // A second identical-scope prompt reuses the sealed recipe.
        let events = run_turn_scoped(&provider, &handle, "again", Some(deny_all_scope()))
            .await
            .expect("second deny_all turn executes");
        assert!(matches!(events.last(), Some(HostEvent::OpFinished(_))));

        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(
            spawns.len(),
            2,
            "ordinary launch spawn + sealed switch spawn: {entries:?}"
        );
        assert_eq!(
            argv_of(&spawns[0]),
            vec!["--profile".to_string(), "sdk".to_string()],
            "the launch spawn is the ordinary recipe"
        );
        let sealed_argv = argv_of(&spawns[1]);
        assert_eq!(sealed_argv.len(), 4);
        assert_eq!(sealed_argv[0..2], ["--profile".to_string(), "sdk".to_string()]);
        assert_eq!(sealed_argv[2], "--patch");
        let nexus_root = std::fs::canonicalize(&dsh_home)
            .expect("canonical selected home")
            .join(SEALED_HOME_SUBDIR);
        let patch_path = PathBuf::from(&sealed_argv[3]);
        assert!(patch_path.is_absolute() && patch_path.starts_with(&nexus_root));
        let sealed_home = PathBuf::from(
            spawns[1]["dsh_home"].as_str().expect("sealed dsh_home logged"),
        );
        assert!(sealed_home.starts_with(&nexus_root));
        assert_ne!(
            sealed_home,
            dsh_home,
            "the sealed runtime never uses the ordinary home"
        );

        // Order: ordinary spawn/init, cooperative close, sealed
        // spawn/init, then the prompts — and prompts went ONLY to the
        // sealed runtime (after its spawn).
        let methods = method_sequence(&entries);
        let prompt_count = methods.iter().filter(|m| *m == "session/prompt").count();
        assert_eq!(prompt_count, 2, "exactly the two admitted prompts: {methods:?}");
        let second_spawn = methods
            .iter()
            .enumerate()
            .filter(|(_, m)| *m == "_spawn")
            .nth(1)
            .map(|(index, _)| index)
            .expect("second spawn recorded");
        let first_prompt = methods
            .iter()
            .position(|m| m == "session/prompt")
            .expect("a prompt was sent");
        assert!(
            first_prompt > second_spawn,
            "no session/prompt reaches the ordinary runtime: {methods:?}"
        );
        let shutdown_index = methods
            .iter()
            .position(|m| m == "shutdown")
            .expect("the ordinary harness was closed");
        assert!(
            shutdown_index < second_spawn,
            "the ordinary close precedes the sealed initialization: {methods:?}"
        );

        // The lease is live while the session runs; the confirmed close of
        // shutdown deletes it.
        assert!(sealed_home.exists(), "the sealed lease is retained while live");
        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider.shutdown(handle).await.expect("confirmed close");
        }
        assert!(
            !sealed_home.exists(),
            "the confirmed close deleted the sealed child home lease"
        );
    }

    /// Once an ordinary prompt has been admitted, a deny_all scope change
    /// is rejected — never a conversation reset or a permission switch.
    #[tokio::test]
    async fn ordinary_then_deny_all_scope_change_is_rejected() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let provider = stub_provider(
            "test-dsh-lock1",
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
        );
        let handle = launch_hermetic(&provider).await;

        let events = run_turn(&provider, &handle, "ordinary first").await;
        assert!(matches!(events.last(), Some(HostEvent::OpFinished(_))));

        let result =
            run_turn_scoped(&provider, &handle, "deny now", Some(deny_all_scope())).await;
        assert!(
            matches!(result, Err(HostError::PolicyDenied { .. })),
            "scope change after admission must be rejected: {result:?}"
        );

        let entries = req_log_entries(&req_log);
        assert_eq!(spawn_records(&entries).len(), 1, "no sealed spawn happened");
        let prompt_count = entries
            .iter()
            .filter(|value| value["method"] == "session/prompt")
            .count();
        assert_eq!(prompt_count, 1, "the rejected prompt never reached a runtime");
    }

    /// Once a deny_all prompt has been admitted, a `None` scope change is
    /// rejected — `None` never escalates out of the sealed recipe.
    #[tokio::test]
    async fn deny_all_then_none_scope_change_is_rejected() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let provider = stub_provider(
            "test-dsh-lock2",
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
        );
        let handle = launch_hermetic(&provider).await;

        let events = run_turn_scoped(&provider, &handle, "sealed first", Some(deny_all_scope()))
            .await
            .expect("sealed turn executes");
        assert!(matches!(events.last(), Some(HostEvent::OpFinished(_))));

        let result = run_turn_scoped(&provider, &handle, "ordinary now", None).await;
        assert!(
            matches!(result, Err(HostError::PolicyDenied { .. })),
            "deny_all → None after admission must be rejected: {result:?}"
        );

        let entries = req_log_entries(&req_log);
        let prompt_count = entries
            .iter()
            .filter(|value| value["method"] == "session/prompt")
            .count();
        assert_eq!(prompt_count, 1, "the rejected prompt never reached a runtime");
    }

    /// A partially permissive scope is `not_supported` BEFORE admission:
    /// no prompt is sent and no sealed switch happens.
    #[tokio::test]
    async fn partially_permissive_scope_is_unsupported_before_admission() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let provider = stub_provider(
            "test-dsh-scope",
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
        );
        let handle = launch_hermetic(&provider).await;

        let result = run_turn_scoped(
            &provider,
            &handle,
            "read only please",
            Some(PromptPermissionScope {
                allow_read: true,
                allow_write: false,
                allow_destructive: false,
            }),
        )
        .await;
        assert!(
            matches!(result, Err(HostError::CapabilityUnsupported { .. })),
            "a partially permissive scope stays not_supported: {result:?}"
        );

        let entries = req_log_entries(&req_log);
        assert_eq!(spawn_records(&entries).len(), 1, "no sealed spawn happened");
        assert!(
            !entries.iter().any(|value| value["method"] == "session/prompt"),
            "no prompt was admitted: {entries:?}"
        );
    }

    // ── Fix wave: lifecycle/security corrections ────────────────────────

    /// Finding 1 (critical): shutdown admission must NOT wait behind an
    /// active `Session::run`. With the fixture holding the turn open
    /// (2s prompt budget), shutdown (200ms budget) returns
    /// cleanup-unconfirmed in well under the prompt budget — the closed
    /// intent and retained final-close owner are established immediately;
    /// a new execute is rejected at once; the retained close completes
    /// after the run's timeout releases the mutex; a retried shutdown
    /// observes the SAME completion and removes the record.
    #[tokio::test]
    async fn shutdown_does_not_wait_behind_an_active_run() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let timeouts = TimeoutConfig {
            prompt_ms: 2000,
            shutdown_ms: 200,
            ..TimeoutConfig::default()
        };
        let mut env = stub_env(&req_log, &temp_dir.path().join("dsh-home"));
        env.insert("HOLD_TURN".to_string(), "1".to_string());
        let provider = stub_provider_with_timeouts("test-dsh-race", env, timeouts);
        let handle = launch_hermetic(&provider).await;

        // Start a turn and drive the stream on a task so the run holds
        // the per-session mutex.
        let stream = {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider
                .execute(
                    &handle,
                    HostOperation::Prompt {
                        op_id: HostOperationId::new(),
                        content: vec![HostContentBlock::Text {
                            text: "held turn".to_string(),
                        }],
                        permission_scope: None,
                    },
                )
                .await
                .expect("execute")
        };
        let turn = tokio::spawn(collect_events(stream));
        // Let the run start (fixture reply is prompt; the turn then
        // holds open).
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let started = std::time::Instant::now();
        let first = provider.shutdown(handle.clone()).await;
        let waited = started.elapsed();
        assert!(
            matches!(first, Err(HostError::CleanupUnconfirmed { .. })),
            "the waiter deadline must fire: {first:?}"
        );
        assert!(
            waited < std::time::Duration::from_millis(1500),
            "shutdown admission must not wait behind the active run \
             (waited {waited:?} of a 2s prompt budget)"
        );
        // The session record survives the unconfirmed close...
        {
            let sessions = provider.sessions.read().await;
            assert!(sessions.contains_key(&handle.session_id));
        }
        // ...and a new execute is rejected IMMEDIATELY (closing intent
        // was set without the run mutex).
        let rejected = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "too late".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await;
        assert!(
            matches!(rejected, Err(HostError::InternalHostError { .. })),
            "execute after shutdown intent must be rejected"
        );

        // The run's 2s timeout releases the mutex; the retained close
        // then completes. A retried shutdown observes the SAME shared
        // completion and removes the record.
        let events = turn.await.expect("turn task");
        assert_eq!(terminal_count(&events), 1);
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        provider
            .shutdown(handle.clone())
            .await
            .expect("the retained close completes and the retry observes it");
        let sessions = provider.sessions.read().await;
        assert!(
            !sessions.contains_key(&handle.session_id),
            "a confirmed close removes the session record"
        );
    }

    /// Finding 2: a sealed-home provisioning failure on the first
    /// deny-all prompt permanently closes the session — no retryable
    /// half-switched state, no ordinary fallback; later `None` and
    /// deny-all executes are both rejected immediately, and no prompt is
    /// ever sent. (The `nexus` component is a FILE, so the exclusive
    /// leaf create fails.)
    #[tokio::test]
    async fn sealed_provision_failure_closes_session_permanently() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        std::fs::create_dir_all(&dsh_home).expect("home dir");
        std::fs::write(dsh_home.join(SEALED_HOME_SUBDIR), b"not a dir").expect("blocker");
        let provider = stub_provider("test-dsh-sealfail", stub_env(&req_log, &dsh_home));
        let handle = launch_hermetic(&provider).await;

        let failed =
            run_turn_scoped(&provider, &handle, "seal me", Some(deny_all_scope())).await;
        assert!(
            matches!(failed, Err(HostError::LaunchFailed { .. })),
            "provisioning failure is the typed launch-class error: {failed:?}"
        );

        // The session is permanently closed: both scope variants are
        // rejected immediately (never a retried switch or an ordinary
        // admission).
        for scope in [None, Some(deny_all_scope())] {
            let rejected = run_turn_scoped(&provider, &handle, "again", scope).await;
            assert!(
                matches!(rejected, Err(HostError::InternalHostError { .. })),
                "scope {scope:?} must be rejected on the closed session: {rejected:?}"
            );
        }
        // The record is retained (closed) and only the ordinary launch
        // spawn ever happened — zero prompts, zero sealed spawns.
        {
            let sessions = provider.sessions.read().await;
            assert!(
                sessions.contains_key(&handle.session_id),
                "the closed session record is retained as evidence"
            );
        }
        let entries = req_log_entries(&req_log);
        assert_eq!(spawn_records(&entries).len(), 1, "no sealed spawn happened");
        assert!(
            !entries.iter().any(|value| value["method"] == "session/prompt"),
            "no prompt was admitted: {entries:?}"
        );

        // A later shutdown still completes (nothing left to close) and
        // removes the record.
        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider.shutdown(handle).await.expect("final close completes");
        }
    }

    /// Finding 3: sealed-home path safety — a relative child path, a
    /// symlinked `nexus` parent, and a non-directory `nexus` parent are
    /// all rejected, and nothing is written through the symlink. A
    /// symlinked SELECTED-HOME anchor above `nexus` (e.g. macOS `/var`)
    /// is legitimate: resolved exactly once at validation with the lease
    /// landing in the real target (fix wave 2: the full ancestor chain
    /// is validated and descriptor-pinned).
    #[test]
    fn sealed_home_rejects_unsafe_paths() {
        assert!(
            provision_sealed_home_at(Path::new("relative/nexus/leaf")).is_err(),
            "a relative child path must be rejected"
        );

        let temp_dir = tempfile::tempdir().expect("temp dir");
        // Non-directory `nexus` component.
        let blocked = temp_dir.path().join("blocked");
        std::fs::create_dir_all(&blocked).expect("blocked dir");
        std::fs::write(blocked.join("nexus"), b"not a dir").expect("blocker file");
        assert!(
            provision_sealed_home_at(&blocked.join("nexus").join("leaf")).is_err(),
            "a non-directory nexus parent must be rejected"
        );

        // Symlinked `nexus` parent: rejected, and the link target stays
        // empty (no escape writes).
        #[cfg(unix)]
        {
            let anchor = temp_dir.path().join("anchor");
            let elsewhere = temp_dir.path().join("elsewhere");
            std::fs::create_dir_all(&anchor).expect("anchor dir");
            std::fs::create_dir_all(&elsewhere).expect("elsewhere dir");
            std::os::unix::fs::symlink(&elsewhere, anchor.join("nexus")).expect("symlink");
            assert!(
                provision_sealed_home_at(&anchor.join("nexus").join("leaf")).is_err(),
                "a symlinked nexus parent must be rejected"
            );
            assert!(
                std::fs::read_dir(&elsewhere)
                    .expect("read elsewhere")
                    .next()
                    .is_none(),
                "nothing may be written through the symlink"
            );

            // Finding 1 (fix wave 2): a symlinked SELECTED-HOME ancestor
            // (above `nexus`, e.g. macOS `/var`) is a legitimate user
            // anchor: it is RESOLVED exactly once at validation and the
            // lease lands in the real target, with the resolved path
            // returned — and everything below the anchor stays
            // descriptor-pinned, so a later swap cannot redirect writes.
            let real_home = temp_dir.path().join("real_home");
            std::fs::create_dir_all(&real_home).expect("real home dir");
            let home_link = temp_dir.path().join("home_link");
            std::os::unix::fs::symlink(&real_home, &home_link).expect("home symlink");
            let resolved = provision_sealed_home_fd(&home_link.join("nexus").join("leaf"))
                .expect("a symlinked home anchor is resolved, not followed blindly");
            let canonical_home = std::fs::canonicalize(&real_home).expect("canonical home");
            assert!(
                resolved.path.starts_with(&canonical_home),
                "the lease lands in the real anchor: {:?} not under {canonical_home:?}",
                resolved.path
            );
            assert!(
                resolved.path.join(DENY_ALL_PATCH_FILENAME).exists(),
                "the layout was written inside the resolved lease"
            );
            remove_lease_tree(&resolved).expect("cleanup");
        }
    }

    /// Findings 4+3: a lease-removal failure after a CONFIRMED close is
    /// an unconfirmed cleanup (never success), the lease evidence AND the
    /// exact failed path are retained, and a later reconciliation attempt
    /// targets that exact path and clears it. (A 0500 subdirectory makes
    /// `remove_dir_all` fail on Unix.)
    #[cfg(unix)]
    #[tokio::test]
    async fn lease_removal_failure_is_unconfirmed_cleanup() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().expect("temp dir");
        let child = temp_dir.path().join("nexus").join("lease123");
        let lease = provision_sealed_home_fd(&child).expect("provision");
        // The lease resolves its anchor exactly once; assertions run
        // against the RESOLVED path (e.g. macOS `/var` symlinks).
        let lease_path = lease.path.clone();
        let blocked = lease_path.join("blocked");
        std::fs::create_dir_all(&blocked).expect("blocked subdir");
        std::fs::write(blocked.join("state"), b"state").expect("state file");
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o500))
            .expect("chmod 0500");

        let retained: RetainedLeases = Arc::new(std::sync::Mutex::new(BTreeSet::new()));
        let result = start_close(None, Some(lease), Arc::clone(&retained)).await;
        let failure = result.expect_err("removal failure is an unconfirmed cleanup");
        assert!(
            failure.close_confirmed,
            "the close itself was confirmed; only the removal failed"
        );
        assert_eq!(
            failure.failed_lease.as_deref(),
            Some(lease_path.as_path()),
            "the exact failed lease path is reported"
        );
        assert!(
            lease_path.exists(),
            "the lease evidence is retained on removal failure"
        );
        assert!(
            retained
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains(&lease_path),
            "the exact failed path is retained for a later retry"
        );

        // A later reconciliation attempt targets the exact path (no
        // scans): once the blockage is fixed, the retry clears it.
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o755))
            .expect("restore perms");
        reconcile_retained_leases(&retained);
        assert!(!lease_path.exists(), "the reconciled lease is removed");
        assert!(
            retained
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "only a confirmed removal clears the record"
        );
    }

    /// Finding 5: a probe deadline that expires during the sealed close
    /// drops the WAIT, never the cleanup owner — the retained spawned
    /// close still finishes and deletes the sealed lease afterwards.
    #[tokio::test]
    async fn probe_deadline_drop_keeps_sealed_cleanup_owner() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        let mut env = stub_env(&req_log, &dsh_home);
        // 600ms close delay: the ordinary close (starting near t=0)
        // completes within the 800ms probe budget; the sealed close
        // (starting after the sealed init) cannot — the probe deadline
        // expires mid-sealed-close.
        env.insert("SHUTDOWN_DELAY_MS".to_string(), "600".to_string());
        let provider = stub_provider("test-dsh-probetimeout", env);

        let probe = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 800 })
            .await;
        assert!(
            matches!(probe, Err(HostError::OperationTimeout { .. })),
            "the probe deadline fires during the sealed close: {probe:?}"
        );

        // The retained sealed close owner keeps running past the dropped
        // wait: after the 600ms delay lands, the lease is deleted.
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(spawns.len(), 2, "both recipes were spawned: {entries:?}");
        let sealed_home = PathBuf::from(
            spawns[1]["dsh_home"].as_str().expect("sealed dsh_home logged"),
        );
        assert!(
            !sealed_home.exists(),
            "the retained cleanup owner deleted the lease after the dropped wait"
        );
    }

    /// Finding 2 (fix wave 2): a probe deadline that expires during the
    /// sealed runtime START drops the wait, never the lease's only path
    /// owner — the retained spawned owner (held since provisioning)
    /// still drives the start to completion, confirms the close, and
    /// removes the lease. No reliance on SDK `kill_on_drop`. (The
    /// fixture delays the `initialize` reply so the deadline lands
    /// mid-start; on the previous implementation the dropped start left
    /// the provisioned lease behind and this test fails.)
    #[tokio::test]
    async fn probe_init_timeout_retains_sealed_owner_and_completes_cleanup() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        let mut env = stub_env(&req_log, &dsh_home);
        // 600ms initialize delay: the ordinary recipe (init 600 + close
        // ~0) fits the 1000ms budget; the sealed start (beginning after
        // the ordinary close) cannot — the probe deadline fires during
        // the sealed START.
        env.insert("INIT_DELAY_MS".to_string(), "600".to_string());
        let provider = stub_provider("test-dsh-probeinit", env);

        let probe = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 1000 })
            .await;
        assert!(
            matches!(probe, Err(HostError::OperationTimeout { .. })),
            "the probe deadline fires during the sealed start: {probe:?}"
        );

        // The retained sealed owner keeps running past the dropped wait:
        // the start completes, the close is confirmed, and the lease is
        // removed.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(spawns.len(), 2, "both recipes were spawned: {entries:?}");
        let sealed_home = PathBuf::from(
            spawns[1]["dsh_home"].as_str().expect("sealed dsh_home logged"),
        );
        assert!(
            !sealed_home.exists(),
            "the retained init owner completed the close and removed the lease"
        );
        assert!(
            provider
                .retained_leases
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "no lease path remains retained"
        );
    }

    /// Tri-QC F-001 (seat 2): a probe deadline that expires during the
    /// ORDINARY runtime START drops the wait, never the only owner of the
    /// spawned ordinary child — the retained spawned owner (installed
    /// before the first cancelable await, symmetric with the sealed owner
    /// above) still drives the start to completion, confirms the close,
    /// and the direct child is eventually reaped. (The fixture delays the
    /// `initialize` reply so the deadline lands mid-ordinary-start; the
    /// DISCRIMINATOR is the cooperative `shutdown` request on the wire —
    /// only a live retained owner running `close()` sends it, while the
    /// previous implementation's dropped wait left the child to the SDK's
    /// drop-kill with no shutdown RPC, and this test fails.)
    #[cfg(unix)]
    #[tokio::test]
    async fn probe_init_timeout_retains_ordinary_owner_and_reaps_child() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        let mut env = stub_env(&req_log, &dsh_home);
        // 600ms initialize delay against a 100ms probe budget: the
        // deadline fires while the ORDINARY start is still in flight,
        // before the sealed recipe is ever reached.
        env.insert("INIT_DELAY_MS".to_string(), "600".to_string());
        let provider = stub_provider("test-dsh-probeordinit", env);

        let probe = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 100 })
            .await;
        assert!(
            matches!(probe, Err(HostError::OperationTimeout { .. })),
            "the probe deadline fires during the ordinary start: {probe:?}"
        );

        // The retained ordinary owner keeps running past the dropped
        // wait: the start completes, the close is CONFIRMED (the
        // cooperative `shutdown` request reaches the child — a drop-kill
        // would never send it), and the direct child is reaped. The
        // sealed recipe never started, so no lease path can be retained.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(
            spawns.len(),
            1,
            "only the ordinary recipe was spawned: {entries:?}"
        );
        let methods = method_sequence(&entries);
        assert!(
            methods.iter().any(|m| m == "shutdown"),
            "the retained owner ran the confirmed close (shutdown RPC) \
             after the dropped wait: {methods:?}"
        );
        let pids = spawn_pids(&entries);
        assert_pid_reaped(pids[0], "the ordinary child was reaped by the retained owner").await;
        assert!(
            provider
                .retained_leases
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "no lease path remains retained"
        );
    }

    /// Finding 3 (fix wave 2): a sealed-lease removal failure keeps the
    /// exact path retryable — a later shutdown targets that path (no
    /// scans), and only its confirmed removal clears the record and
    /// reports success. (On the previous implementation the retry merely
    /// replayed the completed cleanup error and this test fails.)
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_removal_keeps_retryable_path_and_shutdown_retry_clears() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let provider = stub_provider(
            "test-dsh-retrylease",
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
        );
        let handle = launch_hermetic(&provider).await;
        run_turn_scoped(&provider, &handle, "seal me", Some(deny_all_scope()))
            .await
            .expect("the sealed turn completes");

        // Plant a removal blocker inside the live sealed lease.
        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(spawns.len(), 2, "ordinary + sealed spawns: {entries:?}");
        let lease = PathBuf::from(spawns[1]["dsh_home"].as_str().expect("sealed dsh_home"));
        let blocked = lease.join("blocked");
        std::fs::create_dir_all(&blocked).expect("blocked subdir");
        std::fs::write(blocked.join("state"), b"state").expect("state file");
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o500))
            .expect("chmod 0500");

        let first = {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider.shutdown(handle.clone()).await
        };
        assert!(
            matches!(first, Err(HostError::CleanupUnconfirmed { .. })),
            "the removal failure is an unconfirmed cleanup: {first:?}"
        );
        assert!(lease.exists(), "the lease evidence is retained");
        {
            let sessions = provider.sessions.read().await;
            assert!(
                sessions.contains_key(&handle.session_id),
                "the session record is retained while cleanup is unconfirmed"
            );
        }
        assert!(
            provider
                .retained_leases
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains(&lease),
            "the exact failed path is retained for a later attempt"
        );

        // Fix the blockage: the retried shutdown targets the exact
        // retained path, confirms its removal, and only then clears the
        // record and reports success.
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o755))
            .expect("restore perms");
        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider
                .shutdown(handle.clone())
                .await
                .expect("the retry reconciles the retained lease");
        }
        assert!(!lease.exists(), "the reconciled lease is removed");
        let sessions = provider.sessions.read().await;
        assert!(
            !sessions.contains_key(&handle.session_id),
            "only the confirmed removal clears the session record"
        );
        assert!(
            provider
                .retained_leases
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "the retained-path record is cleared"
        );
    }
    // ── Fix wave 3: anchor handoff + session-associated reconciliation ──

    /// Finding 1 (fix wave 3): unsafe owner/permission boundaries on the
    /// selected home are rejected before any sealed layout is written.
    /// Trust model: excludes mutation by OTHER users via group/world
    /// writability; same-uid adversaries are out of scope (architecture
    /// §3.4).
    #[cfg(unix)]
    #[test]
    fn sealed_home_rejects_unsafe_permission_boundary() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let home = temp_dir.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o777))
            .expect("chmod 0777");
        assert!(
            provision_sealed_home_fd(&home.join("nexus").join("leaf")).is_err(),
            "a world-writable selected home without sticky must be rejected"
        );
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755))
            .expect("restore perms");
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o770))
            .expect("chmod group-writable");
        assert!(
            provision_sealed_home_fd(&home.join("nexus").join("leaf")).is_err(),
            "a group-writable selected home must be rejected"
        );
    }

    /// Finding 1 (fix wave 3): the pinned anchor survives provisioning and
    /// `revalidate_for_start` rejects a lexical path swap before SDK start.
    #[cfg(unix)]
    #[test]
    fn revalidate_rejects_lexical_path_swap_before_sdk_start() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let child = temp_dir.path().join("nexus").join("leaf");
        let lease = provision_sealed_home_fd(&child).expect("provision");
        let elsewhere = temp_dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        std::fs::remove_dir_all(&child).expect("remove leaf");
        std::os::unix::fs::symlink(&elsewhere, &child).expect("swap symlink");
        assert!(
            lease.revalidate_for_start().is_err(),
            "a swapped lexical child must fail revalidation"
        );
    }

    /// Finding 1 (fix wave 3): retained reconciliation deletes only the
    /// anchored child entry and never follows a swapped lexical target.
    #[cfg(unix)]
    #[test]
    fn retained_reconcile_targets_anchored_child_not_swapped_lexical_target() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let lease = provision_sealed_home_fd(&temp_dir.path().join("nexus").join("leaf"))
            .expect("provision");
        let path = lease.path.clone();
        let victim = temp_dir.path().join("victim");
        std::fs::create_dir_all(&victim).expect("victim");
        std::fs::write(victim.join("keep"), b"keep").expect("victim file");
        let nexus = path.parent().expect("nexus");
        let leaf_name = path.file_name().expect("leaf");
        std::fs::remove_dir_all(&path).expect("remove real leaf");
        std::os::unix::fs::symlink(&victim, nexus.join(leaf_name)).expect("swap leaf");
        let retained: RetainedLeases = Arc::new(std::sync::Mutex::new(BTreeSet::new()));
        retain_lease_path(&retained, path.clone());
        reconcile_retained_leases(&retained);
        assert_eq!(
            std::fs::read_to_string(victim.join("keep")).expect("victim survives"),
            "keep",
            "the swapped lexical target must never be deleted"
        );
        assert!(
            !nexus.join(leaf_name).exists(),
            "the anchored nexus entry is removed"
        );
        assert!(
            retained
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty(),
            "confirmed anchored removal clears the retained record"
        );
    }

    /// Finding 2 (fix wave 3): a sealed-switch start failure whose immediate
    /// anchored deletion also fails associates the exact lease with the
    /// session; shutdown stays cleanup-unconfirmed until a targeted retry
    /// confirms deletion and only then removes the session record.
    #[cfg(unix)]
    #[tokio::test]
    async fn sealed_switch_start_delete_failure_retains_session_lease_until_shutdown_retry(
    ) {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        std::fs::create_dir_all(&dsh_home).expect("home");
        let mut env = stub_env(&req_log, &dsh_home);
        env.insert("INIT_FAIL_SEALED".to_string(), "1".to_string());
        let provider = stub_provider("test-dsh-switchfail", env);
        let handle = launch_hermetic(&provider).await;

        let failed =
            run_turn_scoped(&provider, &handle, "seal me", Some(deny_all_scope())).await;
        assert!(
            matches!(failed, Err(HostError::LaunchFailed { .. })),
            "sealed start failure is the typed launch-class error: {failed:?}"
        );

        let lease = {
            let sessions = provider.sessions.read().await;
            let native = sessions.get(&handle.session_id).expect("session");
            let guard = native.state.lock().await;
            assert!(guard.closed, "the switch failure permanently closes the session");
            guard.failed_lease.clone().expect("failed_lease is associated")
        };
        assert!(lease.exists(), "the undeleted anchored lease is retained");
        let blocker = lease.join("blocker");
        assert!(blocker.is_dir(), "the mock planted a removal blocker");
        let leaf_mode = std::fs::metadata(&lease)
            .expect("leaf stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(leaf_mode, 0o555, "the mock made the leaf unwritable for removal");

        let first = {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider.shutdown(handle.clone()).await
        };
        assert!(
            matches!(first, Err(HostError::CleanupUnconfirmed { .. })),
            "shutdown must stay cleanup-unconfirmed while the lease remains: {first:?}"
        );
        assert!(
            provider.sessions.read().await.contains_key(&handle.session_id),
            "the session record survives until confirmed lease removal"
        );

        std::fs::set_permissions(&lease, std::fs::Permissions::from_mode(0o700))
            .expect("restore leaf perms");
        std::fs::set_permissions(&blocker, std::fs::Permissions::from_mode(0o755))
            .expect("restore blocker perms");
        {
            let _env_lock = crate::test_support::PROCESS_ENV_LOCK
                .lock()
                .expect("lock env tests");
            provider
                .shutdown(handle.clone())
                .await
                .expect("the retry confirms anchored deletion");
        }
        assert!(!lease.exists(), "the session-associated lease is removed");
        assert!(
            !provider.sessions.read().await.contains_key(&handle.session_id),
            "only confirmed deletion removes the session record"
        );
    }

    // ── Task 3: real child-process SDK-wire proof extensions ───────────

    /// The pid recorded by each `_spawn` entry (fixture-side `os.getpid()`).
    fn spawn_pids(entries: &[serde_json::Value]) -> Vec<i32> {
        spawn_records(entries)
            .iter()
            .map(|spawn| {
                spawn["pid"]
                    .as_i64()
                    .expect("the _spawn record carries the child pid")
                    .try_into()
                    .expect("pid fits i32")
            })
            .collect()
    }

    /// Poll until `pid` is reaped (kill(0) → ESRCH) or the bound expires
    /// (unix-only proof surface; the SDK close ladder reaps the direct
    /// child before `close()` returns, so this settles fast).
    #[cfg(unix)]
    async fn assert_pid_reaped(pid: i32, what: &str) {
        let raw = rustix::process::Pid::from_raw(pid).expect("valid pid");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match rustix::process::test_kill_process(raw) {
                Err(rustix::io::Errno::SRCH) => return,
                _ if std::time::Instant::now() < deadline => {
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
                result => panic!("{what} pid {pid} must be reaped (kill(0) → ESRCH), got {result:?}"),
            }
        }
    }

    /// Copy the SDK-wire fixture into a fresh bin directory under the name
    /// `dsh` (the production bare command), with a `python3` symlink beside
    /// it so the fixture's `#!/usr/bin/env python3` shebang resolves under
    /// an isolated PATH.
    fn install_fixture_as_dsh(bin_dir: &Path) -> PathBuf {
        std::fs::create_dir_all(bin_dir).expect("bin dir");
        let dsh = bin_dir.join("dsh");
        std::fs::copy(MOCK_DSH_AGENT, &dsh).expect("copy fixture as dsh");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dsh, std::fs::Permissions::from_mode(0o755))
                .expect("chmod +x dsh");
            let python = which::which("python3").expect("python3 on the pre-isolation PATH");
            std::os::unix::fs::symlink(python, bin_dir.join("python3"))
                .expect("python3 symlink");
        }
        dsh
    }

    /// Full SDK-wire proof through a real child process whose executable
    /// name is exactly `dsh`, resolved through an isolated PATH (no
    /// explicit override, no DSH_RUNTIME_BIN): ordinary argv is exactly
    /// `--profile sdk` with the caller-env DSH_HOME; the first deny_all
    /// prompt closes the ordinary child (its pid is reaped BEFORE the
    /// sealed turn), initializes the sealed child home with exactly one
    /// owned `--patch`; the turn maps receipt → assistant delta →
    /// `turn/end: completed` → root idle into OpStarted → MessageDelta →
    /// OpFinished(EndTurn); and the confirmed shutdown reaps the sealed
    /// child pid and deletes the lease.
    #[cfg(unix)]
    fn stub_env_scenario(req_log: &Path, home: &Path, scenario: &str) -> HashMap<String, String> {
        let mut env = stub_env(req_log, home);
        env.insert("SCENARIO".to_string(), scenario.to_string());
        env
    }

    fn message_texts(events: &[HostEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match event {
                HostEvent::MessageDelta(delta) => Some(delta.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn terminal_of(events: &[HostEvent]) -> Option<&HostEvent> {
        events.iter().rev().find(|event| {
            matches!(event, HostEvent::OpFinished(_) | HostEvent::OpFailed(_))
        })
    }

    #[tokio::test]
    async fn dsh_two_messages_emit_a_then_b_without_duplicate_final() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider(
            "test-dsh-two-msgs",
            stub_env_scenario(&req_log, &dsh_home, "two_messages"),
        );
        let handle = launch_hermetic(&provider).await;
        let events = run_turn(&provider, &handle, "two").await;
        assert_eq!(message_texts(&events), vec!["A", "B"], "{events:?}");
        assert!(matches!(
            terminal_of(&events),
            Some(HostEvent::OpFinished(_))
        ));
    }

    #[tokio::test]
    async fn dsh_nested_assistant_message_is_ignored() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider(
            "test-dsh-nested",
            stub_env_scenario(&req_log, &dsh_home, "nested"),
        );
        let handle = launch_hermetic(&provider).await;
        let events = run_turn(&provider, &handle, "nested").await;
        assert_eq!(message_texts(&events), vec!["mock dsh reply"], "{events:?}");
    }

    #[tokio::test]
    async fn dsh_malformed_text_block_fails_after_partial_delivery() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider(
            "test-dsh-malformed-text",
            stub_env_scenario(&req_log, &dsh_home, "malformed_text"),
        );
        let handle = launch_hermetic(&provider).await;
        let events = run_turn(&provider, &handle, "bad").await;
        assert!(
            matches!(
                terminal_of(&events),
                Some(HostEvent::OpFailed(f)) if f.error_category == "decode_error"
            ),
            "{events:?}"
        );
    }

    #[tokio::test]
    async fn dsh_non_success_after_streamed_text_still_fails() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider(
            "test-dsh-partial-fail",
            stub_env_scenario(&req_log, &dsh_home, "partial_then_fail"),
        );
        let handle = launch_hermetic(&provider).await;
        let events = run_turn(&provider, &handle, "partial").await;
        assert_eq!(message_texts(&events), vec!["partial"], "{events:?}");
        assert!(
            matches!(
                terminal_of(&events),
                Some(HostEvent::OpFailed(f)) if f.error_category == "max_tokens"
            ),
            "{events:?}"
        );
    }

    #[tokio::test]
    async fn dsh_oversize_fixture_fails_delivery_bounds() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider(
            "test-dsh-oversize",
            stub_env_scenario(&req_log, &dsh_home, "oversize"),
        );
        let handle = launch_hermetic(&provider).await;
        let events = run_turn(&provider, &handle, "big").await;
        assert!(
            matches!(
                terminal_of(&events),
                Some(HostEvent::OpFailed(f)) if f.error_category == "provider_error"
            ),
            "{events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    #[tokio::test]
    async fn dsh_flood_fixture_hits_delivery_overflow() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let mut env = stub_env_scenario(&req_log, &dsh_home, "flood_messages");
        env.insert("FLOOD_COUNT".to_string(), "65".to_string());
        let provider = stub_provider("test-dsh-flood", env);
        let handle = launch_hermetic(&provider).await;
        let events = run_turn(&provider, &handle, "flood").await;
        assert!(
            matches!(
                terminal_of(&events),
                Some(HostEvent::OpFailed(f)) if f.error_category == "provider_error"
            ),
            "{events:?}"
        );
    }

    #[tokio::test]
    async fn dsh_receiver_drop_while_hold_turn_invalidates_session() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let provider = stub_provider(
            "test-dsh-drop",
            stub_env_scenario(&req_log, &dsh_home, "hold_turn"),
        );
        let handle = launch_hermetic(&provider).await;
        let mut stream = Box::pin(
            provider
                .execute(
                    &handle,
                    HostOperation::Prompt {
                        op_id: HostOperationId::new(),
                        content: vec![HostContentBlock::Text {
                            text: "hold".to_string(),
                        }],
                        permission_scope: None,
                    },
                )
                .await
                .expect("execute"),
        );
        let first = stream.next().await.expect("op started").expect("ok");
        assert!(matches!(first, HostEvent::OpStarted(_)));
        drop(stream);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            provider
                .execute(
                    &handle,
                    HostOperation::Prompt {
                        op_id: HostOperationId::new(),
                        content: vec![HostContentBlock::Text {
                            text: "after drop".to_string(),
                        }],
                        permission_scope: None,
                    },
                )
                .await
                .is_err(),
            "session must be unusable after receiver drop"
        );
    }

    async fn dsh_lag_fixture_still_delivers_message_before_terminal() {
        let req_log_dir = tempfile::tempdir().expect("temp dir");
        let req_log = req_log_dir.path().join("reqs.jsonl");
        let dsh_home = req_log_dir.path().join("dsh-home");
        let mut env = stub_env_scenario(&req_log, &dsh_home, "lag");
        env.insert("LAG_MS".to_string(), "100".to_string());
        let provider = stub_provider("test-dsh-lag", env);
        let handle = launch_hermetic(&provider).await;
        let stream = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "lag".to_string(),
                    }],
                    permission_scope: None,
                },
            )
            .await
            .expect("execute");
        let mut stream = std::pin::pin!(stream);
        let started = stream.next().await.expect("started").expect("ok");
        assert!(matches!(started, HostEvent::OpStarted(_)));
        let delta = stream.next().await.expect("delta").expect("ok");
        assert!(matches!(delta, HostEvent::MessageDelta(_)));
        let terminal = stream.next().await.expect("terminal").expect("ok");
        assert!(matches!(terminal, HostEvent::OpFinished(_)));
    }

    #[cfg(unix)]
    #[tokio::test]

    async fn wire_child_named_dsh_full_handshake_and_confirmed_exit() {
        // This test holds the env lock itself (PATH isolation must cover
        // resolution AND spawn), so it cannot call the locking helpers —
        // execute/collect inline instead.
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let bin_dir = temp_dir.path().join("bin");
        install_fixture_as_dsh(&bin_dir);
        let req_log = temp_dir.path().join("reqs.jsonl");
        let dsh_home = temp_dir.path().join("dsh-home");
        let _path_guard = PathGuard::isolate(&bin_dir);
        let _bin_guard = DshRuntimeBinGuard::remove();
        let provider = DshNativeProvider::new(
            ProviderId::new("test-dsh-named"),
            "Test".to_string(),
            None,
            Vec::new(),
            stub_env(&req_log, &dsh_home),
            TimeoutConfig::default(),
        )
        .expect("empty native args are accepted");
        let handle = provider.launch(launch_spec()).await.expect("launch");
        let stream = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "seal me".to_string(),
                    }],
                    permission_scope: Some(deny_all_scope()),
                },
            )
            .await
            .expect("the sealed turn executes");
        let events = collect_events(stream).await;

        // Resolution picked the PATH entry named exactly `dsh`.
        let resolved =
            resolve_dsh_executable(None).expect("PATH dsh resolves under the isolated PATH");
        assert_eq!(resolved.file_name().and_then(|n| n.to_str()), Some("dsh"));

        // Turn mapping: exactly one OpStarted, the assistant text delta,
        // one OpFinished(EndTurn) from `turn/end` kind `completed`.
        assert!(matches!(events.first(), Some(HostEvent::OpStarted(_))));
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(delta) => Some(delta.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["mock dsh reply"], "events: {events:?}");
        assert_eq!(terminal_count(&events), 1);
        assert!(
            matches!(events.last(), Some(HostEvent::OpFinished(f)) if f.reason == FinishReason::EndTurn),
            "turn/end kind 'completed' maps to EndTurn: {events:?}"
        );

        let entries = req_log_entries(&req_log);
        let spawns = spawn_records(&entries);
        assert_eq!(spawns.len(), 2, "ordinary launch + sealed switch: {entries:?}");
        assert_eq!(
            argv_of(&spawns[0]),
            vec!["--profile".to_string(), "sdk".to_string()],
            "ordinary argv is exactly --profile sdk"
        );
        assert_eq!(
            spawns[0]["dsh_home"].as_str(),
            Some(dsh_home.to_string_lossy().as_ref()),
            "the ordinary child env carries the caller-env DSH_HOME"
        );
        let sealed_argv = argv_of(&spawns[1]);
        assert_eq!(sealed_argv.len(), 4, "sealed argv adds one owned patch pair");
        assert_eq!(sealed_argv[0..2], ["--profile".to_string(), "sdk".to_string()]);
        assert_eq!(sealed_argv[2], "--patch");
        let nexus_root = std::fs::canonicalize(&dsh_home)
            .expect("canonical selected home")
            .join(SEALED_HOME_SUBDIR);
        assert!(
            PathBuf::from(&sealed_argv[3]).starts_with(&nexus_root),
            "the owned patch lives inside the sealed child home"
        );
        let sealed_home = PathBuf::from(spawns[1]["dsh_home"].as_str().expect("sealed home"));
        assert!(sealed_home.starts_with(&nexus_root));

        // Wire order: ordinary spawn+init, cooperative ordinary close,
        // sealed spawn+init, then exactly one prompt (receipt/idle/
        // completed observed by the completed run above).
        let methods = method_sequence(&entries);
        assert_eq!(
            methods,
            vec![
                "_spawn".to_string(),
                "initialize".to_string(),
                "shutdown".to_string(),
                "_spawn".to_string(),
                "initialize".to_string(),
                "session/prompt".to_string(),
            ],
            "the ordinary close precedes the sealed init and the one prompt: {methods:?}"
        );

        // Confirmed process exit: the ordinary child is reaped by the
        // switch close (BEFORE the sealed turn completed); both pids are
        // distinct real children.
        let pids = spawn_pids(&entries);
        assert_ne!(pids[0], pids[1], "distinct runtime children");
        assert_pid_reaped(pids[0], "the ordinary child was reaped by the switch close").await;

        // Confirmed shutdown: the sealed child pid is reaped and the
        // lease is deleted.
        provider.shutdown(handle).await.expect("confirmed close");
        assert_pid_reaped(pids[1], "the sealed child was reaped by shutdown").await;
        assert!(
            !sealed_home.exists(),
            "the confirmed close deleted the sealed child home lease"
        );
    }

    /// Absent runtime: an explicit missing executable fails the launch
    /// with the typed unavailable error — no session record, no spawn.
    #[tokio::test]
    async fn absent_runtime_fails_launch_without_session_or_spawn() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let provider = DshNativeProvider::new(
            ProviderId::new("test-dsh-absent"),
            "Test".to_string(),
            Some(
                temp_dir
                    .path()
                    .join("definitely-missing-dsh")
                    .to_string_lossy()
                    .into_owned(),
            ),
            Vec::new(),
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
            TimeoutConfig::default(),
        )
        .expect("empty native args are accepted");

        let result = provider.launch(launch_spec()).await;
        assert!(
            matches!(result, Err(HostError::ProviderUnavailable { .. })),
            "an absent runtime fails the launch closed: {result:?}"
        );
        assert!(
            provider.sessions.read().await.is_empty(),
            "no session record survives a failed launch"
        );
        assert!(!req_log.exists(), "no runtime was ever spawned");
    }

    /// Non-executable runtime: an explicit path naming a real but
    /// non-executable file fails the launch closed — no session, no spawn.
    #[cfg(unix)]
    #[tokio::test]
    async fn non_executable_runtime_fails_launch_without_session_or_spawn() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let not_exec = temp_dir.path().join("dsh-not-executable");
        std::fs::write(&not_exec, b"#!/bin/sh\n").expect("write file");
        std::fs::set_permissions(&not_exec, std::fs::Permissions::from_mode(0o644))
            .expect("chmod 0644");
        let provider = DshNativeProvider::new(
            ProviderId::new("test-dsh-notexec"),
            "Test".to_string(),
            Some(not_exec.to_string_lossy().into_owned()),
            Vec::new(),
            stub_env(&req_log, &temp_dir.path().join("dsh-home")),
            TimeoutConfig::default(),
        )
        .expect("empty native args are accepted");

        let result = provider.launch(launch_spec()).await;
        assert!(
            matches!(result, Err(HostError::ProviderUnavailable { .. })),
            "a non-executable runtime fails the launch closed: {result:?}"
        );
        assert!(provider.sessions.read().await.is_empty());
        assert!(!req_log.exists(), "no runtime was ever spawned");
    }

    /// Wrong server identity: the fixture answers `initialize` with a
    /// foreign `serverInfo.name`; the SDK rejects the handshake, the
    /// launch fails closed with the safe decode-category diagnostic, no
    /// session is registered, and the spawned child is reaped by the
    /// SDK's failed-boot close ladder.
    #[cfg(unix)]
    #[tokio::test]
    async fn wrong_server_identity_fails_closed_and_reaps_child() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let mut env = stub_env(&req_log, &temp_dir.path().join("dsh-home"));
        env.insert("WRONG_IDENTITY".to_string(), "1".to_string());
        let provider = stub_provider("test-dsh-wrongid", env);

        let result = provider.launch(launch_spec()).await;
        let Err(HostError::LaunchFailed { message, .. }) = &result else {
            panic!("a foreign server identity must fail the launch: {result:?}");
        };
        assert_eq!(
            message, "dsh runtime violated the wire protocol",
            "the diagnostic is the safe static decode category: {message}"
        );
        assert!(
            provider.sessions.read().await.is_empty(),
            "no session record survives the rejected handshake"
        );
        let entries = req_log_entries(&req_log);
        let pids = spawn_pids(&entries);
        assert_eq!(pids.len(), 1, "the wrong-identity child was spawned once");
        assert_pid_reaped(pids[0], "the failed-boot child was reaped").await;
    }

    /// Initialize timeout: with the fixture holding the `initialize`
    /// reply past the configured handshake budget, the launch fails
    /// closed with the safe timeout diagnostic, no session is registered,
    /// and the child is reaped (the SDK close ladder runs over the
    /// failed boot).
    #[cfg(unix)]
    #[tokio::test]
    async fn initialize_timeout_fails_launch_and_reaps_child() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let mut env = stub_env(&req_log, &temp_dir.path().join("dsh-home"));
        env.insert("INIT_DELAY_MS".to_string(), "3000".to_string());
        let provider = stub_provider_with_timeouts(
            "test-dsh-inittimeout",
            env,
            TimeoutConfig {
                initialize_ms: 150,
                ..TimeoutConfig::default()
            },
        );

        let result = provider.launch(launch_spec()).await;
        let Err(HostError::LaunchFailed { message, .. }) = &result else {
            panic!("an initialize timeout must fail the launch: {result:?}");
        };
        assert_eq!(
            message, "dsh request timed out: initialize",
            "the diagnostic names only the allowlisted wire method: {message}"
        );
        assert!(provider.sessions.read().await.is_empty());
        let pids = spawn_pids(&req_log_entries(&req_log));
        assert_eq!(pids.len(), 1);
        assert_pid_reaped(pids[0], "the timed-out boot child was reaped").await;
    }

    /// Close error tolerance: the fixture answers the cooperative
    /// `shutdown` request with a JSON-RPC error. Per the SDK close
    /// ladder that failure is diagnostic only — EOF reaps the child —
    /// so shutdown is still CONFIRMED (session record removed, child pid
    /// reaped). A close that genuinely fails is never reported as
    /// success (the unconfirmed arms above pin that direction).
    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_rpc_error_is_diagnostic_and_close_is_confirmed() {
        let _env_lock = crate::test_support::PROCESS_ENV_LOCK
            .lock()
            .expect("lock env tests");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("reqs.jsonl");
        let mut env = stub_env(&req_log, &temp_dir.path().join("dsh-home"));
        env.insert("CLOSE_ERROR".to_string(), "1".to_string());
        let provider = stub_provider("test-dsh-closeerr", env);
        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider
            .shutdown(handle.clone())
            .await
            .expect("a shutdown RPC error is diagnostic; the ladder still confirms the close");
        assert!(
            !provider.sessions.read().await.contains_key(&handle.session_id),
            "a confirmed close removes the session record"
        );
        let entries = req_log_entries(&req_log);
        let methods = method_sequence(&entries);
        assert!(
            methods.contains(&"shutdown".to_string()),
            "the runtime saw the cooperative shutdown request: {methods:?}"
        );
        let pids = spawn_pids(&entries);
        assert_pid_reaped(pids[0], "the child was reaped after the failed shutdown RPC").await;
    }

}
