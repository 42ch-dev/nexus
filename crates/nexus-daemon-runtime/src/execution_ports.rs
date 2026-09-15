//! Port adapters letting the extracted execution authority keep its daemon
//! collaborators without the core linking them (v1.190 P3-T1).
//!
//! The core's `execution` cohort links no agent host and no transport; these
//! adapters implement the core's `RunEventPort` / `ProviderCatalogPort` in
//! terms of the daemon's existing `RunEventRegistry` and `HostFacade`. They
//! are pure translation — no business logic, no second registry.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_core::execution::workflow::{ProviderCatalogPort, RunEventPort};
use nexus_orchestration::run_state::RunRecord;

use nexus_core::execution::run_events::{RunEventRegistry, RunEventSinkMap};

/// Adapts the daemon's bounded per-run SSE registry to the execution port.
///
/// Every operation forwards to the existing registry, so item/byte/subscriber
/// accounting, ring reuse, the shared sink map and terminal closing semantics
/// are unchanged — the execution layer only reserves, publishes and releases.
pub struct DaemonRunEventPort {
    registry: Arc<RunEventRegistry>,
    /// The same sink map the prompt executor was constructed with, so a
    /// reserved ring is visible to that executor's host events.
    sinks: RunEventSinkMap,
}

impl DaemonRunEventPort {
    #[must_use]
    pub fn new(registry: Arc<RunEventRegistry>, sinks: RunEventSinkMap) -> Self {
        Self { registry, sinks }
    }
}

#[async_trait]
impl RunEventPort for DaemonRunEventPort {
    async fn try_register_live(&self, run_id: &str) -> bool {
        let Some(sink) = self.registry.try_register_live(run_id) else {
            return false;
        };
        self.sinks.lock().await.insert(run_id.to_string(), sink);
        true
    }

    async fn remove_live(&self, run_id: &str) {
        self.sinks.lock().await.remove(run_id);
    }

    fn publish_run_state(&self, run_id: &str, record: &RunRecord) {
        self.registry.publish_run_state(run_id, record);
    }

    fn mark_terminal(&self, run_id: &str) {
        self.registry.mark_terminal(run_id);
    }

    fn read_page(
        &self,
        run_id: &str,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> Result<
        nexus_core::execution::run_events::RunPage,
        nexus_core::execution::run_events::PageError,
    > {
        // Forwards to the same bounded ring the SSE surface reads, so caps and
        // explicit-gap semantics stay identical across both readers.
        // The ring owns the page shape; the port forwards it verbatim so the
        // item/byte caps, the parsed sequences and the explicit-gap frames
        // cannot drift between the SSE reader and the typed reader.
        self.registry.read_page(run_id, after_sequence, limit)
    }
}

/// Adapts the daemon's Host provider catalog to the execution port.
///
/// The catalog read is the same one admission always performed; the port just
/// stops the execution layer from naming `HostFacade`/`ProviderId`.
pub struct DaemonProviderCatalogPort {
    host: Arc<dyn nexus_agent_host::HostFacade>,
}

impl DaemonProviderCatalogPort {
    #[must_use]
    pub fn new(host: Arc<dyn nexus_agent_host::HostFacade>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl ProviderCatalogPort for DaemonProviderCatalogPort {
    async fn provider_available(&self, provider_id: &str) -> Result<bool, String> {
        let catalog = self
            .host
            .provider_catalog()
            .await
            .map_err(|e| e.to_string())?;
        Ok(catalog
            .find(&nexus_agent_host::ProviderId::new(provider_id.to_string()))
            .is_some())
    }
}
