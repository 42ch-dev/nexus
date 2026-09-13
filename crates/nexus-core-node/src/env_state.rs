use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use napi::bindgen_prelude::Error;
use napi::Env;
use nexus_contracts::{CoreCloseReport, CoreError, CoreErrorCode};
use nexus_core::CoreService;
use nexus_provider_ports::ProviderPort;
use nexus_agent_host::HostManager;
use tokio::sync::{Mutex, Notify};

pub const MAX_PENDING_BYTES_TOTAL: usize = 1024 * 1024;

/// Per-addon environment instance marker stored via `Env::set_instance_data`.
pub struct EnvInstance {
    pub state: Arc<EnvState>,
}

impl EnvInstance {
    pub fn install(env: &Env, state: Arc<EnvState>) -> napi::Result<()> {
        env.set_instance_data(EnvInstance { state }, (), |ctx| {
            if let Ok(Some(instance)) = ctx.env.get_instance_data::<EnvInstance>() {
                instance.state.tombstone_env();
                instance.state.close_notify.notify_waiters();
            }
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
    /// A rollback or close retained an owner whose cleanup is unconfirmed.
    /// Blocks new opens until a confirmed settlement releases it.
    pub interrupted: AtomicBool,
    /// Environment tombstoned after NAPI cleanup; no further JS calls.
    pub env_dead: AtomicBool,
    /// Concurrent close callers await one settlement.
    pub close_notify_settled: Arc<Notify>,
    pub settled_close: Mutex<Option<CoreCloseReport>>,
    pub close_in_flight: Mutex<bool>,
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
            interrupted: AtomicBool::new(false),
            env_dead: AtomicBool::new(false),
            close_notify_settled: Arc::new(Notify::new()),
            settled_close: Mutex::new(None),
            close_in_flight: Mutex::new(false),
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst) != 0
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
