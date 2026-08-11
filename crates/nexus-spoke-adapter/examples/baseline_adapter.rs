//! Runnable proof that spoke's Surface B (injection orchestration) is genuinely
//! adoptable from the `nexus-spoke-adapter` crate.
//!
//! Implements a minimal in-memory [`NexusBaselineMock`] satisfying all six
//! baseline port traits (with optimistic-concurrency enforcement on both
//! `put_knowledge_entry` and `put_relation`), then drives `orchestrate_promote`
//! (happy path + stale-revision CAS reject), `orchestrate_assemble` (happy
//! path), and `orchestrate_relate` (create happy + update happy + stale CAS
//! reject) end-to-end through it. The mock owns storage I/O only; every
//! lifecycle invariant (validate / apply / scope-filter / packet-build /
//! revision compare) stays in `spoke_operations` — the call-boundary
//! invariant (spec §7) is preserved.
//!
//! Run with:
//! ```text
//! cargo run -p nexus-spoke-adapter --example baseline_adapter
//! ```

// Fixture helpers and the mock struct below are `pub` so the programmatic
// test twin (`tests/orchestration_adoption.rs`) can `#[path]`-import this
// file as a hidden module. In the example binary itself `pub` is a no-op,
// but it widens clippy's view of these items. These two lints are not
// meaningful for fixture builders that are always consumed at the call
// site and whose `.expect(...)` panics are the fixture contract.
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
// The mock's Mutex guards are held across the whole port method (the lock
// protects the in-memory store for the method's duration); the nursery
// significant_drop_* suggestions to shorten their lifetime are noise here.
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::significant_drop_in_scrutinee)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use nexus_spoke_adapter::{
    orchestrate_assemble, orchestrate_promote, orchestrate_relate, AssembleRequest,
    AssembleResponse, Finding, FindingPort, HostCapabilityManifest, HostManifestPort,
    KnowledgeEntry, KnowledgeEntryPort, PromoteRequest, PromoteResponse, RelateRequest,
    RelateResponse, Relation, RelationPort, Rule, RuleQueryPort, Scope, ScopeQueryPort,
    SpokeReject, SpokeRejectCode, SpokeResult, TimelineEvent,
};
use serde_json::{json, Map, Value};
// simplify: `spoke_ok` / `spoke_reject` are convenience constructors for
// `SpokeResult` values used inside the mock's port impls. The adapter crate
// deliberately does not re-export them (that would widen its public API), so
// the example reaches for them directly — `spoke-operations` is already a crate
// dependency and examples are not part of the lib's public surface.
use spoke_operations::{spoke_ok, spoke_reject};

// ── In-memory BaselinePorts mock ───────────────────────────────────────
//
// Each port method is a thin HashMap/Vec get/put plus a `spoke_ok`/`spoke_reject`
// return — no ranking, retrieval, scoring, or product logic. CAS on
// `put_knowledge_entry` is the mock's only non-trivial logic, and it is
// adapter-owned per spec §7 ("adapters own transport, persistence,
// transactions"; the library stays I/O-free).
//
// simplify: `Mutex` for interior mutability — the 0.9.1 port traits are
// `#[async_trait] async fn` (Send futures), so the mock must be `Sync`.
// A production adapter backs these ports with real storage.

/// In-memory store implementing spoke's six baseline port families.
///
/// Visibility note: items here are `pub` so the programmatic test twin at
/// `tests/orchestration_adoption.rs` can `#[path]`-import this file as a
/// hidden module and reuse the mock without duplication. In the example
/// binary itself `pub` is a no-op (the binary crate root sees everything).
pub struct NexusBaselineMock {
    entries: Mutex<HashMap<String, KnowledgeEntry>>,
    relations: Mutex<HashMap<String, Relation>>,
    events: Mutex<Vec<TimelineEvent>>,
    rules: Mutex<HashMap<String, Rule>>,
    findings: Mutex<Vec<Finding>>,
    self_manifest: HostCapabilityManifest,
    peer_manifests: Vec<HostCapabilityManifest>,
    /// One-shot `get_knowledge_entry` overrides used by the test twin to
    /// simulate a get/put race (the orchestrator's `orchestrate_upsert`
    /// derives `expected_base_revision` from the entry returned by get, so
    /// CAS rejects on upsert require the get snapshot to diverge from the
    /// actual stored revision). The override is consumed on the next get for
    /// the given `entry_id`:
    ///   - `Some(rev)` → return the stored entry with `revision` overwritten by `rev`
    ///   - `None`      → return `KnowledgeEntryNotFound` even if the entry exists
    next_get_override: Mutex<HashMap<String, Option<u64>>>,
}

impl NexusBaselineMock {
    /// Build a mock seeded with one provisional `KnowledgeEntry`.
    ///
    /// The current revision of a stored entry is read from
    /// `KnowledgeEntry.revision` directly (the entry IS the storage record) —
    /// no parallel revisions map is kept, matching spoke's own
    /// `put_knowledge_entry_with_occ` pattern. Keeping a second map would
    /// duplicate state and risk drift between the two.
    pub fn with_seeded_entry(entry: KnowledgeEntry) -> Self {
        let mut entries = HashMap::new();
        entries.insert(entry.entry_id.clone(), entry);
        Self {
            entries: Mutex::new(entries),
            relations: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            rules: Mutex::new(HashMap::new()),
            findings: Mutex::new(Vec::new()),
            self_manifest: host_manifest("nexus-baseline-mock", &["nexus"], &["data-store"]),
            peer_manifests: Vec::new(),
            next_get_override: Mutex::new(HashMap::new()),
        }
    }

    /// One-shot: the next `get_knowledge_entry(entry_id)` returns
    /// `KnowledgeEntryNotFound`, simulating a concurrent inserter that has
    /// stored the entry between the caller's read and write window.
    pub fn mask_next_get(&self, entry_id: &str) {
        self.next_get_override
            .lock()
            .expect("mock override mutex")
            .insert(entry_id.into(), None);
    }

    /// One-shot: the next `get_knowledge_entry(entry_id)` returns the stored
    /// entry with its `revision` overwritten by `revision`, simulating a
    /// concurrent writer that advanced (or rewound) the store's revision
    /// between the caller's read and write window. Used to drive the mock's
    /// CAS reject paths in both directions (`stored > expected` → STALE,
    /// `stored < expected` → CONFLICT) through `orchestrate_upsert`.
    pub fn override_next_get_revision(&self, entry_id: &str, revision: u64) {
        self.next_get_override
            .lock()
            .expect("mock override mutex")
            .insert(entry_id.into(), Some(revision));
    }
}

#[async_trait]
impl KnowledgeEntryPort for NexusBaselineMock {
    async fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        // Consume a one-shot override first (test-fixture race simulation).
        if let Some(override_rev) = self
            .next_get_override
            .lock()
            .expect("mock override mutex")
            .remove(entry_id)
        {
            match override_rev {
                None => {
                    let mut details = Map::new();
                    details.insert("entry_id".into(), json!(entry_id));
                    return spoke_reject(
                        SpokeRejectCode::KnowledgeEntryNotFound,
                        format!("KnowledgeEntry not found (masked): {entry_id}"),
                        Some(details),
                    );
                }
                Some(rev) => {
                    if let Some(entry) = self
                        .entries
                        .lock()
                        .expect("mock entries mutex")
                        .get(entry_id)
                        .cloned()
                    {
                        let mut snapshot = entry;
                        snapshot.revision = Some(rev);
                        return spoke_ok(snapshot);
                    }
                    // entry absent: fall through to the normal NotFound path below
                }
            }
        }
        let entries = self.entries.lock().expect("mock entries mutex");
        if let Some(entry) = entries.get(entry_id) {
            return spoke_ok(entry.clone());
        }
        let mut details = Map::new();
        details.insert("entry_id".into(), json!(entry_id));
        spoke_reject(
            SpokeRejectCode::KnowledgeEntryNotFound,
            format!("KnowledgeEntry not found: {entry_id}"),
            Some(details),
        )
    }

    async fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        let mut entries = self.entries.lock().expect("mock entries mutex");
        let existing = entries.get(&entry.entry_id);
        match expected_base_revision {
            None => {
                if existing.is_some() {
                    let mut details = Map::new();
                    details.insert("entry_id".into(), json!(entry.entry_id));
                    return spoke_reject(
                        SpokeRejectCode::KnowledgeEntryAlreadyExists,
                        format!("Entry already exists: {}", entry.entry_id),
                        Some(details),
                    );
                }
            }
            Some(expected) => match existing {
                None => {
                    let mut details = Map::new();
                    details.insert("entry_id".into(), json!(entry.entry_id));
                    details.insert("expectedBaseRevision".into(), json!(expected));
                    return spoke_reject(
                        SpokeRejectCode::StoredRevisionStale,
                        format!("KnowledgeEntry not found for update: {}", entry.entry_id),
                        Some(details),
                    );
                }
                Some(stored) => {
                    let current = stored.revision.unwrap_or(0);
                    if current > expected {
                        // Store is ahead of the caller's expectation — caller
                        // read a stale base. Spec §7.3 CAS contract.
                        let mut details = Map::new();
                        details.insert("expectedBaseRevision".into(), json!(expected));
                        details.insert("storeRevision".into(), json!(current));
                        return spoke_reject(
                            SpokeRejectCode::StoredRevisionStale,
                            format!(
                                "Store revision {current} is ahead of expected base {expected}"
                            ),
                            Some(details),
                        );
                    }
                    if current < expected {
                        // Caller expects a revision the store has never reached
                        // — impossible future base. Spec §7.3 CAS contract.
                        let mut details = Map::new();
                        details.insert("expectedBaseRevision".into(), json!(expected));
                        details.insert("storeRevision".into(), json!(current));
                        return spoke_reject(
                            SpokeRejectCode::RevisionConflict,
                            format!(
                                "Expected base revision {expected} is ahead of store revision {current}"
                            ),
                            Some(details),
                        );
                    }
                    // current == expected: accept the write (fall through).
                }
            },
        }
        entries.insert(entry.entry_id.clone(), entry.clone());
        spoke_ok(entry)
    }
}

#[async_trait]
impl RelationPort for NexusBaselineMock {
    /// In-memory OCC reference — mirrors the production
    /// `nexus_spoke_adapter::NexusAdapter::get_relation`: read the stored
    /// `Relation` by id; absent → `RelationNotFound`.
    async fn get_relation(&self, relation_id: &str) -> SpokeResult<Relation> {
        let relations = self.relations.lock().expect("mock relations mutex");
        if let Some(relation) = relations.get(relation_id) {
            return spoke_ok(relation.clone());
        }
        let mut details = Map::new();
        details.insert("relation_id".to_string(), json!(relation_id));
        spoke_reject(
            SpokeRejectCode::RelationNotFound,
            format!("Relation not found: {relation_id}"),
            Some(details),
        )
    }

    /// In-memory OCC reference — mirrors the production
    /// `nexus_spoke_adapter::NexusAdapter::put_relation` (spec §7.4 OCC
    /// contract):
    ///
    /// | `expected_base_revision` | Path   | Outcome                                                              |
    /// |--------------------------|--------|----------------------------------------------------------------------|
    /// | `None`                   | create | absent → seed `revision = 1` + insert; present → `RelationAlreadyExists` |
    /// | `Some(expected)`         | CAS    | absent → `StoredRevisionStale` (storeRevision=null); `stored > expected` or `stored < expected` → `StoredRevisionStale` (every mismatch collapses, matching production); `stored == expected` → bump to `expected + 1` + insert |
    ///
    /// Unlike `put_knowledge_entry` (where the promote orchestrator bumps the
    /// candidate's revision before calling the port), the relate orchestrator
    /// passes the candidate through untouched — so the port owns the revision
    /// seed/bump, exactly as the production SQLite-backed port does.
    async fn put_relation(
        &self,
        relation: Relation,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<Relation> {
        let mut relations = self.relations.lock().expect("mock relations mutex");
        let relation_id = relation.relation_id.clone();
        match expected_base_revision {
            None => {
                if relations.contains_key(&relation_id) {
                    let mut details = Map::new();
                    details.insert("relation_id".into(), json!(relation_id));
                    return spoke_reject(
                        SpokeRejectCode::RelationAlreadyExists,
                        format!("Relation already exists: {relation_id}"),
                        Some(details),
                    );
                }
                // Seed revision = 1 (spoke convention; matches the production
                // port's INSERT with `revision = 1`).
                let mut stored = relation;
                stored.revision = Some(1);
                relations.insert(relation_id, stored.clone());
                spoke_ok(stored)
            }
            Some(expected) => match relations.get(&relation_id) {
                None => {
                    // Absent + Some(expected): the store has no revision at
                    // all. Collapses to STORED_REVISION_STALE with
                    // storeRevision=null (mirrors production).
                    let mut details = Map::new();
                    details.insert("relation_id".into(), json!(relation_id));
                    details.insert("expectedBaseRevision".into(), json!(expected));
                    details.insert("storeRevision".into(), Value::Null);
                    spoke_reject(
                        SpokeRejectCode::StoredRevisionStale,
                        format!(
                            "Relation not found for update: {relation_id} (expected base {expected})"
                        ),
                        Some(details),
                    )
                }
                Some(stored) => {
                    let current = stored.revision.unwrap_or(0);
                    if current != expected {
                        // The relation-port CAS mapping collapses every
                        // mismatch to STORED_REVISION_STALE (simpler than the
                        // KnowledgeEntryPort 3-way split — the relate
                        // orchestrator pre-routes create vs update, so the
                        // only reachable failure here is "the store moved
                        // since the caller's read"). Matches production.
                        let mut details = Map::new();
                        details.insert("relation_id".into(), json!(relation_id));
                        details.insert("expectedBaseRevision".into(), json!(expected));
                        details.insert("storeRevision".into(), json!(current));
                        return spoke_reject(
                            SpokeRejectCode::StoredRevisionStale,
                            format!(
                                "Store revision {current} is not the expected base {expected} for relation {relation_id}"
                            ),
                            Some(details),
                        );
                    }
                    // current == expected: accept, bump revision to expected + 1
                    // (matches production `update_relationship_in_tx`).
                    let mut updated = relation;
                    updated.revision = Some(expected + 1);
                    relations.insert(relation_id, updated.clone());
                    spoke_ok(updated)
                }
            },
        }
    }
}

#[async_trait]
impl ScopeQueryPort for NexusBaselineMock {
    async fn list_knowledge_entries(&self, _scope: &Scope) -> SpokeResult<Vec<KnowledgeEntry>> {
        // The mock returns all stored entries; spoke's orchestrator applies
        // scope filtering via `filter_knowledge_entries_by_scope`. Re-
        // implementing that helper here would violate the call-boundary
        // invariant (spec §7).
        spoke_ok(
            self.entries
                .lock()
                .expect("mock entries mutex")
                .values()
                .cloned()
                .collect(),
        )
    }

    async fn list_timeline_events(&self, _scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        spoke_ok(self.events.lock().expect("mock events mutex").clone())
    }
}

#[async_trait]
impl FindingPort for NexusBaselineMock {
    async fn put_findings(&self, findings: Vec<Finding>) -> SpokeResult<Vec<Finding>> {
        self.findings
            .lock()
            .expect("mock findings mutex")
            .extend(findings.iter().cloned());
        spoke_ok(findings)
    }
}

#[async_trait]
impl RuleQueryPort for NexusBaselineMock {
    async fn list_rules(&self, rule_refs: &[String]) -> SpokeResult<Vec<Rule>> {
        let rules = self.rules.lock().expect("mock rules mutex");
        let mut resolved = Vec::new();
        for rule_ref in rule_refs {
            if let Some(rule) = rules.get(rule_ref) {
                resolved.push(rule.clone());
            } else {
                let mut details = Map::new();
                details.insert("rule_ref".into(), json!(rule_ref));
                return spoke_reject(
                    SpokeRejectCode::InvalidInput,
                    format!("Rule not found: {rule_ref}"),
                    Some(details),
                );
            }
        }
        spoke_ok(resolved)
    }
}

#[async_trait]
impl HostManifestPort for NexusBaselineMock {
    async fn get_host_capability_manifest(&self) -> SpokeResult<HostCapabilityManifest> {
        spoke_ok(self.self_manifest.clone())
    }

    async fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<HostCapabilityManifest>> {
        spoke_ok(self.peer_manifests.clone())
    }
}

// ── Fixture builders ───────────────────────────────────────────────────

pub fn host_manifest(host_id: &str, namespaces: &[&str], roles: &[&str]) -> HostCapabilityManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": roles,
        "capabilities": ["spoke-baseline"],
        "namespaces": namespaces,
        "extensions": {}
    }))
    .expect("valid HostCapabilityManifest fixture")
}

/// Build the wire shape for a provisional character `KnowledgeEntry`.
pub fn knowledge_entry_wire(entry_id: &str, revision: u64) -> serde_json::Value {
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

pub fn knowledge_entry(entry_id: &str, revision: u64) -> KnowledgeEntry {
    serde_json::from_value(knowledge_entry_wire(entry_id, revision))
        .expect("valid KnowledgeEntry fixture")
}

pub fn promote_request(entry_id: &str, revision: u64) -> PromoteRequest {
    serde_json::from_value(json!({ "candidate": knowledge_entry_wire(entry_id, revision) }))
        .expect("valid PromoteRequest fixture")
}

pub fn assemble_request(scope_id: &str, entry_id: &str) -> AssembleRequest {
    serde_json::from_value(json!({
        "scope": { "scope_id": scope_id, "entry_ids": [entry_id] },
        "max_entries": 10
    }))
    .expect("valid AssembleRequest fixture")
}

/// Build the wire shape for a `Relation`.
///
/// `revision` is parameterized so the same builder serves the create candidate
/// (revision absent → `None`) and the update candidates (revision set
/// explicitly to drive the CAS gate).
pub fn relation_wire(relation_id: &str, revision: Option<u64>) -> serde_json::Value {
    let mut wire = json!({
        "schema_version": 1,
        "relation_id": relation_id,
        "from_id": "kb_mira",
        "to_id": "kb_aran",
        "relation_type": "allied_with",
        "label": "Mira \u{2194} Aran",
        "metadata": { "bond": "inaugural-arc" },
        "extensions": {}
    });
    if let Some(rev) = revision {
        wire["revision"] = json!(rev);
    }
    wire
}

/// Wrap a `Relation` wire into a `RelateRequest` via JSON round-trip (mirrors
/// the daemon's `build_spoke_relate_request`).
pub fn relate_request(relation_id: &str, revision: Option<u64>) -> RelateRequest {
    serde_json::from_value(json!({ "relation": relation_wire(relation_id, revision) }))
        .expect("valid RelateRequest fixture")
}

fn print_reject(label: &str, reject: &SpokeReject) {
    println!(
        "  [{label}] REJECT code={} message={}",
        reject.code.as_str(),
        reject.message
    );
}

#[tokio::main]
async fn main() {
    let entry_id = "kb_mira";
    let mock = NexusBaselineMock::with_seeded_entry(knowledge_entry(entry_id, 2));

    println!("=== nexus-spoke-adapter Surface B adoption example ===");
    println!();
    println!("Mock seeded with KnowledgeEntry entry_id={entry_id} revision=2 status=provisional");
    println!();

    // ── 1. orchestrate_promote — happy path ───────────────────────────
    // The candidate claims revision 2, which matches the stored revision.
    // The orchestrator's `assert_revision_match(2, 2)` passes; it then calls
    // our mock's `put_knowledge_entry(accepted, Some(2))`. The mock's CAS
    // sees stored == expected (2 == 2) and accepts the write; the persisted
    // entry becomes status=confirmed, revision=3.
    println!("--- 1. orchestrate_promote (happy path) ---");
    match orchestrate_promote(&mock, promote_request(entry_id, 2)).await {
        SpokeResult::Ok(PromoteResponse::Variant0 {
            knowledge_entry, ..
        }) => {
            println!(
                "  OK: entry_id={} status={} revision={:?}",
                knowledge_entry.entry_id, knowledge_entry.status, knowledge_entry.revision
            );
        }
        SpokeResult::Ok(_) => println!("  (unexpected promote response variant)"),
        SpokeResult::Reject(reject) => print_reject("promote-happy", &reject),
    }
    println!();

    // ── 2. orchestrate_promote — CAS reject (stale candidate revision) ─
    // The happy promote bumped the stored revision to 3. Issuing another
    // promote whose candidate still claims revision 2 triggers the
    // orchestrator's `assert_revision_match(2, 3)`: the store is ahead of
    // the caller's expectation, so it rejects with `STORED_REVISION_STALE`.
    // This is the CAS gate surfacing through the orchestrator.
    println!("--- 2. orchestrate_promote (stale revision -> CAS reject) ---");
    match orchestrate_promote(&mock, promote_request(entry_id, 2)).await {
        SpokeResult::Ok(_) => println!("  UNEXPECTED OK — expected CAS reject"),
        SpokeResult::Reject(reject) => print_reject("promote-cas", &reject),
    }
    println!();

    // ── 3. orchestrate_assemble — happy path ──────────────────────────
    // The scope query flows through our mock's `list_knowledge_entries`
    // (returns all stored entries); spoke's scope helpers filter to the
    // requested entry_ids and the packet builder assembles the result.
    println!("--- 3. orchestrate_assemble (happy path) ---");
    match orchestrate_assemble(&mock, assemble_request("world_1", entry_id)).await {
        SpokeResult::Ok(AssembleResponse::Variant0 { packet, .. }) => {
            println!(
                "  OK: packet_id={} entries={}",
                packet.packet_id,
                packet.entries.len()
            );
        }
        SpokeResult::Ok(_) => println!("  (unexpected assemble response variant)"),
        SpokeResult::Reject(reject) => print_reject("assemble", &reject),
    }
    println!();

    // ── 4. orchestrate_relate — create happy path ─────────────────────
    // The candidate carries no revision (valid for create per
    // `validate_create_revision`). The orchestrator loads stored → absent →
    // create path → calls our mock's `put_relation(candidate, None)`. The
    // mock seeds revision = 1 (spoke convention) and returns the stored
    // relation. This is the 3rd shipped Surface B cutover (promote + upsert
    // + relate) exercised through the mock.
    println!("--- 4. orchestrate_relate (create happy path) ---");
    match orchestrate_relate(&mock, relate_request("rel_demo", None)).await {
        SpokeResult::Ok(RelateResponse::Variant0 { relation, .. }) => {
            println!(
                "  OK: relation_id={} type={} revision={:?}",
                relation.relation_id, relation.relation_type, relation.revision
            );
        }
        SpokeResult::Ok(_) => println!("  (unexpected relate response variant)"),
        SpokeResult::Reject(reject) => print_reject("relate-create", &reject),
    }
    println!();

    // ── 5. orchestrate_relate — update happy path ─────────────────────
    // The create seeded revision = 1. Issuing a relate whose candidate
    // claims revision 1 matches stored; the orchestrator's
    // `assert_revision_match(1, 1)` passes, then our mock's CAS sees
    // stored(1) == expected(1) → accepts and bumps to revision 2.
    println!("--- 5. orchestrate_relate (update happy path) ---");
    match orchestrate_relate(&mock, relate_request("rel_demo", Some(1))).await {
        SpokeResult::Ok(RelateResponse::Variant0 { relation, .. }) => {
            println!(
                "  OK: relation_id={} revision={:?} (bumped 1 -> 2)",
                relation.relation_id, relation.revision
            );
        }
        SpokeResult::Ok(_) => println!("  (unexpected relate response variant)"),
        SpokeResult::Reject(reject) => print_reject("relate-update", &reject),
    }
    println!();

    // ── 6. orchestrate_relate — stale CAS reject ──────────────────────
    // The update bumped the stored revision to 2. Issuing another relate
    // whose candidate STILL claims revision 1 triggers the orchestrator's
    // `assert_revision_match(1, 2)`: the store is ahead of the caller's
    // expectation, so it rejects with `STORED_REVISION_STALE`. Same shape
    // as the promote CAS reject (block 2) — the CAS gate surfaces through
    // the orchestrator's validation before the mock's put fires.
    println!("--- 6. orchestrate_relate (stale revision -> CAS reject) ---");
    match orchestrate_relate(&mock, relate_request("rel_demo", Some(1))).await {
        SpokeResult::Ok(_) => println!("  UNEXPECTED OK — expected CAS reject"),
        SpokeResult::Reject(reject) => print_reject("relate-cas", &reject),
    }
    println!();
    println!("=== example complete ===");
}
