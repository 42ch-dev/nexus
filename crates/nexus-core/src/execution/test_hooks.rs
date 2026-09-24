//! Test-only seams for the workspace commit authority (v1.188 P3).
//!
//! Compiled ONLY for this package's own tests or with the `test-hooks`
//! feature; the production build contains none of this. Each seam injects a
//! failure/race into a specific commit boundary so the recovery proofs are
//! deterministic instead of timing-based.
//!
//! `cargo test -p nexus-core` enables them through this package's self
//! dev-dependency on the `execution` + `test-hooks` features; the daemon
//! enables the same edge for its own tests through its dev-dependencies.

use std::sync::{Arc, Mutex};

/// Crash-barrier labels understood by [`crash_point_is`].
static CRASH_POINT: Mutex<Option<&'static str>> = Mutex::new(None);

/// Set the next commit crash point (`None` disables simulation).
///
/// # Panics
///
/// Panics if the crash-point mutex was poisoned by a previous panic.
pub fn set_crash_point(point: Option<&'static str>) {
    *CRASH_POINT.lock().expect("crash point lock") = point;
}

/// Clear any armed crash point (the retained owner does this at spawn).
pub fn clear_crash_point() {
    set_crash_point(None);
}

pub(super) fn crash_point_is(point: &str) -> bool {
    *CRASH_POINT.lock().expect("crash point lock") == Some(point)
}

/// Hook fired immediately after a delete captures the target name and before
/// the delete is finalized.
static AFTER_DELETE_CAPTURE: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

/// Install (or clear) the after-delete-capture hook.
///
/// # Panics
///
/// Panics if the hook mutex was poisoned by a previous panic.
pub fn set_after_delete_capture_hook(hook: Option<Arc<dyn Fn() + Send + Sync>>) {
    *AFTER_DELETE_CAPTURE.lock().expect("hook lock") = hook;
}

pub(super) fn run_after_delete_capture_hook() {
    let hook = AFTER_DELETE_CAPTURE.lock().expect("hook lock").clone();
    if let Some(hook) = hook {
        hook();
    }
}

/// Rendezvous at the commit owner's admission boundary, scoped to ONE session.
///
/// Lets a test prove caller-cancellation safety deterministically: the owner
/// signals `admitted` once it holds the claim, the test cancels the awaiting
/// caller, then releases `proceed` and awaits `settled`. No sleeps, no races.
///
/// The gate names the session it governs, so a gate armed by one test never
/// parks another test's commit (unit tests run in parallel).
#[derive(Debug)]
pub struct OwnerGate {
    /// The session whose owner participates in this rendezvous.
    pub session_id: String,
    /// Signalled when the owner has been admitted (claim held).
    pub admitted: Arc<tokio::sync::Notify>,
    /// The owner waits for this before continuing past admission.
    pub proceed: Arc<tokio::sync::Notify>,
    /// Signalled after the owner task settles.
    pub settled: Arc<tokio::sync::Notify>,
}

impl OwnerGate {
    /// A gate governing `session_id`.
    #[must_use]
    pub fn for_session(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            admitted: Arc::new(tokio::sync::Notify::new()),
            proceed: Arc::new(tokio::sync::Notify::new()),
            settled: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

static OWNER_GATE: Mutex<Option<Arc<OwnerGate>>> = Mutex::new(None);

/// Install (or clear) the owner gate.
///
/// # Panics
///
/// Panics if the owner-gate mutex was poisoned by a previous panic.
pub fn set_owner_gate(gate: Option<Arc<OwnerGate>>) {
    *OWNER_GATE.lock().expect("owner gate lock") = gate;
}

fn current_owner_gate(session_id: &str) -> Option<Arc<OwnerGate>> {
    let gate = OWNER_GATE
        .lock()
        .expect("owner gate lock")
        .as_ref()
        .map(Arc::clone)?;
    if gate.session_id == session_id {
        Some(gate)
    } else {
        None
    }
}

pub(super) async fn owner_gate_admitted(session_id: &str) {
    if let Some(gate) = current_owner_gate(session_id) {
        gate.admitted.notify_one();
        gate.proceed.notified().await;
    }
}

pub(super) fn owner_gate_settled(session_id: &str) {
    if let Some(gate) = current_owner_gate(session_id) {
        gate.settled.notify_one();
    }
}

/// Rendezvous at a recovery pass's settlement boundary.
///
/// The commit seams above are keyed by session id; a recovery pass has no
/// single session — it settles every unsettled intent of its scope — so this
/// gate is process-wide and armed by the one test that drives a pass. It lets
/// that test prove caller-cancellation safety deterministically: the pass
/// signals `admitted` once it holds its admission and is about to settle an
/// unsettled intent (the crash-consistent on-disk state), the test cancels the
/// awaiting caller, then releases `proceed` and awaits `settled`. No sleeps, no
/// races.
#[derive(Debug, Default)]
pub struct RecoveryGate {
    /// Signalled once the pass is admitted and about to settle an intent.
    pub admitted: Arc<tokio::sync::Notify>,
    /// The pass waits for this before touching that intent.
    pub proceed: Arc<tokio::sync::Notify>,
    /// Signalled after the retained pass task settles.
    pub settled: Arc<tokio::sync::Notify>,
}

impl RecoveryGate {
    /// An armed gate for one recovery pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

static RECOVERY_GATE: Mutex<Option<Arc<RecoveryGate>>> = Mutex::new(None);

/// Install (or clear) the recovery gate.
///
/// # Panics
///
/// Panics if the recovery-gate mutex was poisoned by a previous panic.
pub fn set_recovery_gate(gate: Option<Arc<RecoveryGate>>) {
    *RECOVERY_GATE.lock().expect("recovery gate lock") = gate;
}

fn current_recovery_gate() -> Option<Arc<RecoveryGate>> {
    RECOVERY_GATE
        .lock()
        .expect("recovery gate lock")
        .as_ref()
        .map(Arc::clone)
}

pub(super) async fn recovery_gate_admitted() {
    if let Some(gate) = current_recovery_gate() {
        gate.admitted.notify_one();
        gate.proceed.notified().await;
    }
}

pub(super) fn recovery_gate_settled() {
    if let Some(gate) = current_recovery_gate() {
        gate.settled.notify_one();
    }
}
