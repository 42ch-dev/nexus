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

type ErasedFuture = Pin<Box<dyn Future<Output = Box<dyn Any + Send>> + 'static>>;
type FutureFactory = Box<dyn FnOnce() -> ErasedFuture + Send + 'static>;

struct ExecuteRequest {
    task_id: u64,
    byte_charge: usize,
    factory: FutureFactory,
    result_tx: Option<oneshot::Sender<Box<dyn Any + Send>>>,
}

enum WorkMessage {
    Execute(ExecuteRequest),
}

enum ControlMessage {
    Cancel { task_id: u64 },
    Shutdown { ack: oneshot::Sender<ShutdownEvidence> },
}

/// Evidence returned after shutdown for proof/reporting.
#[derive(Debug, Clone, Default)]
pub struct ShutdownEvidence {
    pub joined_cleanly: bool,
    pub thread_alive: bool,
    pub pending_requests: usize,
    pub pending_bytes: usize,
    pub active_tasks: usize,
    pub aborted_task_ids: Vec<u64>,
}

/// Live bridge counters (for tests and lifecycle proofs).
#[derive(Debug, Clone, Default)]
pub struct LocalSetBridgeStats {
    pub pending_requests: usize,
    pub pending_bytes: usize,
    pub active_tasks: usize,
    pub shutting_down: bool,
}

#[derive(Debug, Default)]
struct PendingAdmission {
    requests: usize,
    bytes: usize,
}

struct AdmissionCounters {
    pending: Mutex<PendingAdmission>,
    active_tasks: AtomicUsize,
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
}

impl LocalSetBridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(None)),
            counters: Arc::new(AdmissionCounters::new()),
            shutdown_claimed: Arc::new(AtomicBool::new(false)),
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
            return Err(crate::AcpError::connection_failed("LocalSet bridge shutting down"));
        }

        let counters = self.counters.clone();
        let (work_tx, work_rx) = mpsc::channel::<WorkMessage>(MAX_PENDING_REQUESTS);
        let (control_tx, control_rx) = mpsc::channel::<ControlMessage>(64);

        let thread_handle = thread::Builder::new()
            .name("nexus-localset-bridge".to_string())
            .spawn(move || run_localset_thread(work_rx, control_rx, counters))
            .map_err(|e| crate::AcpError::connection_failed(format!("spawn LocalSet thread: {e}")))?;

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
                return Err(crate::AcpError::connection_failed("LocalSet bridge shutting down"));
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
        let (work_tx, control_tx) = self.ensure_runtime()?;

        let task_id = self.counters.next_task_id.fetch_add(1, Ordering::AcqRel);
        let (result_tx, result_rx) = oneshot::channel();

        let factory: FutureFactory = Box::new(move || {
            let future = f();
            Box::pin(async move {
                let value = future.await;
                Box::new(value) as Box<dyn Any + Send>
            })
        });

        if work_tx
            .send(WorkMessage::Execute(ExecuteRequest {
                task_id,
                byte_charge,
                factory,
                result_tx: Some(result_tx),
            }))
            .await
            .is_err()
        {
            self.counters.release_pending(byte_charge);
            return Err(crate::AcpError::connection_failed("LocalSet work channel closed"));
        }

        let cancel_on_drop = CancelGuard {
            control_tx,
            task_id,
            cancelled: Arc::new(AtomicBool::new(false)),
        };

        let recv = result_rx.await;
        cancel_on_drop.disarm();

        match recv {
            Ok(boxed) => {
                let value = *boxed
                    .downcast::<T>()
                    .map_err(|_| crate::AcpError::connection_failed("LocalSet result type mismatch"))?;
                Ok(value)
            }
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
                evidence.thread_alive = true;
            } else {
                match tokio::time::timeout(
                    join_budget,
                    tokio::task::spawn_blocking(move || handle.join()),
                )
                .await
                {
                    Ok(Ok(Ok(()))) => {
                        self.counters.mark_thread_joined();
                        evidence.joined_cleanly = true;
                        evidence.thread_alive = false;
                        debug!("LocalSet bridge thread joined");
                    }
                    Ok(Ok(Err(panic))) => {
                        error!("LocalSet bridge thread panicked: {:?}", panic);
                        evidence.joined_cleanly = false;
                        evidence.thread_alive = false;
                    }
                    Ok(Err(e)) => {
                        warn!("LocalSet join helper failed: {:?}", e);
                        evidence.thread_alive = true;
                    }
                    Err(_) => {
                        warn!(
                            "LocalSet join timed out after {:?}",
                            join_budget
                        );
                        evidence.thread_alive = true;
                    }
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
        evidence
    }

    /// Synchronous shutdown for `Drop` / finalizer paths (bounded join).
    pub fn shutdown_sync(&self) {
        self.shutdown_sync_with_deadline(Instant::now() + SHUTDOWN_JOIN_BUDGET);
    }

    /// Synchronous shutdown bounded by an absolute deadline.
    pub fn shutdown_sync_with_deadline(&self, deadline: Instant) {
        if self
            .shutdown_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        self.counters.shutting_down.store(true, Ordering::SeqCst);
        let handle = {
            let guard = self.runtime.lock().ok();
            let control_tx = guard
                .as_ref()
                .and_then(|g| g.as_ref().map(|rt| rt.control_tx.clone()));
            if let Some(control_tx) = control_tx {
                let (ack_tx, mut ack_rx) = oneshot::channel();
                send_control_reliable(control_tx, ControlMessage::Shutdown { ack: ack_tx });
                let ack_deadline = Instant::now()
                    + deadline.saturating_duration_since(Instant::now());
                while Instant::now() < ack_deadline {
                    if ack_rx.try_recv().is_ok() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            }
            guard.and_then(|mut g| g.take().map(|rt| rt.thread_handle))
        };
        if let Some(handle) = handle {
            let join_deadline = Instant::now()
                + deadline.saturating_duration_since(Instant::now());
            while !handle.is_finished() && Instant::now() < join_deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
                self.counters.mark_thread_joined();
            }
        }
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

struct TaskSlot {
    handle: tokio::task::AbortHandle,
    /// Set when the spawned future is first polled; until then the tracking entry owns the active reservation.
    polled: Arc<AtomicBool>,
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
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.counters.task_finished();
        if let Ok(mut bridge_state) = self.state.lock() {
            bridge_state.tasks.remove(&self.task_id);
            bridge_state.drain_deferred(&self.counters, &self.state);
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
                counters.task_finished();
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

    fn enqueue_execute(
        &mut self,
        req: ExecuteRequest,
        counters: &Arc<AdmissionCounters>,
        state_ref: &ThreadStateRef,
    ) {
        let task_id = req.task_id;
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
        counters.release_pending(req.byte_charge);
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
            },
        );
        counters.task_slot_notify.notify_waiters();
    }

    fn drain_deferred(&mut self, counters: &Arc<AdmissionCounters>, state_ref: &ThreadStateRef) {
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
                            Some(ControlMessage::Shutdown { ack }) => {
                                if let Ok(mut state) = state_ref.lock() {
                                    for (task_id, slot) in state.tasks.drain() {
                                        if !slot.polled.load(Ordering::Acquire) {
                                            counters.task_finished();
                                        }
                                        aborted_ids.push(task_id);
                                        slot.handle.abort();
                                    }
                                    for req in state.deferred.drain(..) {
                                        counters.release_pending(req.byte_charge);
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
