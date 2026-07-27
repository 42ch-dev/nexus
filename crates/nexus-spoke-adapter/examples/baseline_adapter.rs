//! Runnable proof that spoke's Surface B (injection orchestration) is genuinely
//! adoptable from the `nexus-spoke-adapter` crate.
//!
//! Implements a minimal in-memory [`NexusBaselineMock`] satisfying all six
//! baseline port traits (with optimistic-concurrency enforcement on
//! `put_knowledge_entry`), then drives `orchestrate_promote` (happy path +
//! stale-revision CAS reject) and `orchestrate_assemble` (happy path)
//! end-to-end through it. The mock owns storage I/O only; every lifecycle
//! invariant (validate / apply / scope-filter / packet-build / revision
//! compare) stays in `spoke_operations` — the call-boundary invariant (spec
//! §7) is preserved.
//!
//! Run with:
//! ```text
//! cargo run -p nexus-spoke-adapter --example baseline_adapter
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use nexus_spoke_adapter::{
    orchestrate_assemble, orchestrate_promote, AssembleRequest, AssembleResponse, Finding,
    FindingPort, HostCapabilityManifest, HostManifestPort, KnowledgeEntry, KnowledgeEntryPort,
    PromoteRequest, PromoteResponse, Relation, RelationPort, Rule, RuleQueryPort, Scope,
    ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult, TimelineEvent,
};
use serde_json::{json, Map};
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
// simplify: `RefCell` for interior mutability — this example is single-
// threaded. A production adapter backs these ports with real storage.

/// In-memory store implementing spoke's six baseline port families.
struct NexusBaselineMock {
    entries: RefCell<HashMap<String, KnowledgeEntry>>,
    relations: RefCell<Vec<Relation>>,
    events: RefCell<Vec<TimelineEvent>>,
    rules: RefCell<HashMap<String, Rule>>,
    findings: RefCell<Vec<Finding>>,
    self_manifest: HostCapabilityManifest,
    peer_manifests: Vec<HostCapabilityManifest>,
}

impl NexusBaselineMock {
    /// Build a mock seeded with one provisional `KnowledgeEntry`.
    ///
    /// The current revision of a stored entry is read from
    /// `KnowledgeEntry.revision` directly (the entry IS the storage record) —
    /// no parallel revisions map is kept, matching spoke's own
    /// `put_knowledge_entry_with_occ` pattern. Keeping a second map would
    /// duplicate state and risk drift between the two.
    fn with_seeded_entry(entry: KnowledgeEntry) -> Self {
        let mut entries = HashMap::new();
        entries.insert(entry.entry_id.clone(), entry);
        Self {
            entries: RefCell::new(entries),
            relations: RefCell::new(Vec::new()),
            events: RefCell::new(Vec::new()),
            rules: RefCell::new(HashMap::new()),
            findings: RefCell::new(Vec::new()),
            self_manifest: host_manifest("nexus-baseline-mock", &["nexus"], &["data-store"]),
            peer_manifests: Vec::new(),
        }
    }
}

impl KnowledgeEntryPort for NexusBaselineMock {
    fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        let entries = self.entries.borrow();
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

    fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        let mut entries = self.entries.borrow_mut();
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
                    if current != expected {
                        let mut details = Map::new();
                        details.insert("expectedBaseRevision".into(), json!(expected));
                        details.insert("storeRevision".into(), json!(current));
                        return spoke_reject(
                            SpokeRejectCode::StoredRevisionStale,
                            format!(
                                "Store revision {current} does not match expected base {expected}"
                            ),
                            Some(details),
                        );
                    }
                }
            },
        }
        entries.insert(entry.entry_id.clone(), entry.clone());
        spoke_ok(entry)
    }
}

impl RelationPort for NexusBaselineMock {
    fn put_relation(&self, relation: Relation) -> SpokeResult<Relation> {
        self.relations.borrow_mut().push(relation.clone());
        spoke_ok(relation)
    }
}

impl ScopeQueryPort for NexusBaselineMock {
    fn list_knowledge_entries(&self, _scope: &Scope) -> SpokeResult<Vec<KnowledgeEntry>> {
        // The mock returns all stored entries; spoke's orchestrator applies
        // scope filtering via `filter_knowledge_entries_by_scope`. Re-
        // implementing that helper here would violate the call-boundary
        // invariant (spec §7).
        spoke_ok(self.entries.borrow().values().cloned().collect())
    }

    fn list_timeline_events(&self, _scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        spoke_ok(self.events.borrow().clone())
    }
}

impl FindingPort for NexusBaselineMock {
    fn put_findings(&self, findings: Vec<Finding>) -> SpokeResult<Vec<Finding>> {
        self.findings.borrow_mut().extend(findings.iter().cloned());
        spoke_ok(findings)
    }
}

impl RuleQueryPort for NexusBaselineMock {
    fn list_rules(&self, rule_refs: &[String]) -> SpokeResult<Vec<Rule>> {
        let rules = self.rules.borrow();
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

impl HostManifestPort for NexusBaselineMock {
    fn get_host_capability_manifest(&self) -> SpokeResult<HostCapabilityManifest> {
        spoke_ok(self.self_manifest.clone())
    }

    fn list_peer_host_capability_manifests(&self) -> SpokeResult<Vec<HostCapabilityManifest>> {
        spoke_ok(self.peer_manifests.clone())
    }
}

// ── Fixture builders ───────────────────────────────────────────────────

fn host_manifest(host_id: &str, namespaces: &[&str], roles: &[&str]) -> HostCapabilityManifest {
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
fn knowledge_entry_wire(entry_id: &str, revision: u64) -> serde_json::Value {
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

fn knowledge_entry(entry_id: &str, revision: u64) -> KnowledgeEntry {
    serde_json::from_value(knowledge_entry_wire(entry_id, revision))
        .expect("valid KnowledgeEntry fixture")
}

fn promote_request(entry_id: &str, revision: u64) -> PromoteRequest {
    serde_json::from_value(json!({ "candidate": knowledge_entry_wire(entry_id, revision) }))
        .expect("valid PromoteRequest fixture")
}

fn assemble_request(scope_id: &str, entry_id: &str) -> AssembleRequest {
    serde_json::from_value(json!({
        "scope": { "scope_id": scope_id, "entry_ids": [entry_id] },
        "max_entries": 10
    }))
    .expect("valid AssembleRequest fixture")
}

fn print_reject(label: &str, reject: &SpokeReject) {
    println!(
        "  [{label}] REJECT code={} message={}",
        reject.code.as_str(),
        reject.message
    );
}

fn main() {
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
    match orchestrate_promote(&mock, promote_request(entry_id, 2)) {
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
    match orchestrate_promote(&mock, promote_request(entry_id, 2)) {
        SpokeResult::Ok(_) => println!("  UNEXPECTED OK — expected CAS reject"),
        SpokeResult::Reject(reject) => print_reject("promote-cas", &reject),
    }
    println!();

    // ── 3. orchestrate_assemble — happy path ──────────────────────────
    // The scope query flows through our mock's `list_knowledge_entries`
    // (returns all stored entries); spoke's scope helpers filter to the
    // requested entry_ids and the packet builder assembles the result.
    println!("--- 3. orchestrate_assemble (happy path) ---");
    match orchestrate_assemble(&mock, assemble_request("world_1", entry_id)) {
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
    println!("=== example complete ===");
}
