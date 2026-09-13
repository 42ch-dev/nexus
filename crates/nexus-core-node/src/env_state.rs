use std::collections::BTreeMap;
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
    pub close_notify: Notify,
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
    /// Sessions the JS provider reported as live, mapped to their in-flight
    /// operation id (if any).
    ///
    /// The JS adapter owns its ACP child processes; native close must drive
    /// their cancellation and release through the live callback bridge before
    /// the admission fence rises, or an owned child leaks. Empty for
    /// Rust-owned providers.
    pub js_sessions: StdMutex<BTreeMap<String, Option<String>>>,
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
            closing: AtomicU64::new(0),
            lifecycle: StdMutex::new(EnvLifecyclePhase::Closed),
            interrupted: AtomicBool::new(false),
            env_dead: AtomicBool::new(false),
            close_notify_settled: Arc::new(Notify::new()),
            settled_close: Mutex::new(None),
            close_in_flight: Mutex::new(false),
            pending_operation_ids: Mutex::new(Vec::new()),
            js_sessions: StdMutex::new(BTreeMap::new()),
        }
    }

    /// Remember a JS-provider session so close can drive its owned release.
    pub fn record_js_session(&self, session_id: String) {
        if let Ok(mut sessions) = self.js_sessions.lock() {
            sessions.entry(session_id).or_insert(None);
        }
    }

    /// Track the session's in-flight operation so close can cancel it first.
    pub fn record_js_session_operation(&self, session_id: &str, operation_id: String) {
        if let Ok(mut sessions) = self.js_sessions.lock() {
            sessions.insert(session_id.to_string(), Some(operation_id));
        }
    }

    /// The session's operation reached a terminal: nothing left to cancel.
    pub fn clear_js_session_operation(&self, session_id: &str) {
        if let Ok(mut sessions) = self.js_sessions.lock() {
            if let Some(entry) = sessions.get_mut(session_id) {
                *entry = None;
            }
        }
    }

    /// Forget a JS-provider session whose owned cleanup was confirmed.
    pub fn forget_js_session(&self, session_id: &str) {
        if let Ok(mut sessions) = self.js_sessions.lock() {
            sessions.remove(session_id);
        }
    }

    /// Live JS-provider sessions awaiting an owned release.
    pub fn js_sessions_snapshot(&self) -> Vec<(String, Option<String>)> {
        self.js_sessions
            .lock()
            .map(|sessions| {
                sessions
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
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
    }

    pub async fn begin_closing_phase(&self) {
        *self.lifecycle.lock().expect("lifecycle mutex poisoned") = EnvLifecyclePhase::Closing;
    }

    pub async fn publish_closed(&self) {
        *self.lifecycle.lock().expect("lifecycle mutex poisoned") = EnvLifecyclePhase::Closed;
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
                                    // Owned sessions confirmed clean; the host is
                                    // released together with the finalize worker.
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
