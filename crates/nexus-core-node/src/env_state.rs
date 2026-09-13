use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use napi::bindgen_prelude::Error;
use napi::Env;
use nexus_contracts::{CoreCloseReport, CoreError, CoreErrorCode};
use nexus_core::CoreService;
use nexus_provider_ports::ProviderPort;
use nexus_agent_host::HostManager;
use tokio::sync::{Mutex, Notify};
use nexus_agent_host::HostFacade;

pub const MAX_PENDING_BYTES_TOTAL: usize = 1024 * 1024;

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
                .compare_exchange_weak(current, current + bytes, Ordering::AcqRel, Ordering::Acquire)
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
    pub core: Mutex<Option<Arc<CoreService>>>,
    pub host: Mutex<Option<Arc<HostManager>>>,
    pub provider_port: Mutex<Option<Arc<dyn ProviderPort>>>,
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
}

impl EnvState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(1),
            core: Mutex::new(None),
            host: Mutex::new(None),
            provider_port: Mutex::new(None),
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
        }
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

    /// NAPI8 environment cleanup: tombstone and run bounded native-only teardown.
    pub fn run_bounded_native_finalize(state: Arc<EnvState>) {
        state.tombstone_env();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished_flag = finished.clone();
        let worker_state = state.clone();

        let worker = std::thread::spawn(move || {
            if let Ok(rt) = std::panic::catch_unwind(super::runtime::runtime) {
                let _ = rt.block_on(async move {
                    let core = worker_state.core.lock().await.take();
                    if let Some(service) = core {
                        let _ = service.close().await;
                    }
                    let host = worker_state.host.lock().await.take();
                    if let Some(host) = host {
                        host.localset_bridge().shutdown_sync();
                        let _ = host.shutdown().await;
                    }
                    let _port = worker_state.provider_port.lock().await.take();
                });
            }
            finished_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        while !finished.load(std::sync::atomic::Ordering::SeqCst)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        if !finished.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::spawn(move || {
                let _ = worker.join();
            });
        } else {
            let _ = worker.join();
        }
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
