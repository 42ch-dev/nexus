//! The pinned memory provider authority for one `CoreService` open
//! (`n3-provider-binding.md` §4.1).
//!
//! One open pins exactly one binding. Every memory handler and the Character
//! mind projection reach the same provider instance, space and authority, and
//! a live open never re-reads or retargets its selection.

use std::num::NonZeroU64;
use std::sync::Arc;

use spoke_operations::MemoryPort;

use crate::error::{CoreError, CoreResult};
use crate::memory_provider::{LocalMemoryProvider, LOCAL_AUTHORITY_HOST_ID};

/// The local provider provisions its one space at generation `1`
/// (`storage.rs::provision` inserts it with `INSERT OR IGNORE` and never bumps
/// it), so the pinned local authority is that space's own constant generation.
const LOCAL_AUTHORITY_GENERATION: NonZeroU64 = NonZeroU64::MIN;

/// The single provider authority one `CoreService` open serves memory from.
pub(crate) struct MemoryProviderBinding {
    port: Arc<dyn MemoryPort + Send + Sync>,
    local: Option<Arc<LocalMemoryProvider>>,
    space_id: String,
    authority_host_id: String,
    authority_generation: NonZeroU64,
    writable: bool,
}

impl MemoryProviderBinding {
    /// Pin the local provider: its one default space is the authority.
    pub(crate) fn pin_local(provider: Arc<LocalMemoryProvider>) -> Self {
        Self {
            writable: provider.is_writable(),
            space_id: provider.space_id().to_string(),
            authority_host_id: LOCAL_AUTHORITY_HOST_ID.to_string(),
            authority_generation: LOCAL_AUTHORITY_GENERATION,
            port: Arc::clone(&provider) as Arc<dyn MemoryPort + Send + Sync>,
            local: Some(provider),
        }
    }

    /// The `MemoryPort` every standard operation runs through.
    #[allow(dead_code)] // Task 4 routes the adapter's generic operations here.
    pub(crate) fn port(&self) -> &(dyn MemoryPort + Send + Sync) {
        self.port.as_ref()
    }

    /// The provider space this open serves.
    #[allow(dead_code)] // Task 4 replaces the local default constant.
    pub(crate) fn space_id(&self) -> &str {
        &self.space_id
    }

    /// The provider-local concrete handle, for local-only internals.
    ///
    /// # Errors
    ///
    /// Returns an internal refusal when this binding does not serve the local
    /// provider; local-only code must never reach a remote binding.
    pub(crate) fn local(&self) -> CoreResult<&Arc<LocalMemoryProvider>> {
        self.local.as_ref().ok_or_else(|| CoreError::Internal {
            category: "memory binding is not local".to_string(),
        })
    }

    /// Whether this open may mutate provider state.
    #[must_use]
    #[allow(dead_code)] // Task 4 gates the local-only branches on it.
    pub(crate) fn is_writable(&self) -> bool {
        self.writable
    }

    /// The pinned `(authority_host_id, authority_generation)` pair.
    #[must_use]
    #[allow(dead_code)] // exposed for Task 4 assertions and the remote cutover.
    pub(crate) fn authority(&self) -> (&str, NonZeroU64) {
        (&self.authority_host_id, self.authority_generation)
    }
}
