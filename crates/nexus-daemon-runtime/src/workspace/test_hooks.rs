//! Test-only seams for the workspace commit authority (v1.188 P3).
//!
//! Compiled ONLY for this package's own tests or with the `test-hooks`
//! feature; the production build contains none of this. Each seam injects a
//! failure/race into a specific commit boundary so the recovery proofs are
//! deterministic instead of timing-based.
//!
//! `cargo test -p nexus-daemon-runtime` enables them through this package's
//! self dev-dependency on the `test-support` feature.

use std::sync::{Arc, Mutex};

/// Crash-barrier labels understood by [`crash_point_is`].
static CRASH_POINT: Mutex<Option<&'static str>> = Mutex::new(None);

/// Set the next commit crash point (`None` disables simulation).
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
