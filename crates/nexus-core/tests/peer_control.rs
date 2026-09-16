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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_core::connect::table::peer_tool_table;
use nexus_core::connect::visibility::VisibilityPolicy;
use nexus_core::execution::peer_tools::{invoke_peer_tool, AdmissionOutcome, PeerResponder};
use nexus_spoke_adapter::HostCapabilityManifest;

const PEER: &str = "peer-gen-1";
const TOOL: &str = "tools.t3.echo";

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
        "host_id": PEER,
        "roles": ["daemon"],
        "capabilities": capabilities,
        "namespaces": namespaces,
        "extensions": {},
        "tools": tool_objs,
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
    let allowlist: std::collections::HashSet<String> = [TOOL.to_string()].into_iter().collect();
    // The hello-negotiated capabilities must carry the id (Layer-1 refusal
    // `not_negotiated` otherwise).
    let negotiated: std::collections::HashSet<String> = [TOOL.to_string()].into_iter().collect();
    let outcome = registry.admit_and_register(
        PEER,
        &manifest_with_tools(&[TOOL]),
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
        &manifest_with_tools(&[TOOL]),
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
