//! The peer-control lane (v1.190 P4-T3).
//!
//! The enablement and the operation allowlist that together decide which
//! operations one execution owner admits, observed by the Connect surface.
//! Capability dispatch itself lives in [`crate::peer_tools`]: this module only
//! holds the lane's state, and never becomes a second registry.
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

/// The peer-control lane one execution owner admits: the enablement and the
/// operation allowlist that together decide admission, observed by the
/// connect surface.
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

    /// Whether the lane is enabled. Admission reads this beside the
    /// allowlist: a disabled lane admits only the operations that can report
    /// or re-enable it (`status`, `enable`) and refuses the rest — the
    /// deciding gate lives in `nexus-core`'s `peer_control` dispatcher, which
    /// is where the allowlist is consulted too.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Whether `operation` is on the allowlist. An empty allowlist admits
    /// nothing — operators must name the operations they intend — except the
    /// disabled-lane recovery pair (`status`, `enable`), which the dispatcher
    /// exempts so the lane can always be reported and restored.
    #[must_use]
    pub fn allows(&self, operation: &str) -> bool {
        self.allowed_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(operation)
    }

    /// Flip the lane's enablement. `false` stops every operation outside the
    /// reachable set (`status`, `enable`) from being admitted, and `true`
    /// restores them on the next admission — the effect is immediate, which
    /// is what makes `disable` a control rather than a report.
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
