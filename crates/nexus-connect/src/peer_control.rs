//! The peer-control lane (v1.190 P4-T3).
//!
//! Enablement plus the operation allowlist one execution owner admits,
//! observed by the Connect surface. Capability dispatch itself lives in
//! [`crate::peer_tools`]: this module only holds the lane's state, and never
//! becomes a second registry.
//!
//! The `ExecutionHandle` entry points that own a lane (`start_peer_control`,
//! `peer_control`) stay in `nexus-core`'s `execution/handle_ops.rs` — Rust
//! cannot write an inherent impl for a foreign type. The four lane operations
//! they need across the boundary ([`PeerControlLane::new`],
//! [`PeerControlLane::set_enabled`], [`PeerControlLane::allow`],
//! [`PeerControlLane::state`]) are public for exactly that reason.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use nexus_contracts::generated::core::CorePeerControlState;

/// The peer-control lane one execution owner admits: enablement plus the
/// operation allowlist, observed by the connect surface.
#[derive(Debug)]
pub struct PeerControlLane {
    enabled: AtomicBool,
    allowed_operations: Mutex<HashSet<String>>,
}

impl PeerControlLane {
    /// A lane with the given boot enablement and allowlist. An empty
    /// allowlist admits nothing.
    #[must_use]
    pub const fn new(enabled: bool, allowed_operations: HashSet<String>) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            allowed_operations: Mutex::new(allowed_operations),
        }
    }

    /// Whether the lane currently admits peer-control operations.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Whether `operation` is on the allowlist. An empty allowlist admits
    /// nothing — operators must name the operations they intend.
    #[must_use]
    pub fn allows(&self, operation: &str) -> bool {
        self.allowed_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(operation)
    }

    /// Flip enablement (a disabled lane refuses every operation).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Add one operation to the allowlist.
    pub fn allow(&self, operation: &str) {
        self.allowed_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(operation.to_owned());
    }

    /// The lane's wire state, carrying `active_peers` verbatim (ids that fail
    /// the wire grammar are dropped, as before).
    #[must_use]
    pub fn state(&self, active_peers: Vec<String>) -> CorePeerControlState {
        CorePeerControlState {
            enabled: self.enabled(),
            active_peers: active_peers
                .into_iter()
                .filter_map(|id| {
                    nexus_contracts::generated::core::CorePeerControlStateActivePeersItem::try_from(
                        id,
                    )
                    .ok()
                })
                .collect(),
        }
    }
}
