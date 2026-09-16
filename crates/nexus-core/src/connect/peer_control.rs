//! `ExecutionHandle` peer-control operations (v1.190 P4-T3).
//!
//! The typed entry points the transport calls for peer-control state; the
//! handle owns at most one [`PeerControlLane`]. Capability dispatch itself
//! stays in P3 (`crate::execution::peer_tools`): this module only installs
//! and reports the lane's enablement/allowlist, and never becomes a second
//! registry.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nexus_contracts::generated::core::{
    CorePeerControlOptions, CorePeerControlRequest, CorePeerControlState,
};

use crate::error::{CoreError, CoreResult};
use crate::execution::ExecutionHandle;

/// The peer-control lane one execution owner admits: enablement plus the
/// operation allowlist, observed by the connect surface.
#[derive(Debug)]
pub struct PeerControlLane {
    enabled: AtomicBool,
    allowed_operations: Mutex<HashSet<String>>,
}

impl PeerControlLane {
    const fn new(enabled: bool, allowed_operations: HashSet<String>) -> Self {
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

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    fn allow(&self, operation: &str) {
        self.allowed_operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(operation.to_owned());
    }

    fn state(&self, active_peers: Vec<String>) -> CorePeerControlState {
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

impl ExecutionHandle {
    /// Install (or disable) this execution owner's peer-control lane
    /// (P4-T3). Enablement is explicit: `enabled: false` tears the lane down
    /// to a disabled state; an enabled lane starts with exactly the named
    /// `allowed_operations` (empty admits nothing).
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the owning service is closed and
    /// [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification.
    pub async fn start_peer_control(
        &self,
        principal: &crate::principal::Principal,
        request: CorePeerControlOptions,
    ) -> CoreResult<CorePeerControlState> {
        let core = self.linked_core()?;
        core.verify_principal(principal)?;
        let allowed: HashSet<String> = request
            .allowed_operations
            .into_iter()
            .map(|item| item.to_string())
            .collect();
        let mut slot = self
            .peer_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let lane = if let Some(lane) = slot.as_ref() {
            Arc::clone(lane)
        } else {
            let lane = Arc::new(PeerControlLane::new(false, HashSet::new()));
            *slot = Some(Arc::clone(&lane));
            lane
        };
        drop(slot);
        lane.set_enabled(request.enabled);
        if request.enabled {
            for operation in allowed {
                lane.allow(&operation);
            }
        }
        Ok(lane.state(active_peer_ids()))
    }

    /// Drive one peer-control operation against this owner's lane (P4-T3).
    /// `status` reports the lane state; `enable`/`disable` flip enablement;
    /// `allow` adds one operation to the allowlist; `evict` removes a peer's
    /// registered tools. Anything else is `invalid_input`, and every
    /// operation not on the lane's allowlist is refused before any effect —
    /// visibility of a peer tool never implies authority to control it.
    ///
    /// # Errors
    /// As [`Self::start_peer_control`], plus [`CoreError::InvalidInput`]
    /// for an unknown operation and [`CoreError::Forbidden`] when the
    /// operation is not allowlisted.
    pub async fn peer_control(
        &self,
        principal: &crate::principal::Principal,
        request: CorePeerControlRequest,
    ) -> CoreResult<CorePeerControlState> {
        let core = self.linked_core()?;
        core.verify_principal(principal)?;
        let lane = {
            let slot = self
                .peer_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            slot.as_ref().map(Arc::clone)
        }
        .ok_or_else(|| CoreError::InvalidInput {
            field: "peer_id".into(),
            reason: "peer control has not been started".into(),
        })?;
        let operation: &str = &request.operation;
        if !lane.allows(operation) {
            return Err(CoreError::Forbidden {
                resource: format!("peer_control:{operation}"),
            });
        }
        match operation {
            "status" => {}
            "enable" => lane.set_enabled(true),
            "disable" => lane.set_enabled(false),
            "allow" => {
                lane.allow(operation);
            }
            "evict" => {
                let evicted = crate::execution::peer_tools::peer_tool_registry()
                    .evict_peer(&request.peer_id, None);
                if !evicted {
                    return Err(CoreError::NotFound {
                        resource: format!("peer {}", *request.peer_id),
                    });
                }
            }
            other => {
                return Err(CoreError::InvalidInput {
                    field: "operation".into(),
                    reason: format!("unknown peer-control operation: {other}"),
                });
            }
        }
        Ok(lane.state(active_peer_ids()))
    }
}

/// Distinct peers with registered tools in the process-level registry (the
/// P3-owned one; this module never keeps a second).
fn active_peer_ids() -> Vec<String> {
    let registry = crate::execution::peer_tools::peer_tool_registry();
    let mut ids: HashSet<String> = registry
        .entries()
        .iter()
        .map(|e| e.peer_id.clone())
        .collect();
    let mut ordered: Vec<String> = ids.drain().collect();
    ordered.sort();
    ordered
}
