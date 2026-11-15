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
use nexus_connect::peer_tools::{invoke_peer_tool, AdmissionOutcome, PeerResponder};
use nexus_connect::table::peer_tool_table;
use nexus_connect::visibility::VisibilityPolicy;
use nexus_contracts::generated::core::{
    CorePeerControlOptions, CorePeerControlOptionsAllowedOperationsItem, CorePeerControlRequest,
    CorePeerControlRequestOperation, CorePeerControlRequestPeerId,
};
use nexus_contracts::{
    CoreError as WireCoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply,
};
use nexus_core::execution::{ExecutionHandle, RunnerDeps};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_provider_ports::{ProviderPort, ProviderResult};
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
    ) -> nexus_connect::peer_tools::PeerInvokeResult {
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
    assert_eq!(
        registry.peer_tool_ids(PEER_NEGOTIATION),
        [] as [std::string::String; 0]
    );

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
    assert_eq!(
        registry.peer_tool_ids(PEER_ALLOWLIST),
        [] as [std::string::String; 0]
    );

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

// ── Peer-control lane state transitions (v1.190 P4-T3) ────────────────────
//
// The two cases below drive the lane the `ExecutionHandle` owns, not the
// registry: a start REPLACES the allowlist (so a narrowing restart drops the
// earlier grants) and `allow` grants the operation its payload names.

const LANE_CREATOR: &str = "peer-lane-creator";
const LANE_SLUG: &str = "default";
const LANE_PEER: &str = "peer-lane-1";

/// A provider that never performs an effect: the lane cases drive no run.
struct LaneNullProvider;

#[async_trait]
impl ProviderPort for LaneNullProvider {
    async fn call(&self, _request: ProviderCall) -> ProviderResult<ProviderReply> {
        Err(WireCoreError {
            code: CoreErrorCode::Internal,
            message: "null provider: no live model in this test".to_string(),
            details: serde_json::Map::default(),
            http_status: Some(500),
        })
    }

    async fn next(
        &self,
        operation_id: String,
        _max_events: u32,
        _max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        Ok(ProviderEventBatch {
            operation_id,
            events: vec![],
            has_more: false,
            gap: None,
        })
    }
}

struct LaneFixture {
    core: CoreService,
    handle: Arc<ExecutionHandle>,
    _tmp: tempfile::TempDir,
}

/// Open an engine-owner core over a seeded home and start its execution owner,
/// exactly as the daemon does — the lane lives on that owner.
async fn lane_fixture() -> LaneFixture {
    let tmp = tempfile::TempDir::new().expect("temp home");
    let user_home = tmp.path().to_path_buf();
    let home = user_home.join(".nexus42");
    std::fs::create_dir_all(&home).expect("nexus home");
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home,
        LANE_CREATOR,
        LANE_SLUG,
    ))
    .expect("workspace dir");
    std::fs::write(
        home.join("config.toml"),
        format!(
            "active_creator_id = \"{LANE_CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{LANE_CREATOR}\" = \"{LANE_SLUG}\"\n"
        ),
    )
    .expect("config.toml");
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, LANE_CREATOR, LANE_SLUG);
    {
        // The seed guard holds the DB's engine lock; it must be gone before
        // the engine-owner open below claims that lock itself.
        let guarded = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("engine pool init");
        nexus_local_db::ensure_creator_row(guarded.pool(), LANE_CREATOR, "Test")
            .await
            .expect("seed the admitted creator and its holder");
        guarded.pool().close().await;
        drop(guarded);
    }
    nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("engine-owner core open");
    let deps = RunnerDeps {
        workspace_root: Some(nexus_home_layout::operational_workspace_dir(
            &user_home,
            LANE_CREATOR,
            LANE_SLUG,
        )),
        nexus_home: Some(home),
        ..RunnerDeps::default()
    };
    let handle = core
        .start_execution(Arc::new(LaneNullProvider) as Arc<dyn ProviderPort>, deps)
        .await
        .expect("execution owner starts");
    LaneFixture {
        core,
        handle,
        _tmp: tmp,
    }
}

fn start_options(enabled: bool, allowed: &[&str]) -> CorePeerControlOptions {
    CorePeerControlOptions {
        allowed_operations: allowed
            .iter()
            .map(|operation| {
                CorePeerControlOptionsAllowedOperationsItem::try_from(*operation)
                    .expect("fixture operations are non-empty")
            })
            .collect(),
        enabled,
    }
}

/// A control request; `grants` names the operation an `allow` request adds to
/// the allowlist (the request's own `operation` is `"allow"`).
fn control_request(operation: &str, grants: Option<&str>) -> CorePeerControlRequest {
    let mut payload = serde_json::Map::new();
    if let Some(granted) = grants {
        payload.insert(
            "operation".to_string(),
            serde_json::Value::String(granted.to_string()),
        );
    }
    CorePeerControlRequest {
        operation: CorePeerControlRequestOperation::try_from(operation)
            .expect("fixture operations are non-empty"),
        payload,
        peer_id: CorePeerControlRequestPeerId::try_from(LANE_PEER)
            .expect("fixture peer id is non-empty"),
    }
}

const fn forbidden(
    result: &Result<nexus_contracts::generated::core::CorePeerControlState, CoreError>,
) -> bool {
    matches!(result, Err(CoreError::Forbidden { .. }))
}

/// A restart REPLACES the allowlist: the operation the narrower start omits is
/// refused afterwards. Additive install would keep the earlier grant.
#[tokio::test]
#[serial]
async fn peer_control_restart_replaces_the_allowlist() {
    let f = lane_fixture().await;
    let principal = f.core.active_principal().await.expect("active principal");
    f.handle
        .start_peer_control(&principal, start_options(true, &["status", "evict"]))
        .await
        .expect("the lane starts");
    let evict = control_request("evict", None);
    assert!(
        !forbidden(&f.handle.peer_control(&principal, evict.clone()).await),
        "the first start admits evict (its refusal here is not the allowlist)"
    );
    assert!(
        f.handle
            .peer_control(&principal, control_request("status", None))
            .await
            .is_ok(),
        "the first start admits status"
    );

    f.handle
        .start_peer_control(&principal, start_options(true, &["status"]))
        .await
        .expect("the lane restarts");

    let dropped = f.handle.peer_control(&principal, evict).await;
    assert!(
        forbidden(&dropped),
        "a restart that omits evict must drop the earlier grant, got {dropped:?}"
    );
    assert!(
        f.handle
            .peer_control(&principal, control_request("status", None))
            .await
            .is_ok(),
        "the narrowing start keeps the operation it names"
    );
}

/// `allow` grants the operation its payload names — not the literal `allow` —
/// so a later call to that operation passes the allowlist gate.
#[tokio::test]
#[serial]
async fn peer_control_allow_grants_the_named_operation() {
    let f = lane_fixture().await;
    let principal = f.core.active_principal().await.expect("active principal");
    f.handle
        .start_peer_control(&principal, start_options(true, &["allow"]))
        .await
        .expect("the lane starts");

    let status = control_request("status", None);
    assert!(
        forbidden(&f.handle.peer_control(&principal, status.clone()).await),
        "status is not on the allowlist yet"
    );

    let granted = f
        .handle
        .peer_control(&principal, control_request("allow", Some("status")))
        .await;
    assert!(granted.is_ok(), "allow itself is allowlisted: {granted:?}");

    assert!(
        f.handle.peer_control(&principal, status).await.is_ok(),
        "allow must grant the operation its payload names"
    );
    assert!(
        forbidden(
            &f.handle
                .peer_control(&principal, control_request("evict", None))
                .await
        ),
        "an operation allow never named stays refused"
    );
    assert!(
        matches!(
            f.handle
                .peer_control(&principal, control_request("allow", None))
                .await,
            Err(CoreError::InvalidInput { .. })
        ),
        "allow without a payload-named operation is a validation refusal, not a silent grant"
    );
}

/// A restart that lands while `peer_control` is resolving its lane must not
/// swallow the operation: the effect must reach the lane that is installed and
/// the returned state must describe that lane. The seam parks the call at its
/// lane-resolution boundary; the restart runs while it is parked.
#[tokio::test]
#[serial]
async fn peer_control_acts_on_the_lane_installed_when_it_resolves() {
    use nexus_core::execution::test_hooks::{set_peer_control_gate, PeerControlGate};

    let f = lane_fixture().await;
    let principal = f.core.active_principal().await.expect("active principal");
    let allowed = ["disable", "status"];
    f.handle
        .start_peer_control(&principal, start_options(true, &allowed))
        .await
        .expect("the lane starts");

    let gate = Arc::new(PeerControlGate::new());
    set_peer_control_gate(Some(Arc::clone(&gate)));
    let disable = f
        .handle
        .peer_control(&principal, control_request("disable", None));
    // `join!` polls left to right, so `disable` reaches the seam before the
    // restart installs the fresh lane.
    let restart = async {
        gate.admitted.notified().await;
        let restarted = f
            .handle
            .start_peer_control(&principal, start_options(true, &allowed))
            .await;
        gate.proceed.notify_one();
        restarted
    };
    let (disabled, restarted) = tokio::join!(disable, restart);
    set_peer_control_gate(None);
    restarted.expect("the restart installs a fresh enabled lane");
    let returned = disabled.expect("disable is admitted by the allowlist");
    let installed = f
        .handle
        .peer_control(&principal, control_request("status", None))
        .await
        .expect("status is admitted by the allowlist");
    assert_eq!(
        returned.enabled, installed.enabled,
        "the returned state must describe the installed lane: returned {returned:?}, installed {installed:?}"
    );
    assert!(
        !installed.enabled,
        "the disable must reach the installed lane, got {installed:?}"
    );
}

/// `enabled` is reported state, never a second gate: a lane started disabled
/// still serves an allowlisted operation, and its state says the cohort is off.
#[tokio::test]
#[serial]
async fn peer_control_admits_the_allowlist_when_the_lane_reports_disabled() {
    let f = lane_fixture().await;
    let principal = f.core.active_principal().await.expect("active principal");
    let started = f
        .handle
        .start_peer_control(&principal, start_options(false, &["status"]))
        .await
        .expect("the lane starts");
    assert!(!started.enabled, "the start reports the disabled lane");

    let reported = f
        .handle
        .peer_control(&principal, control_request("status", None))
        .await
        .expect("the allowlist alone admits status on a disabled lane");
    assert!(
        !reported.enabled,
        "the lane reports its enablement, it does not gate the allowlist"
    );
}

/// `allow` refuses a `payload.operation` no arm dispatches: a grant nothing
/// would ever match is not a permission, and the refusal names the field.
#[tokio::test]
#[serial]
async fn peer_control_allow_rejects_an_undispatched_operation() {
    let f = lane_fixture().await;
    let principal = f.core.active_principal().await.expect("active principal");
    f.handle
        .start_peer_control(&principal, start_options(true, &["allow"]))
        .await
        .expect("the lane starts");

    let refused = f
        .handle
        .peer_control(&principal, control_request("allow", Some("frobnicate")))
        .await;
    assert!(
        matches!(
            &refused,
            Err(CoreError::InvalidInput { field, .. }) if field.as_str() == "payload.operation"
        ),
        "an undispatched grant target is a payload.operation refusal, got {refused:?}"
    );
    assert!(
        forbidden(
            &f.handle
                .peer_control(&principal, control_request("status", None))
                .await
        ),
        "the refused grant must not authorize the name it carried"
    );
}

/// A start's allowlist is a grant surface too: naming an operation no arm
/// dispatches would authorize nothing forever, so the start refuses it instead
/// of installing a lane that holds it.
#[tokio::test]
#[serial]
async fn peer_control_start_rejects_an_undispatched_operation() {
    let f = lane_fixture().await;
    let principal = f.core.active_principal().await.expect("active principal");
    let refused = f
        .handle
        .start_peer_control(&principal, start_options(true, &["frobnicate"]))
        .await;
    assert!(
        matches!(
            &refused,
            Err(CoreError::InvalidInput { field, .. }) if field.as_str() == "allowed_operations"
        ),
        "an undispatched start allowlist entry is an allowed_operations refusal, got {refused:?}"
    );

    // The refusal precedes the install, so no lane is left behind and the
    // next control call reports the lane-state cause, not a `peer_id`.
    let unstarted = f
        .handle
        .peer_control(&principal, control_request("status", None))
        .await;
    assert!(
        matches!(
            &unstarted,
            Err(CoreError::InvalidInput { field, .. }) if field.as_str() == "lane"
        ),
        "a refused start installs no lane and names the lane, got {unstarted:?}"
    );
}
