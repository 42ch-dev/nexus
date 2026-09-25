use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use napi::bindgen_prelude::Error;
use napi::Env;
use nexus_contracts::{CoreCloseReport, CoreError, CoreErrorCode};
use nexus_core::{CoreService, HostHandle};
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
    /// The Host lifetime slot: the ONE attached core authority (its manager,
    /// Actor registry and composed provider port), never a bare manager. The
    /// core authority owns every Actor effect/observation/close path; native
    /// only borrows its manager for readiness and hosted execution.
    pub host: StdMutex<Option<Arc<HostHandle>>>,
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

    /// Record an operation terminal observed from a real provider event batch,
    /// or an accepted cooperative cancel: the cancel path settles exactly the
    /// requested operation id (never whichever operation happens to be active
    /// for the session). Clears the session's active op and upserts a bounded
    /// terminal record.
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

    /// Clone the open core for the effect-boundary journal seam (P4-T2). The
    /// durable JS-provider journal is owned by [`CoreService`]; no pool handle
    /// is stored or exposed on the environment any more.
    /// Project a domain [`nexus_core::CoreError`] onto the wire taxonomy at
    /// the environment boundary. The journal-relevant arms keep their honest
    /// codes; anything else is the bounded internal fallback.
    fn wire_domain_error(err: nexus_core::CoreError) -> CoreError {
        use nexus_core::CoreError as Domain;
        let (code, http_status) = match &err {
            Domain::Closing => (CoreErrorCode::Closing, Some(503)),
            Domain::Busy | Domain::OwnerBusy => (CoreErrorCode::Busy, Some(503)),
            Domain::Forbidden { .. } | Domain::ForbiddenReason { .. } => {
                (CoreErrorCode::Forbidden, Some(403))
            }
            Domain::WriterFenced => (CoreErrorCode::WriterFenced, Some(409)),
            Domain::SchemaMismatch => (CoreErrorCode::SchemaMismatch, Some(409)),
            _ => (CoreErrorCode::Internal, Some(500)),
        };
        CoreError {
            code,
            message: err.to_string(),
            details: Default::default(),
            http_status,
        }
    }

    pub(crate) fn journal_core(&self) -> Option<Arc<CoreService>> {
        self.core.lock().ok().and_then(|slot| slot.clone())
    }

    /// Journal an operation write-through at the provider boundary through
    /// the [`CoreService`]-owned journal (LIFE-3). The in-memory state
    /// remains authoritative for this process, but the durable mirror is part
    /// of the effect contract: a failed write is returned to the caller,
    /// never discarded. With no open core (service-only shell) there is no
    /// journal and nothing to mirror.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the journal upsert fails.
    pub async fn journal_operation(
        &self,
        operation_id: &str,
        session_id: &str,
        provider_id: &str,
        status: &str,
    ) -> Result<(), CoreError> {
        let Some(core) = self.journal_core() else {
            return Ok(());
        };
        core.journal_provider_write_internal(operation_id, session_id, provider_id, status)
            .await
            .map_err(Self::wire_domain_error)
    }

    /// Inspect the actual delivered provider events WITHOUT mutating state:
    /// `OpFinished` → `Finished`, `OpFailed` → `Failed`, `SessionStopped` →
    /// `Interrupted`. The terminal is derived from the real batch, never a
    /// caller label. Returns the terminal status plus the owning session so
    /// the caller can journal first (LIFE-3) and only then apply the terminal
    /// in memory: a failed journal write retains the consumed batch in the
    /// admitting wrapper for exactly-one re-delivery after the journal retry
    /// succeeds, never a terminal memory without its durable mirror.
    pub fn detect_js_batch_terminal(
        &self,
        operation_id: &str,
        events: &[nexus_contracts::provider_event_batch::NexusProviderHostEvent],
    ) -> Option<(JsOperationStatus, String)> {
        let session_id = self
            .with_js_state(|state| state.operation(operation_id).map(|op| op.session_id))
            .flatten()?;
        use nexus_contracts::provider_event_batch::NexusProviderHostEvent as BatchEvent;
        let mut terminal = None;
        for event in events {
            match event {
                BatchEvent::OpFinished { .. } => {
                    terminal = Some((JsOperationStatus::Finished, session_id.clone()));
                }
                BatchEvent::OpFailed { .. } => {
                    terminal = Some((JsOperationStatus::Failed, session_id.clone()));
                }
                BatchEvent::SessionStopped { .. } => {
                    terminal = Some((JsOperationStatus::Interrupted, session_id.clone()));
                }
                _ => {}
            }
        }
        terminal
    }

    /// Apply a previously detected batch terminal to in-memory truth. Called
    /// only after the durable mirror succeeded, so memory and journal always
    /// agree on the one terminal outcome.
    pub fn apply_js_batch_terminal(
        &self,
        session_id: &str,
        operation_id: &str,
        status: JsOperationStatus,
    ) {
        match status {
            JsOperationStatus::Interrupted => self.record_js_session_stopped(session_id),
            JsOperationStatus::Finished | JsOperationStatus::Failed => {
                self.record_js_operation_terminal(session_id, operation_id, status);
            }
            // Batch events never derive these; only the cancel path settles
            // `Cancelled` and `Running` is the pre-terminal state.
            JsOperationStatus::Running | JsOperationStatus::Cancelled => {}
        }
    }

    /// Journal an operation's terminal status durably through the
    /// [`CoreService`]-owned journal (LIFE-3).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the journal upsert fails.
    pub async fn journal_operation_status(
        &self,
        operation_id: &str,
        status: &str,
    ) -> Result<(), CoreError> {
        let Some(core) = self.journal_core() else {
            return Ok(());
        };
        // The operation record keeps its launch identity: the status write
        // carries the real owning session and provider id, never blanks that
        // would clobber the stored identity columns.
        let (session_id, provider_id) = self
            .with_js_state(|s| {
                s.operation(operation_id).and_then(|op| {
                    s.session(&op.session_id)
                        .map(|rec| (op.session_id.clone(), rec.provider_id.clone()))
                })
            })
            .flatten()
            .unwrap_or_default();
        core.journal_provider_write_internal(operation_id, &session_id, &provider_id, status)
            .await
            .map_err(Self::wire_domain_error)
    }

    /// Drop journaled operations for a cleanly shut-down session through the
    /// [`CoreService`]-owned journal.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the delete fails.
    pub async fn forget_journal_session(&self, session_id: &str) -> Result<(), CoreError> {
        let Some(core) = self.journal_core() else {
            return Ok(());
        };
        core.forget_provider_session_internal(session_id)
            .await
            .map_err(Self::wire_domain_error)
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
        self.service_only_uninitialized
            .store(true, Ordering::SeqCst);
    }

    pub fn clear_service_only_uninitialized(&self) {
        self.service_only_uninitialized
            .store(false, Ordering::SeqCst);
    }

    /// NAPI8 environment cleanup: tombstone and run bounded native-only teardown.
    ///
    /// The legs run in technical contract §3 order, same as the JS-initiated
    /// close: the attached authority's Actor-only quiesce FIRST (while the
    /// core is still open), then the existing core/execution owner — withheld
    /// entirely when the quiesce did not confirm, so core storage never closes
    /// ahead of an admitted Actor effect's drain — and only then this
    /// authority's single `close_before` settlement (Actor drains +
    /// manager/`LocalSet`), withheld whenever the ordered close is
    /// unconfirmed. `cleanup_confirmed` needs all three settlements, so no leg
    /// releases its owner off an unconfirmed result. Dead-environment cleanup
    /// never enters a JS callback: both Host legs run on the native Host
    /// manager.
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

                        // Technical contract §3 order, same as the JS-initiated
                        // close: the Actor quiesce runs while the core is still
                        // open, so core storage cannot close ahead of an
                        // admitted Actor effect's drain. Dead-environment
                        // cleanup enters no JS callback: the quiesce's cancels
                        // run on the native Host manager, exactly like the
                        // `close_before` settlement below.
                        let host = work_state.take_host();
                        let mut quiesce_pending: Vec<String> = Vec::new();
                        let actor_quiesce_confirmed = match host.as_ref() {
                            None => true,
                            Some(authority) => {
                                if budget().is_zero() {
                                    quiesce_pending.push("actor-quiesce-deadline".to_string());
                                    false
                                } else {
                                    // Bounded by the same absolute deadline: the
                                    // joins inside are deliberately unbounded
                                    // (the authority retains its drain
                                    // ownership), so an expired budget cancels
                                    // this caller and keeps the whole
                                    // authority instead of detaching live work.
                                    match tokio::time::timeout_at(
                                        tokio::time::Instant::from_std(deadline),
                                        authority.quiesce_actor_sessions(),
                                    )
                                    .await
                                    {
                                        Ok(Ok(report)) => {
                                            let confirmed = report.cleanup_confirmed;
                                            quiesce_pending.extend(report.pending_operations);
                                            confirmed
                                        }
                                        Ok(Err(err)) => {
                                            quiesce_pending.push(format!("actor-quiesce: {err}"));
                                            false
                                        }
                                        Err(_) => {
                                            quiesce_pending
                                                .push("actor-quiesce-deadline".to_string());
                                            false
                                        }
                                    }
                                }
                            }
                        };

                        let core = work_state.take_core();
                        let mut core_pending: Vec<String> = quiesce_pending;
                        let core_released = match core {
                            None => true,
                            Some(service) => {
                                if !actor_quiesce_confirmed {
                                    // Order from technical contract §3: an
                                    // Actor quiesce that did not confirm leaves
                                    // an admitted operation that may still
                                    // register the drain the quiesce's join
                                    // bounds, so core storage must not close
                                    // ahead of it. The owner is restored (the
                                    // environment reports the withheld close)
                                    // and the authority above stays retained
                                    // for the retry's ordered close.
                                    core_pending.push(
                                        "core-close-withheld: actor-quiesce-unconfirmed"
                                            .to_string(),
                                    );
                                    work_state.restore_core(service);
                                    false
                                } else if budget().is_zero() {
                                    work_state.restore_core(service);
                                    false
                                } else {
                                    let settled = tokio::time::timeout_at(
                                        tokio::time::Instant::from_std(deadline),
                                        service.close(),
                                    )
                                    .await;
                                    // `cleanup_confirmed` needs the existing
                                    // core/JS-owner settlement too (contract
                                    // §3): an unconfirmed report retains the
                                    // owner instead of reading `Ok` as success.
                                    let confirmed = match settled {
                                        Ok(Ok(report)) => {
                                            let confirmed = report.cleanup_confirmed;
                                            core_pending.extend(report.pending_operations);
                                            confirmed
                                        }
                                        Ok(Err(err)) => {
                                            core_pending.push(format!("core-close: {err}"));
                                            false
                                        }
                                        Err(_) => {
                                            core_pending.push("core-close-deadline".to_string());
                                            false
                                        }
                                    };
                                    if confirmed {
                                        true
                                    } else {
                                        work_state.restore_core(service);
                                        false
                                    }
                                }
                            }
                        };

                        let mut host_pending: Vec<String> = Vec::new();
                        let ordered_close_unconfirmed = !actor_quiesce_confirmed || !core_released;
                        let host_released = match host {
                            None => true,
                            Some(authority) => {
                                let released = if ordered_close_unconfirmed {
                                    // The ordered close has not reached this
                                    // authority's settlement: it owns the Actor
                                    // drains a retry must re-join, so both it
                                    // and its manager/`LocalSet` stay retained.
                                    host_pending.push(
                                        "host-close-withheld: ordered-close-unconfirmed"
                                            .to_string(),
                                    );
                                    false
                                } else if budget().is_zero() {
                                    host_pending.push("host-close-deadline".to_string());
                                    false
                                } else {
                                    // The attached authority settles the Actor
                                    // drains and the manager/`LocalSet` ONCE
                                    // (contract §3): the finalizer never runs a
                                    // second shutdown of that manager, and it
                                    // never enters the Actor cancel path that
                                    // could call a dead JS callback.
                                    //
                                    // Not wrapped in an outer timeout: the
                                    // settlement honors this same absolute
                                    // deadline itself, and wrapping it would
                                    // let the wrapper cancel a join that has
                                    // already taken the `LocalSet` thread
                                    // handle — which would detach the thread
                                    // instead of handing it to tracked cleanup.
                                    match authority.close_before(deadline).await {
                                        Ok(report) => {
                                            let confirmed = report.cleanup_confirmed;
                                            host_pending.extend(report.pending_operations);
                                            confirmed
                                        }
                                        Err(err) => {
                                            host_pending.push(format!("host-close: {err}"));
                                            false
                                        }
                                    }
                                };
                                // A join that could not settle inside the budget
                                // hands its `JoinHandle` to tracked cleanup
                                // straight away, so a retained authority never
                                // leaves a detached `LocalSet` thread behind.
                                if let Some(handle) = authority
                                    .manager()
                                    .localset_bridge()
                                    .take_retained_runtime_thread()
                                {
                                    super::cleanup_registry::register_localset_thread(
                                        handle,
                                        work_state.clone(),
                                    );
                                }
                                if released {
                                    true
                                } else {
                                    work_state.restore_host(authority);
                                    false
                                }
                            }
                        };

                        if core_released && host_released {
                            // Owned sessions confirmed clean: the process
                            // reaper is stopped inside the remaining budget so
                            // it cannot outlive this finalize.
                            let stop = super::cleanup_registry::stop_reaper(deadline);
                            if !stop.joined || stop.pending_entries > 0 {
                                let mut pending = work_state.pending_operations_snapshot().await;
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
                            let mut pending = work_state.pending_operations_snapshot().await;
                            if !core_released {
                                pending.push("core-close-unconfirmed".to_string());
                            }
                            pending.extend(core_pending);
                            pending.extend(host_pending);
                            work_state.record_pending_operations(pending).await;
                            work_state.mark_interrupted();
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

    pub fn take_host(&self) -> Option<Arc<HostHandle>> {
        self.host.lock().ok()?.take()
    }

    pub fn restore_host(&self, value: Arc<HostHandle>) {
        if let Ok(mut slot) = self.host.lock() {
            if slot.is_none() {
                *slot = Some(value);
            }
        }
    }

    /// The attached Host authority retained as this environment's Host
    /// lifetime slot, or `None` when no owner is established.
    #[must_use]
    pub fn host_authority(&self) -> Option<Arc<HostHandle>> {
        self.host.lock().ok()?.clone()
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
        let waited =
            tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|closed| *closed)).await;
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
        let notify_waiter =
            tokio::time::timeout(Duration::from_millis(250), state.close_notify.notified()).await;
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
        let session = state
            .with_js_state(|s| s.session("sess-1").cloned())
            .flatten()
            .unwrap();
        assert_eq!(session.provider_id, "mock-acp");
        assert_eq!(session.active_operation_id.as_deref(), Some("op-1"));
        let op = state
            .with_js_state(|s| s.operation("op-1"))
            .flatten()
            .unwrap();
        assert_eq!(op.status.wire(), "running");
        // A real terminal event clears the active op and retains the terminal:
        // detected without mutating, then applied — the durable mirror runs
        // between the two in production (LIFE-3).
        let (status, session_id) = state
            .detect_js_batch_terminal("op-1", &[op_finished("op-1")])
            .expect("the batch carries a terminal");
        assert_eq!(session_id, "sess-1");
        assert_eq!(status.wire(), "finished");
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-1"))
                .flatten()
                .unwrap()
                .status
                .wire(),
            "running",
            "detection alone must not mutate the operation"
        );
        state.apply_js_batch_terminal(&session_id, "op-1", status);
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-1"))
                .flatten()
                .unwrap()
                .status
                .wire(),
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
    fn js_cancel_settles_the_targeted_operation_not_the_session_active_op() {
        let state = EnvState::new();
        state.record_js_session("sess-2".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-2", "op-y".to_string());
        // An accepted cancel for `op-x` while `op-y` is the session's active
        // operation settles exactly `op-x` and never touches `op-y`.
        state.record_js_operation_terminal("sess-2", "op-x", JsOperationStatus::Cancelled);
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-x"))
                .flatten()
                .unwrap()
                .status
                .wire(),
            "cancelled",
            "an accepted cancel must settle the requested operation id"
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-y"))
                .flatten()
                .unwrap()
                .status
                .wire(),
            "running",
            "cancel(op-x) must not mark the session's active op op-y cancelled"
        );
        // Active-op clearing is preserved only when it matches the targeted op.
        let session = state
            .with_js_state(|s| s.session("sess-2").cloned())
            .flatten()
            .unwrap();
        assert_eq!(session.active_operation_id.as_deref(), Some("op-y"));
        // A later terminal event must never downgrade the settled `cancelled`.
        state.record_js_operation_terminal("sess-2", "op-x", JsOperationStatus::Finished);
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-x"))
                .flatten()
                .unwrap()
                .status
                .wire(),
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
            state
                .with_js_state(|s| s.operation("op-3"))
                .flatten()
                .unwrap()
                .status
                .wire(),
            "interrupted"
        );
        assert!(state
            .with_js_state(|s| s.session("sess-3").cloned())
            .flatten()
            .is_none());
    }

    #[test]
    fn js_terminal_operations_are_bounded_and_deduplicated() {
        let state = EnvState::new();
        state.record_js_session("sess-4".to_string(), "mock-acp".to_string());
        for i in 0..(JS_MAX_TERMINAL_OPERATIONS + 10) {
            let op = format!("op-{i}");
            state.record_js_operation_terminal("sess-4", &op, JsOperationStatus::Finished);
        }
        let retained = state.with_js_state(|s| s.terminal.len()).unwrap();
        assert_eq!(
            retained, JS_MAX_TERMINAL_OPERATIONS,
            "terminal history must be bounded"
        );
        // A duplicate terminal upserts rather than appending.
        state.record_js_operation_terminal("sess-4", "op-5", JsOperationStatus::Failed);
        assert_eq!(
            state.with_js_state(|s| s.terminal.len()).unwrap(),
            JS_MAX_TERMINAL_OPERATIONS
        );
        assert_eq!(
            state
                .with_js_state(|s| s.operation("op-5"))
                .flatten()
                .unwrap()
                .status
                .wire(),
            "failed"
        );
    }

    #[test]
    fn js_execute_for_unknown_session_is_not_fabricated() {
        let state = EnvState::new();
        state.record_js_session_operation("no-such-session", "op-x".to_string());
        assert!(
            state
                .with_js_state(|s| s.operation("op-x"))
                .flatten()
                .is_none(),
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

    /// Seed a minimal nexus home + workspace DB and open a real
    /// engine-owner [`CoreService`] installed on the env, so the journal
    /// tests exercise the P4-T2 CoreService-owned journal seam end to end.
    async fn open_env_with_core() -> (tempfile::TempDir, Arc<EnvState>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let user_home = tmp.path().to_path_buf();
        let nexus_home = nexus_home_layout::nexus_root_from_home(&user_home);
        std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home,
            "creator-journal",
            "default",
        ))
        .expect("workspace dir");
        std::fs::write(
            nexus_home.join("config.toml"),
            "active_creator_id = \"creator-journal\"\n\
             [active_workspace_slug_by_creator]\n\
             \"creator-journal\" = \"default\"",
        )
        .expect("config");
        let core = nexus_core::CoreService::open(nexus_core::CoreOpenOptions {
            user_home,
            access: nexus_core::CoreAccess::EngineOwner,
        })
        .await
        .expect("core open");
        let state = Arc::new(EnvState::new());
        state
            .core
            .lock()
            .expect("core mutex")
            .replace(Arc::new(core));
        (tmp, state)
    }

    /// A core whose journal storage can no longer accept writes: every write
    /// fails for real, and the Result must say so — LIFE-3 is never silently
    /// best-effort.
    #[tokio::test]
    async fn journal_failures_are_propagated_not_discarded() {
        let (_tmp, state) = open_env_with_core().await;
        let core = state.journal_core().expect("core installed");
        // Fail the journal by closing the owning service behind the env's
        // back; the owned seam must surface the failure, not swallow it.
        core.close().await.expect("close");
        assert!(
            state
                .journal_operation("op-x", "sess-x", "mock-acp", "running")
                .await
                .is_err(),
            "a failed running upsert must be observable"
        );
        assert!(
            state
                .journal_operation_status("op-x", "cancelled")
                .await
                .is_err(),
            "a failed terminal upsert must be observable"
        );
        assert!(
            state.forget_journal_session("sess-x").await.is_err(),
            "a failed clean-session forget must be observable"
        );
        assert!(
            core.settle_provider_orphans().await.is_err(),
            "a failed open settlement must fail the open path"
        );
    }

    /// Success behavior is unchanged: with the real admitted engine core the
    /// same calls return Ok and the journal reflects the writes.
    #[tokio::test]
    async fn journal_success_path_still_returns_ok() {
        let (_tmp, state) = open_env_with_core().await;
        let core = state.journal_core().expect("core installed");
        state.record_js_session("sess-ok".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-ok", "op-ok".to_string());

        state
            .journal_operation("op-ok", "sess-ok", "mock-acp", "running")
            .await
            .expect("running upsert succeeds");
        state
            .journal_operation_status("op-ok", "cancelled")
            .await
            .expect("terminal upsert succeeds");
        // A cancelled operation is already terminal: settlement has nothing to
        // settle and reports zero affected rows, not an error.
        let settled = core.settle_provider_orphans().await.expect("settlement");
        assert_eq!(settled, 0);
        state
            .forget_journal_session("sess-ok")
            .await
            .expect("clean-session forget succeeds");
    }
}
