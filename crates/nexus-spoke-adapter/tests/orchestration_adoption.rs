// contract — docs/specs/architecture/spoke-adapter-architecture.md.
//! Programmatic Surface B adoption test — the test-rig twin of
//! `examples/baseline_adapter.rs`.
//!
//! Reuses the example's [`NexusBaselineMock`] via a `#[path]`-import and
//! exercises every orchestrate_* entrypoint the adapter re-exports across
//! happy + CAS-reject + capability-missing paths. The mock owns storage I/O
//! only; every lifecycle invariant stays in `spoke_operations` — the
//! call-boundary invariant (spec §7) is preserved.
//!
//! The CAS distinction (spec §7.3 `put_knowledge_entry` CAS contract) is
//! demonstrated in both directions through `orchestrate_upsert`: a one-shot
//! get-override simulates a get/put race so the mock's adapter-owned CAS
//! fires `STORED_REVISION_STALE` when the store is ahead and
//! `REVISION_CONFLICT` when the caller claims an impossible future revision.

#![allow(dead_code)]
#![allow(unused_imports)]

#[path = "../examples/baseline_adapter.rs"]
#[allow(dead_code, unused_imports)]
mod baseline_adapter;

use async_trait::async_trait;
use baseline_adapter::{
    assemble_request, knowledge_entry, knowledge_entry_wire, promote_request, NexusBaselineMock,
};
use nexus_spoke_adapter::{
    orchestrate_assemble, orchestrate_check, orchestrate_project, orchestrate_promote,
    orchestrate_upsert, AssembleResponse, CheckRequest, CheckResponse, ComputablePort,
    ComputablePorts, FindingPort, HostManifestPort, KnowledgeEntry, KnowledgeEntryPort,
    KnowledgeQueryPort, ProjectRequest, PromoteResponse, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult, UpsertRequest, UpsertResponse,
};
use serde_json::{json, Value};
use spoke_operations::spoke_ok;

const SEEDED_ENTRY_ID: &str = "kb_mira";
const SEEDED_REVISION: u64 = 2;

/// Fresh mock seeded with one provisional `KnowledgeEntry` (revision 2).
fn seeded_mock() -> NexusBaselineMock {
    NexusBaselineMock::with_seeded_entry(knowledge_entry(SEEDED_ENTRY_ID, SEEDED_REVISION))
}

fn upsert_request(entries: &[Value]) -> UpsertRequest {
    serde_json::from_value(json!({ "knowledge_entries": entries })).expect("valid UpsertRequest")
}

/// Wire shape for an upsert candidate with the requested id / revision.
fn upsert_entry_wire(entry_id: &str, revision: u64) -> Value {
    json!({
        "schema_version": 1,
        "entry_id": entry_id,
        "entry_type": "character",
        "canonical_name": "Mira Vale",
        "status": "provisional",
        "revision": revision,
        "body": { "summary": "Protagonist of the inaugural arc" },
        "extensions": {}
    })
}

fn check_request(scope_id: &str, entry_id: &str) -> CheckRequest {
    serde_json::from_value(json!({
        "scope": { "scope_id": scope_id, "entry_ids": [entry_id] }
    }))
    .expect("valid CheckRequest fixture")
}

fn project_request(entry_id: &str) -> ProjectRequest {
    serde_json::from_value(json!({
        "session_id": "sess_baseline",
        "entry_id": entry_id,
        "state": { "anchor": "inaugural-arc" }
    }))
    .expect("valid ProjectRequest fixture")
}

#[tokio::test]
async fn orchestrate_upsert_happy_create_path() {
    let mock = seeded_mock();
    let new_id = "kb_aran";
    let request = upsert_request(&[upsert_entry_wire(new_id, 0)]);

    let result = orchestrate_upsert(&mock, request).await;
    match result {
        SpokeResult::Ok(UpsertResponse::Variant0 {
            knowledge_entries, ..
        }) => {
            assert_eq!(knowledge_entries.len(), 1);
            assert_eq!(knowledge_entries[0].entry_id, new_id);
        }
        _ => panic!("expected upsert success, got {result:?}"),
    }

    let stored = mock
        .get_knowledge_entry(new_id)
        .await
        .expect_ok("stored after upsert");
    assert_eq!(stored.entry_id, new_id);
    assert_eq!(stored.revision, Some(0));
}

#[tokio::test]
async fn orchestrate_upsert_cas_reject_create_when_exists() {
    let mock = seeded_mock();
    mock.mask_next_get(SEEDED_ENTRY_ID);

    let request = upsert_request(&[upsert_entry_wire(SEEDED_ENTRY_ID, 0)]);

    let result = orchestrate_upsert(&mock, request).await;
    expect_reject_with_code(result, SpokeRejectCode::KnowledgeEntryAlreadyExists);
}

#[tokio::test]
async fn orchestrate_upsert_happy_update_path() {
    let mock = seeded_mock();
    let updated_wire = json!({
        "schema_version": 1,
        "entry_id": SEEDED_ENTRY_ID,
        "entry_type": "character",
        "canonical_name": "Mira Vale (revised)",
        "status": "provisional",
        "revision": SEEDED_REVISION,
        "body": { "summary": "Protagonist of the inaugural arc" },
        "extensions": {}
    });
    let request = upsert_request(&[updated_wire]);

    let result = orchestrate_upsert(&mock, request).await;
    match result {
        SpokeResult::Ok(UpsertResponse::Variant0 {
            knowledge_entries, ..
        }) => {
            assert_eq!(knowledge_entries.len(), 1);
            assert_eq!(knowledge_entries[0].entry_id, SEEDED_ENTRY_ID);
            assert_eq!(knowledge_entries[0].revision, Some(SEEDED_REVISION));
        }
        _ => panic!("expected upsert success, got {result:?}"),
    }

    let stored = mock
        .get_knowledge_entry(SEEDED_ENTRY_ID)
        .await
        .expect_ok("stored after upsert");
    assert_eq!(stored.revision, Some(SEEDED_REVISION));
    assert_eq!(stored.canonical_name.as_str(), "Mira Vale (revised)");
}

#[tokio::test]
async fn orchestrate_upsert_cas_reject_stale_revision() {
    let mock = NexusBaselineMock::with_seeded_entry(knowledge_entry(SEEDED_ENTRY_ID, 3));
    mock.override_next_get_revision(SEEDED_ENTRY_ID, 1);

    let request = upsert_request(&[upsert_entry_wire(SEEDED_ENTRY_ID, 1)]);

    let result = orchestrate_upsert(&mock, request).await;
    expect_reject_with_code(result, SpokeRejectCode::StoredRevisionStale);
}

// ── 5. orchestrate_upsert — CAS reject: stored < expected (CONFLICT) ────
//
// This is the T2-review minor finding made concrete: the mock previously
// collapsed *all* revision mismatches into `STORED_REVISION_STALE`. The
// patched mock now distinguishes the two reject paths per spec §7.3:
//   - stored > expected → `STORED_REVISION_STALE` (test #4 above)
//   - stored < expected → `REVISION_CONFLICT` (this test)

#[tokio::test]
async fn orchestrate_upsert_cas_reject_conflict() {
    let mock = NexusBaselineMock::with_seeded_entry(knowledge_entry(SEEDED_ENTRY_ID, 1));
    mock.override_next_get_revision(SEEDED_ENTRY_ID, 5);

    let request = upsert_request(&[upsert_entry_wire(SEEDED_ENTRY_ID, 5)]);

    let result = orchestrate_upsert(&mock, request).await;
    expect_reject_with_code(result, SpokeRejectCode::RevisionConflict);
}

#[tokio::test]
async fn orchestrate_promote_happy_path() {
    // The orchestrator's `assert_revision_match(2, 2)` passes, the promote
    // gate validates, and the mock's CAS sees stored(2) == expected(2) →
    // accepts. Persisted entry is `confirmed` at revision 3.
    let mock = seeded_mock();

    let result =
        orchestrate_promote(&mock, promote_request(SEEDED_ENTRY_ID, SEEDED_REVISION)).await;
    match result {
        SpokeResult::Ok(PromoteResponse::Variant0 {
            knowledge_entry, ..
        }) => {
            assert_eq!(knowledge_entry.entry_id, SEEDED_ENTRY_ID);
            assert_eq!(knowledge_entry.status, "confirmed");
            assert_eq!(knowledge_entry.revision, Some(SEEDED_REVISION + 1));
        }
        _ => panic!("expected promote success, got {result:?}"),
    }
}

#[tokio::test]
async fn orchestrate_promote_cas_reject_stale() {
    let mock = seeded_mock();

    let result =
        orchestrate_promote(&mock, promote_request(SEEDED_ENTRY_ID, SEEDED_REVISION - 1)).await;
    expect_reject_with_code(result, SpokeRejectCode::StoredRevisionStale);
}

#[tokio::test]
async fn orchestrate_assemble_happy_path() {
    let mock = seeded_mock();

    let result =
        orchestrate_assemble(&mock, assemble_request("world_1", SEEDED_ENTRY_ID), None).await;
    match result {
        SpokeResult::Ok(AssembleResponse::Variant0 { packet, .. }) => {
            assert_eq!(packet.packet_id, "assemble:world_1");
            assert_eq!(packet.entries.len(), 1);
            assert_eq!(packet.entries[0].entry_id, SEEDED_ENTRY_ID);
        }
        _ => panic!("expected assemble success, got {result:?}"),
    }
}

#[tokio::test]
async fn orchestrate_check_happy_path() {
    let mock = seeded_mock();
    let request = check_request("world_1", SEEDED_ENTRY_ID);

    let result = orchestrate_check(&mock, request, |input| {
        assert_eq!(input.entries.len(), 1);
        assert_eq!(input.entries[0].entry_id, SEEDED_ENTRY_ID);
        assert!(input.events.is_empty());
        assert!(input.rules.is_empty());
        spoke_ok(Vec::new())
    })
    .await;

    match result {
        SpokeResult::Ok(CheckResponse::Variant0 { findings, .. }) => {
            assert!(findings.is_empty());
        }
        _ => panic!("expected check success, got {result:?}"),
    }
}

struct BaselineOnlyPorts(NexusBaselineMock);

#[async_trait]
impl KnowledgeEntryPort for BaselineOnlyPorts {
    async fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        self.0.get_knowledge_entry(entry_id).await
    }
    async fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        self.0
            .put_knowledge_entry(entry, expected_base_revision)
            .await
    }
}

#[async_trait]
impl nexus_spoke_adapter::RelationPort for BaselineOnlyPorts {
    async fn get_relation(&self, relation_id: &str) -> SpokeResult<nexus_spoke_adapter::Relation> {
        self.0.get_relation(relation_id).await
    }

    async fn put_relation(
        &self,
        relation: nexus_spoke_adapter::Relation,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<nexus_spoke_adapter::Relation> {
        self.0.put_relation(relation, expected_base_revision).await
    }
}

#[async_trait]
impl KnowledgeQueryPort for BaselineOnlyPorts {
    async fn list_knowledge_entries(
        &self,
        scope: &nexus_spoke_adapter::Scope,
    ) -> SpokeResult<Vec<KnowledgeEntry>> {
        self.0.list_knowledge_entries(scope).await
    }
}

#[async_trait]
impl ScopeQueryPort for BaselineOnlyPorts {
    async fn list_timeline_events(
        &self,
        scope: &nexus_spoke_adapter::Scope,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::TimelineEvent>> {
        self.0.list_timeline_events(scope).await
    }
}

#[async_trait]
impl FindingPort for BaselineOnlyPorts {
    async fn put_findings(
        &self,
        findings: Vec<nexus_spoke_adapter::Finding>,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::Finding>> {
        self.0.put_findings(findings).await
    }
}

#[async_trait]
impl RuleQueryPort for BaselineOnlyPorts {
    async fn list_rules(
        &self,
        rule_refs: &[String],
    ) -> SpokeResult<Vec<nexus_spoke_adapter::Rule>> {
        self.0.list_rules(rule_refs).await
    }
}

#[async_trait]
impl HostManifestPort for BaselineOnlyPorts {
    async fn get_host_capability_manifest(
        &self,
    ) -> SpokeResult<nexus_spoke_adapter::HostCapabilityManifest> {
        self.0.get_host_capability_manifest().await
    }
    async fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::HostCapabilityManifest>> {
        self.0.list_peer_host_capability_manifests().await
    }
}

impl ComputablePorts for BaselineOnlyPorts {
    fn as_computable(&self) -> Option<&dyn ComputablePort> {
        None
    }
}

#[tokio::test]
async fn capability_missing_reject() {
    let ports = BaselineOnlyPorts(seeded_mock());

    let result = orchestrate_project(&ports, project_request(SEEDED_ENTRY_ID)).await;
    expect_reject_with_code(result, SpokeRejectCode::CapabilityPortMissing);
}

fn expect_reject_with_code<T: std::fmt::Debug>(result: SpokeResult<T>, code: SpokeRejectCode) {
    match result {
        SpokeResult::Reject(reject) => {
            assert_eq!(
                reject.code, code,
                "reject code mismatch (message: {})",
                reject.message
            );
        }
        SpokeResult::Ok(_) => panic!("expected reject {code:?}, got Ok"),
    }
}

/// Unwrap a `SpokeResult<KnowledgeEntry>` the way `Result::expect` would —
/// `SpokeResult` is a custom enum (not `Result`), so `?` / `.expect` don't
/// apply directly.
trait ExpectOkEntry {
    fn expect_ok(self, context: &str) -> KnowledgeEntry;
}

impl ExpectOkEntry for SpokeResult<KnowledgeEntry> {
    fn expect_ok(self, context: &str) -> KnowledgeEntry {
        match self {
            Self::Ok(entry) => entry,
            Self::Reject(reject) => panic!("{context}: {reject:?}"),
        }
    }
}

const _: fn(&str, u64) -> Value = knowledge_entry_wire;
