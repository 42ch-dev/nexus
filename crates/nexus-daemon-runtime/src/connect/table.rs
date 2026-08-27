//! Process-global `PeerToolTable` (V1.174 P0, AR-68 §4.1-§4.3).
//!
//! The daemon-side ingestion surface for authenticated dialer manifests:
//! `tool_id → PeerToolEntry { peer_id, descriptor, responder handle }` plus
//! per-peer session records. It is a **storage + admission** table, NOT a
//! second dispatch table — the spine (`CapabilityRegistry::lookup` /
//! `dispatch`) consults it inside the existing single-table path (AR-68 #4).
//!
//! Admission chain (AR-68 #2), per authenticated manifest:
//! 1. `validate_manifest_tools` whole-manifest — failure ⇒ zero ingestion
//!    from that manifest (`ManifestInvalid`), session stays up.
//! 2. Per-tool named exact-id filters, in order:
//!    - spoke grammar (`parse_tool_capability_id`; defensive — the typed
//!      `ToolDescriptorCapabilityId` already enforces the pattern);
//!    - reserved namespaces: `tools.nexus.` prefix refused; id equal to a
//!      builtin (`host_tool_registry`) or user capability name refused;
//!      the `reserved_tool_ids` set is derived **live** at admission time
//!      from the shared capability holder (builtin ids ∪ current user-cap
//!      names — V1.176 P1 QC fix W-A), so hot-reloaded names stay reserved;
//!    - negotiated membership: id must be in the daemon hello
//!      `capabilities[]` (the `daemon_capabilities` set);
//!    - operator allowlist: id must be in `tool_allowlist` (missing/empty
//!      allowlist = default deny ⇒ zero admitted).
//! 3. Collision policy (AR-68 #3 + DF-91): two peers, same id ⇒ the
//!    policy read from the LIVE `PeerToolsConfig` snapshot at admission
//!    decides. `first_stays` (default): later peer refused (first stays
//!    bound; skip + warn). `priority_order`: the peer ranked EARLIER in
//!    `peer_priority` wins — a later-registering higher-priority peer
//!    preempts (its collided rows replace the lower-priority peer's via
//!    the AR-68 #3 eviction path; the preempted peer's session is
//!    untouched); equal or unlisted rank ⇒ first stays. Same peer
//!    reconnect ⇒ evict-then-admit (deterministic last-wins, mirroring
//!    `PeerSessionManager::register`).
//!
//! Eviction (AR-68 #8): `evict_peer` removes every entry of that `peer_id`
//! from the table in the same tick as close observation; the
//! expected-responder guard prevents a stale monitor from evicting a
//! replacement session's rows.
//!
//! The table is a process-global `LazyLock<Arc<PeerToolTable>>` singleton
//! (`peer_tool_table()`); a single `Mutex<TableInner>` guards both maps so
//! no lock ordering hazard exists between tools and sessions. Poisoned
//! locks are recovered via [`std::sync::PoisonError::into_inner`] (daemon
//! policy). All critical sections are synchronous (no `.await` under a
//! guard).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use crate::connect::config::{CollisionPolicy, PeerToolsConfig};
use nexus_orchestration::CapabilityRegistryHolder;
use spoke_connect::remote::ConnectResponder;
use spoke_operations::{
    parse_tool_capability_id, validate_manifest_tools, SpokeResult, ToolDescriptor,
};
use spoke_schemas::HostCapabilityManifest;

/// One admitted peer tool row.
#[derive(Clone)]
pub struct PeerToolEntry {
    /// The authenticated dialer peer id that owns this tool.
    pub peer_id: String,
    /// The manifest's tool descriptor (input/output schemas + description
    /// carried verbatim, AR-68 #2(iv)).
    pub descriptor: ToolDescriptor,
    /// The reverse-invoke face for the owning session.
    pub responder: Arc<ConnectResponder>,
}

/// Per-peer session record inside the table.
#[derive(Clone)]
pub struct PeerSessionTools {
    /// The responder handle for this peer's live session.
    pub responder: Arc<ConnectResponder>,
    /// Tool ids admitted for this peer (subset of the manifest `tools[]`
    /// that passed the full admission chain).
    pub tool_ids: Vec<String>,
}

/// Interior state of the process-global table.
struct TableInner {
    /// `tool_id → entry` (the dispatchable peer surface).
    tools: HashMap<String, PeerToolEntry>,
    /// `peer_id → session tools` (eviction + reconnect bookkeeping).
    sessions: HashMap<String, PeerSessionTools>,
}

/// The process-global peer tool table (AR-68 #1).
pub struct PeerToolTable {
    inner: Mutex<TableInner>,
    /// Live `PeerToolsConfig` snapshot holder (DF-91): the table reads
    /// `collision_policy` + `peer_priority` from the CURRENT config at
    /// each admission (live-derivation precedent: `live_reserved_tool_ids`
    /// reads the capability holder the same way — p1's reload swaps the
    /// Arc and NEW registrations pick up the new policy without further
    /// table mutation). `None` (the [`PeerToolTable::new`] default) ⇒
    /// `first_stays` + empty rank — the AR-68 #3 default is preserved.
    config: Mutex<Option<Arc<PeerToolsConfig>>>,
}

/// Named admission outcome for one manifest ingestion (AR-68 #2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionOutcome {
    /// Whole-manifest validation failed ⇒ zero ingestion, session stays.
    ManifestInvalid { message: String },
    /// The manifest was valid and every tool passed the chain.
    Admitted { tool_ids: Vec<String> },
}

/// Named per-tool refusal reason (AR-68 #2(iii) named refusals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRefusal {
    /// Spoke grammar violation (defensive; typed ids make this rare).
    Grammar,
    /// Reserved namespace: `tools.nexus.` prefix or a builtin/user-cap
    /// name collision.
    ReservedNamespace,
    /// Not in the daemon hello `capabilities[]` (AR-69 #1 negotiation).
    NotNegotiated,
    /// Not in the operator allowlist (missing/empty allowlist = deny).
    NotAllowlisted,
    /// Another live peer already owns this id (AR-68 #3, first stays).
    DuplicatePeer,
}

/// Named MCP-catalog refusal (AR-70 §3) — a separate projection layer over
/// the registration table.
///
/// The MCP tools surface only carries JSON-Schema object tools; a peer row
/// whose `input` is not a root `type: "object"` is refused from the catalog
/// but its registration lane is untouched: the tool stays in
/// [`PeerToolTable`] and remains dispatchable through the spine
/// (lockstep-pinned per AR-74).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpCatalogRefusal {
    /// `descriptor.input` does not declare a root `type: "object"`.
    InputSchemaNotRootObject,
}

/// MCP catalog admission gate (AR-70 §3): the peer tool's `input` schema
/// must declare a root `type: "object"` to be listed on the MCP tools
/// surface.
///
/// This is a CATALOG-layer filter only — it never touches the registration
/// lane (`admit_and_register` admission chain).
///
/// # Errors
///
/// Returns [`McpCatalogRefusal::InputSchemaNotRootObject`] when the input
/// schema does not declare a root `type: "object"`.
pub fn mcp_catalog_admission(descriptor: &ToolDescriptor) -> Result<(), McpCatalogRefusal> {
    let root_object = descriptor
        .input
        .get("type")
        .and_then(serde_json::Value::as_str)
        == Some("object");
    if root_object {
        Ok(())
    } else {
        Err(McpCatalogRefusal::InputSchemaNotRootObject)
    }
}

/// Whether the peer descriptor's `output` schema may be carried on the MCP
/// tools surface (inclusion rule, AR-70 §3: present AND root-object).
#[must_use]
pub fn mcp_catalog_output_root_object(descriptor: &ToolDescriptor) -> bool {
    descriptor
        .output
        .get("type")
        .and_then(serde_json::Value::as_str)
        == Some("object")
}

impl PeerToolTable {
    /// Create an empty table.
    ///
    /// The default carries NO config holder ⇒ `first_stays` + empty
    /// `peer_priority` (DF-91) — the AR-68 #3 collision behavior every
    /// existing deployment relies on.
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

    /// Wire the live config snapshot (DF-91).
    ///
    /// Called once at boot by the peer-tools lane
    /// ([`crate::connect::start_peer_tools_lane`]); p1's reload swaps the
    /// Arc and NEW registrations pick up the new policy without any
    /// further table mutation (live-derivation precedent:
    /// `live_reserved_tool_ids` reads the capability holder the same way).
    pub fn set_config(&self, config: Option<Arc<PeerToolsConfig>>) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = config;
    }

    /// Admit one authenticated manifest.
    ///
    /// Runs the AR-68 #2 chain: whole-manifest validation first (failure ⇒
    /// [`AdmissionOutcome::ManifestInvalid`], zero ingestion), then the
    /// per-tool named filters. `daemon_capabilities` is the daemon hello
    /// `capabilities[]` (negotiation, AR-69 #1); `tool_allowlist` is the
    /// operator allowlist (empty = default deny); `reserved_tool_ids` is
    /// the caller's reserved set — computed **live** at admission time from
    /// the shared capability holder (builtin ids + current user-cap names,
    /// V1.176 P1 QC fix W-A) so hot-reloaded names stay reserved.
    ///
    /// Same-peer reconnect evicts the peer's prior rows before admitting
    /// (deterministic last-wins). Returns the admitted id set.
    pub fn admit_and_register(
        &self,
        peer_id: &str,
        manifest: &HostCapabilityManifest,
        responder: &Arc<ConnectResponder>,
        daemon_capabilities: &HashSet<String>,
        tool_allowlist: &HashSet<String>,
        reserved_tool_ids: &HashSet<String>,
    ) -> AdmissionOutcome {
        // (i) Whole-manifest gate — fail-closed: zero ingestion, session
        // stays up (AR-68 #2(i)).
        if let SpokeResult::Reject(reject) = validate_manifest_tools(manifest) {
            return AdmissionOutcome::ManifestInvalid {
                message: reject.message,
            };
        }

        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);

        // Same-peer reconnect: evict prior rows first (AR-68 #3).
        if let Some(prior) = inner.sessions.remove(peer_id) {
            for id in &prior.tool_ids {
                if inner.tools.get(id).is_some_and(|e| e.peer_id == peer_id) {
                    inner.tools.remove(id);
                }
            }
        }

        // DF-91: the collision policy + peer rank are read from the LIVE
        // config snapshot at admission time (live-derivation precedent:
        // `live_reserved_tool_ids`). The guard is scoped to the read (the
        // rank Vec is cloned once per manifest — admission is not a hot
        // path) so no lock is held across the per-tool loop.
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
            // (iii) named exact-id filters (grammar → reserved-ns →
            // negotiated → allowlist). The duplicate-peer collision is
            // policy-aware (DF-91) and decided below.
            if let Some(refusal) =
                refuse_tool(&id, daemon_capabilities, tool_allowlist, reserved_tool_ids)
            {
                tracing::warn!(%peer_id, tool_id = %id, refusal = ?refusal, "peer tool refused at admission");
                continue;
            }
            // (iii) duplicate-peer collision (AR-68 #3 + DF-91).
            match collision_decision(&id, peer_id, &inner.tools, policy, &peer_priority) {
                CollisionDecision::Admit => {}
                CollisionDecision::Preempt => {
                    // AR-68 #3 eviction path (reconnect=replace): the
                    // lower-priority peer's row is removed and rebound to
                    // the higher-priority peer. The preempted peer's
                    // session record is untouched (its other rows keep
                    // dispatching); in-flight invokes on the evicted row
                    // resolve as honest per-call failures per AR-76 #4 —
                    // the spine reads the table at invoke time, so a
                    // rebound row dispatches to the new owner and a
                    // removed row yields `not_supported`, never a silent
                    // retry.
                    inner.tools.remove(&id);
                }
                CollisionDecision::Refuse => {
                    tracing::warn!(%peer_id, tool_id = %id, refusal = ?ToolRefusal::DuplicatePeer, "peer tool refused at admission (collision)");
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

        tracing::info!(%peer_id, admitted = admitted.len(), "peer tool table admission complete");
        AdmissionOutcome::Admitted { tool_ids: admitted }
    }

    /// Evict every row owned by `peer_id` (AR-68 #8).
    ///
    /// `expected` guards against a stale monitor evicting a replacement
    /// session's rows (same `Arc::ptr_eq` guard as
    /// [`crate::connect::session::PeerSessionManager::evict`]). Returns
    /// `false` when nothing was evicted (missing / guarded-out).
    pub fn evict_peer(&self, peer_id: &str, expected: Option<&Arc<ConnectResponder>>) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(session) = inner.sessions.get(peer_id) else {
            return false;
        };
        if expected.is_some_and(|e| !Arc::ptr_eq(e, &session.responder)) {
            return false;
        }
        inner.sessions.remove(peer_id);
        inner.tools.retain(|_id, entry| entry.peer_id != peer_id);
        true
    }

    /// Look up one entry by tool id.
    #[must_use]
    pub fn get(&self, tool_id: &str) -> Option<PeerToolEntry> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tools
            .get(tool_id)
            .cloned()
    }

    /// All admitted tool ids.
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

    /// All admitted entries.
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

    /// Tool ids admitted for one live peer.
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

    /// Number of admitted tool rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tools
            .len()
    }

    /// Whether the table holds no tool rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for PeerToolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// AR-68 #2(ii) reserved namespaces, derived **live** at admission time
/// (V1.176 P1 QC fix W-A): builtin host-tool ids ∪ the current user
/// capability names in the shared holder.
///
/// The peer lane's options are built once at boot; holding the shared
/// [`CapabilityRegistryHolder`] instead of a frozen name set means a user
/// capability hot-added (or removed) AFTER the lane spawned is immediately
/// reserved against (or freed for) peer admission — the AR-68 #3 collision
/// contract survives hot reload. `None` (no holder wired) reserves only
/// the static builtin host-tool ids.
#[must_use]
pub(crate) fn live_reserved_tool_ids(
    capability_registry: Option<&CapabilityRegistryHolder>,
) -> HashSet<String> {
    let mut reserved: HashSet<String> = crate::capability_registry::host_tool_registry()
        .ids()
        .map(ToOwned::to_owned)
        .collect();
    if let Some(holder) = capability_registry {
        if let Some(reg) = holder.get() {
            reserved.extend(reg.iter().map(|cap| cap.name().to_owned()));
        }
    }
    reserved
}

/// The per-tool named refusal chain (AR-68 #2(iii)).
///
/// Order: grammar → reserved-ns → negotiated → allowlist. The
/// duplicate-peer collision is NOT part of this chain — it is
/// policy-aware (DF-91) and decided by [`collision_decision`] in
/// `admit_and_register`.
fn refuse_tool(
    id: &str,
    daemon_capabilities: &HashSet<String>,
    tool_allowlist: &HashSet<String>,
    reserved_tool_ids: &HashSet<String>,
) -> Option<ToolRefusal> {
    // Grammar (defensive; typed ids already enforce the pattern).
    if !matches!(parse_tool_capability_id(id), SpokeResult::Ok(_)) {
        return Some(ToolRefusal::Grammar);
    }
    // Reserved namespaces: `tools.nexus.` prefix or builtin/user collision.
    if id.starts_with("tools.nexus.") || reserved_tool_ids.contains(id) {
        return Some(ToolRefusal::ReservedNamespace);
    }
    // Negotiated membership (AR-69 #1): the daemon hello must have listed
    // the exact id.
    if !daemon_capabilities.contains(id) {
        return Some(ToolRefusal::NotNegotiated);
    }
    // Operator allowlist (missing/empty = default deny).
    if !tool_allowlist.contains(id) {
        return Some(ToolRefusal::NotAllowlisted);
    }
    None
}

/// Policy-aware duplicate-id collision decision (DF-91).
///
/// `first_stays` (the default): the existing row stays, the new peer is
/// refused (AR-68 #3). `priority_order`: the peer ranked EARLIER in
/// `peer_priority` wins — a later-registering higher-priority peer
/// preempts (the caller rebinds the row); equal or unlisted rank falls
/// back to registration order (first stays). Unlisted peers rank below
/// every listed peer. A same-peer collision (duplicate id within one
/// manifest) is always refused — the AR-68 #3 duplicate guard.
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
        // Duplicate id within one manifest: the first occurrence stays.
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

/// Array-order rank: EARLIER in `peer_priority` = higher priority
/// (smaller rank). Unlisted peers rank below every listed peer.
fn peer_rank(peer_id: &str, peer_priority: &[String]) -> usize {
    peer_priority
        .iter()
        .position(|p| p == peer_id)
        .unwrap_or(usize::MAX)
}

/// Policy-aware collision outcome for one tool id (DF-91).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionDecision {
    /// No existing row — admit.
    Admit,
    /// The new peer outranks the existing owner — the caller removes the
    /// old row and admits the new one (rows rebind; the preempted peer's
    /// session is untouched).
    Preempt,
    /// The existing row stays — the new peer is refused.
    Refuse,
}

/// Process-global singleton accessor (AR-68 #1).
#[must_use]
pub fn peer_tool_table() -> &'static Arc<PeerToolTable> {
    static TABLE: LazyLock<Arc<PeerToolTable>> = LazyLock::new(|| Arc::new(PeerToolTable::new()));
    &TABLE
}

#[cfg(test)]
mod tests {
    use super::*;
    use spoke_connect::remote::loopback_transport_pair;
    use spoke_connect::remote::{connect_responder, ConnectResponderOptions, RemoteIdentity};
    use spoke_schemas::HostCapabilityManifest;

    fn manifest_with_tools(tools: &[&str]) -> HostCapabilityManifest {
        let mut capabilities: Vec<String> = vec!["spoke-baseline".to_owned()];
        capabilities.extend(tools.iter().map(|s| (*s).to_owned()));
        let tool_objs: Vec<serde_json::Value> = tools
            .iter()
            .map(|id| {
                serde_json::json!({
                    "schema_version": 1,
                    "capability_id": id,
                    "op": id,
                    "description": format!("{id} test tool"),
                    "input": { "type": "object" },
                    "output": { "type": "object" },
                })
            })
            .collect();
        let namespaces: Vec<String> = tools
            .iter()
            .filter_map(|id| id.split('.').nth(1))
            .map(ToOwned::to_owned)
            .collect();
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "host_id": "dialer",
            "roles": ["data-store"],
            "capabilities": capabilities,
            "namespaces": namespaces,
            "extensions": {},
            "tools": tool_objs,
        }))
        .expect("valid manifest")
    }

    fn responder() -> Arc<ConnectResponder> {
        let pair = loopback_transport_pair();
        let options = ConnectResponderOptions {
            transport: Arc::new(pair.server),
            identity: RemoteIdentity { seed: [0x40; 32] },
            manifest: manifest_with_tools(&[]),
            allowlist: Vec::new(),
            peer_keys: HashMap::new(),
            ports: None,
            invoke_timeout_ms: Some(1000),
        };
        // The responder runs its handshake in the background; for table
        // unit tests we only need the handle (no dialer is required).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        rt.block_on(connect_responder(options))
    }

    fn caps(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn valid_manifest_admits_exact_id_set_with_schemas_verbatim() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.t3.echo"]);
        let outcome = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            outcome,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let entry = table.get("tools.t3.echo").expect("entry present");
        assert_eq!(entry.peer_id, "peer-a");
        assert_eq!(
            String::from(entry.descriptor.capability_id.clone()),
            "tools.t3.echo"
        );
        assert_eq!(
            entry.descriptor.input,
            serde_json::json!({"type": "object"})
                .as_object()
                .cloned()
                .unwrap()
        );
        assert_eq!(
            entry.descriptor.output,
            serde_json::json!({"type": "object"})
                .as_object()
                .cloned()
                .unwrap()
        );
    }

    #[test]
    fn empty_allowlist_is_default_deny() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.t3.echo"]);
        let outcome = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(
            outcome,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            }
        );
        assert!(table.is_empty());
    }

    #[test]
    fn whole_manifest_invalid_refuses_zero_ingestion() {
        let table = PeerToolTable::new();
        // A manifest whose tool id is NOT in its own capabilities[] fails
        // validate_manifest_tools (capability_id must appear in
        // capabilities[] — AR-68 #2(i) whole-manifest gate).
        let manifest = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "host_id": "dialer",
            "roles": ["data-store"],
            "capabilities": ["spoke-baseline"],
            "namespaces": ["t3"],
            "extensions": {},
            "tools": [{
                "schema_version": 1,
                "capability_id": "tools.t3.echo",
                "op": "tools.t3.echo",
                "description": "echo",
                "input": { "type": "object" },
                "output": { "type": "object" },
            }],
        }))
        .expect("manifest parses");
        let outcome = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert!(
            matches!(outcome, AdmissionOutcome::ManifestInvalid { .. }),
            "expected ManifestInvalid, got {outcome:?}"
        );
        assert!(table.is_empty());
    }

    #[test]
    fn reserved_namespace_refused() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.nexus.evil"]);
        let outcome = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.nexus.evil"]),
            &caps(&["tools.nexus.evil"]),
            &HashSet::new(),
        );
        assert_eq!(
            outcome,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            }
        );
        assert!(table.is_empty());
    }
    /// Write an admitted `<name>/capability.json` trio (AR-35 layout): a
    /// hash-consistent `manifest.json` + `<module-id>.wasm` pair so the
    /// AR-43 admission gates pass inside the scan. House convention
    /// (qc1 S-1): each test crate carries its own copy of this fixture; a
    /// feature-gated shared helper is the documented follow-up if the
    /// copies keep churning.
    fn write_capability_dir(root: &std::path::Path, name: &str) {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let wasm = b"fake module bytes";
        let sha: String = {
            let mut hex = String::with_capacity(64);
            for b in Sha256::digest(wasm) {
                let _ = write!(hex, "{b:02x}");
            }
            hex
        };
        let descriptor = format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#
        );
        std::fs::write(dir.join("capability.json"), descriptor).unwrap();
        let manifest = format!(
            r#"{{
                "module_id": "basic-combat",
                "name": "Basic Combat",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": "{sha}"
            }}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
    }

    /// W-A (V1.176 P1 QC fix): the reserved set is derived LIVE from the
    /// shared capability holder at admission time. A user capability
    /// hot-added AFTER the peer lane's options were built is still
    /// reserved — peer admission with the same id fails closed (the
    /// AR-68 #3 collision contract, restored for hot reloads).
    #[test]
    fn hot_admitted_user_cap_name_is_reserved_against_peer_admission() {
        use nexus_orchestration::capability::watch::rebuild_registry_with_merge;
        use nexus_orchestration::{CapabilityRegistryHolder, CapabilityRuntimeDeps};

        let tmp = tempfile::tempdir().unwrap();
        let scan_dir = tmp.path().join("caps");
        std::fs::create_dir_all(&scan_dir).unwrap();
        // The lane's options are built BEFORE this swap; only the shared
        // holder is handed over (boot passes the same holder the watcher
        // swaps into — the hot admission lands here).
        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: None,
            daemon_tool_dispatch: None,
            cdn_config: None,
        };
        write_capability_dir(&scan_dir, "tools.operator.demo");
        let (registry, outcome) = rebuild_registry_with_merge(&deps, None, None, &scan_dir, &[]);
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        let holder = CapabilityRegistryHolder::new();
        holder.swap(std::sync::Arc::new(registry));

        // Live derivation: the hot-added name is in the reserved set even
        // though it did not exist when the lane's options were built.
        let reserved = live_reserved_tool_ids(Some(&holder));
        assert!(
            reserved.contains("tools.operator.demo"),
            "hot-admitted user-cap name must be reserved at admission time"
        );

        // End-to-end chain: peer registration with the same id fails
        // closed (zero ingestion of that tool, no shadowing row).
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.operator.demo"]);
        let outcome = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.operator.demo"]),
            &caps(&["tools.operator.demo"]),
            &reserved,
        );
        assert_eq!(
            outcome,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            },
            "peer admission with a hot-admitted user-cap id fails closed"
        );
        assert!(table.is_empty(), "no peer row shadows the user capability");
    }

    #[test]
    fn not_negotiated_refused() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.t3.echo"]);
        let outcome = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.other"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        // Whole-manifest validation passes (id IS in capabilities[] of the
        // manifest itself); negotiation (daemon hello) refuses it.
        assert_eq!(
            outcome,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            }
        );
        assert!(table.is_empty());
    }

    #[test]
    fn duplicate_id_two_peer_collision_later_refused_first_stays() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.t3.echo"]);
        let first = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let second = table.admit_and_register(
            "peer-b",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            }
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("first stays").peer_id,
            "peer-a"
        );
    }

    // ── DF-91: policy-aware collision (priority_order) ──────────────────

    /// A config snapshot with the given collision policy + peer rank
    /// (everything else defaulted).
    fn config_with(policy: CollisionPolicy, peer_priority: &[&str]) -> Arc<PeerToolsConfig> {
        Arc::new(PeerToolsConfig {
            collision_policy: policy,
            peer_priority: peer_priority.iter().map(|s| (*s).to_owned()).collect(),
            ..PeerToolsConfig::default()
        })
    }

    /// Admit one manifest of tools for `peer_id` against `table`.
    fn admit(table: &PeerToolTable, peer_id: &str, tools: &[&str]) -> AdmissionOutcome {
        let manifest = manifest_with_tools(tools);
        table.admit_and_register(
            peer_id,
            &manifest,
            &responder(),
            &caps(tools),
            &caps(tools),
            &HashSet::new(),
        )
    }

    #[test]
    fn priority_order_collision_selects_higher_ranked_peer() {
        // DF-91: under `priority_order`, the peer ranked EARLIER in
        // `peer_priority` wins the collision — even when it registers
        // later.
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(
            CollisionPolicy::PriorityOrder,
            &["peer-b", "peer-a"],
        )));
        let first = admit(&table, "peer-a", &["tools.t3.echo"]);
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let second = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            },
            "higher-ranked peer wins the collision"
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("row present").peer_id,
            "peer-b"
        );
    }

    #[test]
    fn priority_order_equal_rank_falls_back_to_registration_order() {
        // DF-91: equal rank (both unlisted) ⇒ first stays (registration
        // order), exactly like the AR-68 #3 default.
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(CollisionPolicy::PriorityOrder, &[])));
        let first = admit(&table, "peer-a", &["tools.t3.echo"]);
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let second = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            },
            "equal rank falls back to registration order (first stays)"
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("first stays").peer_id,
            "peer-a"
        );
    }

    #[test]
    fn priority_order_later_higher_rank_preempts_existing_row() {
        // DF-91: a later-registering higher-priority peer PREEMPTS — its
        // collided row replaces the lower-priority registrant's via the
        // AR-68 #3 eviction path. The preempted peer's session record is
        // untouched (its other rows keep dispatching; its tool_ids list is
        // not rewritten).
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(
            CollisionPolicy::PriorityOrder,
            &["peer-b", "peer-a"],
        )));
        let responder_a = responder();
        let responder_b = responder();
        let manifest = manifest_with_tools(&["tools.t3.echo", "tools.t3.ping"]);
        let first = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder_a,
            &caps(&["tools.t3.echo", "tools.t3.ping"]),
            &caps(&["tools.t3.echo", "tools.t3.ping"]),
            &HashSet::new(),
        );
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned(), "tools.t3.ping".to_owned()]
            }
        );
        let manifest_b = manifest_with_tools(&["tools.t3.echo"]);
        let second = table.admit_and_register(
            "peer-b",
            &manifest_b,
            &responder_b,
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        // The collided row rebinds to the higher-priority peer — responder
        // handle included (dispatch would reverse-invoke peer-b).
        let rebound = table.get("tools.t3.echo").expect("rebound row");
        assert_eq!(rebound.peer_id, "peer-b");
        assert!(
            Arc::ptr_eq(&rebound.responder, &responder_b),
            "rebound row carries the higher-priority peer's responder"
        );
        // The preempted peer's non-collided row is untouched.
        assert_eq!(
            table.get("tools.t3.ping").expect("untouched row").peer_id,
            "peer-a"
        );
        // The preempted peer's SESSION record is untouched — it still lists
        // the collided id (the row itself moved; the session did not).
        assert_eq!(
            table.peer_tool_ids("peer-a"),
            vec!["tools.t3.echo".to_owned(), "tools.t3.ping".to_owned()],
            "preempted peer's session record is untouched"
        );
        assert_eq!(
            table.peer_tool_ids("peer-b"),
            vec!["tools.t3.echo".to_owned()]
        );
    }

    #[test]
    fn priority_order_unlisted_peer_loses_to_listed_peer() {
        // DF-91: unlisted peers rank below every listed peer — a listed
        // peer's row survives a later unlisted registration.
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(
            CollisionPolicy::PriorityOrder,
            &["peer-a"],
        )));
        let first = admit(&table, "peer-a", &["tools.t3.echo"]);
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let second = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            },
            "unlisted peer loses to a listed peer"
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("listed stays").peer_id,
            "peer-a"
        );
    }

    #[test]
    fn priority_order_listed_peer_preempts_unlisted_peer() {
        // DF-91: a listed peer registering later preempts an unlisted
        // peer's row (unlisted ranks below all listed peers).
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(
            CollisionPolicy::PriorityOrder,
            &["peer-b"],
        )));
        let first = admit(&table, "peer-a", &["tools.t3.echo"]);
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let second = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            },
            "listed peer preempts the unlisted peer"
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("listed preempts").peer_id,
            "peer-b"
        );
    }

    #[test]
    fn first_stays_default_holds_when_config_wired() {
        // DF-91: `first_stays` (the serde default) keeps the AR-68 #3
        // behavior even when a config snapshot IS wired — a higher-ranked
        // later peer is still refused.
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(CollisionPolicy::FirstStays, &["peer-b"])));
        let first = admit(&table, "peer-a", &["tools.t3.echo"]);
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let second = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            second,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            },
            "first_stays refuses the later peer regardless of rank"
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("first stays").peer_id,
            "peer-a"
        );
    }

    #[test]
    fn policy_read_live_at_admission_swaps_apply_to_new_registrations() {
        // DF-91 live-derivation: the table reads the CURRENT config at
        // each admission — swapping the Arc (p1's reload shape) changes
        // the collision outcome for NEW registrations without any further
        // table mutation.
        let table = PeerToolTable::new();
        table.set_config(Some(config_with(CollisionPolicy::FirstStays, &[])));
        let first = admit(&table, "peer-a", &["tools.t3.echo"]);
        assert_eq!(
            first,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            }
        );
        let refused = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            refused,
            AdmissionOutcome::Admitted {
                tool_ids: Vec::new()
            }
        );
        // Swap to priority_order with peer-b ranked first: a NEW peer-b
        // registration now preempts peer-a's row.
        table.set_config(Some(config_with(
            CollisionPolicy::PriorityOrder,
            &["peer-b"],
        )));
        let preempted = admit(&table, "peer-b", &["tools.t3.echo"]);
        assert_eq!(
            preempted,
            AdmissionOutcome::Admitted {
                tool_ids: vec!["tools.t3.echo".to_owned()]
            },
            "live policy swap applies to new registrations"
        );
        assert_eq!(
            table.get("tools.t3.echo").expect("rebound").peer_id,
            "peer-b"
        );
    }

    #[test]
    fn same_peer_reconnect_evicts_then_readmits() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.t3.echo"]);
        let _ = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        let _ = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo"]),
            &caps(&["tools.t3.echo"]),
            &HashSet::new(),
        );
        assert_eq!(table.len(), 1);
        assert_eq!(
            table.get("tools.t3.echo").expect("readmitted").peer_id,
            "peer-a"
        );
    }

    #[test]
    fn evict_removes_all_rows_same_tick() {
        let table = PeerToolTable::new();
        let manifest = manifest_with_tools(&["tools.t3.echo", "tools.t3.ping"]);
        let _ = table.admit_and_register(
            "peer-a",
            &manifest,
            &responder(),
            &caps(&["tools.t3.echo", "tools.t3.ping"]),
            &caps(&["tools.t3.echo", "tools.t3.ping"]),
            &HashSet::new(),
        );
        assert_eq!(table.len(), 2);
        assert!(table.evict_peer("peer-a", None));
        assert!(table.is_empty());
        assert!(table.peer_tool_ids("peer-a").is_empty());
    }
}
