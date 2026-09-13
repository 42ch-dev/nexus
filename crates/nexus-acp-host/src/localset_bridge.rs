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
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot, Notify};
use tracing::{debug, error, warn};

/// Maximum queued execute requests (count).
pub const MAX_PENDING_REQUESTS: usize = 16;
/// Maximum queued execute request bytes (sum of charged payloads).
pub const MAX_PENDING_BYTES: usize = 1024 * 1024;
/// Maximum concurrently running LocalSet tasks.
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
/// Owner tasks are created on the LocalSet thread and are tracked by task id and
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
#[derive(Debug, Clone, Default)]
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

    fn mark_thread_joined(&self) {
        self.thread_joined.store(true, Ordering::SeqCst);
    }

    fn thread_joined(&self) -> bool {
        self.thread_joined.load(Ordering::SeqCst)
    }
}

struct BridgeRuntime {
    work_tx: mpsc::Sender<WorkMessage>,
    control_tx: mpsc::Sender<ControlMessage>,
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
}

impl LocalSetBridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(None)),
            counters: Arc::new(AdmissionCounters::new()),
            shutdown_claimed: Arc::new(AtomicBool::new(false)),
            retained_thread: Arc::new(Mutex::new(None)),
        }
    }

    /// Whether a runtime thread `JoinHandle` is retained after a join timeout.
    #[must_use]
    pub fn has_retained_runtime_thread(&self) -> bool {
        self.retained_thread
            .lock()
            .ok()
            .is_some_and(|slot| slot.is_some())
    }

    /// Take a retained runtime thread handle for tracked cleanup registration.
    pub fn take_retained_runtime_thread(&self) -> Option<JoinHandle<()>> {
        self.retained_thread.lock().ok()?.take()
    }

    fn retain_runtime_thread(&self, handle: JoinHandle<()>) {
        if let Ok(mut slot) = self.retained_thread.lock() {
            if let Some(existing) = slot.take() {
                if existing.is_finished() {
                    let _ = existing.join();
                } else {
                    *slot = Some(existing);
                    warn!("replacing retained LocalSet thread handle while prior thread still alive");
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
    ) -> Result<(mpsc::Sender<WorkMessage>, mpsc::Sender<ControlMessage>), crate::AcpError> {
        let mut guard = self
            .runtime
            .lock()
            .map_err(|_| crate::AcpError::connection_failed("LocalSet bridge mutex poisoned"))?;
        if let Some(rt) = guard.as_ref() {
            return Ok((rt.work_tx.clone(), rt.control_tx.clone()));
        }
        if self.counters.shutting_down.load(Ordering::Acquire) {
            return Err(crate::AcpError::connection_failed(
                "LocalSet bridge shutting down",
            ));
        }

        let counters = self.counters.clone();
        let (work_tx, work_rx) = mpsc::channel::<WorkMessage>(MAX_PENDING_REQUESTS);
        let (control_tx, control_rx) = mpsc::channel::<ControlMessage>(64);

        let thread_handle = thread::Builder::new()
            .name("nexus-localset-bridge".to_string())
            .spawn(move || run_localset_thread(work_rx, control_rx, counters))
            .map_err(|e| {
                crate::AcpError::connection_failed(format!("spawn LocalSet thread: {e}"))
            })?;

        let runtime = BridgeRuntime {
            work_tx: work_tx.clone(),
            control_tx: control_tx.clone(),
            thread_handle,
        };
        *guard = Some(runtime);
        Ok((work_tx, control_tx))
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
        let (work_tx, control_tx) = self.ensure_runtime()?;

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
            control_tx,
            task_id,
            cancelled: Arc::new(AtomicBool::new(false)),
                };
                let recv = result_rx.await;
                cancel_on_drop.disarm();
                return Self::task_result(recv);
            }
        };
        send_control(control_tx, message);

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

    /// Execute with timeout; dropped/expired waiter cancels the owned LocalSet task.
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

    /// Wait until a `TaskSlot` is installed on the LocalSet thread (before first poll).
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
    pub fn spawn_owned<F>(&self, f: F) -> crate::AcpResult<OwnedTaskHandle>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + 'static,
    {
        let (_, control_tx) = self.ensure_runtime()?;
        let task_id = self.counters.next_task_id.fetch_add(1, Ordering::AcqRel);
        let msg = ControlMessage::SpawnOwned(OwnedRequest {
            task_id,
            factory: Box::new(f),
        });
        send_control(control_tx.clone(), msg);
        Ok(OwnedTaskHandle {
            control_tx,
            task_id,
        })
    }

    /// Cancel an owned task by id via the control channel.
    pub async fn cancel_task(&self, task_id: u64) -> Result<(), crate::AcpError> {
        let (_, control_tx) = self.ensure_runtime()?;
        control_tx
            .send(ControlMessage::Cancel { task_id })
            .await
            .map_err(|_| crate::AcpError::connection_failed("LocalSet control channel closed"))?;
        Ok(())
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
            let mut evidence = self.evidence_from_state();
            if join_thread {
                evidence = self.join_runtime_thread(evidence, deadline).await;
            }
            return evidence;
        }
        self.counters.shutting_down.store(true, Ordering::SeqCst);

        if self
            .runtime
            .lock()
            .ok()
            .is_some_and(|guard| guard.is_none())
        {
            return ShutdownEvidence {
                joined_cleanly: true,
                thread_alive: false,
                pending_requests: 0,
                pending_bytes: 0,
                active_tasks: 0,
                owned_tasks: 0,
                control_tasks: 0,
                queued_at_shutdown: 0,
                queued_bytes_at_shutdown: 0,
                active_at_shutdown: 0,
                owned_at_shutdown: 0,
                control_at_shutdown: 0,
                aborted_task_ids: vec![],
            };
        }

        let control_tx = {
            let guard = self.runtime.lock().ok();
            guard.and_then(|g| g.as_ref().map(|rt| rt.control_tx.clone()))
        };

        let mut evidence = if let Some(control_tx) = control_tx {
            let (ack_tx, ack_rx) = oneshot::channel();
            let sent = control_tx
                .send(ControlMessage::Shutdown { ack: ack_tx })
                .await
                .is_ok();
            if sent {
                let ack_budget = deadline.saturating_duration_since(Instant::now());
                match tokio::time::timeout(ack_budget, ack_rx).await {
                    Ok(Ok(ev)) => ev,
                    Ok(Err(_)) => {
                        warn!("LocalSet shutdown ack channel closed before evidence");
                        self.evidence_from_state()
                    }
                    Err(_) => {
                        warn!(
                            "LocalSet shutdown ack timed out after {:?}",
                            ack_budget
                        );
                        self.evidence_from_state()
                    }
                }
            } else {
                self.evidence_from_state()
            }
        } else {
            self.evidence_from_state()
        };

        if join_thread {
            evidence = self.join_runtime_thread(evidence, deadline).await;
        }
        evidence
    }

    fn evidence_from_state(&self) -> ShutdownEvidence {
        let stats = self.counters.stats();
        let thread_alive = self.runtime_thread_alive();
        let never_started = self
            .runtime
            .lock()
            .ok()
            .is_some_and(|guard| guard.is_none());
        ShutdownEvidence {
            joined_cleanly: never_started || (self.counters.thread_joined() && !thread_alive),
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
        let guard = self.runtime.lock().ok();
        match guard {
            Some(g) => g
                .as_ref()
                .map(|rt| !rt.thread_handle.is_finished())
                .unwrap_or(false),
            None => false,
        }
    }

    async fn join_runtime_thread(
        &self,
        mut evidence: ShutdownEvidence,
        deadline: Instant,
    ) -> ShutdownEvidence {
        let handle = {
            let mut guard = self
                .runtime
                .lock()
                .expect("LocalSet bridge mutex poisoned");
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
            evidence.joined_cleanly =
                evidence.joined_cleanly || self.counters.thread_joined();
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
            let mut evidence = self.evidence_from_state();
            let join_budget = deadline.saturating_duration_since(Instant::now());
            if !join_budget.is_zero() {
                evidence = self.join_runtime_thread_sync(evidence, deadline);
            }
            return evidence;
        }
        self.counters.shutting_down.store(true, Ordering::SeqCst);

        // Take the control sender in its own scope: the runtime mutex must be
        // released before `evidence_from_state` touches it again.
        let control_tx = {
            let guard = self.runtime.lock().ok();
            guard.and_then(|g| g.as_ref().map(|rt| rt.control_tx.clone()))
        };

        let mut evidence = if let Some(control_tx) = control_tx {
            let (ack_tx, mut ack_rx) = oneshot::channel();
            send_control_reliable(control_tx, ControlMessage::Shutdown { ack: ack_tx });
            let ack_deadline =
                Instant::now() + deadline.saturating_duration_since(Instant::now());
            let mut acked = None;
            while Instant::now() < ack_deadline {
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
        evidence
    }

    fn join_runtime_thread_sync(
        &self,
        mut evidence: ShutdownEvidence,
        deadline: Instant,
    ) -> ShutdownEvidence {
        let handle = {
            let mut guard = self
                .runtime
                .lock()
                .expect("LocalSet bridge mutex poisoned");
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
            evidence.joined_cleanly =
                evidence.joined_cleanly || self.counters.thread_joined();
        }
        evidence
    }
}

fn send_control_reliable(control_tx: mpsc::Sender<ControlMessage>, msg: ControlMessage) {
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::Builder::new()
            .name("nexus-localset-control".into())
            .spawn(move || {
                let _ = control_tx.blocking_send(msg);
            })
            .expect("spawn control relay thread");
    } else {
        let _ = control_tx.blocking_send(msg);
    }
}

/// Enqueue a control message without blocking the caller's thread.
///
/// The control channel has its own capacity and is drained ahead of the bounded
/// work queue, so `try_send` succeeds on the ownership/cancel paths in practice;
/// a full channel falls back to the reliable relay so a task is never left
/// without its cancellation.
fn send_control(control_tx: mpsc::Sender<ControlMessage>, msg: ControlMessage) {
    match control_tx.try_send(msg) {
        Ok(()) | Err(mpsc::error::TrySendError::Closed(_)) => {}
        Err(mpsc::error::TrySendError::Full(msg)) => send_control_reliable(control_tx, msg),
    }
}

/// Abort handle for a tracked session-ownership task.
#[derive(Debug)]
pub struct OwnedTaskHandle {
    control_tx: mpsc::Sender<ControlMessage>,
    task_id: u64,
}

impl OwnedTaskHandle {
    /// Task id used in shutdown evidence.
    #[must_use]
    pub fn task_id(&self) -> u64 {
        self.task_id
    }

    /// Abort exactly this owned task on the LocalSet thread. Idempotent.
    pub fn abort(&self) {
        send_control(
            self.control_tx.clone(),
            ControlMessage::Cancel {
                task_id: self.task_id,
            },
        );
    }
}

/// Cancels the owned LocalSet task when the awaiting future is dropped or times out.
struct CancelGuard {
    control_tx: mpsc::Sender<ControlMessage>,
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
        send_control_reliable(
            self.control_tx.clone(),
            ControlMessage::Cancel {
                task_id: self.task_id,
            },
        );
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

fn run_localset_thread(
    mut work_rx: mpsc::Receiver<WorkMessage>,
    mut control_rx: mpsc::Receiver<ControlMessage>,
    counters: Arc<AdmissionCounters>,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("LocalSet bridge tokio runtime");
    runtime.block_on(async move {
        let local = tokio::task::LocalSet::new();
        local.run_until(async move {
            let state_ref: ThreadStateRef = Arc::new(Mutex::new(BridgeThreadState {
                tasks: HashMap::new(),
                deferred: VecDeque::new(),
                cancelled_before_start: HashSet::new(),
            }));
            let mut aborted_ids: Vec<u64> = Vec::new();

            loop {
                tokio::select! {
                    biased;
                    msg = control_rx.recv() => {
                        match msg {
                            Some(ControlMessage::Cancel { task_id }) => {
                                if let Ok(mut state) = state_ref.lock() {
                                    state.process_cancel(task_id, &counters, &mut aborted_ids);
                                }
                            }
                            Some(ControlMessage::SpawnOwned(req)) => {
                                if let Ok(mut state) = state_ref.lock() {
                                    state.spawn_owned_task(req, &counters, &state_ref);
                                }
                            }
                            Some(ControlMessage::RunControl(req)) => {
                                if let Ok(mut state) = state_ref.lock() {
                                    state.enqueue_execute(req, &counters, &state_ref);
                                }
                            }
                            Some(ControlMessage::Shutdown { ack }) => {
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
                                                "LocalSet bridge shutdown before this request started"
                                                    .to_string(),
                                            ));
                                        }
                                    }
                                }
                                counters.shutting_down.store(true, Ordering::SeqCst);
                                let stats = counters.stats();
                                let evidence = ShutdownEvidence {
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
                                    aborted_task_ids: aborted_ids,
                                };
                                let _ = ack.send(evidence);
                                break;
                            }
                            None => break,
                        }
                    }
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
            return;
        }
        if tokio::runtime::Handle::try_current().is_ok() {
            return;
        }
        self.shutdown_sync();
    }
}
