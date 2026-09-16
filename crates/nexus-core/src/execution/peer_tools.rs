//! Protocol-neutral peer tool registry (v1.190 P3-T3).
//!
//! The reverse-invoke spine: an admitted peer's tools become dispatchable by
//! id, exactly like a builtin. Moved out of the daemon's
//! `connect/table.rs` so a non-Connect host (the TS service, an independent
//! product) owns the SAME registry instead of a second one.
//!
//! # Protocol neutrality
//!
//! The daemon's table named `spoke_connect::ConnectResponder` directly, which
//! welded the registry to one transport. Here the responder is
//! [`PeerResponder`] — a port whose only methods are "invoke this tool" and
//! "who am I". A Connect host implements it over its wire; another host
//! implements it over its own. The registry itself never names a protocol.
//!
//! # Visibility is not authorization
//!
//! Two axes stay SEPARATE, and conflating them is the classic bug this module
//! is shaped to prevent:
//!
//! - **Registration/admission** ([`PeerToolRegistry::admit_and_register`]):
//!   decides whether a tool id becomes DISPATCHABLE.
//! - **Visibility** (the serving seam's own policy type, e.g. the daemon's
//!   `connect::visibility::VisibilityPolicy`): decides whether a dispatchable
//!   tool is LISTED to a consumer.
//!
//! A tool hidden by a visibility policy is still dispatchable — hiding is a
//! catalog courtesy, never a grant, and never a revocation. Conversely, a
//! tool listed by a visibility policy still passes the full admission chain
//! on every call.
//!
//! # Reserved namespaces
//!
//! A peer tool may never shadow a builtin or a user capability: the reserved
//! set is computed LIVE from the shared capability holder at each admission,
//! so a capability hot-added after the peer lane spawned is immediately
//! protected. Reserving from a frozen snapshot would let a peer squat a name
//! that a later reload makes real.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use nexus_spoke_adapter::{
    parse_tool_capability_id, validate_manifest_tools, validate_tool_arguments, SpokeResult,
    ToolDescriptor,
};

/// One tool invocation's outcome, in protocol-neutral terms.
pub type PeerInvokeResult = Result<serde_json::Value, PeerInvokeError>;

/// A protocol-neutral invocation failure.
///
/// The registry maps these onto its own refusals; the transport that produced
/// them keeps its wire detail (the daemon preserves `details.kind` for the
/// timeout/disconnect classes).
#[derive(Debug, Clone)]
pub enum PeerInvokeError {
    /// The peer refused the call (a policy deny, not a transport fault).
    Denied {
        /// The peer's own wire code, when it supplies one — preserved
        /// verbatim so a consumer can distinguish the peer's precise reason
        /// from the spine's generic `not_supported`.
        wire_code: Option<String>,
        /// Human-readable refusal.
        message: String,
    },
    /// The invoke timed out.
    Timeout {
        /// Human-readable detail.
        message: String,
    },
    /// The peer's session was torn down mid-invoke.
    Disconnected {
        /// Human-readable detail.
        message: String,
    },
    /// Any other peer-side failure.
    Internal {
        /// Human-readable detail.
        message: String,
    },
}

/// The reverse-invoke port a peer transport implements.
///
/// Deliberately two methods: the registry needs to invoke a tool and to
/// identify the owning session. Anything else (framing, reconnection,
/// negotiation) belongs to the transport, not here.
#[async_trait::async_trait]
pub trait PeerResponder: Send + Sync {
    /// Invoke `tool_id` with `arguments` on this peer's session.
    async fn invoke_tool(&self, tool_id: &str, arguments: serde_json::Value) -> PeerInvokeResult;

    /// The authenticated peer id this responder serves.
    fn peer_id(&self) -> &str;
}

/// One admitted peer tool row.
#[derive(Clone)]
pub struct PeerToolEntry {
    /// The authenticated peer that owns this tool.
    pub peer_id: String,
    /// The manifest's tool descriptor (input/output schemas + description
    /// carried verbatim).
    pub descriptor: ToolDescriptor,
    /// The reverse-invoke face for the owning session.
    pub responder: Arc<dyn PeerResponder>,
}

/// Per-peer session record inside the registry.
#[derive(Clone)]
pub struct PeerSessionTools {
    /// The responder handle for this peer's live session.
    pub responder: Arc<dyn PeerResponder>,
    /// Tool ids admitted for this peer.
    pub tool_ids: Vec<String>,
}

/// Interior state of the process-global registry.
struct TableInner {
    /// `tool_id → entry` (the dispatchable peer surface).
    tools: HashMap<String, PeerToolEntry>,
    /// `peer_id → session tools` (eviction + reconnect bookkeeping).
    sessions: HashMap<String, PeerSessionTools>,
}

/// Collision policy when two peers offer the same tool id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollisionPolicy {
    /// The first registrant keeps the id (the default).
    #[default]
    FirstStays,
    /// The peer ranked earlier in the priority list wins.
    PriorityOrder,
}

/// Live config the registry reads at each admission.
#[derive(Debug, Clone, Default)]
pub struct PeerToolsConfig {
    /// Duplicate-id collision policy.
    pub collision_policy: CollisionPolicy,
    /// Peer rank: earlier = higher priority.
    pub peer_priority: Vec<String>,
}

/// The outcome of one manifest admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionOutcome {
    /// The manifest was admitted; the listed ids are now dispatchable.
    Admitted {
        /// The ids that were admitted.
        tool_ids: Vec<String>,
    },
    /// The whole manifest was refused — zero ingestion, session stays up.
    ManifestInvalid {
        /// Why it was refused.
        message: String,
    },
}

/// Why a single tool id was refused at admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRefusal {
    /// The id does not match the `tools.<ns>.<tool_id>` grammar.
    Grammar,
    /// The id is reserved (a builtin or user capability).
    ReservedNamespace,
    /// The daemon hello did not negotiate this exact id.
    NotNegotiated,
    /// The operator allowlist does not include the id.
    NotAllowlisted,
    /// Another peer already owns the id.
    DuplicatePeer,
}

/// The peer tool registry.
pub struct PeerToolRegistry {
    inner: Mutex<TableInner>,
    /// Live config snapshot. The registry reads the collision policy from the
    /// CURRENT config at each admission, so a reload takes effect for new
    /// registrations without further mutation.
    ///
    /// # Lock rank
    ///
    /// `inner` ranks BEFORE `config`: admission holds `inner` across the whole
    /// operation and takes `config` inside it. The REVERSE order is forbidden.
    /// `set_config` takes ONLY `config`, so a reload can never invert the rank.
    config: Mutex<Option<Arc<PeerToolsConfig>>>,
}

impl PeerToolRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(TableInner {
                tools: HashMap::new(),
                sessions: HashMap::new(),
            }),
            config: Mutex::new(None),
        }
    }

    /// Wire the live config snapshot.
    pub fn set_config(&self, config: Option<Arc<PeerToolsConfig>>) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = config;
    }

    /// Whether a config snapshot is wired.
    #[must_use]
    pub fn has_config(&self) -> bool {
        self.config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Admit one authenticated manifest.
    ///
    /// The whole-manifest validation runs FIRST: a failure ingests nothing and
    /// leaves the session up. Then each tool id passes the named filter chain
    /// (grammar → reserved → negotiated → allowlist) and the policy-aware
    /// duplicate decision.
    ///
    /// `reserved_tool_ids` is computed LIVE by the caller from the shared
    /// capability holder, so hot-reloaded capability names stay reserved.
    pub fn admit_and_register(
        &self,
        peer_id: &str,
        manifest: &nexus_spoke_adapter::HostCapabilityManifest,
        responder: &Arc<dyn PeerResponder>,
        daemon_capabilities: &HashSet<String>,
        tool_allowlist: &HashSet<String>,
        reserved_tool_ids: &HashSet<String>,
    ) -> AdmissionOutcome {
        if let SpokeResult::Reject(reject) = validate_manifest_tools(manifest) {
            return AdmissionOutcome::ManifestInvalid {
                message: reject.message,
            };
        }

        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);

        // A same-peer reconnect evicts the peer's prior rows first
        // (deterministic last-wins).
        if let Some(prior) = inner.sessions.remove(peer_id) {
            for id in &prior.tool_ids {
                if inner.tools.get(id).is_some_and(|e| e.peer_id == peer_id) {
                    inner.tools.remove(id);
                }
            }
        }

        // The config guard is scoped to this block and dropped once the rank
        // vector is cloned; `inner` stays held for the whole admission.
        let (policy, peer_priority) = {
            let config_guard = self.config.lock().unwrap_or_else(PoisonError::into_inner);
            config_guard
                .as_ref()
                .map_or((CollisionPolicy::FirstStays, Vec::new()), |cfg| {
                    (cfg.collision_policy, cfg.peer_priority.clone())
                })
        };

        let mut admitted: Vec<String> = Vec::new();
        for tool in &manifest.tools {
            let id = String::from(tool.capability_id.clone());
            if let Some(refusal) =
                refuse_tool(&id, daemon_capabilities, tool_allowlist, reserved_tool_ids)
            {
                tracing::warn!(%peer_id, tool_id = %id, refusal = ?refusal, "peer tool refused at admission");
                continue;
            }
            match collision_decision(&id, peer_id, &inner.tools, policy, &peer_priority) {
                CollisionDecision::Admit => {}
                CollisionDecision::Preempt => {
                    // The preempted peer's session record is untouched: its
                    // other rows keep dispatching. An in-flight invoke on the
                    // evicted row resolves as an honest per-call failure — the
                    // spine reads the registry at invoke time, so a rebound row
                    // dispatches to the new owner and a removed row yields
                    // `not_supported`, never a silent retry.
                    inner.tools.remove(&id);
                }
                CollisionDecision::Refuse => {
                    tracing::warn!(
                        %peer_id,
                        tool_id = %id,
                        refusal = ?ToolRefusal::DuplicatePeer,
                        "peer tool refused at admission (collision)"
                    );
                    continue;
                }
            }
            inner.tools.insert(
                id.clone(),
                PeerToolEntry {
                    peer_id: peer_id.to_owned(),
                    descriptor: tool.clone(),
                    responder: Arc::clone(responder),
                },
            );
            admitted.push(id);
        }

        inner.sessions.insert(
            peer_id.to_owned(),
            PeerSessionTools {
                responder: Arc::clone(responder),
                tool_ids: admitted.clone(),
            },
        );
        drop(inner);

        tracing::info!(%peer_id, admitted = admitted.len(), "peer tool admission complete");
        AdmissionOutcome::Admitted { tool_ids: admitted }
    }

    /// Evict every row owned by `peer_id`.
    ///
    /// `expected` guards against a stale monitor evicting a REPLACEMENT
    /// session's rows (pointer identity). Returns `false` when nothing was
    /// evicted.
    pub fn evict_peer(&self, peer_id: &str, expected: Option<&Arc<dyn PeerResponder>>) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(session) = inner.sessions.get(peer_id) else {
            return false;
        };
        if expected.is_some_and(|e| !Arc::ptr_eq(e, &session.responder)) {
            return false;
        }
        let ids = session.tool_ids.clone();
        inner.sessions.remove(peer_id);
        let mut evicted = false;
        for id in ids {
            if inner.tools.get(&id).is_some_and(|e| e.peer_id == peer_id) {
                inner.tools.remove(&id);
                evicted = true;
            }
        }
        evicted
    }

    /// Look up a dispatchable peer entry by tool id.
    #[must_use]
    pub fn get(&self, tool_id: &str) -> Option<PeerToolEntry> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tools
            .get(tool_id)
            .cloned()
    }

    /// Every dispatchable peer tool id.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tools
            .keys()
            .cloned()
            .collect()
    }

    /// Every dispatchable peer entry.
    #[must_use]
    pub fn entries(&self) -> Vec<PeerToolEntry> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tools
            .values()
            .cloned()
            .collect()
    }

    /// The tool ids admitted for one peer.
    #[must_use]
    pub fn peer_tool_ids(&self, peer_id: &str) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .sessions
            .get(peer_id)
            .map(|s| s.tool_ids.clone())
            .unwrap_or_default()
    }

    /// Number of dispatchable peer tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tools
            .len()
    }

    /// Whether no peer tool is dispatchable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for PeerToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The process-global peer tool registry.
#[must_use]
pub fn peer_tool_registry() -> &'static Arc<PeerToolRegistry> {
    static TABLE: LazyLock<Arc<PeerToolRegistry>> =
        LazyLock::new(|| Arc::new(PeerToolRegistry::new()));
    &TABLE
}

/// Invoke an admitted peer tool, validating its arguments first.
///
/// The structural argument gate runs before any transport I/O: a malformed
/// call is refused locally and never reaches the peer.
///
/// # Errors
/// `PeerInvokeError::Denied` for a structural argument refusal (no peer call
/// is made), and the responder's own error otherwise.
pub async fn invoke_peer_tool(
    entry: &PeerToolEntry,
    arguments: serde_json::Value,
) -> PeerInvokeResult {
    if let SpokeResult::Reject(reject) =
        validate_tool_arguments(&entry.descriptor, &arguments)
    {
        return Err(PeerInvokeError::Denied {
            wire_code: None,
            message: reject.message,
        });
    }
    entry
        .responder
        .invoke_tool(&entry.descriptor.capability_id, arguments)
        .await
}

/// The reserved tool ids: every builtin host tool plus every live user
/// capability name.
///
/// Computed LIVE at admission time so a capability hot-added after the peer
/// lane spawned is immediately reserved against. `None` reserves only the
/// builtins.
#[must_use]
pub fn live_reserved_tool_ids(
    builtin_ids: impl IntoIterator<Item = String>,
    capability_registry: Option<&nexus_orchestration::CapabilityRegistryHolder>,
) -> HashSet<String> {
    let mut reserved: HashSet<String> = builtin_ids.into_iter().collect();
    if let Some(holder) = capability_registry {
        if let Some(reg) = holder.get() {
            reserved.extend(reg.iter().map(|cap| cap.name().to_owned()));
        }
    }
    reserved
}

/// Whether a peer descriptor's input schema may be carried on an MCP surface.
///
/// The surface only carries root-object tools; a non-object input is omitted
/// from the CATALOG while its registration lane is untouched — the tool stays
/// dispatchable.
#[must_use]
pub fn catalog_input_root_object(descriptor: &ToolDescriptor) -> bool {
    descriptor
        .input
        .get("type")
        .and_then(serde_json::Value::as_str)
        == Some("object")
}

/// Whether a peer descriptor's output schema may be carried on an MCP surface
/// (present AND root-object; a non-object output is omitted, never wrapped).
#[must_use]
pub fn catalog_output_root_object(descriptor: &ToolDescriptor) -> bool {
    descriptor
        .output
        .get("type")
        .and_then(serde_json::Value::as_str)
        == Some("object")
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// The per-tool named refusal chain.
///
/// Order: grammar → reserved-ns → negotiated → allowlist. The duplicate-peer
/// collision is deliberately NOT here: it is policy-aware and decided by
/// [`collision_decision`].
fn refuse_tool(
    id: &str,
    daemon_capabilities: &HashSet<String>,
    tool_allowlist: &HashSet<String>,
    reserved_tool_ids: &HashSet<String>,
) -> Option<ToolRefusal> {
    if !matches!(parse_tool_capability_id(id), SpokeResult::Ok(_)) {
        return Some(ToolRefusal::Grammar);
    }
    if id.starts_with("tools.nexus.") || reserved_tool_ids.contains(id) {
        return Some(ToolRefusal::ReservedNamespace);
    }
    if !daemon_capabilities.contains(id) {
        return Some(ToolRefusal::NotNegotiated);
    }
    if !tool_allowlist.contains(id) {
        return Some(ToolRefusal::NotAllowlisted);
    }
    None
}

/// Policy-aware duplicate-id collision decision.
///
/// `first_stays`: the existing row stays, the new peer is refused.
/// `priority_order`: a later-registering higher-priority peer preempts; equal
/// or unlisted rank falls back to registration order. A same-peer collision
/// (duplicate id within one manifest) is always refused.
fn collision_decision(
    id: &str,
    new_peer: &str,
    existing: &HashMap<String, PeerToolEntry>,
    policy: CollisionPolicy,
    peer_priority: &[String],
) -> CollisionDecision {
    let Some(entry) = existing.get(id) else {
        return CollisionDecision::Admit;
    };
    if entry.peer_id == new_peer {
        return CollisionDecision::Refuse;
    }
    if policy == CollisionPolicy::PriorityOrder {
        let new_rank = peer_rank(new_peer, peer_priority);
        let existing_rank = peer_rank(&entry.peer_id, peer_priority);
        if new_rank < existing_rank {
            return CollisionDecision::Preempt;
        }
    }
    CollisionDecision::Refuse
}

/// Array-order rank: earlier in `peer_priority` = higher priority (smaller
/// rank). Unlisted peers rank below every listed peer.
fn peer_rank(peer_id: &str, peer_priority: &[String]) -> usize {
    peer_priority
        .iter()
        .position(|p| p == peer_id)
        .unwrap_or(usize::MAX)
}

/// Policy-aware collision outcome for one tool id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionDecision {
    /// No existing row — admit.
    Admit,
    /// The new peer outranks the existing owner; the caller removes the old
    /// row and rebinds it.
    Preempt,
    /// The existing row stays — the new peer is refused.
    Refuse,
}
