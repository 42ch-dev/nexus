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
//!    - reserved namespaces: `tools.nexus.` prefix refused; id equal to an
//!      existing builtin (`host_tool_registry`) or user capability name
//!      refused; id in the caller-supplied `reserved_tool_ids` refused;
//!    - negotiated membership: id must be in the daemon hello
//!      `capabilities[]` (the `daemon_capabilities` set);
//!    - operator allowlist: id must be in `tool_allowlist` (missing/empty
//!      allowlist = default deny ⇒ zero admitted).
//! 3. Collision policy (AR-68 #3): two peers, same id ⇒ later peer refused
//!    (first stays bound; skip + warn). Same peer reconnect ⇒
//!    evict-then-admit (deterministic last-wins, mirroring
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

impl PeerToolTable {
    /// Create an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(TableInner {
                tools: HashMap::new(),
                sessions: HashMap::new(),
            }),
        }
    }

    /// Admit one authenticated manifest.
    ///
    /// Runs the AR-68 #2 chain: whole-manifest validation first (failure ⇒
    /// [`AdmissionOutcome::ManifestInvalid`], zero ingestion), then the
    /// per-tool named filters. `daemon_capabilities` is the daemon hello
    /// `capabilities[]` (negotiation, AR-69 #1); `tool_allowlist` is the
    /// operator allowlist (empty = default deny); `reserved_tool_ids` is
    /// the caller-computed reserved set (builtin ids + user-cap names).
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

        let mut admitted: Vec<String> = Vec::new();
        for tool in &manifest.tools {
            let id = String::from(tool.capability_id.clone());
            // (iii) named exact-id filters.
            if let Some(refusal) = refuse_tool(
                &id,
                daemon_capabilities,
                tool_allowlist,
                reserved_tool_ids,
                &inner.tools,
            ) {
                tracing::warn!(%peer_id, tool_id = %id, refusal = ?refusal, "peer tool refused at admission");
                continue;
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

/// The per-tool named refusal chain (AR-68 #2(iii)).
///
/// Order: grammar → reserved-ns → negotiated → allowlist → duplicate-peer.
/// `existing` is the current table's tool map (for the collision check).
fn refuse_tool(
    id: &str,
    daemon_capabilities: &HashSet<String>,
    tool_allowlist: &HashSet<String>,
    reserved_tool_ids: &HashSet<String>,
    existing: &HashMap<String, PeerToolEntry>,
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
    // Duplicate-id two-peer collision: later peer refused, first stays.
    if existing.contains_key(id) {
        return Some(ToolRefusal::DuplicatePeer);
    }
    None
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
