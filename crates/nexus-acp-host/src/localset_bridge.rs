//! `LocalSet` bridge for `!Send` ACP SDK futures with tracked tasks and bounded admission.
//!
//! One lazy current-thread Tokio `LocalSet` OS thread per bridge instance. Send-side
//! callers submit factory closures; the `!Send` future is created on the owning thread.
//! Shutdown uses a dedicated control channel that bypasses the bounded work queue.

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};
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

type ErasedFuture =
    Pin<Box<dyn Future<Output = Box<dyn Any + Send>> + 'static>>;

type FutureFactory = Box<dyn FnOnce() -> ErasedFuture + Send + 'static>;

struct ExecuteRequest {
    task_id: u64,
    byte_charge: usize,
    factory: FutureFactory,
    result_tx: Option<oneshot::Sender<Box<dyn Any + Send>>>,
}

enum WorkMessage {
    Execute(ExecuteRequest),
    ReleasePending { bytes: usize },
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

struct AdmissionCounters {
    pending_requests: AtomicUsize,
    pending_bytes: AtomicUsize,
    active_tasks: AtomicUsize,
    shutting_down: AtomicBool,
    next_task_id: AtomicU64,
}

impl AdmissionCounters {
    fn new() -> Self {
        Self {
            pending_requests: AtomicUsize::new(0),
            pending_bytes: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            shutting_down: AtomicBool::new(false),
            next_task_id: AtomicU64::new(1),
        }
    }

    fn stats(&self) -> LocalSetBridgeStats {
        LocalSetBridgeStats {
            pending_requests: self.pending_requests.load(Ordering::Relaxed),
            pending_bytes: self.pending_bytes.load(Ordering::Relaxed),
            active_tasks: self.active_tasks.load(Ordering::Relaxed),
            shutting_down: self.shutting_down.load(Ordering::Relaxed),
        }
    }

    fn try_reserve_pending(&self, bytes: usize) -> bool {
        if self.shutting_down.load(Ordering::Acquire) {
            return false;
        }
        let cur_count = self.pending_requests.load(Ordering::Acquire);
        let cur_bytes = self.pending_bytes.load(Ordering::Acquire);
        if cur_count >= MAX_PENDING_REQUESTS {
            return false;
        }
        if cur_bytes + bytes > MAX_PENDING_BYTES {
            return false;
        }
        if self.active_tasks.load(Ordering::Acquire) >= MAX_ACTIVE_TASKS {
            return false;
        }
        self.pending_requests.fetch_add(1, Ordering::AcqRel);
        self.pending_bytes.fetch_add(bytes, Ordering::AcqRel);
        true
    }

    fn release_pending(&self, bytes: usize) {
        self.pending_requests.fetch_sub(1, Ordering::AcqRel);
        self.pending_bytes.fetch_sub(bytes, Ordering::AcqRel);
    }

    fn task_started(&self) {
        self.active_tasks.fetch_add(1, Ordering::AcqRel);
    }

    fn task_finished(&self) {
        self.active_tasks.fetch_sub(1, Ordering::AcqRel);
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

    fn ensure_runtime(&self) -> Result<(mpsc::Sender<WorkMessage>, mpsc::Sender<ControlMessage>), crate::AcpError> {
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
        let (control_tx, control_rx) = mpsc::channel::<ControlMessage>(4);

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
    pub async fn execute<F, T>(&self, f: F) -> crate::AcpResult<T>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = T> + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        const BYTE_CHARGE: usize = 0;
        self.wait_admission(BYTE_CHARGE).await?;
        let (work_tx, control_tx) = self.ensure_runtime()?;

        let task_id = self
            .counters
            .next_task_id
            .fetch_add(1, Ordering::AcqRel);
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
                byte_charge: BYTE_CHARGE,
                factory,
                result_tx: Some(result_tx),
            }))
            .await
            .is_err()
        {
            self.counters.release_pending(BYTE_CHARGE);
            return Err(crate::AcpError::connection_failed("LocalSet work channel closed"));
        }

        let cancel_on_drop = CancelGuard {
            control_tx,
            task_id,
            work_tx,
            byte_charge: BYTE_CHARGE,
            counters: self.counters.clone(),
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
        f: F,
        timeout_duration: Duration,
        operation_name: &str,
    ) -> crate::AcpResult<T>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = T> + 'static>> + Send + 'static,
        T: Send + 'static,
    {
        tokio::time::timeout(timeout_duration, self.execute(f))
            .await
            .map_err(|_| crate::AcpError::timeout(operation_name, timeout_duration))?
    }

    /// Explicit shutdown with join evidence. Idempotent.
    pub async fn shutdown(&self) -> ShutdownEvidence {
        if self
            .shutdown_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return self.join_evidence_only();
        }
        self.counters.shutting_down.store(true, Ordering::SeqCst);
        let control_tx = {
            let guard = self
                .runtime
                .lock()
                .map_err(|_| crate::AcpError::connection_failed("LocalSet bridge mutex poisoned"));
            match guard {
                Ok(g) => g.as_ref().map(|rt| rt.control_tx.clone()),
                Err(_) => None,
            }
        };

        if let Some(control_tx) = control_tx {
            let (ack_tx, ack_rx) = oneshot::channel();
            if control_tx
                .try_send(ControlMessage::Shutdown { ack: ack_tx })
                .is_ok()
            {
                match tokio::time::timeout(SHUTDOWN_JOIN_BUDGET, ack_rx).await {
                    Ok(Ok(evidence)) => {
                        self.join_runtime_thread().await;
                        return evidence;
                    }
                    Ok(Err(_)) => {
                        warn!("LocalSet shutdown ack channel closed before evidence");
                    }
                    Err(_) => {
                        warn!("LocalSet shutdown ack timed out after {:?}", SHUTDOWN_JOIN_BUDGET);
                    }
                }
            }
        }

        let mut evidence = self.counters.stats().into_evidence(vec![]);
        self.join_runtime_thread().await;
        evidence.joined_cleanly = !evidence.thread_alive;
        evidence
    }

    fn join_evidence_only(&self) -> ShutdownEvidence {
        let mut evidence = self.counters.stats().into_evidence(vec![]);
        evidence.joined_cleanly = !evidence.thread_alive;
        evidence
    }

    async fn join_runtime_thread(&self) {
        let handle = {
            let mut guard = self
                .runtime
                .lock()
                .expect("LocalSet bridge mutex poisoned");
            guard.take().map(|rt| rt.thread_handle)
        };
        if let Some(handle) = handle {
            let join_result = tokio::task::spawn_blocking(move || handle.join()).await;
            match join_result {
                Ok(Ok(())) => debug!("LocalSet bridge thread joined"),
                Ok(Err(panic)) => error!("LocalSet bridge thread panicked: {:?}", panic),
                Err(e) => warn!("LocalSet join helper failed: {:?}", e),
            }
        }
    }
}

impl LocalSetBridgeStats {
    fn into_evidence(self, aborted_task_ids: Vec<u64>) -> ShutdownEvidence {
        ShutdownEvidence {
            joined_cleanly: false,
            thread_alive: false,
            pending_requests: self.pending_requests,
            pending_bytes: self.pending_bytes,
            active_tasks: self.active_tasks,
            aborted_task_ids,
        }
    }
}

/// Cancels the owned LocalSet task when the awaiting future is dropped or times out.
struct CancelGuard {
    control_tx: mpsc::Sender<ControlMessage>,
    task_id: u64,
    work_tx: mpsc::Sender<WorkMessage>,
    byte_charge: usize,
    counters: Arc<AdmissionCounters>,
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
        let _ = self
            .control_tx
            .try_send(ControlMessage::Cancel { task_id: self.task_id });
        let _ = self.work_tx.try_send(WorkMessage::ReleasePending {
            bytes: self.byte_charge,
        });
        self.counters.release_pending(self.byte_charge);
    }
}

struct ActiveGuard(Arc<AdmissionCounters>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.task_finished();
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
            let mut tasks: HashMap<u64, tokio::task::AbortHandle> = HashMap::new();
            let mut aborted_ids: Vec<u64> = Vec::new();

            loop {
                tokio::select! {
                    biased;
                    msg = control_rx.recv() => {
                        match msg {
                            Some(ControlMessage::Cancel { task_id }) => {
                                if let Some(handle) = tasks.remove(&task_id) {
                                    handle.abort();
                                    aborted_ids.push(task_id);
                                }
                            }
                            Some(ControlMessage::Shutdown { ack }) => {
                                for (task_id, handle) in tasks.drain() {
                                    handle.abort();
                                    aborted_ids.push(task_id);
                                }
                                counters.active_tasks.store(0, Ordering::SeqCst);
                                counters.shutting_down.store(true, Ordering::SeqCst);
                                let evidence = counters.stats().into_evidence(aborted_ids);
                                let _ = ack.send(evidence);
                                break;
                            }
                            None => break,
                        }
                    }
                    msg = work_rx.recv() => {
                        match msg {
                            Some(WorkMessage::Execute(req)) => {
                                let task_id = req.task_id;
                                let byte_charge = req.byte_charge;
                                let factory = req.factory;
                                let result_tx = req.result_tx;
                                let task_counters = counters.clone();
                                let handle = tokio::task::spawn_local(async move {
                                    task_counters.task_started();
                                    let _active = ActiveGuard(task_counters.clone());
                                    let future = factory();
                                    let output = future.await;
                                    if let Some(tx) = result_tx {
                                        let _ = tx.send(output);
                                    }
                                });
                                tasks.insert(task_id, handle.abort_handle());
                            }
                            Some(WorkMessage::ReleasePending { bytes }) => {
                                counters.release_pending(bytes);
                            }
                            None => break,
                        }
                    }
                }
            }
        }).await;
    });
}

impl Default for LocalSetBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LocalSetBridge {
    fn drop(&mut self) {
        if Arc::strong_count(&self.runtime) == 1
            && self
                .shutdown_claimed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            self.counters.shutting_down.store(true, Ordering::SeqCst);
            if let Ok(guard) = self.runtime.lock() {
                if let Some(rt) = guard.as_ref() {
                    let (ack_tx, _ack_rx) = oneshot::channel();
                    let _ = rt
                        .control_tx
                        .try_send(ControlMessage::Shutdown { ack: ack_tx });
                }
            } }
    }
}


