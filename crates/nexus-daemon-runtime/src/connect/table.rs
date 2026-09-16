//! Peer tool table — daemon-side adapter over the core registry.
//!
//! v1.190 P3-T3: the table BODY (admission chain, collision policy, eviction,
//! the process-global singleton) moved to
//! [`nexus_core::execution::peer_tools`]. What remains is the daemon's own
//! translation:
//!
//! - [`ConnectResponderAdapter`] implements the core's protocol-neutral
//!   [`PeerResponder`] port over `spoke_connect::ConnectResponder`, mapping
//!   the wire failure classes onto the port's typed refusals.
//! - The re-exports below keep the daemon's call sites (the Connect accept
//!   lane, the catalog builders) compiling against the same names.
//!
//! The daemon no longer owns a table: one registry per process is the point of
//! the cutover, because two tables would let the builtin and peer id sets
//! diverge.

use std::collections::HashSet;
use std::sync::Arc;

use nexus_core::execution::peer_tools::{PeerInvokeError, PeerInvokeResult, PeerResponder};
use nexus_orchestration::CapabilityRegistryHolder;
use spoke_connect::remote::ConnectResponder;
use spoke_operations::{SpokeRejectCode, SpokeResult};

pub use nexus_core::execution::peer_tools::{
    AdmissionOutcome, CollisionPolicy, PeerSessionTools, PeerToolEntry, PeerToolsConfig,
    ToolRefusal, peer_tool_registry,
};

/// The core registry type, under the daemon's historical name.
///
/// Kept as an alias so the Connect lane's type positions (`Arc<PeerToolTable>`
/// in the config watcher) need no rename while there is still exactly one
/// registry.
pub type PeerToolTable = nexus_core::execution::peer_tools::PeerToolRegistry;

/// MCP catalog admission: the peer descriptor's input must be a root object.
#[must_use]
pub fn mcp_catalog_admission(
    descriptor: &spoke_operations::ToolDescriptor,
) -> Result<(), McpCatalogRefusal> {
    if nexus_core::execution::peer_tools::catalog_input_root_object(descriptor) {
        Ok(())
    } else {
        Err(McpCatalogRefusal::InputSchemaNotRootObject)
    }
}

/// Whether the peer descriptor's output schema may be carried on an MCP
/// surface.
#[must_use]
pub fn mcp_catalog_output_root_object(descriptor: &spoke_operations::ToolDescriptor) -> bool {
    nexus_core::execution::peer_tools::catalog_output_root_object(descriptor)
}

/// MCP catalog refusal for a peer descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpCatalogRefusal {
    /// `descriptor.input` does not declare a root `type: "object"`.
    InputSchemaNotRootObject,
}

/// Adapter presenting a `spoke_connect` responder as the core's neutral port.
pub struct ConnectResponderAdapter {
    /// The underlying wire responder.
    inner: Arc<ConnectResponder>,
    /// The authenticated peer id this responder serves.
    peer_id: String,
}

impl ConnectResponderAdapter {
    /// Wrap a wire responder, binding it to `peer_id`.
    #[must_use]
    pub fn new(inner: Arc<ConnectResponder>, peer_id: String) -> Self {
        Self { inner, peer_id }
    }

    /// The wrapped wire responder.
    #[must_use]
    pub fn inner(&self) -> &Arc<ConnectResponder> {
        &self.inner
    }
}

#[async_trait::async_trait]
impl PeerResponder for ConnectResponderAdapter {
    async fn invoke_tool(&self, tool_id: &str, arguments: serde_json::Value) -> PeerInvokeResult {
        match self.inner.invoke_tool(tool_id, arguments).await {
            SpokeResult::Ok(value) => Ok(value),
            SpokeResult::Reject(reject) => {
                // The honest-refusal matrix (spoke frozen contract §8.2
                // `details.kind`): a timeout and a torn-down session are
                // distinguishable TRANSPORT faults, never a peer deny, so the
                // core can label them differently from a policy refusal.
                let kind = reject
                    .details
                    .as_ref()
                    .and_then(|d| d.get("kind"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if reject.code == SpokeRejectCode::InternalError {
                    match kind {
                        "timeout" => {
                            return Err(PeerInvokeError::Timeout {
                                message: reject.message,
                            });
                        }
                        "session_closed" | "transport" => {
                            return Err(PeerInvokeError::Disconnected {
                                message: reject.message,
                            });
                        }
                        _ => {}
                    }
                }
                // A peer-side deny keeps the peer's own lowercase code
                // verbatim; it is never re-parsed from the message text.
                let wire_code = reject
                    .details
                    .as_ref()
                    .and_then(|d| d.get("wire_code"))
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| reject.code.as_str().to_string(), ToOwned::to_owned);
                Err(PeerInvokeError::Denied {
                    wire_code: Some(wire_code),
                    message: reject.message,
                })
            }
        }
    }

    fn peer_id(&self) -> &str {
        &self.peer_id
    }
}

/// The reserved tool ids: builtins plus live user-capability names.
///
/// Computed LIVE so a capability hot-added after the peer lane spawned is
/// immediately reserved against.
#[must_use]
pub fn live_reserved_tool_ids(
    capability_registry: Option<&CapabilityRegistryHolder>,
) -> HashSet<String> {
    nexus_core::execution::peer_tools::live_reserved_tool_ids(
        nexus_core::execution::capabilities::host_tool_registry()
            .ids()
            .map(ToOwned::to_owned),
        capability_registry,
    )
}

/// The process-global registry (kept as the daemon's historical accessor name).
#[must_use]
pub fn peer_tool_table() -> &'static Arc<PeerToolRegistry> {
    peer_tool_registry()
}
