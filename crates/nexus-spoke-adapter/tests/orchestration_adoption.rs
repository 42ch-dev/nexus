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

// The imported example file carries its own `fn main` + `print_reject` (dead
// in this test crate) plus main-only `use` entries. Silence those lints on
// the module so the test crate compiles warning-clean.
#![allow(dead_code)]
#![allow(unused_imports)]

#[path = "../examples/baseline_adapter.rs"]
#[allow(dead_code, unused_imports)]
mod baseline_adapter;

use baseline_adapter::{
    assemble_request, knowledge_entry, knowledge_entry_wire, promote_request, NexusBaselineMock,
};
use nexus_spoke_adapter::{
    orchestrate_assemble, orchestrate_check, orchestrate_project, orchestrate_promote,
    orchestrate_upsert, AssembleResponse, CheckRequest, CheckResponse, ComputablePort,
    ComputablePorts, FindingPort, HostManifestPort, KnowledgeEntry, KnowledgeEntryPort,
    ProjectRequest, PromoteResponse, RelationPort, RuleQueryPort, ScopeQueryPort, SpokeReject,
    SpokeRejectCode, SpokeResult, UpsertRequest, UpsertResponse,
};
use serde_json::{json, Value};
use spoke_operations::spoke_ok;

// ── local fixtures ─────────────────────────────────────────────────────

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

// ── 1. orchestrate_upsert — happy create path ──────────────────────────

#[test]
fn orchestrate_upsert_happy_create_path() {
    // Seeded mock holds kb_mira; we upsert a *different* entry so the
    // create path (`expected_base_revision = None`, entry absent) fires.
    let mock = seeded_mock();
    let new_id = "kb_aran";
    let request = upsert_request(&[upsert_entry_wire(new_id, 0)]);

    let result = orchestrate_upsert(&mock, request);
    match result {
        SpokeResult::Ok(UpsertResponse::Variant0 {
            knowledge_entries, ..
        }) => {
            assert_eq!(knowledge_entries.len(), 1);
            assert_eq!(knowledge_entries[0].entry_id, new_id);
        }
        _ => panic!("expected upsert success, got {result:?}"),
    }

    // Entry is actually in the store.
    let stored = mock
        .get_knowledge_entry(new_id)
        .expect_ok("stored after upsert");
    assert_eq!(stored.entry_id, new_id);
    assert_eq!(stored.revision, Some(0));
}

// ── 2. orchestrate_upsert — CAS reject: create when entry already exists ──

#[test]
fn orchestrate_upsert_cas_reject_create_when_exists() {
    // kb_mira is already seeded. Mask the orchestrator's get so it believes
    // the entry is absent → derives `expected_base_revision = None` and
    // treats this as a create (`validate_create_path` requires revision ≤ 0
    // on create, hence candidate_rev=0). The orchestrator then calls our
    // mock's put with `None` against an entry that IS present →
    // `KnowledgeEntryAlreadyExists`.
    let mock = seeded_mock();
    mock.mask_next_get(SEEDED_ENTRY_ID);

    let request = upsert_request(&[upsert_entry_wire(SEEDED_ENTRY_ID, 0)]);

    let result = orchestrate_upsert(&mock, request);
    expect_reject_with_code(result, SpokeRejectCode::KnowledgeEntryAlreadyExists);
}

// ── 3. orchestrate_upsert — happy update path ──────────────────────────

#[test]
fn orchestrate_upsert_happy_update_path() {
    // Upsert is CAS-guarded replace, not an increment: the candidate MUST
    // carry the same revision as stored (the library's
    // `validate_update_path` calls `assert_revision_match(candidate.revision,
    // stored.revision)` before touching our port). The orchestrator then
    // derives `expected_base_revision = Some(stored_rev)`, which equals the
    // candidate's revision, and the mock's CAS accepts. The persisted entry
    // reflects the candidate's body (canonical_name updated here to prove
    // the replace took effect).
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

    let result = orchestrate_upsert(&mock, request);
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
        .expect_ok("stored after upsert");
    assert_eq!(stored.revision, Some(SEEDED_REVISION));
    assert_eq!(stored.canonical_name.as_str(), "Mira Vale (revised)");
}

// ── 4. orchestrate_upsert — CAS reject: stored > expected (STALE) ───────

#[test]
fn orchestrate_upsert_cas_reject_stale_revision() {
    // Seeded entry is at revision 3. Override the orchestrator's get so it
    // returns revision 1 → orchestrator derives `expected_base_revision =
    // Some(1)` → calls put(candidate, Some(1)). Mock CAS sees stored(3) >
    // expected(1) → `STORED_REVISION_STALE` (caller read a stale base).
    let mock = NexusBaselineMock::with_seeded_entry(knowledge_entry(SEEDED_ENTRY_ID, 3));
    mock.override_next_get_revision(SEEDED_ENTRY_ID, 1);

    let request = upsert_request(&[upsert_entry_wire(SEEDED_ENTRY_ID, 1)]);

    let result = orchestrate_upsert(&mock, request);
    expect_reject_with_code(result, SpokeRejectCode::StoredRevisionStale);
}

// ── 5. orchestrate_upsert — CAS reject: stored < expected (CONFLICT) ────
//
// This is the T2-review minor finding made concrete: the mock previously
// collapsed *all* revision mismatches into `STORED_REVISION_STALE`. The
// patched mock now distinguishes the two reject paths per spec §7.3:
//   - stored > expected → `STORED_REVISION_STALE` (test #4 above)
//   - stored < expected → `REVISION_CONFLICT` (this test)

#[test]
fn orchestrate_upsert_cas_reject_conflict() {
    // Seeded entry is at revision 1. Override the orchestrator's get so it
    // returns revision 5 → orchestrator derives `expected_base_revision =
    // Some(5)` → calls put(candidate, Some(5)). Mock CAS sees stored(1) <
    // expected(5) → `REVISION_CONFLICT` (caller expects an impossible
    // future revision the store has never reached).
    let mock = NexusBaselineMock::with_seeded_entry(knowledge_entry(SEEDED_ENTRY_ID, 1));
    mock.override_next_get_revision(SEEDED_ENTRY_ID, 5);

    let request = upsert_request(&[upsert_entry_wire(SEEDED_ENTRY_ID, 5)]);

    let result = orchestrate_upsert(&mock, request);
    expect_reject_with_code(result, SpokeRejectCode::RevisionConflict);
}

// ── 6. orchestrate_promote — happy path ────────────────────────────────

#[test]
fn orchestrate_promote_happy_path() {
    // Candidate claims revision 2, matching the stored provisional entry.
    // The orchestrator's `assert_revision_match(2, 2)` passes, the promote
    // gate validates, and the mock's CAS sees stored(2) == expected(2) →
    // accepts. Persisted entry is `confirmed` at revision 3.
    let mock = seeded_mock();

    let result = orchestrate_promote(&mock, promote_request(SEEDED_ENTRY_ID, SEEDED_REVISION));
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

// ── 7. orchestrate_promote — CAS reject: stale candidate revision ──────

#[test]
fn orchestrate_promote_cas_reject_stale() {
    // Candidate claims revision 1; stored is at revision 2. The
    // orchestrator's `assert_revision_match(1, 2)` fires before reaching
    // the mock's put: actual(2) > expected(1) → `STORED_REVISION_STALE`.
    let mock = seeded_mock();

    let result = orchestrate_promote(&mock, promote_request(SEEDED_ENTRY_ID, SEEDED_REVISION - 1));
    expect_reject_with_code(result, SpokeRejectCode::StoredRevisionStale);
}

// ── 8. orchestrate_assemble — happy path ───────────────────────────────

#[test]
fn orchestrate_assemble_happy_path() {
    // Scope filters the mock's full entry list down to [kb_mira]; spoke's
    // packet builder assembles the scoped entries into an AssemblePacket
    // keyed by `assemble:<scope_id>`.
    let mock = seeded_mock();

    let result = orchestrate_assemble(&mock, assemble_request("world_1", SEEDED_ENTRY_ID));
    match result {
        SpokeResult::Ok(AssembleResponse::Variant0 { packet, .. }) => {
            assert_eq!(packet.packet_id, "assemble:world_1");
            assert_eq!(packet.entries.len(), 1);
            assert_eq!(packet.entries[0].entry_id, SEEDED_ENTRY_ID);
        }
        _ => panic!("expected assemble success, got {result:?}"),
    }
}

// ── 9. orchestrate_check — happy path ──────────────────────────────────

#[test]
fn orchestrate_check_happy_path() {
    // Loads scoped entries/events, resolves rules (none requested), invokes
    // the product checker callback, persists findings. A trivial callback
    // returning Ok(empty) exercises the full pipeline without product
    // logic.
    let mock = seeded_mock();
    let request = check_request("world_1", SEEDED_ENTRY_ID);

    let result = orchestrate_check(&mock, request, |input| {
        assert_eq!(input.entries.len(), 1);
        assert_eq!(input.entries[0].entry_id, SEEDED_ENTRY_ID);
        assert!(input.events.is_empty());
        assert!(input.rules.is_empty());
        spoke_ok(Vec::new())
    });

    match result {
        SpokeResult::Ok(CheckResponse::Variant0 { findings, .. }) => {
            assert!(findings.is_empty());
        }
        _ => panic!("expected check success, got {result:?}"),
    }
}

// ── 10. (stretch) orchestrate_project — capability-missing reject ──────
//
// Constructs a `BaselineOnlyPorts` wrapper around `NexusBaselineMock` that
// implements the six baseline port families (delegating to the mock) and
// manually implements `ComputablePorts` returning `None` — i.e. it claims
// the computable capability boundary without backing it with a
// `ComputablePort` impl. `orchestrate_project` probes
// `ComputablePorts::as_computable()` and surfaces
// `CAPABILITY_PORT_MISSING` (spec §7.3 capability reject path) before any
// request validation.

struct BaselineOnlyPorts(NexusBaselineMock);

impl KnowledgeEntryPort for BaselineOnlyPorts {
    fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        self.0.get_knowledge_entry(entry_id)
    }
    fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        self.0.put_knowledge_entry(entry, expected_base_revision)
    }
}

impl nexus_spoke_adapter::RelationPort for BaselineOnlyPorts {
    fn get_relation(&self, relation_id: &str) -> SpokeResult<nexus_spoke_adapter::Relation> {
        self.0.get_relation(relation_id)
    }

    fn put_relation(
        &self,
        relation: nexus_spoke_adapter::Relation,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<nexus_spoke_adapter::Relation> {
        self.0.put_relation(relation, expected_base_revision)
    }
}

impl ScopeQueryPort for BaselineOnlyPorts {
    fn list_knowledge_entries(
        &self,
        scope: &nexus_spoke_adapter::Scope,
    ) -> SpokeResult<Vec<KnowledgeEntry>> {
        self.0.list_knowledge_entries(scope)
    }
    fn list_timeline_events(
        &self,
        scope: &nexus_spoke_adapter::Scope,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::TimelineEvent>> {
        self.0.list_timeline_events(scope)
    }
}

impl FindingPort for BaselineOnlyPorts {
    fn put_findings(
        &self,
        findings: Vec<nexus_spoke_adapter::Finding>,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::Finding>> {
        self.0.put_findings(findings)
    }
}

impl RuleQueryPort for BaselineOnlyPorts {
    fn list_rules(&self, rule_refs: &[String]) -> SpokeResult<Vec<nexus_spoke_adapter::Rule>> {
        self.0.list_rules(rule_refs)
    }
}

impl HostManifestPort for BaselineOnlyPorts {
    fn get_host_capability_manifest(
        &self,
    ) -> SpokeResult<nexus_spoke_adapter::HostCapabilityManifest> {
        self.0.get_host_capability_manifest()
    }
    fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::HostCapabilityManifest>> {
        self.0.list_peer_host_capability_manifests()
    }
}

// Manual `ComputablePorts` returning `None` — overrides the blanket impl
// (which only fires for `BaselinePorts + ComputablePort`). Because
// `BaselineOnlyPorts` does NOT impl `ComputablePort`, the blanket does not
// apply and this manual impl is the one the orchestrator sees.
impl ComputablePorts for BaselineOnlyPorts {
    fn as_computable(&self) -> Option<&dyn ComputablePort> {
        None
    }
}

#[test]
fn capability_missing_reject() {
    let ports = BaselineOnlyPorts(seeded_mock());

    // `require_port_method(as_computable().is_some(), "project")` fires
    // before `validate_project_request`, so the request body is irrelevant.
    let result = orchestrate_project(&ports, project_request(SEEDED_ENTRY_ID));
    expect_reject_with_code(result, SpokeRejectCode::CapabilityPortMissing);
}

// ── assertion helpers ──────────────────────────────────────────────────

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

// Keep `knowledge_entry_wire` reachable — it is part of the example's
// fixture API re-exported above and may be exercised by future scenarios.
const _: fn(&str, u64) -> Value = knowledge_entry_wire;
