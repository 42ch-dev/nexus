//! Local database schema version constants
//!
//! DB schema version is independent from contract `schema_version`.
//! Version-line separation follows the retired local-db-refactor-legacy baseline (local record).

/// Current local database schema version
///
/// This version tracks `SQLite` structure migrations only.
/// Increment when adding new tables, columns, or modifying DDL.
// V1.148: 10 → 11 — spoke_rules table (RuleQueryPort production) + the V1.145–V1.147 schema additions that were also unbumped (compute_sessions direct-lane columns, kb_relationships.extensions_nexus_json, modules_json). Adopting a stricter per-structural-change bump going forward.
// V1.150: 11 → 12 — moment_directives table (Moment Directive, DF-75 P1).
// V1.150 (residual close): 12 → 13 — moment_directives rebuild dropping last_chapter_no + new moment_directive_chapter_anchors table (R-V1150P2-004/R-V1150P2-008: per-(directive, work) chapter delta TTL burn).
// V1.155: 13 → 14 — peer_hosts table (N-C3 multi-host production, P0 T1).
// V1.158: 14 → 15 — peer_hosts.last_peer_id column (R-V1155P0-001 peer_id session recording, P2 T2).
// V1.163: 15 → 16 — legacy outbox table DROP (DR-08; migration 20260812_drop_legacy_outbox.sql).
// V1.164: 16 → 17 — narrative_timeline_events.modules_json column (migration 20260814000001_add_timeline_event_modules_json.sql).
// V1.164: 17 → 18 — mind_states table (migration 20260814000002_create_mind_states.sql).
// V1.165: 18 → 19 — world_findings table (DR-68 / AR-1; migration 20260815000001_create_world_findings.sql).
// V1.184: 19 → 20 — characters + actor_world_bindings (P0 actor contract).
// V1.185: 20 → 21 — characters.revision + characters.lifecycle_epoch (P0 character lifecycle; migration 20260906000001_character_lifecycle.sql).
// V1.185: 21 → 22 — actor_world_bindings.revision (P1 binding maintenance; migration 20260906000002_binding_revision.sql).
// V1.185: 22 → 23 — character run capture receipts + pending provenance (P3; migration 20260906000003_character_run_capture.sql).
// V1.191: 23 → 24 — Actor holder registry, native governance columns on kb_key_blocks (legacy creator_only removed), import quarantine and World/Character knowledge_revision (P1 T3; migration 20260918000001_holder_registry.sql).
// V1.207 P3: 24 → 25 — operation_receipts table + its writer-protocol guard/outbox family (RN-OGA-5; migration 20260919000001_operation_receipts.sql).
// V1.227 NXM-1: 25 → 26 — memory control metadata: memory_spaces / memory_subjects / memory_grants / memory_receipts (spoke-memory §6.2/§6.3/§6.5/§6.10) + their writer-protocol guard/outbox families (migration 20261101000001_memory_control_metadata.sql).
// V1.227 NXM-1: 26 → 27 — memory content metadata: memory_candidates / memory_records / memory_profiles (spoke-memory §6.6/§6.7/§6.8) carrying revision + stable record/candidate identity + their writer-protocol guard/outbox families (migration 20261101000002_memory_content_metadata.sql).
// V1.227 NXM-2: 27 → 28 — memory_receipts rebuilt so a settled receipt may carry no payload at all, the content-free §6.10 dedup tombstone left after body eviction, plus the `created_at` retention-sweep index (migration 20261101000003_memory_receipt_tombstones.sql).
// V1.227 NXM-2: 28 → 29 — memory_provider_meta key/value table, the provider-owned home of the durable §6.10 retention clock floor, plus its writer-protocol guard/outbox family (migration 20261101000004_memory_provider_meta.sql).
// V1.227 NXM-3: 29 → 30 — memory_journal durable intent journal for cross-store (SQL + file) writes (§5.2/D-10): the planned file targets are committed with the effect rows, recovery settles or quarantines them, plus its writer-protocol guard/outbox family (migration 20261101000005_memory_journal.sql).
// V1.229 N3 P4 (D18): 30 → 31 — content-free remote binding-memory dependency metadata: independent per-wire-attempt admissions plus monotone known candidate/record facts, both RESTRICT-bound to the Nexus binding (migration 20261104000001_remote_binding_memory_dependencies.sql).
// V1.230 N4-R: 31 → 32 — append-only memory_reflections envelopes (spoke-memory §7.25–§7.27): verbatim pinned selection/filter/basis JSON with the insert-only guard + insert outbox family (reflections are immutable — no update/delete triggers, no retention sweep; migration 20261110000001_memory_reflections.sql).
// V1.231: 32 → 33 — knowledge_entries + reference_sources DROP (ADR K1 §3.1–§3.2: the archived User KnowledgeStore and reference registry are retired, not sealed; migration 20261111000001_retire_user_and_reference_stores.sql).
pub const DB_SCHEMA_VERSION: u32 = 33;

/// Contract schema version from generated wire types
///
/// Re-exported from nexus-contracts for convenience.
/// This tracks network contract compatibility, NOT local DB structure.
pub use nexus_contracts::generated::LATEST_SCHEMA_VERSION as SCHEMA_VERSION;
