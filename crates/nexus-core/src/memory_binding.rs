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
    /// The established Connect session a remote binding serves through
    /// (§4.1). Only a successful negotiated, authority-pinned open constructs
    /// one, so `Some` here always means `port` is that adapter.
    #[cfg(feature = "memory-remote")]
    remote: Option<Arc<spoke_connect::remote::RemoteAdapter>>,
    /// The loaded public client identity the remote dial authenticated with
    /// (§5.1/D18): the Ed25519 public peer id, never a credential id,
    /// profile/path, seed hash or payload principal. `None` for local; R1
    /// pins it before exposing a remote binding, and a nonlocal binding
    /// without it fails closed.
    client_identity: Option<String>,
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
            client_identity: None,
            space_id: provider.space_id().to_string(),
            authority_host_id: LOCAL_AUTHORITY_HOST_ID.to_string(),
            authority_generation: LOCAL_AUTHORITY_GENERATION,
            port: Arc::clone(&provider) as Arc<dyn MemoryPort + Send + Sync>,
            local: Some(provider),
            #[cfg(feature = "memory-remote")]
            remote: None,
        }
    }

    /// Pin one established remote session (`n3-provider-binding.md` §4.1).
    ///
    /// The caller ([`crate::memory_remote`]) only reaches this after the signed
    /// hello, the negotiated `spoke-memory` family, `describe` v1, the exact
    /// configured space and the authority host all held, so the pinned pair and
    /// the client identity are the provider's own advertised values rather
    /// than defaults.
    #[cfg(feature = "memory-remote")]
    pub(crate) fn pin_remote(
        adapter: Arc<spoke_connect::remote::RemoteAdapter>,
        space_id: String,
        authority_host_id: String,
        authority_generation: NonZeroU64,
        client_identity: String,
        writable: bool,
    ) -> Self {
        Self {
            writable,
            client_identity: Some(client_identity),
            space_id,
            authority_host_id,
            authority_generation,
            port: Arc::clone(&adapter) as Arc<dyn MemoryPort + Send + Sync>,
            local: None,
            remote: Some(adapter),
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

    /// Close the provider handle this binding pins (§4.1 `close`).
    ///
    /// A local binding closes the one provider it owns, so the writer
    /// admission that provider opened ends with this open. A remote binding
    /// closes its established session through the adapter's own idempotent
    /// `close`; it never chooses another provider. Idempotent either way.
    pub(crate) async fn close(&self) {
        if let Some(local) = &self.local {
            local.close().await;
        }
        #[cfg(feature = "memory-remote")]
        if let Some(remote) = &self.remote {
            remote.close();
        }
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
    pub(crate) fn authority(&self) -> (&str, NonZeroU64) {
        (&self.authority_host_id, self.authority_generation)
    }

    /// The actual public client identity the remote dial authenticated with
    /// (§5.1/D18), or `None` for a local binding.
    ///
    /// This is the Ed25519 public peer id R1 derives from the loaded signing
    /// key's verifying key — not `credential_id`, a profile/path, a seed hash
    /// or any payload principal. Nexus dependency facts are namespaced by it,
    /// and a nonlocal binding without one fails closed instead of falling back
    /// to another identity.
    #[must_use]
    pub(crate) fn client_identity(&self) -> Option<&str> {
        self.client_identity.as_deref()
    }
}
