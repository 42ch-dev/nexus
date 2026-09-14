use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use napi::bindgen_prelude::Error;
use napi::Env;
use nexus_agent_host::HostFacade;
use nexus_agent_host::HostManager;
use nexus_contracts::{CoreCloseReport, CoreError, CoreErrorCode};
use nexus_core::CoreService;
use nexus_provider_ports::ProviderPort;
use tokio::sync::{Mutex, Notify};

pub const MAX_PENDING_BYTES_TOTAL: usize = 1024 * 1024;
pub const FINALIZE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// Serialized native environment lifecycle phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvLifecyclePhase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// Per-addon environment instance marker stored via `Env::set_instance_data`.
pub struct EnvInstance {
    pub state: Arc<EnvState>,
}

impl EnvInstance {
    pub fn install(env: &Env, state: Arc<EnvState>) -> napi::Result<()> {
        let finalize_state = state.clone();
        env.set_instance_data(EnvInstance { state }, (), move |_ctx| {
            EnvState::run_bounded_native_finalize(finalize_state);
        })?;
        Ok(())
    }

    pub fn get(env: &Env) -> Result<Arc<EnvState>, Error> {
        match env.get_instance_data::<EnvInstance>()? {
            Some(instance) => Ok(instance.state.clone()),
            None => Err(Error::from_reason("native environment not initialized")),
        }
    }
}

pub struct PendingBudget {
    used: AtomicUsize,
}

impl PendingBudget {
    #[must_use]
    pub fn new() -> Self {
        Self {
            used: AtomicUsize::new(0),
        }
    }

    pub fn try_charge(&self, bytes: usize) -> Result<PendingBudgetGuard<'_>, CoreError> {
        if bytes > MAX_PENDING_BYTES_TOTAL {
            return Err(CoreError {
                code: CoreErrorCode::InvalidInput,
                message: "input_too_large".into(),
                details: Default::default(),
                http_status: Some(413),
            });
        }
        loop {
            let current = self.used.load(Ordering::Acquire);
            if current + bytes > MAX_PENDING_BYTES_TOTAL {
                return Err(CoreError {
                    code: CoreErrorCode::Busy,
                    message: "callback byte budget exhausted".into(),
                    details: Default::default(),
                    http_status: Some(503),
                });
            }
            if self
                .used
                .compare_exchange_weak(
                    current,
                    current + bytes,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return Ok(PendingBudgetGuard {
                    budget: self,
                    bytes,
                });
            }
        }
    }
}

pub struct PendingBudgetGuard<'a> {
    budget: &'a PendingBudget,
    bytes: usize,
}

impl Drop for PendingBudgetGuard<'_> {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::Release);
    }
}

pub struct EnvState {
    pub generation: AtomicU64,
    pub core: StdMutex<Option<Arc<CoreService>>>,
    pub host: StdMutex<Option<Arc<HostManager>>>,
    pub provider_port: StdMutex<Option<Arc<dyn ProviderPort>>>,
    pub pending_budget: PendingBudget,
    /// One-shot close signal used by cancellation paths that can afford to miss
    /// a wake (they re-check `is_closing`).
    pub close_notify: Notify,
    /// Race-free close signal. A `watch` receiver carries a version, so a close
    /// that lands between a subscriber's check and its await can never be lost —
    /// unlike `Notify::notify_waiters`, which fires only at already-registered
    /// waiters.
    pub close_watch: tokio::sync::watch::Sender<bool>,
    pub closing: AtomicU64,
    pub lifecycle: StdMutex<EnvLifecyclePhase>,
    /// A rollback or close retained an owner whose cleanup is unconfirmed.
    pub interrupted: AtomicBool,
    /// Environment tombstoned after NAPI cleanup; no further JS calls.
    pub env_dead: AtomicBool,
    /// Concurrent close callers await one settlement.
    pub close_notify_settled: Arc<Notify>,
    pub settled_close: Mutex<Option<CoreCloseReport>>,
    pub close_in_flight: Mutex<bool>,
    /// Last observed pending operation/task IDs for close reports.
    pub pending_operation_ids: Mutex<Vec<String>>,
    /// Bounded structured state for JS-provider sessions and operations.
    ///
    /// `hostQuery` also reads it as a fallback when a hydration miss occurs.
    pub js_state: StdMutex<JsProviderState>,
    /// Durable journal pool for JS-provider operations (LIFE-3). Set when the
    /// core opens; `None` for the service-only shell. Survives process exit so
    /// a previously active op is still queryable as `interrupted`.
    pub journal_pool: StdMutex<Option<sqlx::SqlitePool>>,
    /// Service-only shell: compatibility/status without DB, host, or principal.
    pub service_only_uninitialized: AtomicBool,
}

/// Terminal statuses a JS-provider operation can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsOperationStatus {
    Running,
    Finished,
    Failed,
    Interrupted,
    /// A cooperative cancel was accepted. This is terminal and observable — a
    /// repeated GET reports `cancelled`, never a stale `running`.
    Cancelled,
}

impl JsOperationStatus {
    /// The generated wire status string for this terminal/live state.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            JsOperationStatus::Running => "running",
            JsOperationStatus::Finished => "finished",
            JsOperationStatus::Failed => "failed",
            JsOperationStatus::Interrupted => "interrupted",
            JsOperationStatus::Cancelled => "cancelled",
        }
    }
}

/// One JS-provider operation retained for exact operation responses.
#[derive(Debug, Clone)]
pub struct JsOperationRecord {
    pub operation_id: String,
    pub session_id: String,
    pub status: JsOperationStatus,
}

/// A JS-provider session plus its active operation.
#[derive(Debug, Clone)]
pub struct JsSessionRecord {
    pub session_id: String,
    pub provider_id: String,
    pub active_operation_id: Option<String>,
}

/// Bounded structured state for JS-provider sessions and operations.
///
/// Session/active-op state is bounded by the transport reservation
/// ([`JS_PROVIDER_MAX_ACTIVE_SESSIONS`]); retained terminal operations by
/// [`JS_MAX_TERMINAL_OPERATIONS`]. No collection grows unbounded.
#[derive(Debug)]
pub struct JsProviderState {
    sessions: BTreeMap<String, JsSessionRecord>,
    terminal: VecDeque<JsOperationRecord>,
    /// Slots reserved by an in-flight launch, counted against the session cap so
    /// concurrent launches cannot race past the bound.
    reserved: usize,
}

/// Hard cap on retained terminal JS operations across all JS sessions.
pub const JS_MAX_TERMINAL_OPERATIONS: usize = 64;

/// Explicit transport cap on concurrent JS-provider sessions, reserved before a
/// launch is dispatched so a successful launched child is never dropped (which
/// would leak an owned ACP child untracked by close).
pub const JS_PROVIDER_MAX_ACTIVE_SESSIONS: usize = 16;

impl JsProviderState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            terminal: VecDeque::new(),
            reserved: 0,
        }
    }

    /// Upsert a terminal record by operation_id, so a duplicate terminal event
    /// can never retain a stale status or waste the cap.
    ///
    /// The first terminal wins: a later event must never downgrade an
    /// already-observed terminal (in particular, a `cancelled` accepted by the
    /// cooperative cancel path stays `cancelled` even if a subsequent stream
    /// event would otherwise rewrite it).
    fn upsert_terminal(&mut self, record: JsOperationRecord) {
        if let Some(existing) = self
            .terminal
            .iter_mut()
            .find(|op| op.operation_id == record.operation_id)
        {
            if existing.status == JsOperationStatus::Running {
                *existing = record;
            }
            return;
        }
        self.terminal.push_back(record);
        while self.terminal.len() > JS_MAX_TERMINAL_OPERATIONS {
            self.terminal.pop_front();
        }
    }

    /// Look up an active-or-terminal operation by id.
    #[must_use]
    pub fn operation(&self, operation_id: &str) -> Option<JsOperationRecord> {
        for session in self.sessions.values() {
            if session.active_operation_id.as_deref() == Some(operation_id) {
                return Some(JsOperationRecord {
                    operation_id: operation_id.to_string(),
                    session_id: session.session_id.clone(),
                    status: JsOperationStatus::Running,
                });
            }
        }
        self.terminal
            .iter()
            .find(|op| op.operation_id == operation_id)
            .cloned()
    }

    #[must_use]
    pub fn session(&self, session_id: &str) -> Option<&JsSessionRecord> {
        self.sessions.get(session_id)
    }

    #[must_use]
    pub fn sessions(&self) -> impl Iterator<Item = &JsSessionRecord> {
        self.sessions.values()
    }
}

impl EnvState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(1),
            core: StdMutex::new(None),
            host: StdMutex::new(None),
            provider_port: StdMutex::new(None),
            pending_budget: PendingBudget::new(),
            close_notify: Notify::new(),
            close_watch: tokio::sync::watch::channel(false).0,
            closing: AtomicU64::new(0),
            lifecycle: StdMutex::new(EnvLifecyclePhase::Closed),
            interrupted: AtomicBool::new(false),
            env_dead: AtomicBool::new(false),
            close_notify_settled: Arc::new(Notify::new()),
            settled_close: Mutex::new(None),
            close_in_flight: Mutex::new(false),
            pending_operation_ids: Mutex::new(Vec::new()),
            js_state: StdMutex::new(JsProviderState::new()),
            journal_pool: StdMutex::new(None),
            service_only_uninitialized: AtomicBool::new(false),
        }
    }

    /// Race-safe pre-launch reservation for a JS-provider session.
    ///
    /// Reserves one of the bounded session slots *before* the launch is
    /// dispatched, so a launched child is never dropped for lack of tracking
    /// (which would leak an owned ACP child). Returns `false` when the transport
    /// session cap is reached.
    pub fn try_reserve_js_session(&self) -> bool {
        if let Ok(mut state) = self.js_state.lock() {
            if state.sessions.len() + state.reserved >= JS_PROVIDER_MAX_ACTIVE_SESSIONS {
                return false;
            }
            state.reserved += 1;
            return true;
        }
        false
    }

    /// Release a reservation whose launch did not produce a session.
    pub fn release_js_session_reservation(&self) {
        if let Ok(mut state) = self.js_state.lock() {
            state.reserved = state.reserved.saturating_sub(1);
        }
    }

    /// Record a launched JS-provider session and its admitted provider id,
    /// consuming a pending reservation if one exists.
    pub fn record_js_session(&self, session_id: String, provider_id: String) {
        if let Ok(mut state) = self.js_state.lock() {
            let consumed_reservation = state.reserved > 0;
            if consumed_reservation {
                state.reserved -= 1;
            } else if state.sessions.len() >= JS_PROVIDER_MAX_ACTIVE_SESSIONS {
                return;
            }
            state.sessions.insert(
                session_id.clone(),
                JsSessionRecord {
                    session_id,
                    provider_id,
                    active_operation_id: None,
                },
            );
        }
    }

    /// Record an executed JS-provider operation. Requires the admitted launch
    /// record: an unknown session is never fabricated with an empty provider id.
    pub fn record_js_session_operation(&self, session_id: &str, operation_id: String) {
        if let Ok(mut state) = self.js_state.lock() {
            if let Some(session) = state.sessions.get_mut(session_id) {
                session.active_operation_id = Some(operation_id);
            }
        }
    }

    /// Record a cooperative cancel acknowledgement. The accepted cancel settles
    /// the session's active operation to the `cancelled` terminal so a repeated
    /// GET observes `cancelled` rather than a stale `running`. The terminal is
    /// upserted monotonically (a later `running` write can never downgrade it).
    pub fn record_js_cancel_ack(&self, session_id: &str) {
        if let Ok(mut state) = self.js_state.lock() {
            let active = state
                .sessions
                .get(session_id)
                .and_then(|s| s.active_operation_id.clone());
            let Some(op_id) = active else {
                return;
            };
            if let Some(session) = state.sessions.get_mut(session_id) {
                session.active_operation_id = None;
            }
            state.upsert_terminal(JsOperationRecord {
                operation_id: op_id,
                session_id: session_id.to_string(),
                status: JsOperationStatus::Cancelled,
            });
        }
    }

    /// Record an operation terminal observed from a real provider event batch.
    /// Clears the session's active op and upserts a bounded terminal record.
    pub fn record_js_operation_terminal(
        &self,
        session_id: &str,
        operation_id: &str,
        status: JsOperationStatus,
    ) {
        if let Ok(mut state) = self.js_state.lock() {
            if let Some(session) = state.sessions.get_mut(session_id) {
                if session.active_operation_id.as_deref() == Some(operation_id) {
                    session.active_operation_id = None;
                }
            }
            state.upsert_terminal(JsOperationRecord {
                operation_id: operation_id.to_string(),
                session_id: session_id.to_string(),
                status,
            });
        }
    }

    /// A `SessionStopped` marks any active op interrupted and removes the session
    /// from the active set, consistently with shutdown semantics.
    pub fn record_js_session_stopped(&self, session_id: &str) {
        if let Ok(mut state) = self.js_state.lock() {
            let active = state
                .sessions
                .get(session_id)
                .and_then(|s| s.active_operation_id.clone());
            if let Some(op_id) = active {
                state.upsert_terminal(JsOperationRecord {
                    operation_id: op_id,
                    session_id: session_id.to_string(),
                    status: JsOperationStatus::Interrupted,
                });
            }
            state.sessions.remove(session_id);
        }
    }

    /// Forget a JS-provider session whose owned cleanup was confirmed.
    pub fn forget_js_session(&self, session_id: &str) {
        if let Ok(mut state) = self.js_state.lock() {
            state.sessions.remove(session_id);
        }
    }

    /// Live JS-provider sessions awaiting an owned release.
    pub fn js_sessions_snapshot(&self) -> Vec<(String, Option<String>)> {
        self.js_state
            .lock()
            .map(|state| {
                state
                    .sessions
                    .values()
                    .map(|s| (s.session_id.clone(), s.active_operation_id.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Session ids still awaiting release, for close reports.
    pub fn js_session_ids(&self) -> Vec<String> {
        self.js_sessions_snapshot()
            .into_iter()
            .map(|(session_id, _)| session_id)
            .collect()
    }

    /// Run `f` against the structured JS state under one lock.
    pub fn with_js_state<T>(&self, f: impl FnOnce(&JsProviderState) -> T) -> Option<T> {
        self.js_state.lock().ok().map(|state| f(&state))
    }

    /// Install the durable journal pool when the core opens.
    pub fn set_journal_pool(&self, pool: sqlx::SqlitePool) {
        if let Ok(mut slot) = self.journal_pool.lock() {
            *slot = Some(pool);
        }
    }

    /// A cloned journal pool handle, if the core opened.
    #[must_use]
    pub fn journal_pool(&self) -> Option<sqlx::SqlitePool> {
        self.journal_pool.lock().ok().and_then(|slot| slot.clone())
    }

    /// Journal an operation write-through. Best-effort: a journal failure never
    /// fails the provider effect (the in-memory state remains authoritative for
    /// this process).
    pub async fn journal_operation(&self, operation_id: &str, session_id: &str, provider_id: &str, status: &str) {
        let Some(pool) = self.journal_pool() else {
            return;
        };
        let _ = nexus_local_db::js_provider_journal::upsert_operation(
            &pool,
            operation_id,
            session_id,
            provider_id,
            status,
        )
        .await;
    }

    /// Settle orphaned journal entries on open: any non-terminal operation the
    /// predecessor process left behind becomes `interrupted` (LIFE-3).
    pub async fn settle_journal_on_open(&self) {
        let Some(pool) = self.journal_pool() else {
            return;
        };
        let _ = nexus_local_db::js_provider_journal::settle_orphaned_as_interrupted(&pool).await;
    }

    /// Inspect the actual delivered provider events and update JS operation
    /// truth: `OpFinished` → finished, `OpFailed` → failed, `SessionStopped` →
    /// interrupted (any active op) and the session leaves the active set. The
    /// terminal is derived from the real batch, never a caller label. Returns
    /// the wire status if this batch carried a terminal, for durable mirroring.
    pub fn apply_js_batch_terminals(
        &self,
        operation_id: &str,
        events: &[nexus_contracts::provider_event_batch::NexusProviderHostEvent],
    ) -> Option<&'static str> {
        let Some(session_id) = self
            .with_js_state(|state| state.operation(operation_id).map(|op| op.session_id))
            .flatten()
        else {
            return None;
        };
        use nexus_contracts::provider_event_batch::NexusProviderHostEvent as BatchEvent;
        let mut terminal = None;
        for event in events {
            match event {
                BatchEvent::OpFinished { .. } => {
                    self.record_js_operation_terminal(
                        &session_id,
                        operation_id,
                        JsOperationStatus::Finished,
                    );
                    terminal = Some(JsOperationStatus::Finished.wire());
                }
                BatchEvent::OpFailed { .. } => {
                    self.record_js_operation_terminal(
                        &session_id,
                        operation_id,
                        JsOperationStatus::Failed,
                    );
                    terminal = Some(JsOperationStatus::Failed.wire());
                }
                BatchEvent::SessionStopped { .. } => {
                    self.record_js_session_stopped(&session_id);
                    terminal = Some(JsOperationStatus::Interrupted.wire());
                }
                _ => {}
            }
        }
        terminal
    }

    /// Journal an operation's terminal status durably (LIFE-3).
    pub async fn journal_operation_status(&self, operation_id: &str, status: &str) {
        let Some(pool) = self.journal_pool() else {
            return;
        };
        let (session_id, provider_id) = self
            .with_js_state(|s| {
                s.operation(operation_id)
                    .map(|op| (op.session_id, String::new()))
            })
            .flatten()
            .unwrap_or_default();
        let _ = nexus_local_db::js_provider_journal::upsert_operation(
            &pool,
            operation_id,
            &session_id,
            &provider_id,
            status,
        )
        .await;
    }

    /// Drop journaled operations for a cleanly shut-down session.
    pub async fn forget_journal_session(&self, session_id: &str) {
        let Some(pool) = self.journal_pool() else {
            return;
        };
        let _ = nexus_local_db::js_provider_journal::forget_session(&pool, session_id).await;
    }

    pub fn lifecycle_phase(&self) -> EnvLifecyclePhase {
        *self.lifecycle.lock().expect("lifecycle mutex poisoned")
    }

    pub async fn try_begin_open(&self) -> Result<(), String> {
        let mut phase = self.lifecycle.lock().expect("lifecycle mutex poisoned");
        match *phase {
            EnvLifecyclePhase::Closed => {
                *phase = EnvLifecyclePhase::Opening;
                Ok(())
            }
            EnvLifecyclePhase::Open => Err("already open".to_string()),
            EnvLifecyclePhase::Opening => Err("opening".to_string()),
            EnvLifecyclePhase::Closing => Err("closing".to_string()),
        }
    }

    pub async fn publish_open(&self) {
        *self.lifecycle.lock().expect("lifecycle mutex poisoned") = EnvLifecyclePhase::Open;
        // The close signal is a level, not a pulse: reopening must clear it or
        // every later callback would observe "closing" and be rejected. Only the
        // `false` transition is broadcast on open; the next close sets it again.
        self.close_watch.send_replace(false);
    }

    pub async fn begin_closing_phase(&self) {
        *self.lifecycle.lock().expect("lifecycle mutex poisoned") = EnvLifecyclePhase::Closing;
    }

    pub async fn publish_closed(&self) {
        *self.lifecycle.lock().expect("lifecycle mutex poisoned") = EnvLifecyclePhase::Closed;
        self.clear_service_only_uninitialized();
    }

    /// Subscribe to the race-free close signal.
    ///
    /// `wait_for` returns immediately when the value already satisfies the
    /// predicate, so a close that happened before the subscription is observed
    /// rather than lost.
    #[must_use]
    pub fn close_watch(&self) -> tokio::sync::watch::Receiver<bool> {
        self.close_watch.subscribe()
    }

    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst) != 0
            || matches!(self.lifecycle_phase(), EnvLifecyclePhase::Closing)
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    pub fn mark_interrupted(&self) {
        self.interrupted.store(true, Ordering::SeqCst);
    }

    pub fn clear_interrupted(&self) {
        self.interrupted.store(false, Ordering::SeqCst);
    }

    pub fn begin_close(&self) {
        self.closing
            .store(self.generation.load(Ordering::SeqCst), Ordering::SeqCst);
        self.close_notify.notify_waiters();
        // `send_replace` always updates the stored value, so a receiver created
        // after this point still observes the closed state.
        self.close_watch.send_replace(true);
    }

    pub fn is_env_dead(&self) -> bool {
        self.env_dead.load(Ordering::SeqCst)
    }

    pub async fn record_pending_operations(&self, ids: Vec<String>) {
        *self.pending_operation_ids.lock().await = ids;
    }

    pub async fn pending_operations_snapshot(&self) -> Vec<String> {
        self.pending_operation_ids.lock().await.clone()
    }

    pub fn is_service_only_uninitialized(&self) -> bool {
        self.service_only_uninitialized.load(Ordering::SeqCst)
    }

    pub fn mark_service_only_uninitialized(&self) {
        self.service_only_uninitialized.store(true, Ordering::SeqCst);
    }

    pub fn clear_service_only_uninitialized(&self) {
        self.service_only_uninitialized.store(false, Ordering::SeqCst);
    }

    /// A host cleanup is only ever confirmed by a successful `HostResult`.
    ///
    /// `Err` (including `cleanup_unconfirmed`) and an outer deadline timeout
    /// both mean the owned sessions were not proven clean, so the environment
    /// keeps the owner and stays interrupted rather than reporting success.
    #[must_use]
    pub fn host_shutdown_confirmed(
        result: &Result<nexus_agent_host::HostResult<()>, tokio::time::error::Elapsed>,
    ) -> bool {
        matches!(result, Ok(Ok(())))
    }

    /// Whether a bounded native finalize may release the host owner.
    ///
    /// Release requires BOTH halves: the LocalSet bridge *settled* (a clean join
    /// with no live work — `!thread_alive` alone is not settlement, because a
    /// thread that panicked also reports `thread_alive == false`) *and* the host
    /// reported a confirmed shutdown.
    #[must_use]
    pub const fn finalize_owner_released(bridge_settled: bool, host_confirmed: bool) -> bool {
        bridge_settled && host_confirmed
    }

/// NAPI8 environment cleanup: tombstone and run bounded native-only teardown.
    pub fn run_bounded_native_finalize(state: Arc<EnvState>) {
        state.tombstone_env();
        let deadline = std::time::Instant::now() + FINALIZE_BUDGET;
        let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed_flag = completed.clone();
        let work_state = state.clone();

        let worker = std::thread::Builder::new()
            .name("nexus-native-finalize".into())
            .spawn(move || {
                if let Ok(rt) = std::panic::catch_unwind(super::runtime::runtime) {
                    rt.block_on(async move {
                        let budget =
                            || deadline.saturating_duration_since(std::time::Instant::now());

                        let core = work_state.take_core();
                        if let Some(service) = core {
                            if budget().is_zero() {
                                work_state.restore_core(service);
                            } else {
                                match tokio::time::timeout_at(
                                    tokio::time::Instant::from_std(deadline),
                                    service.close(),
                                )
                                .await
                                {
                                    Ok(_) => {}
                                    Err(_) => work_state.restore_core(service),
                                }
                            }
                        }

                        let host = work_state.take_host();
                        if let Some(host) = host {
                            if budget().is_zero() {
                                work_state.mark_interrupted();
                                work_state.restore_host(host);
                            } else {
                                host.localset_bridge().begin_drain();
                                let bridge_evidence =
                                    host.localset_bridge().shutdown_sync_with_deadline(deadline);
                                // A join that could not settle inside the budget
                                // hands its `JoinHandle` to tracked cleanup
                                // straight away, so the owner stays retained even
                                // if the host shutdown below consumes the rest of
                                // the deadline. Never detached.
                                if bridge_evidence.thread_alive {
                                    if let Some(handle) =
                                        host.localset_bridge().take_retained_runtime_thread()
                                    {
                                        super::cleanup_registry::register_localset_thread(
                                            handle,
                                            work_state.clone(),
                                        );
                                    }
                                }
                                // The typed `HostResult` inside the timeout is
                                // authoritative, and it only counts once the
                                // bridge itself settled: a panicked or still-live
                                // LocalSet thread is never a clean cleanup.
                                let host_confirmed = if bridge_evidence.is_settled()
                                    && !budget().is_zero()
                                {
                                    let result = tokio::time::timeout_at(
                                        tokio::time::Instant::from_std(deadline),
                                        host.shutdown(),
                                    )
                                    .await;
                                    Self::host_shutdown_confirmed(&result)
                                } else {
                                    false
                                };
                                if Self::finalize_owner_released(
                                    bridge_evidence.is_settled(),
                                    host_confirmed,
                                ) {
                                    // Owned sessions confirmed clean: the host is
                                    // released with the worker, and the process
                                    // reaper is stopped inside the remaining
                                    // budget so it cannot outlive this finalize.
                                    let stop = super::cleanup_registry::stop_reaper(deadline);
                                    if !stop.joined || stop.pending_entries > 0 {
                                        let mut pending =
                                            work_state.pending_operations_snapshot().await;
                                        if !stop.joined {
                                            pending.push("cleanup-reaper-unjoined".to_string());
                                        }
                                        if stop.pending_entries > 0 {
                                            pending.push(format!(
                                                "cleanup-registry-pending:{}",
                                                stop.pending_entries
                                            ));
                                        }
                                        work_state.record_pending_operations(pending).await;
                                    }
                                } else {
                                    let mut pending =
                                        work_state.pending_operations_snapshot().await;
                                    pending.extend(bridge_evidence.unsettled_pending());
                                    if !host_confirmed {
                                        pending.push("host-shutdown-unconfirmed".to_string());
                                    }
                                    work_state.record_pending_operations(pending).await;
                                    work_state.mark_interrupted();
                                    work_state.restore_host(host);
                                }
                            }
                        }

                        let port = work_state.take_provider_port();
                        if let Some(port) = port {
                            work_state.restore_provider_port(port);
                        }
                    });
                }
                completed_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            })
            .expect("spawn finalize worker");

        while !completed.load(std::sync::atomic::Ordering::SeqCst)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        if completed.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = worker.join();
        } else {
            super::cleanup_registry::register_pending(worker, state, completed);
        }
    }

    pub fn take_core(&self) -> Option<Arc<CoreService>> {
        self.core.lock().ok()?.take()
    }

    pub fn restore_core(&self, value: Arc<CoreService>) {
        if let Ok(mut slot) = self.core.lock() {
            if slot.is_none() {
                *slot = Some(value);
            }
        }
    }

    pub fn take_host(&self) -> Option<Arc<HostManager>> {
        self.host.lock().ok()?.take()
    }

    pub fn restore_host(&self, value: Arc<HostManager>) {
        if let Ok(mut slot) = self.host.lock() {
            if slot.is_none() {
                *slot = Some(value);
            }
        }
    }

    pub fn take_provider_port(&self) -> Option<Arc<dyn ProviderPort>> {
        self.provider_port.lock().ok()?.take()
    }

    pub fn restore_provider_port(&self, value: Arc<dyn ProviderPort>) {
        if let Ok(mut slot) = self.provider_port.lock() {
            if slot.is_none() {
                *slot = Some(value);
            }
        }
    }

    pub fn owner_slots_present(&self) -> bool {
        self.core.lock().ok().is_some_and(|g| g.is_some())
            || self.host.lock().ok().is_some_and(|g| g.is_some())
            || self.provider_port.lock().ok().is_some_and(|g| g.is_some())
    }

    /// NAPI8 environment cleanup: revoke generation and gate admissions.
    pub fn tombstone_env(&self) {
        self.env_dead.store(true, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.begin_close();
    }

    pub fn encode_principal(principal: &nexus_core::Principal, generation: u64) -> String {
        format!(
            "p:{}:{}:{}",
            generation,
            principal.creator_id(),
            principal.workspace_slug()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Wait for the race-free close signal, bounded.
    async fn wait_closed(mut rx: tokio::sync::watch::Receiver<bool>) -> bool {
        // Bind the result before returning: a tail expression's temporaries are
        // dropped after the locals they borrow.
        let waited = tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|closed| *closed)).await;
        waited.is_ok()
    }

    /// Q3-W3: the lost-wake shape. A close fires while no waiter is registered;
    /// `Notify::notified()` created afterwards would never resolve, so the
    /// versioned close signal must report the closed state to a late subscriber.
    #[tokio::test]
    async fn close_signal_is_observed_by_a_late_subscriber() {
        let state = EnvState::new();
        state.begin_close();

        let rx = state.close_watch();
        let late = tokio::time::timeout(Duration::from_millis(250), wait_closed(rx)).await;
        assert!(
            late.is_ok_and(|closed| closed),
            "a close that already happened must be observed, not lost"
        );

        // Contrast: a `Notify` waiter registered after the notify never fires.
        // This is the exact lost-wake the versioned signal replaces, and it shows
        // the assertion above is meaningful rather than trivially true.
        let notify_waiter = tokio::time::timeout(
            Duration::from_millis(250),
            state.close_notify.notified(),
        )
        .await;
        assert!(
            notify_waiter.is_err(),
            "the Notify-based waiter would be lost here — that is the bug being fixed"
        );
    }

    /// Q3-W3: admission racing close under contention. Subscribers register while
    /// the close fires; not one may miss it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn close_signal_races_subscribers_without_lost_wakes() {
        let state = Arc::new(EnvState::new());
        let mut handles = Vec::new();
        for _ in 0..64 {
            let state = state.clone();
            handles.push(tokio::spawn(async move {
                wait_closed(state.close_watch()).await
            }));
        }
        tokio::task::yield_now().await;
        state.begin_close();

        for handle in handles {
            assert!(
                handle.await.expect("join"),
                "no subscriber may miss the close signal"
            );
        }
    }

    /// The close signal is a LEVEL, not a pulse: reopening must clear it or every
    /// later callback would observe "closing" and be rejected (which is exactly
    /// what the callback-lifecycle suite caught).
    #[tokio::test]
    async fn reopen_clears_the_close_signal() {
        let state = EnvState::new();
        state.begin_close();
        state.publish_open().await;

        let rx = state.close_watch();
        assert!(
            rx.borrow().eq(&false),
            "a reopened environment must report open, not closing"
        );
        let observed = tokio::time::timeout(Duration::from_millis(150), wait_closed(rx)).await;
        assert!(
            observed.is_err(),
            "a reopened environment must not keep signalling closed"
        );

        // And a later close is still observed.
        state.begin_close();
        let rx_after = state.close_watch();
        let closed = tokio::time::timeout(Duration::from_secs(2), wait_closed(rx_after)).await;
        assert!(
            closed.is_ok_and(|closed| closed),
            "a close after reopen must still be observed"
        );
    }

    /// A pre-close subscriber observes the transition, not the initial value.
    #[tokio::test]
    async fn close_signal_wakes_a_pre_registered_subscriber() {
        let state = Arc::new(EnvState::new());
        let rx_owner = state.clone();
        let waiter = tokio::spawn(async move { wait_closed(rx_owner.close_watch()).await });
        tokio::task::yield_now().await;
        state.begin_close();
        assert!(
            waiter.await.expect("join"),
            "close must wake a registered waiter"
        );
    }

    fn op_finished(op_id: &str) -> nexus_contracts::provider_event_batch::NexusProviderHostEvent {
        use nexus_contracts::provider_event_batch::{
            NexusProviderHostEvent, ProviderEventBatchEventsItemOpFinishedReason,
        };
        NexusProviderHostEvent::OpFinished {
            op_id: op_id.to_string(),
            session_id: "s".to_string(),
            reason: ProviderEventBatchEventsItemOpFinishedReason::EndTurn,
        }
    }

    #[test]
    fn js_state_tracks_launch_execute_and_terminal_from_real_events() {
        let state = EnvState::new();
        assert!(state.try_reserve_js_session());
        state.record_js_session("sess-1".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-1", "op-1".to_string());
        // Active op is live and queryable with the admitted provider id.
        let session = state.with_js_state(|s| s.session("sess-1").cloned()).flatten().unwrap();
        assert_eq!(session.provider_id, "mock-acp");
        assert_eq!(session.active_operation_id.as_deref(), Some("op-1"));
        let op = state.with_js_state(|s| s.operation("op-1")).flatten().unwrap();
        assert_eq!(op.status.wire(), "running");
        // A real terminal event clears the active op and retains the terminal.
        state.apply_js_batch_terminals("op-1", &[op_finished("op-1")]);
        assert_eq!(
            state.with_js_state(|s| s.operation("op-1")).flatten().unwrap().status.wire(),
            "finished"
        );
        assert!(
            state
                .with_js_state(|s| s.session("sess-1").cloned())
                .flatten()
                .unwrap()
                .active_operation_id
                .is_none(),
            "terminal must clear the session's active op"
        );
    }

    #[test]
    fn js_cancel_ack_settles_the_operation_to_cancelled() {
        let state = EnvState::new();
        state.record_js_session("sess-2".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-2", "op-2".to_string());
        state.record_js_cancel_ack("sess-2");
        assert_eq!(
            state.with_js_state(|s| s.operation("op-2")).flatten().unwrap().status.wire(),
            "cancelled",
            "an accepted cancel must settle the op to the cancelled terminal"
        );
        // The active op is cleared, so the session is no longer busy.
        let session = state.with_js_state(|s| s.session("sess-2").cloned()).flatten().unwrap();
        assert!(session.active_operation_id.is_none());
        // A later terminal event must never downgrade the settled `cancelled`.
        state.record_js_operation_terminal("sess-2", "op-2", JsOperationStatus::Finished);
        assert_eq!(
            state.with_js_state(|s| s.operation("op-2")).flatten().unwrap().status.wire(),
            "cancelled",
            "a later terminal must not downgrade an already-cancelled op"
        );
    }

    #[test]
    fn js_session_stop_marks_active_op_interrupted_and_removes_session() {
        let state = EnvState::new();
        state.record_js_session("sess-3".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-3", "op-3".to_string());
        state.record_js_session_stopped("sess-3");
        assert_eq!(
            state.with_js_state(|s| s.operation("op-3")).flatten().unwrap().status.wire(),
            "interrupted"
        );
        assert!(state.with_js_state(|s| s.session("sess-3").cloned()).flatten().is_none());
    }

    #[test]
    fn js_terminal_operations_are_bounded_and_deduplicated() {
        let state = EnvState::new();
        state.record_js_session("sess-4".to_string(), "mock-acp".to_string());
        for i in 0..(JS_MAX_TERMINAL_OPERATIONS + 10) {
            let op = format!("op-{i}");
            state.record_js_operation_terminal("sess-4", &op, JsOperationStatus::Finished);
        }
        let retained = state
            .with_js_state(|s| s.terminal.len())
            .unwrap();
        assert_eq!(retained, JS_MAX_TERMINAL_OPERATIONS, "terminal history must be bounded");
        // A duplicate terminal upserts rather than appending.
        state.record_js_operation_terminal("sess-4", "op-5", JsOperationStatus::Failed);
        assert_eq!(state.with_js_state(|s| s.terminal.len()).unwrap(), JS_MAX_TERMINAL_OPERATIONS);
        assert_eq!(
            state.with_js_state(|s| s.operation("op-5")).flatten().unwrap().status.wire(),
            "failed"
        );
    }

    #[test]
    fn js_execute_for_unknown_session_is_not_fabricated() {
        let state = EnvState::new();
        state.record_js_session_operation("no-such-session", "op-x".to_string());
        assert!(
            state.with_js_state(|s| s.operation("op-x")).flatten().is_none(),
            "an execute for an unlaunched session must not create state"
        );
    }

    #[test]
    fn js_session_cap_is_enforced_by_reservation() {
        let state = EnvState::new();
        let mut reserved = 0;
        while state.try_reserve_js_session() {
            reserved += 1;
            assert!(reserved <= JS_PROVIDER_MAX_ACTIVE_SESSIONS);
        }
        assert_eq!(reserved, JS_PROVIDER_MAX_ACTIVE_SESSIONS);
        // Releasing a reservation frees exactly one slot.
        state.release_js_session_reservation();
        assert!(state.try_reserve_js_session());
    }
}
