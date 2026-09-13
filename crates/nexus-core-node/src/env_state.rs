use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use nexus_core::{CoreService, Principal};
use nexus_provider_ports::ProviderPort;
use tokio::sync::Mutex;

pub struct EnvState {
    pub generation: AtomicU64,
    pub core: Mutex<Option<Arc<CoreService>>>,
    pub provider_port: Mutex<Option<Arc<dyn ProviderPort>>>,
    pub pending_provider: StdMutex<Option<Arc<dyn ProviderPort>>>,
    pub closing: AtomicU64,
}

impl EnvState {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(1),
            core: Mutex::new(None),
            provider_port: Mutex::new(None),
            pending_provider: StdMutex::new(None),
            closing: AtomicU64::new(0),
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst) != 0
    }

    pub fn begin_close(&self) {
        self.closing.store(self.generation.load(Ordering::SeqCst), Ordering::SeqCst);
    }

    pub fn encode_principal(principal: &Principal, generation: u64) -> String {
        format!(
            "p:{}:{}:{}",
            generation,
            principal.creator_id(),
            principal.workspace_slug()
        )
    }

    pub fn verify_principal_handle(&self, handle: &str) -> Result<(u64, String, String), String> {
        let parts: Vec<&str> = handle.split(':').collect();
        if parts.len() != 4 || parts[0] != "p" {
            return Err("invalid principal handle".to_string());
        }
        let gen = parts[1]
            .parse::<u64>()
            .map_err(|_| "invalid principal generation".to_string())?;
        if gen != self.generation.load(Ordering::SeqCst) {
            return Err("stale principal handle".to_string());
        }
        Ok((gen, parts[2].to_string(), parts[3].to_string()))
    }
}
