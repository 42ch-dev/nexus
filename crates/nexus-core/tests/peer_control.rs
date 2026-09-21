//! P4-T3 peer-control acceptance anchor (core connect stack).
//!
//! Invariants pinned here:
//! 1. A discovered peer tool is VISIBLE in the process-level registry but
//!    denied BEFORE invocation unless authorized — the visibility seam
//!    short-circuits a hidden-tool call and the spine deny path never
//!    reaches the responder.
//! 2. Session close clears visibility: eviction removes the dispatchable
//!    rows, so a later invoke is an honest not-found, never a dispatch.
//! 3. A late response/completion from a SUPERSEDED generation is ignored:
//!    the reconnect evicted the prior generation's rows, and a stale
//!    monitor carrying the old responder cannot evict or resurrect the
//!    replacement session's rows.
//! 4. Admission is scoped by the whole-manifest validation plus the named
//!    per-id refusal chain (negotiated → allowlist → reserved namespace):
//!    only ids that survive every gate enter the dispatchable surface, and
//!    the admitted descriptor's schemas are carried VERBATIM.
//!
//! MIGRATED (v1.193 P2-T10): invariant 4 carries the registry-level domain
//! assertions of the retired `crates/nexus-daemon-runtime/tests/peer_tool.rs`
//! cases `valid_manifest_admits_exact_id_set_with_schemas_verbatim`,
//! `grammar_reserved_negotiated_allowlist_refusals_are_named` and
//! `empty_allowlist_yields_zero_rows_table_and_catalog`; the HTTP
//! tool-executions/catalog envelopes those fixtures drove are deleted with
//! the retired Rust daemon host.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_core::connect::table::peer_tool_table;
use nexus_core::connect::visibility::VisibilityPolicy;
use nexus_core::execution::peer_tools::{invoke_peer_tool, AdmissionOutcome, PeerResponder};
use nexus_spoke_adapter::HostCapabilityManifest;
use serial_test::serial;

const PEER: &str = "peer-gen-1";
const TOOL: &str = "tools.t3.echo";

/// Admission-matrix peers/tools (invariant 4). Distinct from the
/// visibility/eviction anchors above so the process-global registry never
/// mixes the two fixtures.
const PEER_ADMITTED: &str = "peer-adm-1";
const PEER_NEGOTIATION: &str = "peer-adm-2";
const PEER_ALLOWLIST: &str = "peer-adm-3";
const PEER_RESERVED: &str = "peer-adm-4";
const PEER_UNDECLARED: &str = "peer-adm-5";
const TOOL_ALPHA: &str = "tools.adm.alpha";
const TOOL_BETA: &str = "tools.adm.beta";
const TOOL_RESERVED: &str = "tools.nexus.evil";

/// The descriptor schemas every fixture tool advertises, derived from its id
/// so a test can prove the registry carries them VERBATIM. The keys are
/// deliberately inert for the argument gate (`validate_tool_arguments` reads
/// only `type` + `required`), so an invocation with `{}` still dispatches.
fn tool_input(id: &str) -> serde_json::Value {
    serde_json::json!({ "type": "object", "title": id })
}

fn tool_output(id: &str) -> serde_json::Value {
    serde_json::json!({ "type": "object", "x-nexus-tool": id })
}

fn tool_objects(tools: &[&str]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|id| {
            serde_json::json!({
                "schema_version": 1,
                "capability_id": id,
                "op": id,
                "description": format!("{id} test tool"),
                "input": tool_input(id),
                "output": tool_output(id),
            })
        })
        .collect()
}

fn namespaces_of(tools: &[&str]) -> Vec<String> {
    tools
        .iter()
        .filter_map(|id| id.split('.').nth(1))
        .map(ToOwned::to_owned)
        .collect()
}

fn manifest_with_tools(host_id: &str, tools: &[&str]) -> HostCapabilityManifest {
    let mut capabilities: Vec<String> = vec!["spoke-baseline".to_owned()];
    capabilities.extend(tools.iter().map(|s| (*s).to_owned()));
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["daemon"],
        "capabilities": capabilities,
        "namespaces": namespaces_of(tools),
        "extensions": {},
        "tools": tool_objects(tools),
    }))
    .expect("test manifest is valid")
}

/// A manifest whose `tools[]` advertises ids its OWN `capabilities[]` does
/// not declare — the whole-manifest validation failure shape (AR-68 #2): the
/// registry must refuse the entire ingest, not the offending id alone.
fn manifest_with_undeclared_tools(host_id: &str, tools: &[&str]) -> HostCapabilityManifest {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["daemon"],
        "capabilities": ["spoke-baseline"],
        "namespaces": namespaces_of(tools),
        "extensions": {},
        "tools": tool_objects(tools),
    }))
    .expect("test manifest is valid")
}

/// Counting responder: proves a denial happened BEFORE any peer effect.
#[derive(Default)]
struct CountingResponder {
    invocations: AtomicUsize,
}

impl CountingResponder {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn count(&self) -> usize {
        self.invocations.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PeerResponder for CountingResponder {
    async fn invoke_tool(
        &self,
        _tool_id: &str,
        _arguments: serde_json::Value,
    ) -> nexus_core::execution::peer_tools::PeerInvokeResult {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({ "echo": true }))
    }

    fn peer_id(&self) -> &str {
        PEER
    }
}

fn empty_set() -> std::collections::HashSet<String> {
    std::collections::HashSet::new()
}

#[tokio::test]
async fn visible_peer_tool_still_requires_authorization() {
    let registry = peer_tool_table();
    let responder = CountingResponder::new();

    // Admission: the peer tool enters the dispatchable surface.
    // The operator allowlist is default-deny: admitting the anchor tool
    // requires naming it (the AR-68 #2(iii) contract).
    let allowlist: std::collections::HashSet<String> = std::iter::once(TOOL.to_string()).collect();
    // The hello-negotiated capabilities must carry the id (Layer-1 refusal
    // `not_negotiated` otherwise).
    let negotiated: std::collections::HashSet<String> = std::iter::once(TOOL.to_string()).collect();
    let outcome = registry.admit_and_register(
        PEER,
        &manifest_with_tools(PEER, &[TOOL]),
        &(Arc::clone(&responder) as Arc<dyn PeerResponder>),
        &negotiated,
        &allowlist,
        &empty_set(),
    );
    let AdmissionOutcome::Admitted { tool_ids } = outcome else {
        panic!("admission must succeed for the anchor peer");
    };
    assert_eq!(tool_ids, vec![TOOL.to_string()]);
    assert!(
        registry.get(TOOL).is_some(),
        "the admitted tool is visible in the registry"
    );

    // An operator visibility policy that does NOT name the peer tool hides
    // it from the consumer: the seam refuses the call BEFORE the backend —
    // and therefore before any responder invocation.
    let hiding = VisibilityPolicy::from_visible(vec!["tools.other.unrelated".to_owned()]);
    assert!(
        !hiding.is_visible(TOOL),
        "a tool outside the visible subset is hidden"
    );
    assert_eq!(
        responder.count(),
        0,
        "a hidden tool is denied before any invocation"
    );

    // Authorized dispatch: the absent policy (all visible, byte-identical
    // current behavior) lets the call reach the responder exactly once.
    let entry = registry.get(TOOL).expect("entry still dispatchable");
    let invoked = invoke_peer_tool(&entry, serde_json::json!({})).await;
    assert!(
        invoked.is_ok(),
        "the authorized invocation reaches the peer"
    );
    assert_eq!(responder.count(), 1, "exactly one peer invocation");

    // Session close clears visibility: eviction removes the dispatchable
    // rows, so a later invoke is an honest not-found, never a dispatch.
    let evicted = registry.evict_peer(
        PEER,
        Some(&(Arc::clone(&responder) as Arc<dyn PeerResponder>)),
    );
    assert!(evicted, "the live session's own close evicts its rows");
    assert!(
        registry.get(TOOL).is_none(),
        "visibility is cleared by session close"
    );
    // The registry-side lookup is the dispatch authority: after close the
    // row is gone, so a late consumer cannot resolve or resurrect it.
    assert!(registry.get(TOOL).is_none());
    assert_eq!(
        responder.count(),
        1,
        "the closed session performs no further peer invocations"
    );

    // Late response ignored by generation: a reconnect is a NEW generation
    // (deterministic last-wins evicts the prior rows); the superseded
    // generation's monitor carrying the OLD responder can neither evict the
    // replacement nor resurrect its own rows.
    let replacement = CountingResponder::new();
    let outcome = registry.admit_and_register(
        PEER,
        &manifest_with_tools(PEER, &[TOOL]),
        &(Arc::clone(&replacement) as Arc<dyn PeerResponder>),
        &negotiated,
        &allowlist,
        &empty_set(),
    );
    let AdmissionOutcome::Admitted { .. } = outcome else {
        panic!("reconnect admission must succeed");
    };
    let stale_evict = registry.evict_peer(
        PEER,
        Some(&(Arc::clone(&responder) as Arc<dyn PeerResponder>)),
    );
    assert!(
        !stale_evict,
        "a stale generation's monitor cannot evict the replacement session"
    );
    assert!(
        registry.get(TOOL).is_some(),
        "the replacement generation's rows stay live"
    );
    assert_eq!(
        replacement.count(),
        0,
        "no late response performed any replacement-generation effect"
    );
}

fn set_of(ids: &[&str]) -> std::collections::HashSet<String> {
    ids.iter().map(|s| (*s).to_owned()).collect()
}

/// MIGRATED (v1.193 P2-T10) from
/// `crates/nexus-daemon-runtime/tests/peer_tool.rs`:
/// `valid_manifest_admits_exact_id_set_with_schemas_verbatim`,
/// `grammar_reserved_negotiated_allowlist_refusals_are_named`,
/// `empty_allowlist_yields_zero_rows_table_and_catalog`.
///
/// Preserved: the whole-manifest validation runs FIRST and refuses the
/// entire ingest (zero rows); the per-id refusal chain (negotiated →
/// operator allowlist → reserved namespace) is what narrows a manifest to
/// its dispatchable set; an admitted id carries its descriptor's schemas and
/// description VERBATIM; an empty allowlist is default-deny.
///
/// Deleted with the retired host: the `POST …/tool-executions` and
/// `GET /v1/daemon/tools` envelopes those fixtures asserted around the same
/// registry, and the daemon `WorkspaceState` router they booted.
#[tokio::test]
#[serial]
async fn peer_tool_admission_is_scoped_by_manifest_negotiation_allowlist_and_reservations() {
    let registry = peer_tool_table();
    let responder = CountingResponder::new();
    let responder_dyn = Arc::clone(&responder) as Arc<dyn PeerResponder>;

    // (1) Whole-manifest validation first: a manifest advertising an id its
    //     own `capabilities[]` does not declare refuses the WHOLE ingest —
    //     no row, no session bookkeeping.
    let outcome = registry.admit_and_register(
        PEER_UNDECLARED,
        &manifest_with_undeclared_tools(PEER_UNDECLARED, &[TOOL_ALPHA]),
        &responder_dyn,
        &set_of(&[TOOL_ALPHA]),
        &set_of(&[TOOL_ALPHA]),
        &empty_set(),
    );
    let AdmissionOutcome::ManifestInvalid { message } = outcome else {
        panic!("an undeclared tool must refuse the whole manifest, got {outcome:?}");
    };
    assert!(
        message.contains("missing from manifest capabilities[]"),
        "the refusal names the declaration gap: {message}"
    );
    assert!(
        registry.get(TOOL_ALPHA).is_none(),
        "a refused manifest ingests nothing"
    );
    assert!(
        registry.peer_tool_ids(PEER_UNDECLARED).is_empty(),
        "a refused manifest registers no session rows"
    );

    // (2) Not negotiated (absent from the daemon hello): refused even when
    //     the operator allowlist names it.
    let outcome = registry.admit_and_register(
        PEER_NEGOTIATION,
        &manifest_with_tools(PEER_NEGOTIATION, &[TOOL_ALPHA]),
        &responder_dyn,
        &empty_set(),
        &set_of(&[TOOL_ALPHA]),
        &empty_set(),
    );
    assert_eq!(
        outcome,
        AdmissionOutcome::Admitted {
            tool_ids: Vec::new()
        },
        "an un-negotiated id admits nothing"
    );
    assert!(
        registry.get(TOOL_ALPHA).is_none(),
        "an un-negotiated id leaves zero dispatchable rows"
    );
    assert!(registry.peer_tool_ids(PEER_NEGOTIATION).is_empty());

    // (3) Default deny: an empty operator allowlist admits zero ids even
    //     though the id is negotiated and the peer is authenticated.
    let outcome = registry.admit_and_register(
        PEER_ALLOWLIST,
        &manifest_with_tools(PEER_ALLOWLIST, &[TOOL_ALPHA]),
        &responder_dyn,
        &set_of(&[TOOL_ALPHA]),
        &empty_set(),
        &empty_set(),
    );
    assert_eq!(
        outcome,
        AdmissionOutcome::Admitted {
            tool_ids: Vec::new()
        }
    );
    assert!(
        registry.get(TOOL_ALPHA).is_none(),
        "zero rows for an empty operator allowlist"
    );
    assert!(registry.peer_tool_ids(PEER_ALLOWLIST).is_empty());

    // (4) Reserved namespace: the daemon-owned `tools.nexus.*` family is
    //     refused even when negotiated AND explicitly allowlisted.
    let outcome = registry.admit_and_register(
        PEER_RESERVED,
        &manifest_with_tools(PEER_RESERVED, &[TOOL_RESERVED]),
        &responder_dyn,
        &set_of(&[TOOL_RESERVED]),
        &set_of(&[TOOL_RESERVED]),
        &empty_set(),
    );
    assert_eq!(
        outcome,
        AdmissionOutcome::Admitted {
            tool_ids: Vec::new()
        }
    );
    assert!(
        registry.get(TOOL_RESERVED).is_none(),
        "a reserved-namespace id never becomes dispatchable"
    );

    // (5) Exact-id admission: both advertised ids are negotiated, but the
    //     operator allowlist names only one — the admitted set is exactly
    //     that id, and its descriptor is carried verbatim.
    let outcome = registry.admit_and_register(
        PEER_ADMITTED,
        &manifest_with_tools(PEER_ADMITTED, &[TOOL_ALPHA, TOOL_BETA]),
        &responder_dyn,
        &set_of(&[TOOL_ALPHA, TOOL_BETA]),
        &set_of(&[TOOL_ALPHA]),
        &empty_set(),
    );
    assert_eq!(
        outcome,
        AdmissionOutcome::Admitted {
            tool_ids: vec![TOOL_ALPHA.to_owned()]
        },
        "the allowlist narrows the negotiated manifest to its exact id set"
    );
    let entry = registry
        .get(TOOL_ALPHA)
        .expect("admitted tool is registered");
    assert_eq!(entry.peer_id, PEER_ADMITTED);
    assert_eq!(
        String::from(entry.descriptor.capability_id.clone()),
        TOOL_ALPHA
    );
    assert_eq!(
        entry.descriptor.input,
        tool_input(TOOL_ALPHA)
            .as_object()
            .cloned()
            .expect("fixture input is an object"),
        "the manifest's input schema is carried verbatim"
    );
    assert_eq!(
        entry.descriptor.output,
        tool_output(TOOL_ALPHA)
            .as_object()
            .cloned()
            .expect("fixture output is an object"),
        "the manifest's output schema is carried verbatim"
    );
    assert_eq!(
        String::from(entry.descriptor.description.clone()),
        format!("{TOOL_ALPHA} test tool")
    );
    assert_eq!(
        registry.peer_tool_ids(PEER_ADMITTED),
        vec![TOOL_ALPHA.to_owned()],
        "the session records exactly the admitted ids"
    );
    assert!(
        registry.get(TOOL_BETA).is_none(),
        "an id outside the operator allowlist is never dispatchable"
    );

    // The authorized dispatch still reaches the peer exactly once.
    let invoked = invoke_peer_tool(&entry, serde_json::json!({})).await;
    assert!(invoked.is_ok(), "the admitted row dispatches: {invoked:?}");
    assert_eq!(responder.count(), 1);

    // Cleanup: leave no rows in the process-global registry.
    for peer_id in [
        PEER_ADMITTED,
        PEER_NEGOTIATION,
        PEER_ALLOWLIST,
        PEER_RESERVED,
        PEER_UNDECLARED,
    ] {
        registry.evict_peer(peer_id, None);
    }
    assert!(
        registry.get(TOOL_ALPHA).is_none(),
        "the admission-matrix fixture leaves no rows behind"
    );
}
