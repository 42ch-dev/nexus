//! `LocalSet` bridge for `!Send` ACP SDK futures with tracked tasks and bounded admission.
//!
//! One lazy current-thread Tokio `LocalSet` OS thread per bridge instance. Send-side
//! callers submit factory closures; the `!Send` future is created on the owning thread.
//! Shutdown uses a dedicated control channel that bypasses the bounded work queue.

use std::any::Any;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot, Notify};
use tracing::{debug, error, warn};

/// Maximum queued execute requests (count).
pub const MAX_PENDING_REQUESTS: usize = 16;
/// Maximum queued execute request bytes (sum of charged payloads).
pub const MAX_PENDING_BYTES: usize = 1024 * 1024;
/// Maximum concurrently running `LocalSet` tasks.
pub const MAX_ACTIVE_TASKS: usize = 32;
/// Admission wait before returning Busy.
pub const ADMISSION_WAIT: Duration = Duration::from_secs(2);
/// Total shutdown join budget.
pub const SHUTDOWN_JOIN_BUDGET: Duration = Duration::from_secs(5);

type TaskResult = Result<Box<dyn Any + Send>, String>;
type ErasedFuture = Pin<Box<dyn Future<Output = TaskResult> + 'static>>;
type FutureFactory = Box<dyn FnOnce() -> ErasedFuture + Send + 'static>;
type OwnedFactory =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + 'static>;

struct ExecuteRequest {
    task_id: u64,
    byte_charge: usize,
    class: TaskClass,
    factory: FutureFactory,
    result_tx: Option<oneshot::Sender<TaskResult>>,
}

/// A tracked long-lived session-ownership future (e.g. the SDK connection loop).
///
/// Owner tasks are created on the `LocalSet` thread and are tracked by task id and
/// abort handle like every other task, but they are not work *requests*: they do
/// not consume the pending/active request budget, whose caps bound concurrent
/// work (`Concurrent native calls` in the lifecycle contract). Their count is
/// bounded by host session admission (at most one per connected session).
struct OwnedRequest {
    task_id: u64,
    factory: OwnedFactory,
}

enum WorkMessage {
    Execute(ExecuteRequest),
}

enum ControlMessage {
    Cancel {
        task_id: u64,
    },
    SpawnOwned(OwnedRequest),
    RunControl(ExecuteRequest),
    Shutdown {
        ack: oneshot::Sender<ShutdownEvidence>,
    },
}

/// Evidence returned after shutdown for proof/reporting.
///
/// The `*_at_shutdown` fields are the queue snapshot taken when shutdown was
/// requested; the plain counters are the final values after the drain, so a
/// settled shutdown reports zero live work alongside what it actually drained.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShutdownEvidence {
    pub joined_cleanly: bool,
    pub thread_alive: bool,
    pub pending_requests: usize,
    pub pending_bytes: usize,
    pub active_tasks: usize,
    pub owned_tasks: usize,
    pub control_tasks: usize,
    /// Requests queued (admitted, not yet started) when shutdown was requested.
    pub queued_at_shutdown: usize,
    /// Charged bytes of those queued requests.
    pub queued_bytes_at_shutdown: usize,
    /// Work tasks running when shutdown was requested.
    pub active_at_shutdown: usize,
    /// Session-ownership tasks running when shutdown was requested.
    pub owned_at_shutdown: usize,
    /// Cancellation/teardown tasks running when shutdown was requested.
    pub control_at_shutdown: usize,
    pub aborted_task_ids: Vec<u64>,
}

impl ShutdownEvidence {
    /// Whether the bridge actually settled.
    ///
    /// Settlement requires a **clean join** of the owning thread — not merely a
    /// dead one. A thread that panicked reports `thread_alive == false` *and*
    /// `joined_cleanly == false`, so `!thread_alive` on its own must never be
    /// read as success. Live work counters must also be drained.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        self.joined_cleanly
            && !self.thread_alive
            && self.active_tasks == 0
            && self.pending_requests == 0
            && self.owned_tasks == 0
            && self.control_tasks == 0
    }

    /// Actionable detail describing why this bridge is not settled.
    ///
    /// Rendered into `CoreCloseReport.pending_operations` so a retained owner
    /// is diagnosable instead of silent.
    #[must_use]
    pub fn unsettled_pending(&self) -> Vec<String> {
        let mut entries = Vec::new();
        if self.thread_alive {
            entries.push("localset-thread-alive".to_string());
        } else if !self.joined_cleanly {
            entries.push("localset-thread-join-failed".to_string());
        }
        if self.active_tasks > 0 {
            entries.push(format!("localset-active-tasks:{}", self.active_tasks));
        }
        if self.pending_requests > 0 {
            entries.push(format!(
                "localset-pending-requests:{}",
                self.pending_requests
            ));
        }
        if self.owned_tasks > 0 {
            entries.push(format!("localset-owned-tasks:{}", self.owned_tasks));
        }
        if self.control_tasks > 0 {
            entries.push(format!("localset-control-tasks:{}", self.control_tasks));
        }
        entries
    }
}

/// Live bridge counters (for tests and lifecycle proofs).
#[derive(Debug, Clone, Default)]
pub struct LocalSetBridgeStats {
    pub pending_requests: usize,
    pub pending_bytes: usize,
    pub active_tasks: usize,
    pub owned_tasks: usize,
    pub control_tasks: usize,
    /// Set once the environment begins closing: new work is refused.
    pub draining: bool,
    pub shutting_down: bool,
}

impl LocalSetBridge {
    /// Refuse new work and freeze the queue: the environment is closing.
    ///
    /// Active work keeps running so the close can cancel/drain it cooperatively;
    /// queued requests are not started, and cancellation/teardown work still runs.
    pub fn begin_drain(&self) {
        self.counters.draining.store(true, Ordering::SeqCst);
    }

    /// Render `CoreCloseReport.pending_operations` entries for this close.
    ///
    /// `at_close` is the queue snapshot taken when the close began; `evidence` is
    /// the settled shutdown result. A confirmed close reports what it drained:
    /// queued request count/bytes and running work/ownership/teardown task counts,
    /// plus every tracked task id the control channel aborted. An idle bridge
    /// contributes nothing.
    #[must_use]
    pub fn close_entries(
        &self,
        at_close: &LocalSetBridgeStats,
        evidence: &ShutdownEvidence,
    ) -> Vec<String> {
        let mut entries = Vec::new();
        if at_close.pending_requests > 0 || at_close.pending_bytes > 0 {
            entries.push(format!(
                "localset-queued-at-close:{}",
                at_close.pending_requests
            ));
            entries.push(format!(
                "localset-queued-bytes-at-close:{}",
                at_close.pending_bytes
            ));
        }
        if at_close.active_tasks > 0 {
            entries.push(format!(
                "localset-active-at-close:{}",
                at_close.active_tasks
            ));
        }
        if at_close.owned_tasks > 0 {
            entries.push(format!("localset-owned-at-close:{}", at_close.owned_tasks));
        }
        if at_close.control_tasks > 0 {
            entries.push(format!(
                "localset-control-at-close:{}",
                at_close.control_tasks
            ));
        }
        entries.extend(
            evidence
                .aborted_task_ids
                .iter()
                .map(|task_id| format!("localset-aborted-task:{task_id}")),
        );
        entries
    }
}

#[derive(Debug, Default)]
struct PendingAdmission {
    requests: usize,
    bytes: usize,
}

struct AdmissionCounters {
    pending: Mutex<PendingAdmission>,
    active_tasks: AtomicUsize,
    owned_tasks: AtomicUsize,
    control_tasks: AtomicUsize,
    draining: AtomicBool,
    shutting_down: AtomicBool,
    next_task_id: AtomicU64,
    /// Set when the owning thread is spawned. A taken `JoinHandle` is not
    /// evidence that no thread ever existed.
    thread_started: AtomicBool,
    thread_joined: AtomicBool,
    /// Fires when a `TaskSlot` is installed (before first poll). Used by tests.
    task_slot_notify: Notify,
}

impl AdmissionCounters {
    fn new() -> Self {
        Self {
            pending: Mutex::new(PendingAdmission::default()),
            active_tasks: AtomicUsize::new(0),
            owned_tasks: AtomicUsize::new(0),
            control_tasks: AtomicUsize::new(0),
            draining: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            next_task_id: AtomicU64::new(1),
            thread_started: AtomicBool::new(false),
            thread_joined: AtomicBool::new(false),
            task_slot_notify: Notify::new(),
        }
    }

    fn stats(&self) -> LocalSetBridgeStats {
        let pending = self.pending.lock().expect("admission mutex poisoned");
        LocalSetBridgeStats {
            pending_requests: pending.requests,
            pending_bytes: pending.bytes,
            active_tasks: self.active_tasks.load(Ordering::Acquire),
            owned_tasks: self.owned_tasks.load(Ordering::Acquire),
            control_tasks: self.control_tasks.load(Ordering::Acquire),
            draining: self.draining.load(Ordering::Acquire),
            shutting_down: self.shutting_down.load(Ordering::Acquire),
        }
    }

    /// Atomically reserve pending count + bytes under one lock.
    fn try_reserve_pending(&self, bytes: usize) -> bool {
        if self.shutting_down.load(Ordering::Acquire) {
            return false;
        }
        let mut pending = self.pending.lock().expect("admission mutex poisoned");
        if pending.requests >= MAX_PENDING_REQUESTS {
            return false;
        }
        if pending.bytes.saturating_add(bytes) > MAX_PENDING_BYTES {
            return false;
        }
        pending.requests += 1;
        pending.bytes += bytes;
        true
    }

    fn release_pending(&self, bytes: usize) {
        let mut pending = self.pending.lock().expect("admission mutex poisoned");
        pending.requests = pending.requests.saturating_sub(1);
        pending.bytes = pending.bytes.saturating_sub(bytes);
    }

    /// Reserve an active-task slot synchronously before `spawn_local`.
    fn try_reserve_active(&self) -> bool {
        loop {
            let current = self.active_tasks.load(Ordering::Acquire);
            if current >= MAX_ACTIVE_TASKS {
                return false;
            }
            if self
                .active_tasks
                .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }

    fn task_finished(&self) {
        self.active_tasks.fetch_sub(1, Ordering::AcqRel);
    }

    fn owned_task_finished(&self) {
        self.owned_tasks.fetch_sub(1, Ordering::AcqRel);
    }

    /// Charge a cancellation/teardown task (not part of the request budget).
    fn track_control(&self) {
        self.control_tasks.fetch_add(1, Ordering::AcqRel);
    }

    fn control_task_finished(&self) {
        self.control_tasks.fetch_sub(1, Ordering::AcqRel);
    }

    fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }

    /// Charge a session-ownership task (not part of the request budget).
    fn track_owned(&self) {
        self.owned_tasks.fetch_add(1, Ordering::AcqRel);
    }

    fn mark_thread_started(&self) {
        self.thread_started.store(true, Ordering::SeqCst);
    }

    fn thread_started(&self) -> bool {
        self.thread_started.load(Ordering::SeqCst)
    }

    fn mark_thread_joined(&self) {
        self.thread_joined.store(true, Ordering::SeqCst);
    }

    fn thread_joined(&self) -> bool {
        self.thread_joined.load(Ordering::SeqCst)
    }
}

/// Bounded, bridge-owned control relay.
///
/// Callers push here — including `Drop` paths that cannot await — and the
/// `LocalSet` thread drains it ahead of data work, so a cancellation never waits
/// behind queued requests. Nothing on this path spawns an OS thread.
///
/// Cancels are idempotent per task, so they are coalesced by task id: that keeps
/// the queue bounded by the number of tracked tasks however many waiters are
/// dropped, while every *distinct* cancellation is still delivered. Other
/// control messages are capped at [`MAX_CONTROL_QUEUE`]; anything refused past
/// that cap is counted in the `refused` total rather than silently disappearing.
struct ControlRelay {
    inner: Mutex<RelayState>,
    notify: Notify,
    closed: AtomicBool,
    /// Monotonic totals. Unlike the queue (which the owning thread drains) these
    /// never decrease, so tests and diagnostics can observe them race-free.
    accepted: AtomicU64,
    cancels_accepted: AtomicU64,
}

/// Hard cap on messages held in the relay queue. Cancels coalesce per task, so
/// in practice the queue tracks live tasks; this constant bounds the pathological
/// case of a flood of distinct task ids.
const MAX_CONTROL_QUEUE: usize = 128;

/// Seals the relay when the owning thread exits — normally or by unwinding — so
/// a later push cannot queue work that nothing will ever drain.
struct RelayCloser(Arc<ControlRelay>);

impl Drop for RelayCloser {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl std::fmt::Debug for ControlRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.inner.lock().ok();
        let (queued, refused) = state
            .as_ref()
            .map_or((0, 0), |state| (state.queue.len(), state.refused));
        f.debug_struct("ControlRelay")
            .field("queued", &queued)
            .field("refused", &refused)
            .field("closed", &self.closed.load(Ordering::Acquire))
            .field("accepted", &self.accepted.load(Ordering::Acquire))
            .field(
                "cancels_accepted",
                &self.cancels_accepted.load(Ordering::Acquire),
            )
            .field("notify", &self.notify)
            .finish()
    }
}

#[derive(Default)]
struct RelayState {
    queue: VecDeque<ControlMessage>,
    pending_cancels: HashSet<u64>,
    refused: u64,
}

impl ControlRelay {
    fn new() -> Self {
        Self {
            inner: Mutex::new(RelayState::default()),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
            accepted: AtomicU64::new(0),
            cancels_accepted: AtomicU64::new(0),
        }
    }

    /// Enqueue control work. Never blocks and never spawns a thread. Returns
    /// `false` when the owning thread is gone or the queue shed the message, so
    /// a caller that needs an acknowledgement can fail fast instead of waiting
    /// out its deadline.
    #[must_use]
    fn push(&self, msg: ControlMessage) -> bool {
        {
            let Ok(mut state) = self.inner.lock() else {
                return false;
            };
            if self.closed.load(Ordering::Acquire) {
                return false;
            }
            if let ControlMessage::Cancel { task_id } = &msg {
                if !state.pending_cancels.insert(*task_id) {
                    // A cancel for this task is already pending: idempotent.
                    return true;
                }
                if state.queue.len() >= MAX_CONTROL_QUEUE {
                    // Hard bound: the relay never grows past the cap, even if
                    // a caller floods it with distinct task ids.
                    state.pending_cancels.remove(task_id);
                    state.refused += 1;
                    return false;
                }
                state.queue.push_back(msg);
                self.cancels_accepted.fetch_add(1, Ordering::AcqRel);
            } else {
                if state.queue.len() >= MAX_CONTROL_QUEUE {
                    state.refused += 1;
                    return false;
                }
                state.queue.push_back(msg);
            }
            self.accepted.fetch_add(1, Ordering::AcqRel);
        }
        self.notify.notify_one();
        true
    }

    /// Take everything queued, in FIFO order.
    fn drain(&self) -> Vec<ControlMessage> {
        let Ok(mut state) = self.inner.lock() else {
            return Vec::new();
        };
        state.pending_cancels.clear();
        state.queue.drain(..).collect()
    }

    /// Stop accepting work (the owning thread has exited).
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut state) = self.inner.lock() {
            state.queue.clear();
            state.pending_cancels.clear();
        }
        self.notify.notify_waiters();
    }

    /// Distinct cancellations enqueued so far (monotonic).
    #[cfg(test)]
    fn cancels_accepted(&self) -> u64 {
        self.cancels_accepted.load(Ordering::Acquire)
    }
}

/// Threads whose owning bridge was dropped inside a Tokio context, where a
/// synchronous join would block the executor.
///
/// They are tracked here and reaped by a bounded background joiner, so a
/// forgotten `shutdown()` can never leave an untracked detached OS thread.
static ABANDONED_THREADS: OnceLock<Mutex<Vec<JoinHandle<()>>>> = OnceLock::new();
/// Monotonic count of threads ever handed to the fallback owner. Used by tests:
/// the live list can be drained by the reaper at any moment, so a test cannot
/// assert on its size without racing.
static ABANDONED_TOTAL: AtomicUsize = AtomicUsize::new(0);
static ABANDONED_REAPER: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();

fn abandoned_threads() -> &'static Mutex<Vec<JoinHandle<()>>> {
    ABANDONED_THREADS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Join every finished abandoned bridge thread and drop its entry.
fn reap_abandoned_threads() -> usize {
    let Ok(mut threads) = abandoned_threads().lock() else {
        return 0;
    };
    let mut reaped = 0usize;
    let mut i = 0;
    while i < threads.len() {
        if threads[i].is_finished() {
            let handle = threads.swap_remove(i);
            let _ = handle.join();
            reaped += 1;
        } else {
            i += 1;
        }
    }
    reaped
}

/// Hand a bridge thread to the tracked fallback owner.
fn track_abandoned_thread(handle: JoinHandle<()>) {
    ABANDONED_TOTAL.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut threads) = abandoned_threads().lock() {
        threads.push(handle);
    }
    ensure_abandoned_reaper();
}

/// One bounded joiner that exits once every tracked thread has been reaped.
fn ensure_abandoned_reaper() {
    let owner = ABANDONED_REAPER.get_or_init(|| Mutex::new(None));
    let Ok(mut slot) = owner.lock() else {
        return;
    };
    if slot.as_ref().is_some_and(|handle| !handle.is_finished()) {
        return;
    }
    if let Some(finished) = slot.take() {
        let _ = finished.join();
    }
    *slot = Some(
        thread::Builder::new()
            .name("nexus-localset-reaper".into())
            .spawn(|| loop {
                thread::sleep(Duration::from_millis(25));
                let _ = reap_abandoned_threads();
                let empty = abandoned_threads()
                    .lock()
                    .map_or(true, |threads| threads.is_empty());
                if empty {
                    break;
                }
            })
            .expect("spawn LocalSet abandoned-thread reaper"),
    );
}

/// Number of bridge threads currently tracked by the fallback owner (tests).
#[cfg(test)]
#[must_use]
pub fn abandoned_thread_count() -> usize {
    abandoned_threads()
        .lock()
        .map_or(0, |threads| threads.len())
}

/// Monotonic count of bridge threads ever handed to the fallback owner (tests).
#[cfg(test)]
#[must_use]
pub fn abandoned_threads_tracked_total() -> usize {
    ABANDONED_TOTAL.load(Ordering::SeqCst)
}

struct BridgeRuntime {
    work_tx: mpsc::Sender<WorkMessage>,
    control: Arc<ControlRelay>,
    thread_handle: JoinHandle<()>,
}

/// Bridge between async tokio world and `!Send` `LocalSet` world.
#[derive(Clone)]
pub struct LocalSetBridge {
    runtime: Arc<Mutex<Option<BridgeRuntime>>>,
    counters: Arc<AdmissionCounters>,
    shutdown_claimed: Arc<AtomicBool>,
    /// Retained when a bounded join times out — never detached.
    retained_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// The one settled shutdown result, shared with concurrent callers so a
    /// loser observes the real settlement instead of inventing a snapshot.
    settled: Arc<Mutex<Option<ShutdownEvidence>>>,
}

impl LocalSetBridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(None)),
            counters: Arc::new(AdmissionCounters::new()),
            shutdown_claimed: Arc::new(AtomicBool::new(false)),
            retained_thread: Arc::new(Mutex::new(None)),
            settled: Arc::new(Mutex::new(None)),
        }
    }

    /// Whether a runtime thread `JoinHandle` is retained after a join timeout.
    #[must_use]
    pub fn has_retained_runtime_thread(&self) -> bool {
        self.retained_thread.lock().is_ok_and(|slot| slot.is_some())
    }

    /// Take a retained runtime thread handle for tracked cleanup registration.
    #[must_use]
    pub fn take_retained_runtime_thread(&self) -> Option<JoinHandle<()>> {
        self.retained_thread.lock().ok()?.take()
    }

    /// The settled shutdown result, if the owning shutdown has finished.
    fn settled_evidence(&self) -> Option<ShutdownEvidence> {
        self.settled.lock().ok().and_then(|slot| slot.clone())
    }

    /// Publish the bridge's single settlement. First writer wins, so a bridge
    /// can never settle twice; returns whether this call was the one that
    /// published.
    fn publish_settled(&self, evidence: &ShutdownEvidence) -> bool {
        if let Ok(mut slot) = self.settled.lock() {
            if slot.is_none() {
                *slot = Some(evidence.clone());
                return true;
            }
        }
        false
    }

    /// Wait for the owning shutdown to publish its settlement. `None` means the
    /// deadline passed without one.
    async fn await_settlement(&self, deadline: Instant) -> Option<ShutdownEvidence> {
        loop {
            if let Some(evidence) = self.settled_evidence() {
                return Some(evidence);
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Synchronous counterpart of [`Self::await_settlement`].
    fn await_settlement_sync(&self, deadline: Instant) -> Option<ShutdownEvidence> {
        loop {
            if let Some(evidence) = self.settled_evidence() {
                return Some(evidence);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn retain_runtime_thread(&self, handle: JoinHandle<()>) {
        if let Ok(mut slot) = self.retained_thread.lock() {
            if let Some(existing) = slot.take() {
                if existing.is_finished() {
                    let _ = existing.join();
                } else {
                    *slot = Some(existing);
                    warn!(
                        "replacing retained LocalSet thread handle while prior thread still alive"
                    );
                    return;
                }
            }
            *slot = Some(handle);
        }
    }

    #[must_use]
    pub fn stats(&self) -> LocalSetBridgeStats {
        self.counters.stats()
    }

    fn ensure_runtime(
        &self,
    ) -> Result<(mpsc::Sender<WorkMessage>, Arc<ControlRelay>), crate::AcpError> {
        let mut guard = self
            .runtime
            .lock()
            .map_err(|_| crate::AcpError::connection_failed("LocalSet bridge mutex poisoned"))?;
        if let Some(rt) = guard.as_ref() {
            return Ok((rt.work_tx.clone(), rt.control.clone()));
        }
        if self.counters.shutting_down.load(Ordering::Acquire) {
            return Err(crate::AcpError::connection_failed(
                "LocalSet bridge shutting down",
            ));
        }

        let counters = self.counters.clone();
        let (work_tx, work_rx) = mpsc::channel::<WorkMessage>(MAX_PENDING_REQUESTS);
        let control = Arc::new(ControlRelay::new());

        let thread_counters = self.counters.clone();
        let thread_control = control.clone();
        let closer_control = control.clone();
        let thread_handle = thread::Builder::new()
            .name("nexus-localset-bridge".to_string())
            .spawn(move || {
                let _closer = RelayCloser(closer_control);
                run_localset_thread(work_rx, thread_control, thread_counters);
            })
            .map_err(|e| {
                crate::AcpError::connection_failed(format!("spawn LocalSet thread: {e}"))
            })?;

        counters.mark_thread_started();
        let runtime = BridgeRuntime {
            work_tx: work_tx.clone(),
            control: control.clone(),
            thread_handle,
        };
        *guard = Some(runtime);
        drop(guard);
        Ok((work_tx, control))
    }

    async fn wait_admission(&self, bytes: usize) -> Result<(), crate::AcpError> {
        let deadline = Instant::now() + ADMISSION_WAIT;
        while Instant::now() < deadline {
            if self.counters.shutting_down.load(Ordering::Acquire) {
                return Err(crate::AcpError::connection_failed(
                    "LocalSet bridge shutting down",
                ));
            }
            if self.counters.is_draining() {
                return Err(crate::AcpError::connection_failed(
                    "LocalSet bridge closing",
                ));
            }
            if self.counters.try_reserve_pending(bytes) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(crate::AcpError::connection_failed(
            "LocalSet bridge admission busy after 2s",
        ))
    }

    /// Execute a `!Send` future on the `LocalSet` thread.
    ///
    /// `byte_charge` is the UTF-8 byte size of the logical request payload charged
    /// against the pending-byte budget before admission.
    ///
    /// # Errors
    ///
    /// Returns an error when admission fails — the bridge is shutting down or
    /// draining, or the pending budget stays exhausted for [`ADMISSION_WAIT`] —
    /// when the owning thread cannot be spawned or has exited, when the task
    /// is cancelled or aborted before it delivers a result, or when the
    /// closure's output type does not match `T`.
    pub async fn execute<F, T>(&self, byte_charge: usize, f: F) -> crate::AcpResult<T>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = T> + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        self.wait_admission(byte_charge).await?;
        self.dispatch(TaskClass::Work, byte_charge, f).await
    }

    /// Execute cancellation/teardown work on the `LocalSet` thread.
    ///
    /// Control work is tracked and aborted by shutdown like every other task, but
    /// it is never queued and is not charged against the pending/active request
    /// budget: a full work queue must not block the cancellation that ends it.
    ///
    /// # Errors
    ///
    /// Returns an error when the owning thread cannot be spawned or has
    /// exited, or when the task is cancelled or aborted before it delivers a
    /// result. Control work never waits on admission, so queue pressure alone
    /// cannot fail it.
    pub async fn execute_control<F, T>(&self, byte_charge: usize, f: F) -> crate::AcpResult<T>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = T> + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        self.dispatch(TaskClass::Control, byte_charge, f).await
    }

    async fn dispatch<F, T>(
        &self,
        class: TaskClass,
        byte_charge: usize,
        f: F,
    ) -> crate::AcpResult<T>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = T> + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        let (work_tx, control) = self.ensure_runtime()?;

        let task_id = self.counters.next_task_id.fetch_add(1, Ordering::AcqRel);
        let (result_tx, result_rx) = oneshot::channel();

        let factory: FutureFactory = Box::new(move || {
            let future = f();
            Box::pin(async move {
                let value = future.await;
                Ok(Box::new(value) as Box<dyn Any + Send>)
            })
        });

        let message = match class {
            TaskClass::Control => ControlMessage::RunControl(ExecuteRequest {
                task_id,
                byte_charge,
                class,
                factory,
                result_tx: Some(result_tx),
            }),
            TaskClass::Work | TaskClass::Owned => {
                if work_tx
                    .send(WorkMessage::Execute(ExecuteRequest {
                        task_id,
                        byte_charge,
                        class,
                        factory,
                        result_tx: Some(result_tx),
                    }))
                    .await
                    .is_err()
                {
                    if class == TaskClass::Work {
                        self.counters.release_pending(byte_charge);
                    }
                    return Err(crate::AcpError::connection_failed(
                        "LocalSet work channel closed",
                    ));
                }
                let cancel_on_drop = CancelGuard {
                    control,
                    task_id,
                    cancelled: Arc::new(AtomicBool::new(false)),
                };
                let recv = result_rx.await;
                cancel_on_drop.disarm();
                return Self::task_result(recv);
            }
        };
        let _ = control.push(message);

        let recv = result_rx.await;
        Self::task_result(recv)
    }

    fn task_result<T>(recv: Result<TaskResult, oneshot::error::RecvError>) -> crate::AcpResult<T>
    where
        T: Send + 'static,
    {
        match recv {
            Ok(Ok(boxed)) => {
                let value = *boxed.downcast::<T>().map_err(|_| {
                    crate::AcpError::connection_failed("LocalSet result type mismatch")
                })?;
                Ok(value)
            }
            Ok(Err(message)) => Err(crate::AcpError::connection_failed(message)),
            Err(_) => Err(crate::AcpError::connection_failed(
                "LocalSet bridge response channel closed",
            )),
        }
    }

    /// Execute with timeout; dropped/expired waiter cancels the owned `LocalSet` task.
    ///
    /// # Errors
    ///
    /// Returns a timeout error naming `operation_name` when the task does not
    /// finish within `timeout_duration` (the dropped waiter cancels the owned
    /// `LocalSet` task), and otherwise the same errors as [`Self::execute`].
    pub async fn execute_with_timeout<F, T>(
        &self,
        byte_charge: usize,
        f: F,
        timeout_duration: Duration,
        operation_name: &str,
    ) -> crate::AcpResult<T>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = T> + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        tokio::time::timeout(timeout_duration, self.execute(byte_charge, f))
            .await
            .map_err(|_| crate::AcpError::timeout(operation_name, timeout_duration))?
    }

    /// Wait until a `TaskSlot` is installed on the `LocalSet` thread (before first poll).
    pub async fn wait_task_slot_installed(&self) {
        self.counters.task_slot_notify.notified().await;
    }

    /// Spawn a tracked session-ownership future on the `LocalSet` thread.
    ///
    /// Ownership futures (the SDK connection loop that keeps an owned ACP child's
    /// transport alive) are long lived by construction. They are registered in
    /// the same task map as work requests — so shutdown aborts them by task id
    /// and reports them — but they do not consume the pending/active *request*
    /// budget. Charging them would starve the bounded work queue permanently,
    /// because a queued request can only start when an active task finishes.
    ///
    /// Returns a handle that aborts exactly this tracked task.
    ///
    /// # Errors
    ///
    /// Returns an error when the bridge is shutting down or its owning thread
    /// cannot be spawned or has exited.
    pub fn spawn_owned<F>(&self, f: F) -> crate::AcpResult<OwnedTaskHandle>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + 'static,
    {
        let (_, control) = self.ensure_runtime()?;
        let task_id = self.counters.next_task_id.fetch_add(1, Ordering::AcqRel);
        let msg = ControlMessage::SpawnOwned(OwnedRequest {
            task_id,
            factory: Box::new(f),
        });
        let _ = control.push(msg);
        Ok(OwnedTaskHandle { control, task_id })
    }

    /// Cancel an owned task by id via the control channel.
    ///
    /// The cancel is enqueued on the bounded control relay and applied by the
    /// owning thread; the returned future completes immediately.
    ///
    /// # Errors
    ///
    /// Returns an error when the bridge is shutting down or its owning thread
    /// cannot be spawned or has exited. An enqueue failure after the runtime
    /// exists (relay closed or full) is ignored: cancellation is best-effort,
    /// matching the `CancelGuard` drop path.
    #[must_use = "the cancel is only enqueued once the returned future is polled"]
    pub fn cancel_task(&self, task_id: u64) -> impl Future<Output = Result<(), crate::AcpError>> {
        std::future::ready(self.ensure_runtime().map(|(_, control)| {
            let _ = control.push(ControlMessage::Cancel { task_id });
        }))
    }

    /// Explicit shutdown with join evidence. Idempotent.
    pub async fn shutdown(&self) -> ShutdownEvidence {
        let deadline = Instant::now() + SHUTDOWN_JOIN_BUDGET;
        self.shutdown_inner(true, deadline).await
    }

    /// Shutdown using a caller-supplied absolute deadline (shared across ack + join).
    pub async fn shutdown_before(&self, deadline: Instant) -> ShutdownEvidence {
        self.shutdown_inner(true, deadline).await
    }

    async fn shutdown_inner(&self, join_thread: bool, deadline: Instant) -> ShutdownEvidence {
        if self
            .shutdown_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            // One settlement: a concurrent caller observes the owner's result
            // rather than manufacturing a snapshot of its own. The owner
            // publishes on every exit path, so this returns as soon as it does.
            if let Some(evidence) = self.await_settlement(deadline).await {
                return evidence;
            }
            // Last resort only (the owner never published): report the honest
            // observed state — never claim a settlement the owner did not make,
            // and never join a thread the owner still owns.
            warn!("LocalSet shutdown settlement not published before deadline");
            return self.evidence_from_state();
        }
        self.counters.shutting_down.store(true, Ordering::SeqCst);

        // One exit path for every winner case — an unstarted bridge, a
        // failed-open bridge, and a started bridge all reach the single
        // `publish_settled` below, so concurrent losers always observe the same
        // settlement instead of waiting out their deadline.
        let control = {
            let guard = self.runtime.lock().ok();
            guard.and_then(|g| g.as_ref().map(|rt| rt.control.clone()))
        };

        let mut evidence = if let Some(control) = control {
            let (ack_tx, ack_rx) = oneshot::channel();
            // Only wait for an acknowledgement the owning thread can still send;
            // a dead thread must fail fast rather than burn the whole budget.
            let sent = control.push(ControlMessage::Shutdown { ack: ack_tx })
                && self.runtime_thread_alive();
            if sent {
                let ack_deadline =
                    Instant::now() + deadline.saturating_duration_since(Instant::now());
                let mut ack_rx = ack_rx;
                let mut acked = None;
                while Instant::now() < ack_deadline {
                    // A dead owner can never acknowledge; stop waiting at once
                    // instead of burning the whole close budget.
                    if !self.runtime_thread_alive() {
                        break;
                    }
                    match tokio::time::timeout(Duration::from_millis(10), &mut ack_rx).await {
                        Ok(Ok(ev)) => {
                            acked = Some(ev);
                            break;
                        }
                        Ok(Err(_)) => {
                            warn!("LocalSet shutdown ack channel closed before evidence");
                            break;
                        }
                        Err(_) => {}
                    }
                }
                acked.unwrap_or_else(|| self.evidence_from_state())
            } else {
                self.evidence_from_state()
            }
        } else {
            // No runtime thread was ever spawned (or one already failed to
            // open): trivially settled.
            self.evidence_from_state()
        };

        if join_thread {
            evidence = self.join_runtime_thread(evidence, deadline).await;
        }
        self.publish_settled(&evidence);
        evidence
    }

    fn evidence_from_state(&self) -> ShutdownEvidence {
        let stats = self.counters.stats();
        let thread_alive = self.runtime_thread_alive();
        ShutdownEvidence {
            // A missing `JoinHandle` is NOT evidence that the thread never
            // existed: it may have been taken by an earlier bounded join and
            // retained. Only an unspawned thread counts as trivially joined.
            joined_cleanly: !self.counters.thread_started()
                || (self.counters.thread_joined() && !thread_alive),
            thread_alive,
            pending_requests: stats.pending_requests,
            pending_bytes: stats.pending_bytes,
            active_tasks: stats.active_tasks,
            owned_tasks: stats.owned_tasks,
            control_tasks: stats.control_tasks,
            // No shutdown request was processed on the owning thread, so the
            // live counters *are* the observable snapshot.
            queued_at_shutdown: stats.pending_requests,
            queued_bytes_at_shutdown: stats.pending_bytes,
            active_at_shutdown: stats.active_tasks,
            owned_at_shutdown: stats.owned_tasks,
            control_at_shutdown: stats.control_tasks,
            aborted_task_ids: vec![],
        }
    }

    fn runtime_thread_alive(&self) -> bool {
        // A retained handle means the thread outlived a bounded join: it is
        // still (or may still be) running even though the runtime slot is empty.
        if let Ok(slot) = self.retained_thread.lock() {
            if let Some(handle) = slot.as_ref() {
                return !handle.is_finished();
            }
        }
        let guard = self.runtime.lock().ok();
        guard.is_some_and(|g| g.as_ref().is_some_and(|rt| !rt.thread_handle.is_finished()))
    }

    async fn join_runtime_thread(
        &self,
        mut evidence: ShutdownEvidence,
        deadline: Instant,
    ) -> ShutdownEvidence {
        let handle = {
            let mut guard = self.runtime.lock().expect("LocalSet bridge mutex poisoned");
            guard.take().map(|rt| rt.thread_handle)
        };
        if let Some(handle) = handle {
            let join_budget = deadline.saturating_duration_since(Instant::now());
            if join_budget.is_zero() {
                warn!("LocalSet join skipped: no remaining shutdown budget");
                self.retain_runtime_thread(handle);
                evidence.thread_alive = true;
            } else {
                let join_deadline = Instant::now() + join_budget;
                while !handle.is_finished() && Instant::now() < join_deadline {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                if handle.is_finished() {
                    match handle.join() {
                        Ok(()) => {
                            self.counters.mark_thread_joined();
                            evidence.joined_cleanly = true;
                            evidence.thread_alive = false;
                            debug!("LocalSet bridge thread joined");
                        }
                        Err(panic) => {
                            error!("LocalSet bridge thread panicked: {:?}", panic);
                            evidence.joined_cleanly = false;
                            evidence.thread_alive = false;
                        }
                    }
                } else {
                    warn!(
                        "LocalSet join timed out after {:?}; retaining thread handle",
                        join_budget
                    );
                    self.retain_runtime_thread(handle);
                    evidence.thread_alive = true;
                }
            }
        } else {
            evidence.thread_alive = false;
            evidence.joined_cleanly = evidence.joined_cleanly || self.counters.thread_joined();
        }
        let stats = self.counters.stats();
        evidence.pending_requests = stats.pending_requests;
        evidence.pending_bytes = stats.pending_bytes;
        evidence.active_tasks = stats.active_tasks;
        evidence.owned_tasks = stats.owned_tasks;
        evidence.control_tasks = stats.control_tasks;
        evidence
    }

    /// Synchronous shutdown for `Drop` / finalizer paths (bounded join).
    #[must_use]
    pub fn shutdown_sync(&self) -> ShutdownEvidence {
        self.shutdown_sync_with_deadline(Instant::now() + SHUTDOWN_JOIN_BUDGET)
    }

    /// Synchronous shutdown bounded by an absolute deadline.
    pub fn shutdown_sync_with_deadline(&self, deadline: Instant) -> ShutdownEvidence {
        if self
            .shutdown_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            // One settlement: observe the owner's result, never a fabricated
            // one. Symmetric with the async path.
            if let Some(evidence) = self.await_settlement_sync(deadline) {
                return evidence;
            }
            warn!("LocalSet sync shutdown settlement not published before deadline");
            // Last resort only: honest observed state, and never join a thread
            // the owning caller still owns.
            return self.evidence_from_state();
        }
        self.counters.shutting_down.store(true, Ordering::SeqCst);

        // Take the control sender in its own scope: the runtime mutex must be
        // released before `evidence_from_state` touches it again.
        let control = {
            let guard = self.runtime.lock().ok();
            guard.and_then(|g| g.as_ref().map(|rt| rt.control.clone()))
        };

        let mut evidence = if let Some(control) = control {
            let (ack_tx, mut ack_rx) = oneshot::channel();
            let queued = control.push(ControlMessage::Shutdown { ack: ack_tx })
                && self.runtime_thread_alive();
            if !queued {
                return self.evidence_from_state();
            }
            let ack_deadline = Instant::now() + deadline.saturating_duration_since(Instant::now());
            let mut acked = None;
            while Instant::now() < ack_deadline {
                if !self.runtime_thread_alive() {
                    break;
                }
                match ack_rx.try_recv() {
                    Ok(ev) => {
                        acked = Some(ev);
                        break;
                    }
                    Err(oneshot::error::TryRecvError::Empty) => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(oneshot::error::TryRecvError::Closed) => break,
                }
            }
            acked.unwrap_or_else(|| self.evidence_from_state())
        } else {
            self.evidence_from_state()
        };

        evidence = self.join_runtime_thread_sync(evidence, deadline);
        let stats = self.counters.stats();
        evidence.pending_requests = stats.pending_requests;
        evidence.pending_bytes = stats.pending_bytes;
        evidence.active_tasks = stats.active_tasks;
        evidence.owned_tasks = stats.owned_tasks;
        evidence.control_tasks = stats.control_tasks;
        self.publish_settled(&evidence);
        evidence
    }

    fn join_runtime_thread_sync(
        &self,
        mut evidence: ShutdownEvidence,
        deadline: Instant,
    ) -> ShutdownEvidence {
        let handle = {
            let mut guard = self.runtime.lock().expect("LocalSet bridge mutex poisoned");
            guard.take().map(|rt| rt.thread_handle)
        };
        if let Some(handle) = handle {
            let join_budget = deadline.saturating_duration_since(Instant::now());
            if join_budget.is_zero() {
                warn!("LocalSet sync join skipped: no remaining shutdown budget");
                self.retain_runtime_thread(handle);
                evidence.thread_alive = true;
            } else {
                let join_deadline = Instant::now() + join_budget;
                while !handle.is_finished() && Instant::now() < join_deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                if handle.is_finished() {
                    match handle.join() {
                        Ok(()) => {
                            self.counters.mark_thread_joined();
                            evidence.joined_cleanly = true;
                            evidence.thread_alive = false;
                        }
                        Err(panic) => {
                            error!("LocalSet bridge thread panicked: {:?}", panic);
                            evidence.joined_cleanly = false;
                            evidence.thread_alive = false;
                        }
                    }
                } else {
                    warn!(
                        "LocalSet sync join timed out after {:?}; retaining thread handle",
                        join_budget
                    );
                    self.retain_runtime_thread(handle);
                    evidence.thread_alive = true;
                }
            }
        } else {
            evidence.thread_alive = false;
            evidence.joined_cleanly = evidence.joined_cleanly || self.counters.thread_joined();
        }
        evidence
    }
}

/// Abort handle for a tracked session-ownership task.
#[derive(Debug)]
pub struct OwnedTaskHandle {
    control: Arc<ControlRelay>,
    task_id: u64,
}

impl OwnedTaskHandle {
    /// Task id used in shutdown evidence.
    #[must_use]
    pub const fn task_id(&self) -> u64 {
        self.task_id
    }

    /// Abort exactly this owned task on the `LocalSet` thread. Idempotent.
    pub fn abort(&self) {
        let _ = self.control.push(ControlMessage::Cancel {
            task_id: self.task_id,
        });
    }
}

/// Cancels the owned `LocalSet` task when the awaiting future is dropped or times out.
struct CancelGuard {
    control: Arc<ControlRelay>,
    task_id: u64,
    cancelled: Arc<AtomicBool>,
}

impl CancelGuard {
    fn disarm(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        // The Drop path only enqueues: it never awaits and never spawns a thread.
        let _ = self.control.push(ControlMessage::Cancel {
            task_id: self.task_id,
        });
    }
}

/// Which admission budget a tracked task belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskClass {
    /// A bounded work request: charged against the pending/active request budget.
    Work,
    /// A long-lived session-ownership task: tracked, not charged.
    Owned,
    /// Cancellation/teardown work: tracked, not charged, never queued.
    ///
    /// Cooperative cancel must reach the peer while the bounded work queue is
    /// full, exactly like the shutdown control channel: charging it would let a
    /// saturated queue block the very teardown meant to drain it.
    Control,
}

struct TaskSlot {
    handle: tokio::task::AbortHandle,
    /// Set when the spawned future is first polled; until then the tracking entry owns the reservation.
    polled: Arc<AtomicBool>,
    class: TaskClass,
}

struct BridgeThreadState {
    tasks: HashMap<u64, TaskSlot>,
    deferred: VecDeque<ExecuteRequest>,
    cancelled_before_start: HashSet<u64>,
}

type ThreadStateRef = Arc<Mutex<BridgeThreadState>>;

struct ActiveGuard {
    counters: Arc<AdmissionCounters>,
    state: ThreadStateRef,
    task_id: u64,
    class: TaskClass,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        match self.class {
            TaskClass::Work => {
                self.counters.task_finished();
                if let Ok(mut bridge_state) = self.state.lock() {
                    bridge_state.tasks.remove(&self.task_id);
                    bridge_state.drain_deferred(&self.counters, &self.state);
                }
            }
            TaskClass::Owned => {
                self.counters.owned_task_finished();
                if let Ok(mut bridge_state) = self.state.lock() {
                    bridge_state.tasks.remove(&self.task_id);
                }
            }
            TaskClass::Control => {
                self.counters.control_task_finished();
                if let Ok(mut bridge_state) = self.state.lock() {
                    bridge_state.tasks.remove(&self.task_id);
                    bridge_state.drain_deferred(&self.counters, &self.state);
                }
            }
        }
    }
}

impl BridgeThreadState {
    fn process_cancel(
        &mut self,
        task_id: u64,
        counters: &AdmissionCounters,
        aborted_ids: &mut Vec<u64>,
    ) {
        if let Some(slot) = self.tasks.remove(&task_id) {
            if !slot.polled.load(Ordering::Acquire) {
                match slot.class {
                    TaskClass::Work => counters.task_finished(),
                    TaskClass::Owned => counters.owned_task_finished(),
                    TaskClass::Control => counters.control_task_finished(),
                }
            }
            slot.handle.abort();
            aborted_ids.push(task_id);
            return;
        }
        if let Some(pos) = self.deferred.iter().position(|req| req.task_id == task_id) {
            let req = self.deferred.remove(pos).expect("deferred position");
            counters.release_pending(req.byte_charge);
            aborted_ids.push(task_id);
            return;
        }
        self.cancelled_before_start.insert(task_id);
    }

    /// Track and start a session-ownership future. Never queued.
    fn spawn_owned_task(
        &mut self,
        req: OwnedRequest,
        counters: &Arc<AdmissionCounters>,
        state_ref: &ThreadStateRef,
    ) {
        let task_id = req.task_id;
        if self.cancelled_before_start.remove(&task_id) {
            return;
        }
        let factory = req.factory;
        let task_counters = counters.clone();
        let shared_state = state_ref.clone();
        let polled = Arc::new(AtomicBool::new(false));
        let polled_watch = polled.clone();
        task_counters.track_owned();
        let handle = tokio::task::spawn_local(async move {
            polled_watch.store(true, Ordering::SeqCst);
            let _active = ActiveGuard {
                counters: task_counters.clone(),
                state: shared_state.clone(),
                task_id,
                class: TaskClass::Owned,
            };
            let future = factory();
            future.await;
        });
        self.tasks.insert(
            task_id,
            TaskSlot {
                handle: handle.abort_handle(),
                polled,
                class: TaskClass::Owned,
            },
        );
        counters.task_slot_notify.notify_waiters();
    }

    fn enqueue_execute(
        &mut self,
        req: ExecuteRequest,
        counters: &Arc<AdmissionCounters>,
        state_ref: &ThreadStateRef,
    ) {
        let task_id = req.task_id;
        if req.class == TaskClass::Control {
            if self.cancelled_before_start.remove(&task_id) {
                return;
            }
            self.begin_task(req, counters, state_ref);
            return;
        }
        if self.cancelled_before_start.remove(&task_id) {
            counters.release_pending(req.byte_charge);
            return;
        }
        if !counters.try_reserve_active() {
            self.deferred.push_back(req);
            return;
        }
        self.begin_task(req, counters, state_ref);
    }

    fn begin_task(
        &mut self,
        req: ExecuteRequest,
        counters: &Arc<AdmissionCounters>,
        state_ref: &ThreadStateRef,
    ) {
        let task_id = req.task_id;
        let class = req.class;
        match class {
            TaskClass::Work => counters.release_pending(req.byte_charge),
            TaskClass::Control => counters.track_control(),
            TaskClass::Owned => {}
        }
        let factory = req.factory;
        let result_tx = req.result_tx;
        let task_counters = counters.clone();
        let shared_state = state_ref.clone();
        let polled = Arc::new(AtomicBool::new(false));
        let polled_watch = polled.clone();
        let handle = tokio::task::spawn_local(async move {
            polled_watch.store(true, Ordering::SeqCst);
            let _active = ActiveGuard {
                counters: task_counters.clone(),
                state: shared_state.clone(),
                task_id,
                class,
            };
            let future = factory();
            let output = future.await;
            if let Some(tx) = result_tx {
                let _ = tx.send(output);
            }
        });
        self.tasks.insert(
            task_id,
            TaskSlot {
                handle: handle.abort_handle(),
                polled,
                class,
            },
        );
        counters.task_slot_notify.notify_waiters();
    }

    fn drain_deferred(&mut self, counters: &Arc<AdmissionCounters>, state_ref: &ThreadStateRef) {
        if counters.is_draining() {
            return;
        }
        while let Some(req) = self.deferred.pop_front() {
            if !counters.try_reserve_active() {
                self.deferred.push_front(req);
                break;
            }
            let task_id = req.task_id;
            if self.cancelled_before_start.remove(&task_id) {
                counters.release_pending(req.byte_charge);
                counters.task_finished();
                continue;
            }
            self.begin_task(req, counters, state_ref);
        }
    }
}

/// Apply every queued control message on the owning thread.
///
/// Returns the shutdown ack sender and its evidence when a shutdown request
/// was drained; the caller answers the ack and exits its loop.
fn apply_control_messages(
    control: &ControlRelay,
    state_ref: &ThreadStateRef,
    counters: &Arc<AdmissionCounters>,
    aborted_ids: &mut Vec<u64>,
) -> Option<(oneshot::Sender<ShutdownEvidence>, ShutdownEvidence)> {
    let mut shutdown_ack = None;
    for msg in control.drain() {
        match msg {
            ControlMessage::Cancel { task_id } => {
                if let Ok(mut state) = state_ref.lock() {
                    state.process_cancel(task_id, counters, aborted_ids);
                }
            }
            ControlMessage::SpawnOwned(req) => {
                if let Ok(mut state) = state_ref.lock() {
                    state.spawn_owned_task(req, counters, state_ref);
                }
            }
            ControlMessage::RunControl(req) => {
                if let Ok(mut state) = state_ref.lock() {
                    state.enqueue_execute(req, counters, state_ref);
                }
            }
            ControlMessage::Shutdown { ack } => {
                let evidence = abort_tracked_work(state_ref, counters, aborted_ids);
                shutdown_ack = Some((ack, evidence));
            }
        }
    }
    shutdown_ack
}

/// Abort every tracked task, fail every queued request, and snapshot the
/// counters for the shutdown ack. The `*_at_shutdown` fields hold the snapshot
/// taken before the abort, so the evidence reports what shutdown drained.
fn abort_tracked_work(
    state_ref: &ThreadStateRef,
    counters: &Arc<AdmissionCounters>,
    aborted_ids: &mut Vec<u64>,
) -> ShutdownEvidence {
    let snapshot = counters.stats();
    if let Ok(mut state) = state_ref.lock() {
        for (task_id, slot) in state.tasks.drain() {
            if !slot.polled.load(Ordering::Acquire) {
                match slot.class {
                    TaskClass::Work => counters.task_finished(),
                    TaskClass::Owned => counters.owned_task_finished(),
                    TaskClass::Control => counters.control_task_finished(),
                }
            }
            aborted_ids.push(task_id);
            slot.handle.abort();
        }
        for req in state.deferred.drain(..) {
            counters.release_pending(req.byte_charge);
            if let Some(tx) = req.result_tx {
                let _ = tx.send(Err(
                    "LocalSet bridge shutdown before this request started".to_string()
                ));
            }
        }
    }
    counters.shutting_down.store(true, Ordering::SeqCst);
    let stats = counters.stats();
    ShutdownEvidence {
        joined_cleanly: false,
        thread_alive: true,
        pending_requests: stats.pending_requests,
        pending_bytes: stats.pending_bytes,
        active_tasks: stats.active_tasks,
        owned_tasks: stats.owned_tasks,
        control_tasks: stats.control_tasks,
        queued_at_shutdown: snapshot.pending_requests,
        queued_bytes_at_shutdown: snapshot.pending_bytes,
        active_at_shutdown: snapshot.active_tasks,
        owned_at_shutdown: snapshot.owned_tasks,
        control_at_shutdown: snapshot.control_tasks,
        aborted_task_ids: std::mem::take(aborted_ids),
    }
}

fn run_localset_thread(
    mut work_rx: mpsc::Receiver<WorkMessage>,
    control: Arc<ControlRelay>,
    counters: Arc<AdmissionCounters>,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("LocalSet bridge tokio runtime");
    runtime.block_on(async move {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let state_ref: ThreadStateRef = Arc::new(Mutex::new(BridgeThreadState {
                    tasks: HashMap::new(),
                    deferred: VecDeque::new(),
                    cancelled_before_start: HashSet::new(),
                }));
                let mut aborted_ids: Vec<u64> = Vec::new();

                loop {
                    // Register interest before draining so a push racing this check
                    // still wakes the select below (no lost wake).
                    let notified = control.notify.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();

                    // Control (cancellation/teardown) is drained ahead of data work:
                    // a close never waits behind queued requests.
                    if let Some((ack, evidence)) =
                        apply_control_messages(&control, &state_ref, &counters, &mut aborted_ids)
                    {
                        let _ = ack.send(evidence);
                        break;
                    }

                    tokio::select! {
                        biased;
                        () = &mut notified => {}
                        msg = work_rx.recv() => {
                            match msg {
                                Some(WorkMessage::Execute(req)) => {
                                    if let Ok(mut state) = state_ref.lock() {
                                        state.enqueue_execute(req, &counters, &state_ref);
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
            })
            .await;
    });
}

impl Default for LocalSetBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LocalSetBridge {
    fn drop(&mut self) {
        if Arc::strong_count(&self.runtime) != 1 {
            // Another clone still owns the thread; that clone will settle it.
            return;
        }
        if tokio::runtime::Handle::try_current().is_ok() {
            // Inside a Tokio context a synchronous join would block the
            // executor. Ask the owning thread to stop without blocking and hand
            // its handle to the tracked fallback owner — never a silent detach.
            self.counters.shutting_down.store(true, Ordering::SeqCst);
            self.shutdown_claimed.store(true, Ordering::Release);
            if let Some(rt) = self.runtime.lock().ok().and_then(|mut guard| guard.take()) {
                let (ack, _ack_rx) = oneshot::channel();
                let _ = rt.control.push(ControlMessage::Shutdown { ack });
                track_abandoned_thread(rt.thread_handle);
            }
            return;
        }
        // Drop is a finalizer: the settled evidence has no consumer here. The
        // shutdown itself still runs with its bounded join.
        let _ = self.shutdown_sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Install a runtime whose owning thread panics, so `JoinHandle::join()`
    /// returns `Err` — the join-failure shape this contract is about. (A panic
    /// inside a `spawn_local` task is caught by the task harness and never
    /// reaches the thread, so the failure has to be injected here.)
    fn install_panicked_thread(bridge: &LocalSetBridge) {
        let (work_tx, _work_rx) = mpsc::channel(MAX_PENDING_REQUESTS);
        let handle = thread::spawn(|| panic!("injected LocalSet thread panic"));
        bridge.counters.mark_thread_started();
        *bridge.runtime.lock().unwrap() = Some(BridgeRuntime {
            work_tx,
            control: Arc::new(ControlRelay::new()),
            thread_handle: handle,
        });
    }

    #[tokio::test]
    async fn panicked_runtime_thread_is_never_settled() {
        let bridge = LocalSetBridge::new();
        install_panicked_thread(&bridge);

        let evidence = bridge.shutdown().await;
        assert!(
            !evidence.joined_cleanly,
            "a join failure must not report a clean join: {evidence:?}"
        );
        assert!(
            !evidence.thread_alive,
            "the panicked thread is gone: {evidence:?}"
        );
        assert!(
            !evidence.is_settled(),
            "a panicked thread is not settlement, even though thread_alive is false: {evidence:?}"
        );
        assert!(
            evidence
                .unsettled_pending()
                .iter()
                .any(|entry| entry == "localset-thread-join-failed"),
            "the failure must be actionable: {:?}",
            evidence.unsettled_pending()
        );

        // A later read must not claim settlement just because the runtime slot
        // is now empty — taking the handle is not joining it.
        let again = bridge.shutdown().await;
        assert!(
            !again.is_settled(),
            "a second read must not turn a broken join into success: {again:?}"
        );
        assert!(
            again
                .unsettled_pending()
                .iter()
                .any(|entry| entry == "localset-thread-join-failed"),
            "{:?}",
            again.unsettled_pending()
        );
    }

    /// Two callers starting together on an unstarted bridge must BOTH return the
    /// one published settlement promptly. The `runtime == None` winner used to
    /// return before publishing, so the loser waited out its whole deadline and
    /// then reported a snapshot of its own.
    #[tokio::test]
    async fn concurrent_shutdown_on_unstarted_bridge_shares_one_settlement() {
        let bridge = LocalSetBridge::new();
        let started = tokio::time::Instant::now();
        let (a, b) = tokio::join!(bridge.shutdown(), {
            let bridge = bridge.clone();
            async move { bridge.shutdown().await }
        });
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_secs(1),
            "both callers must return promptly, not wait out the budget: {elapsed:?}"
        );
        assert_eq!(a, b, "concurrent callers must observe identical evidence");
        assert!(
            a.is_settled(),
            "an unstarted bridge settles trivially: {a:?}"
        );
        assert_eq!(
            bridge.settled_evidence().as_ref(),
            Some(&a),
            "the returned evidence must be the published settlement"
        );
        // Exactly one settlement: a later publisher is refused, so the first
        // published result can never be overwritten.
        let mut usurper = a.clone();
        usurper.pending_requests = 99;
        assert!(
            !bridge.publish_settled(&usurper),
            "a bridge must not settle twice"
        );
        assert_eq!(
            bridge.settled_evidence().as_ref(),
            Some(&a),
            "the first settlement stands"
        );
    }

    /// The sync path already had a single publish site; assert the symmetry so a
    /// future edit cannot reintroduce the async-only bug.
    #[test]
    fn concurrent_sync_shutdown_on_unstarted_bridge_shares_one_settlement() {
        let bridge = LocalSetBridge::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let started = Instant::now();
        let first = bridge.shutdown_sync_with_deadline(deadline);
        // A second (loser) caller observes the published settlement.
        let second = bridge.shutdown_sync_with_deadline(deadline);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "sync shutdown must not wait out the budget"
        );
        assert_eq!(
            first, second,
            "sync callers must observe identical evidence"
        );
        assert!(first.is_settled(), "{first:?}");
        assert_eq!(bridge.settled_evidence().as_ref(), Some(&first));
    }

    /// Q3-C2: the control path is a bounded relay, not a thread factory.
    /// Duplicate cancels for one task coalesce (they are idempotent), distinct
    /// cancels are all retained, and non-cancel control is capped rather than
    /// growing without bound.
    #[test]
    fn control_relay_is_bounded_and_coalesces_cancels() {
        let relay = ControlRelay::new();

        // 10_000 cancels for the same in-flight task: the Drop path may run this
        // often when many waiters are dropped, and nothing may accumulate.
        for _ in 0..10_000 {
            assert!(relay.push(ControlMessage::Cancel { task_id: 7 }));
        }
        assert_eq!(
            relay.drain().len(),
            1,
            "duplicate cancels for one task must coalesce to a single message"
        );

        // Distinct tasks are all preserved.
        for task_id in 0..MAX_ACTIVE_TASKS as u64 {
            assert!(relay.push(ControlMessage::Cancel { task_id }));
        }
        assert_eq!(
            relay.drain().len(),
            MAX_ACTIVE_TASKS,
            "every distinct cancellation must be retained"
        );

        // A closed relay refuses work instead of queueing it forever.
        relay.close();
        assert!(
            !relay.push(ControlMessage::Cancel { task_id: 99 }),
            "a closed relay must refuse, not queue"
        );
    }

    /// Q3-C2: the `CancelGuard` Drop path enqueues into the bounded relay.
    ///
    /// Exercised against a STANDALONE relay (no owning thread to drain it), so the
    /// assertions are deterministic. The Drop path is synchronous by
    /// construction — it awaits nothing and calls no thread API — which is the
    /// property that replaces the old per-cancel `std::thread` spawn.
    #[tokio::test]
    async fn dropped_waiter_cancels_through_the_relay() {
        let control = Arc::new(ControlRelay::new());

        // A drop storm: 64 rounds over a bounded set of live tasks.
        for _ in 0..64 {
            for task_id in 0..(MAX_ACTIVE_TASKS as u64) {
                drop(CancelGuard {
                    control: control.clone(),
                    task_id,
                    cancelled: Arc::new(AtomicBool::new(false)),
                });
            }
        }
        assert_eq!(
            control.drain().len(),
            MAX_ACTIVE_TASKS,
            "a drop storm must coalesce to one cancel per live task, never grow"
        );
        assert_eq!(
            control.cancels_accepted(),
            MAX_ACTIVE_TASKS as u64,
            "each distinct task must be cancelled exactly once across the storm"
        );

        // The bridge owning such a relay still settles.
        let bridge = LocalSetBridge::new();
        bridge
            .execute(4, || Box::pin(async { 1 }))
            .await
            .expect("execute");
        let evidence = bridge.shutdown().await;
        assert!(evidence.is_settled(), "{evidence:?}");
    }

    /// Q3-S9: dropping a bridge inside a Tokio context must not leave an
    /// untracked detached thread. The handle goes to the bounded fallback owner,
    /// which joins it — no `Drop` panic and no orphan.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drop_inside_tokio_tracks_the_thread_instead_of_detaching_it() {
        // The registry is process-global and other tests run in parallel, so the
        // assertion uses the monotonic "ever tracked" count rather than the live
        // list size (which the reaper may drain at any moment).
        let tracked_before = abandoned_threads_tracked_total();
        {
            let bridge = LocalSetBridge::new();
            // Force the runtime thread to exist.
            bridge
                .execute(4, || Box::pin(async { 1 }))
                .await
                .expect("execute");
            // Dropped here, inside the Tokio context.
        }

        assert!(
            abandoned_threads_tracked_total() > tracked_before,
            "a forgotten shutdown inside a runtime must be tracked, not detached"
        );

        // The fallback owner joins it: the live list eventually drains.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while abandoned_thread_count() > 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(
            abandoned_thread_count(),
            0,
            "every tracked thread must be joined and removed by the fallback reaper"
        );
    }

    #[tokio::test]
    async fn clean_join_settles() {
        let bridge = LocalSetBridge::new();
        let value: i32 = bridge
            .execute(4, || Box::pin(async { 7 }))
            .await
            .expect("execute");
        assert_eq!(value, 7);
        let evidence = bridge.shutdown().await;
        assert!(evidence.is_settled(), "{evidence:?}");
        assert!(evidence.unsettled_pending().is_empty());
    }

    #[tokio::test]
    async fn unstarted_bridge_is_settled() {
        let bridge = LocalSetBridge::new();
        let evidence = bridge.shutdown().await;
        assert!(
            evidence.is_settled(),
            "a bridge that never spawned a thread has nothing to settle: {evidence:?}"
        );
    }
}
