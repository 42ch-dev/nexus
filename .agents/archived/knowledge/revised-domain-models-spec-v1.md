# Revised Domain Models Specification v1

> **Author**: @architect
> **Date**: 2026-04-06
> **Status**: Superseded by implementation (Wave 1 complete) — all 15 domain aggregates implemented per this spec.
> **Inputs**: `schemas/domain/*.schema.json`, `schemas/common/*.schema.json`
> **Relationship**: This spec serves as the **authoritative revision input** for `.agents/plans/2025-04-05-domain-models.md`. It does NOT modify the plan file directly.

---

## 1. Overview

### 1.1 Revision Goal

The existing plan (`.agents/plans/2025-04-05-domain-models.md`) was written against an earlier understanding of the domain model. Architecture review identified **6 P1 critical gaps** and **2 P2 gaps**, this document provides a comprehensive field-by-field revision that:

1. **Aligns every field definition** with the canonical JSON Schema truth source (`schemas/domain/`, `schemas/common/`)
2. **Replaces all wrong enum values** with spec-correct ones from the domain model specification (§7)
3. **Adds all missing V1.0 mandatory aggregates** (Creator, Pairing, StoryManifest, ForkBranch, ReferenceSource)
4. **Rewrites domain logic methods** to match consistency gate requirements
5. **Defines integration strategy** with `nexus-contracts` generated types

### 1.2 Alignment Strategy

| Source | Authority Level | Usage |
|--------|----------------|-------|
| `schemas/domain/*.schema.json` | **Truth source** — field names, types, constraints, required/optional | Every field definition must match exactly |
| `schemas/common/*.schema.json` | **Truth source** — shared enums, type aliases, value objects | BlockType, MemoryType, SourceAnchor, etc. |
| `schemas/domain/*.schema.json` | **Truth source** — field names, types, constraints, required/optional | Domain semantics are documented inline in §2 below |
| Consistency rules | **Normative spec** — hard invariants, gates | confirm/promote logic, validation rules (inlined in §2) |
| V1.0 scope | **Scope gate** — V1.0 mandatory items | Determines which aggregates are in-scope |
| `crates/nexus-contracts/src/generated/` | **Generated types** — Rust structs from codegen | Integration testing baseline (must match schemas) |

### 1.3 Gap Summary (from Architecture Review)

| ID | Aggregate | Severity | Gap Description |
|----|-----------|----------|----------------|
| G1 | KeyBlock | **P1 Critical** | BlockType enum wrong (6 vs 8 values, only 2 overlap); field names don't match schema; confirm() oversimplified |
| G2 | SourceAnchor | **P1 Critical** | Completely wrong structure (standalone {anchor_ref, story_ref, excerpt} vs nested {story_summary_refs, excerpt, summary}) |
| G3 | MemoryItem | **P1 Critical** | Missing 8 spec-required fields; 2 of 4 enum values wrong (Experience/Soul/Knowledge/ReferenceExcerpt vs canon/working/experience) |
| G4 | Missing aggregates | **P1 Critical** | Creator, Pairing, StoryManifest, ForkBranch, ReferenceSource not in plan |
| G5 | TimelineEvent | **P2** | Plan has Timeline struct, spec has TimelineEvent aggregate; missing ForkBranch |
| G6 | ManuscriptPhase | **P2** | Phase names differ (write vs draft, provisional vs review, canon vs finalize) |

---

## 2. Per-Aggregate Revision

### 2.1 KeyBlock (P1 Critical — G1)

**Spec Anchor**: `schemas/domain/key-block.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/key_block.rs` → `KeyBlock`
**Generated TS type**: `packages/nexus-contracts/src/generated/KeyBlock.ts` → `KeyBlock`

#### 2.1.1 Current Plan Deviations

| Aspect | Plan (Wrong) | Spec/Schema (Correct) |
|--------|-------------|----------------------|
| Enum name | `KBType` | `BlockType` (from `common.schema.json`) |
| Enum values | Event, Character, Location, Object, Concept, Relationship (6) | character, ability, scene, organization, item, conflict, info_point, event (8) |
| ID field | `kb_ref` | `key_block_id` (pattern: `^kb_[a-zA-Z0-9]+$`) |
| Name field | `title` | `canonical_name` |
| Creator ref | `confirming_creator_id` | Not a direct field — derived from `WorldMembership.permissions.can_confirm_canon` |
| Body field | `content` (JSON value) | `body` (structured: `{summary, attributes, tags}`) |
| Source ref | `source_anchor_refs: Vec<String>` | `source_anchor: SourceAnchor` (structured object) |
| Confirm logic | Simple setter | Multi-gate check (role, base_versions, completeness, traceability, no conflicts) |

#### 2.1.2 Revised Field Definitions

All fields from `schemas/domain/key-block.schema.json`, line by line:

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | common.schema.json#/definitions/SchemaVersion |
| 2 | `key_block_id` | `KeyBlockId` (String) | ✅ | — | `pattern: ^kb_[a-zA-Z0-9]+$` | common.schema.json#/definitions/KeyBlockId |
| 3 | `world_id` | `WorldId` (String) | ✅ | — | `pattern: ^wld_[a-zA-Z0-9]+$` | common.schema.json#/definitions/WorldId |
| 4 | `block_type` | `BlockType` (enum) | ✅ | — | See enum table below | common.schema.json#/definitions/BlockType |
| 5 | `canonical_name` | `String` | ✅ | — | `minLength: 1, maxLength: 200` | Inline in key-block.schema.json |
| 6 | `status` | `String` | ✅ | — | enum: provisional/confirmed/deprecated/merged/deleted | Inline in key-block.schema.json |
| 7 | `revision` | `Option<u64>` | ❌ | — | `minimum: 0` | Inline in key-block.schema.json |
| 8 | `body.summary` | `Option<String>` | ❌ | — | — | body object |
| 9 | `body.attributes` | `Option<serde_json::Value>` | ❌ | — | Free-form key-value | body object |
| 10 | `body.tags` | `Option<Vec<String>>` | ❌ | — | Classification tags | body object |
| 11 | `source_anchor` | `Option<SourceAnchor>` | ❌ | — | See §6.1 | source-anchor.schema.json |
| 12 | `created_from_command_id` | `Option<CommandId>` | ❌ | — | `pattern: ^cmd_[a-zA-Z0-9]+$` | common.schema.json#/definitions/CommandId |
| 13 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 / ISO 8601 UTC | common.schema.json#/definitions/Timestamp |
| 14 | `updated_at` | `Option<Timestamp>` | ❌ | — | RFC 3339 / ISO 8601 UTC | common.schema.json#/definitions/Timestamp |

**BlockType enum** (from `common.schema.json` L74-78):

| Value | Plan Equivalent | Status |
|-------|----------------|--------|
| `character` | ✅ Character (renamed) | Match |
| `ability` | ❌ Missing | **New** |
| `scene` | ❌ Missing | **New** |
| `organization` | ❌ Missing | **New** |
| `item` | ✅ Object (renamed) | Match |
| `conflict` | ❌ Missing | **New** |
| `info_point` | ❌ Missing | **New** |
| `event` | ✅ Event (renamed) | Match |
| ~~Location~~ | Plan-only | **Removed** |
| ~~Concept~~ | Plan-only | **Removed** |
| ~~Relationship~~ | Plan-only | **Removed** |

**KeyBlock status enum** (from `key-block.schema.json` L33-34):

| Value | Meaning |
|-------|---------|
| `provisional` | Draft fact, not shared canon baseline |
| `confirmed` | Passed explicit gate; cannot be silently overwritten by normal update |
| `deprecated` | Superseded by newer version |
| `merged` | Merged into another KB |
| `deleted` | Soft-deleted |

#### 2.1.3 Revised Domain Logic Methods

```rust
impl KeyBlock {
    /// Create a new provisional KeyBlock.
    /// Precondition: caller must have WorldMembership with can_sync_kb=true.
    pub fn new(world_id: &str, block_type: BlockType, canonical_name: &str) -> Self;

    /// Transition provisional → confirmed.
    ///
    /// Gate requirements:
    /// 1. Initiator must have can_confirm_canon permission on the world
    /// 2. base_versions / revision must match server current (no version mismatch)
    /// 3. All required fields present and schema-valid
    /// 4. source_anchor must satisfy minimum traceability requirements
    /// 5. No unresolved hard conflicts
    ///
    /// Post-condition: Once confirmed, body facts cannot be silently overwritten.
    /// New information must go through supplementary fields, related entities,
    /// timeline append, or fork.
    pub fn confirm(
        &mut self,
        membership: &WorldMembership,
        base_revision: u64,
        conflict_check: &ConflictCheckResult,
    ) -> Result<(), DomainError>;

    /// Deprecate this KeyBlock (mark as superseded).
    pub fn deprecate(&mut self, replacement_kb_id: Option<&str>) -> Result<(), DomainError>;

    /// Merge this KeyBlock into another.
    pub fn merge_into(&mut self, target_kb_id: &str) -> Result<(), DomainError>;

    /// Soft-delete this KeyBlock.
    pub fn delete(&mut self) -> Result<(), DomainError>;

    /// Check if this KB is in confirmed state.
    pub fn is_confirmed(&self) -> bool;

    /// Check if body modifications are allowed.
    /// Only provisional KBs allow body updates; confirmed KBs require fork/append.
    pub fn can_modify_body(&self) -> bool;

    /// Validate source_anchor traceability.
    /// Per G6: source_anchor must reference visible story manifests in same world.
    pub fn validate_source_anchor(&self, world_id: &str) -> Result<(), DomainError>;
}
```

#### 2.1.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-KB-1 | `test_create_provisional_keyblock` | Create KB with valid block_type and canonical_name | KB created with `status: "provisional"`, `revision: None`, `schema_version: 1` |
| TC-KB-2 | `test_confirm_with_permission` | Confirm with owner membership (can_confirm_canon: true), matching revision, no conflicts | Status transitions to `"confirmed"`, revision increments |
| TC-KB-3 | `test_confirm_without_permission` | Attempt confirm with collaborator (can_confirm_canon: false) | Returns `DomainError::PermissionDenied` |
| TC-KB-4 | `test_confirm_with_conflict` | Attempt confirm when unresolved hard conflict exists | Returns `DomainError::UnresolvedConflict` |
| TC-KB-5 | `test_modify_confirmed_body_rejected` | Attempt to update body.summary on confirmed KB | Returns `DomainError::ImmutableConfirmedState` |
| TC-KB-6 | `test_all_block_types_serialize` | Serialize/deserialize all 8 BlockType variants | Roundtrip matches; serde output uses snake_case |
| TC-KB-7 | `test_keyblock_schema_contract_match` | Construct KeyBlock from domain, serialize to JSON, validate against `key-block.schema.json` | JSON validates against schema |
| TC-KB-8 | `test_keyblock_matches_generated_type` | Domain KeyBlock fields → nexus-contracts `KeyBlock` struct | Field names and types match exactly; compile-time assertion via `let _: contracts::KeyBlock = domain_kb.into()` |

---

### 2.2 SourceAnchor (P1 Critical — G2)

**Spec Anchor**: `schemas/common/source-anchor.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/common_types.rs` → `SourceAnchor`, `SourceSummaryRef`

> **Note**: SourceAnchor is a **value object**, not an aggregate. It is embedded in KeyBlock, Delta, and other entities.

#### 2.2.1 Current Plan Deviations

| Aspect | Plan (Wrong) | Spec/Schema (Correct) |
|--------|-------------|----------------------|
| Structure | Standalone struct `{anchor_ref, story_ref, excerpt, created_at}` | Value object `{story_summary_refs, excerpt, summary}` |
| Story ref | `story_ref: String` (single) | `story_summary_refs: Vec<SourceSummaryRef>` (multi, structured) |
| Sub-structure | Missing | `SourceSummaryRef {story_manifest_id, summary_unit_id, unit_kind}` |
| Extra fields | `anchor_ref`, `created_at` | Not in spec; removed |
| Summary field | Missing | `summary: Option<String>` — optional anchor summary |

#### 2.2.2 Revised Field Definitions

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `story_summary_refs` | `Option<Vec<SourceSummaryRef>>` | ❌ | None | Array of references | source-anchor.schema.json L9-31 |
| 2 | `excerpt` | `Option<String>` | ❌ | None | Max 1024 chars (G6 traceability rule, pending OpenAPI freeze) | source-anchor.schema.json L33-35 |
| 3 | `summary` | `Option<String>` | ❌ | None | Optional anchor summary | source-anchor.schema.json L37-39 |

**SourceSummaryRef sub-structure**:

| # | Field | Type | Required | Constraint |
|---|-------|------|----------|-----------|
| 1 | `story_manifest_id` | String | ✅ | `pattern: ^stm_[a-zA-Z0-9]+$` |
| 2 | `summary_unit_id` | String | ✅ | `pattern: ^sum_[a-zA-Z0-9]+$` |
| 3 | `unit_kind` | `Option<String>` | ❌ | e.g., `"chapter_summary"` |

#### 2.2.3 Revised Domain Logic Methods

```rust
impl SourceAnchor {
    /// Create SourceAnchor referencing a single story summary unit.
    pub fn new(
        story_manifest_id: &str,
        summary_unit_id: &str,
        unit_kind: Option<&str>,
    ) -> Self;

    /// Create SourceAnchor with excerpt only (no story refs).
    pub fn from_excerpt(excerpt: &str) -> Self;

    /// Add a story summary reference.
    pub fn add_summary_ref(&mut self, story_manifest_id: &str, summary_unit_id: &str, unit_kind: Option<&str>);

    /// Validate excerpt length (max 1024 chars per G6).
    pub fn validate_excerpt(&self) -> Result<(), DomainError>;

    /// Validate all story_summary_refs point to visible manifests in the given world.
    /// Per G6: must reference same world_id, caller-visible entities.
    pub fn validate_refs(&self, world_id: &str, visible_manifests: &[&str]) -> Result<(), DomainError>;
}
```

#### 2.2.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-SA-1 | `test_create_with_summary_ref` | Create with story_manifest_id and summary_unit_id | `story_summary_refs.len() == 1`, all fields populated |
| TC-SA-2 | `test_excerpt_max_length` | Create with 1024-char excerpt | `validate_excerpt()` returns Ok |
| TC-SA-3 | `test_excerpt_exceeds_limit` | Create with 1025-char excerpt | `validate_excerpt()` returns `DomainError::ExcerptTooLong` |
| TC-SA-4 | `test_null_excerpt_valid` | Create with no excerpt | `validate_excerpt()` returns Ok (excerpt is optional) |
| TC-SA-5 | `test_multi_summary_refs` | Add 3 story summary refs | `story_summary_refs.len() == 3` |
| TC-SA-6 | `test_serialize_roundtrip` | Serialize/deserialize | JSON matches schema; roundtrip identity |
| TC-SA-7 | `test_matches_generated_type` | Domain SourceAnchor → contracts::SourceAnchor | Compile-time match via Into trait |

---

### 2.3 MemoryItem (P1 Critical — G3)

**Spec Anchor**: `schemas/domain/memory.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/memory.rs` → `Memory`
**Note**: The generated struct is named `Memory` (not `MemoryItem`) — this is a codegen artifact; the domain spec name is `MemoryItem`.

#### 2.3.1 Current Plan Deviations

| Aspect | Plan (Wrong) | Spec/Schema (Correct) |
|--------|-------------|----------------------|
| Struct name | `Memory` | `MemoryItem` (spec) / `Memory` (generated) — domain uses spec name |
| ID field | `memory_id` | `memory_item_id` (pattern: `^mem_[a-zA-Z0-9]+$`) |
| Creator ref | `creator_ref` | `creator_id` |
| MemoryType enum | Experience/Soul/Knowledge/ReferenceExcerpt (4) | canon/working/experience (3) |
| World scope | `world_ref` | `world_id` |
| Title | `title: String` | Not in spec — removed |
| Content | `content: Option<Value>` | Not in spec — replaced by `summary` |
| Weight | `weight: Option<f64>` | Not in spec — removed |
| KB ref | `kb_ref: Option<String>` | Not in spec — use `source_refs` |
| Status field | Missing | `status: active/superseded/archived` |
| Memory kind | Missing | `memory_kind: generic/story_summary/research_material/review_note` |
| Embedding ref | Missing | `embedding_ref: Option<String>` |
| Source refs | `source_anchor_ref: Option<String>` | `source_refs: Option<Vec<{kind, id}>>` |
| Timestamps | Missing | `last_accessed_at`, `last_reinforced_at` |

#### 2.3.2 Revised Field Definitions

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | common.schema.json |
| 2 | `memory_item_id` | String | ✅ | — | `pattern: ^mem_[a-zA-Z0-9]+$` | memory.schema.json L14-16 |
| 3 | `creator_id` | `CreatorId` (String) | ✅ | — | `pattern: ^ctr_[a-zA-Z0-9]+$` | common.schema.json |
| 4 | `world_id` | `WorldId` (String) | ✅ | — | `pattern: ^wld_[a-zA-Z0-9]+$` | common.schema.json |
| 5 | `memory_type` | `MemoryType` (enum) | ✅ | — | canon/working/experience | common.schema.json |
| 6 | `memory_kind` | `Option<String>` | ❌ | None | enum: generic/story_summary/research_material/review_note | memory.schema.json L31-33 |
| 7 | `status` | `String` | ✅ | — | enum: active/superseded/archived | memory.schema.json L36-38 |
| 8 | `summary` | `Option<String>` | ❌ | None | Memory summary text | memory.schema.json L41-42 |
| 9 | `embedding_ref` | `Option<String>` | ❌ | None | Reference to vector embedding | memory.schema.json L45-46 |
| 10 | `source_refs[].kind` | String | ✅ (in item) | — | Source ref kind (e.g., "command") | memory.schema.json L55 |
| 11 | `source_refs[].id` | String | ✅ (in item) | — | Source entity ID | memory.schema.json L59 |
| 12 | `last_accessed_at` | `Option<Timestamp>` | ❌ | None | Last access timestamp (nullable) | memory.schema.json L67-68 |
| 13 | `last_reinforced_at` | `Option<Timestamp>` | ❌ | None | Last reinforcement timestamp (nullable) | memory.schema.json L71-72 |
| 14 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | common.schema.json |
| 15 | `updated_at` | `Option<Timestamp>` | ❌ | None | RFC 3339 UTC | common.schema.json |

**MemoryType enum** (from `common.schema.json` L79-83):

| Value | Scope | Meaning |
|-------|-------|---------|
| `canon` | World-scoped | Shared canonical memory, visible to all world members |
| `working` | Creator+World | Working memory for active creation context |
| `experience` | Creator-scoped | Creator's personal experience accumulation |

**MemoryKind enum** (from `memory.schema.json` L31-33):

| Value | Meaning |
|-------|---------|
| `generic` | General memory |
| `story_summary` | Story/Chapter summary |
| `research_material` | Research/reference excerpt |
| `review_note` | Review/feedback note |

#### 2.3.3 Revised Domain Logic Methods

```rust
impl MemoryItem {
    /// Create a new memory item.
    /// Precondition: creator must have active pairing (for persistence).
    pub fn new(
        creator_id: &str,
        world_id: &str,
        memory_type: MemoryType,
        memory_kind: Option<&str>,
    ) -> Self;

    /// Transition status: active → superseded.
    /// Newer memory item replaces this one.
    pub fn supersede(&mut self, replacement_id: &str) -> Result<(), DomainError>;

    /// Archive this memory item.
    pub fn archive(&mut self) -> Result<(), DomainError>;

    /// Record access for decay/reinforcement weighting.
    pub fn record_access(&mut self);

    /// Record reinforcement (e.g., from context assembly hit).
    pub fn record_reinforcement(&mut self);

    /// Add a source reference for provenance.
    pub fn add_source_ref(&mut self, kind: &str, id: &str);

    /// Validate creator/world scope and quota.
    pub fn validate_scope(&self, creator_quota: &CreatorQuota) -> Result<(), DomainError>;

    /// Check if this memory is active and accessible.
    pub fn is_active(&self) -> bool;
}
```

#### 2.3.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-MI-1 | `test_create_canon_memory` | Create canon memory with all required fields | `memory_type: Canon`, `status: "active"`, `memory_item_id` starts with `mem_` |
| TC-MI-2 | `test_supersede_transition` | Supersede active memory | `status` → `"superseded"` |
| TC-MI-3 | `test_archive_active_memory` | Archive active memory | `status` → `"archived"` |
| TC-MI-4 | `test_record_access_updates_timestamp` | Record access | `last_accessed_at` is set to current time |
| TC-MI-5 | `test_all_memory_types` | Create with canon, working, experience types | All 3 types serialize correctly |
| TC-MI-6 | `test_source_refs_accumulation` | Add 3 source refs | `source_refs.len() == 3` |
| TC-MI-7 | `test_schema_contract_match` | Serialize → validate against memory.schema.json | JSON validates |
| TC-MI-8 | `test_matches_generated_memory_type` | Domain fields → `contracts::Memory` | Compile-time match |

---

### 2.4 Creator (P1 Critical — New Aggregate — G4)

**Spec Anchor**: `schemas/domain/creator.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/creator.rs` → `Creator`
**V1.0 Scope**: Creator independent registration, Pairing, and permission layering are mandatory for V1.0.

#### 2.4.1 Current Plan Deviations

The existing plan has **no Creator aggregate at all**. This is a V1.0 mandatory item.

#### 2.4.2 Revised Field Definitions

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | creator.schema.json |
| 2 | `creator_id` | `CreatorId` (String) | ✅ | — | `pattern: ^ctr_[a-zA-Z0-9]+$` | common.schema.json |
| 3 | `user_id` | `Option<UserId>` | ❌ | None | `pattern: ^usr_[a-zA-Z0-9]+$`; null = unpaired | creator.schema.json L17-20 |
| 4 | `display_name` | `String` | ✅ | — | `minLength: 1, maxLength: 100` | creator.schema.json L22-25 |
| 5 | `status` | `String` | ✅ | — | enum: active/archived/locked | creator.schema.json L28-30 |
| 6 | `is_platform_owned` | `Option<bool>` | ❌ | `false` | Platform-hosted vs user-owned | creator.schema.json L33-35 |
| 7 | `api_key_ref` | `Option<String>` | ❌ | None | Reference to ACP/agent credential | creator.schema.json L38-39 |
| 8 | `registration_source` | `String` | ✅ | — | enum: cli/web_agent/platform | creator.schema.json L42-44 |
| 9 | `persona_summary` | `Option<String>` | ❌ | None | `maxLength: 500` | creator.schema.json L47-49 |
| 10 | `style_profile.tone` | `Option<Vec<String>>` | ❌ | None | Style tone tags | creator.schema.json L55 |
| 11 | `style_profile.narrative_preferences` | `Option<Vec<String>>` | ❌ | None | Narrative preference tags | creator.schema.json L58 |
| 12 | `style_profile.forbidden_patterns` | `Option<Vec<String>>` | ❌ | None | Forbidden pattern tags | creator.schema.json L61 |
| 13 | `experience_revision` | `Option<u64>` | ❌ | `0` | `minimum: 0`; 0 = template-only | creator.schema.json L73-76 |
| 14 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | creator.schema.json |
| 15 | `updated_at` | `Option<Timestamp>` | ❌ | None | RFC 3339 UTC | creator.schema.json |

**Key semantics**:
- `user_id` represents the **current default pairing** only; full pairing history lives in `Pairing` aggregate
- `is_platform_owned` is NOT a capability subset switch; it only indicates runtime location
- `experience_revision: 0` = unpaired creator, template-only experience; persistent experience requires active pairing
- `api_key_ref` points to platform secure storage for ACP/agent access credentials

#### 2.4.3 Revised Domain Logic Methods

```rust
impl Creator {
    /// Register a new creator (independent of User).
    /// Creator can register without User login (V1.0 mandatory).
    pub fn register(
        display_name: &str,
        registration_source: RegistrationSource,
        is_platform_owned: bool,
    ) -> Self;

    /// Pair this creator with a user.
    /// Creates a Pairing record and updates user_id.
    pub fn pair_with_user(&mut self, user_id: &str, pairing_source: PairingSource) -> Result<Pairing, DomainError>;

    /// Unpair from current user (revokes pairing).
    pub fn unpair(&mut self) -> Result<(), DomainError>;

    /// Update style profile.
    pub fn update_style_profile(&mut self, tone: Vec<String>, narrative_prefs: Vec<String>, forbidden: Vec<String>);

    /// Increment experience revision (after experience distillation).
    pub fn increment_experience_revision(&mut self);

    /// Check if creator can persist experience (requires active pairing).
    pub fn can_persist_experience(&self) -> bool;

    /// Archive this creator.
    pub fn archive(&mut self) -> Result<(), DomainError>;

    /// Lock this creator (admin action).
    pub fn lock(&mut self) -> Result<(), DomainError>;
}
```

#### 2.4.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-CR-1 | `test_register_independent_creator` | Register with registration_source=cli | Created with `status: "active"`, `user_id: None`, `experience_revision: 0` |
| TC-CR-2 | `test_pair_with_user` | Pair unpaired creator with user | `user_id` set; Pairing record created with status `"active"` |
| TC-CR-3 | `test_persist_experience_requires_pairing` | Increment experience on unpaired creator | `can_persist_experience()` returns `false` |
| TC-CR-4 | `test_platform_owned_creator` | Register platform-owned creator | `is_platform_owned: true` |
| TC-CR-5 | `test_style_profile_update` | Update tone and narrative preferences | Style profile fields updated correctly |
| TC-CR-6 | `test_registration_sources` | Register via cli, web_agent, platform | All 3 sources produce valid creators |
| TC-CR-7 | `test_schema_contract_match` | Serialize → validate against creator.schema.json | JSON validates |
| TC-CR-8 | `test_matches_generated_creator_type` | Domain Creator → `contracts::Creator` | Compile-time match |

---

### 2.5 Pairing (P1 Critical — New Aggregate — G4)

**Spec Anchor**: `schemas/domain/pairing.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/pairing.rs` → `Pairing`
**V1.0 Scope**: Pairing and permission layering are mandatory for V1.0.

#### 2.5.1 Current Plan Deviations

The existing plan has **no Pairing aggregate**. This is a V1.0 mandatory item.

#### 2.5.2 Revised Field Definitions

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | pairing.schema.json |
| 2 | `pairing_id` | String | ✅ | — | `pattern: ^prg_[a-zA-Z0-9]+$` | pairing.schema.json L14-16 |
| 3 | `creator_id` | `CreatorId` (String) | ✅ | — | `pattern: ^ctr_[a-zA-Z0-9]+$` | pairing.schema.json |
| 4 | `user_id` | `UserId` (String) | ✅ | — | `pattern: ^usr_[a-zA-Z0-9]+$` | pairing.schema.json |
| 5 | `pairing_source` | `String` | ✅ | — | enum: auto_cli/manual_web/platform_auto | pairing.schema.json L25-27 |
| 6 | `status` | `String` | ✅ | — | enum: active/revoked | pairing.schema.json L30-32 |
| 7 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | pairing.schema.json |
| 8 | `revoked_at` | `Option<Timestamp>` | ❌ | None | Null if active | pairing.schema.json L38-39 |

**Key semantics**:
- `Creator.user_id` is ONLY the current default pairing — NOT a replacement for Pairing audit history
- Unpaired creators can still register, be invited to worlds, and propose to authorized worlds
- Unpaired creators' long-term Soul/Experience/Memory does NOT persist by default

#### 2.5.3 Revised Domain Logic Methods

```rust
impl Pairing {
    /// Create a new pairing between creator and user.
    pub fn new(creator_id: &str, user_id: &str, pairing_source: PairingSource) -> Self;

    /// Revoke this pairing.
    /// Sets status to "revoked" and records revoked_at timestamp.
    pub fn revoke(&mut self) -> Result<(), DomainError>;

    /// Check if this pairing is active.
    pub fn is_active(&self) -> bool;

    /// Validate that this pairing authorizes the given creator+user combination.
    pub fn authorizes(&self, creator_id: &str, user_id: &str) -> bool;
}
```

#### 2.5.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-PR-1 | `test_create_auto_cli_pairing` | Auto-CLI pairing creation | `pairing_source: "auto_cli"`, `status: "active"` |
| TC-PR-2 | `test_revoke_pairing` | Revoke active pairing | `status` → `"revoked"`, `revoked_at` is set |
| TC-PR-3 | `test_authorizes_correct_pair` | Check authorization with matching IDs | Returns `true` |
| TC-PR-4 | `test_authorizes_wrong_user` | Check authorization with wrong user_id | Returns `false` |
| TC-PR-5 | `test_schema_contract_match` | Serialize → validate against pairing.schema.json | JSON validates |
| TC-PR-6 | `test_matches_generated_pairing_type` | Domain Pairing → `contracts::Pairing` | Compile-time match |

---

### 2.6 StoryManifest (P1 Critical — New Aggregate — G4)

**Spec Anchor**: `schemas/domain/story-manifest.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/story_manifest.rs` → `StoryManifest`
**V1.0 Scope**: StoryManifest is mandatory for V1.0 as platform summary authority.

#### 2.6.1 Current Plan Deviations

The existing plan has **no StoryManifest aggregate**. This is a V1.0 mandatory item (blocking correction).

#### 2.6.2 Revised Field Definitions

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | story-manifest.schema.json |
| 2 | `story_manifest_id` | String | ✅ | — | `pattern: ^stm_[a-zA-Z0-9]+$` | story-manifest.schema.json L14-16 |
| 3 | `world_id` | `WorldId` (String) | ✅ | — | `pattern: ^wld_[a-zA-Z0-9]+$` | story-manifest.schema.json |
| 4 | `creator_id` | `CreatorId` (String) | ✅ | — | `pattern: ^ctr_[a-zA-Z0-9]+$` | story-manifest.schema.json |
| 5 | `manifest_type` | `String` | ✅ | — | enum: chapter/arc/story/excerpt | story-manifest.schema.json L25-27 |
| 6 | `status` | `String` | ✅ | — | enum: summary_ready/staged_for_publish/published/archived | story-manifest.schema.json L30-32 |
| 7 | `title` | `String` | ✅ | — | `minLength: 1` | story-manifest.schema.json L35 |
| 8 | `summary_unit_id` | String | ✅ | — | `pattern: ^sum_[a-zA-Z0-9]+$` | story-manifest.schema.json L40-42 |
| 9 | `summary_text` | `Option<String>` | ❌ | None | Platform-authoritative summary | story-manifest.schema.json L45-46 |
| 10 | `output_manuscript` | `Option<bool>` | ❌ | `true` | Whether manuscript output enabled | story-manifest.schema.json L49-50 |
| 11 | `manuscript_storage` | `Option<String>` | ❌ | None | enum: none/local_workspace/platform_sandbox | story-manifest.schema.json L54-56 |
| 12 | `local_path` | `Option<String>` | ❌ | None | When manuscript_storage=local_workspace | story-manifest.schema.json L59 |
| 13 | `sandbox_path` | `Option<String>` | ❌ | None | When manuscript_storage=platform_sandbox | story-manifest.schema.json L63 |
| 14 | `content_hash` | `Option<String>` | ❌ | None | `sha256:xxx` format | story-manifest.schema.json L67 |
| 15 | `published_artifact_id` | `Option<String>` | ❌ | None | Published artifact reference | story-manifest.schema.json L71 |
| 16 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | story-manifest.schema.json |
| 17 | `updated_at` | `Option<Timestamp>` | ❌ | None | RFC 3339 UTC | story-manifest.schema.json |

**Key semantics**:
- StoryManifest is the **platform-side chapter/arc manifest and summary authority**
- `summary_text` is generated by CLI/execution side, platform receives, validates, and stores
- `output_manuscript=false` means platform still accepts summary_text, KB, Timeline, but no manuscript file required
- `manuscript_storage` determines where body files live: local workspace, platform sandbox, or none
- `summary_unit_id` is used by `SourceAnchor` to establish traceability

#### 2.6.3 Revised Domain Logic Methods

```rust
impl StoryManifest {
    /// Create a new story manifest.
    pub fn new(
        world_id: &str,
        creator_id: &str,
        manifest_type: ManifestType,
        title: &str,
        summary_unit_id: &str,
    ) -> Self;

    /// Set summary text (platform-authoritative).
    pub fn set_summary_text(&mut self, text: &str);

    /// Configure manuscript output and storage.
    pub fn configure_manuscript(
        &mut self,
        output: bool,
        storage: ManuscriptStorage,
        local_path: Option<&str>,
        sandbox_path: Option<&str>,
    ) -> Result<(), DomainError>;

    /// Stage for publishing.
    /// Pre: summary_text is set, content_hash is computed.
    pub fn stage_for_publish(&mut self) -> Result<(), DomainError>;

    /// Mark as published.
    pub fn publish(&mut self, artifact_id: &str) -> Result<(), DomainError>;

    /// Archive this manifest.
    pub fn archive(&mut self) -> Result<(), DomainError>;

    /// Validate storage configuration consistency.
    /// e.g., manuscript_storage=local_workspace requires local_path.
    pub fn validate_storage_config(&self) -> Result<(), DomainError>;
}
```

#### 2.6.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-SM-1 | `test_create_chapter_manifest` | Create with manifest_type=chapter | `status: "summary_ready"`, fields populated |
| TC-SM-2 | `test_stage_for_publish` | Stage manifest with summary set | `status` → `"staged_for_publish"` |
| TC-SM-3 | `test_publish_with_artifact` | Publish staged manifest | `status` → `"published"`, `published_artifact_id` set |
| TC-SM-4 | `test_storage_config_validation` | Set local_workspace without local_path | Returns `DomainError::InvalidStorageConfig` |
| TC-SM-5 | `test_all_manifest_types` | Create chapter, arc, story, excerpt | All 4 types serialize correctly |
| TC-SM-6 | `test_schema_contract_match` | Serialize → validate against story-manifest.schema.json | JSON validates |
| TC-SM-7 | `test_matches_generated_type` | Domain StoryManifest → `contracts::StoryManifest` | Compile-time match |

---

### 2.7 ForkBranch (P1 Critical — New Aggregate — G4)

**Spec Anchor**: Domain specification (ForkBranch aggregate defined from V1.0 data model; schema not yet created)
**Schema Status**: ❌ **No schema file exists yet** — `schemas/domain/fork-branch.schema.json` is MISSING
**Generated Rust type**: ❌ Not generated
**V1.0 Scope**: World Fork is mandatory for V1.0.

> **IMPORTANT**: This aggregate has no corresponding JSON Schema. A schema file MUST be created before codegen can generate types. This is a prerequisite task.

#### 2.7.1 Field Definitions (from spec only)

| # | Field | Type | Required | Default | Constraint | Spec Location |
|---|-------|------|----------|---------|-----------|---------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | (V1.0 spec) |
| 2 | `fork_branch_id` | String | ✅ | — | `pattern: ^fbk_[a-zA-Z0-9]+$` (convention) | (V1.0 spec) |
| 3 | `world_id` | `WorldId` (String) | ✅ | — | Child world ID | (V1.0 spec) |
| 4 | `parent_world_id` | `WorldId` (String) | ✅ | — | Parent world ID | (V1.0 spec) |
| 5 | `parent_branch_id` | String | ✅ | — | Parent fork branch ID | (V1.0 spec) |
| 6 | `forked_from_event_id` | `TimelineEventId` (String) | ✅ | — | Event where fork occurred | (V1.0 spec) |
| 7 | `status` | `String` | ✅ | — | enum: active/archived | (V1.0 spec) |
| 8 | `verification_status` | `String` | ✅ | — | enum: unverified/requested/verified/rejected | (V1.0 spec) |
| 9 | `created_by_creator_id` | `CreatorId` (String) | ✅ | — | Creator who initiated fork | (V1.0 spec) |
| 10 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | (V1.0 spec) |

**ForkBranch status enum**: `active, archived`
**ForkBranch verification_status enum**: `unverified, requested, verified, rejected`

#### 2.7.2 Revised Domain Logic Methods

```rust
impl ForkBranch {
    /// Create a new fork from a parent world at a specific event.
    /// Must reference valid parent_world_id, parent_branch_id, and forked_from_event_id.
    pub fn fork_from(
        world_id: &str,
        parent_world_id: &str,
        parent_branch_id: &str,
        forked_from_event_id: &str,
        creator_id: &str,
    ) -> Self;

    /// Request verification of this fork.
    pub fn request_verification(&mut self) -> Result<(), DomainError>;

    /// Verify this fork (admin/policy action).
    pub fn verify(&mut self) -> Result<(), DomainError>;

    /// Reject this fork verification.
    pub fn reject(&mut self, reason: &str) -> Result<(), DomainError>;

    /// Archive this fork branch.
    pub fn archive(&mut self) -> Result<(), DomainError>;

    /// Validate that structured writes only go to child world/branch.
    pub fn validate_write_scope(&self, target_world_id: &str) -> Result<(), DomainError>;
}
```

#### 2.7.3 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-FB-1 | `test_create_fork` | Fork from parent at event evt_123 | All fields populated, `status: "active"`, `verification_status: "unverified"` |
| TC-FB-2 | `test_request_verification` | Request verification | `verification_status` → `"requested"` |
| TC-FB-3 | `test_verify_fork` | Verify fork | `verification_status` → `"verified"` |
| TC-FB-4 | `test_write_scope_validation` | Attempt write to parent world | Returns `DomainError::InvalidForkWriteScope` |
| TC-FB-5 | `test_all_verification_statuses` | Cycle through unverified→requested→verified/rejected | All transitions valid |

---

### 2.8 ReferenceSource (P1 Critical — New Aggregate — G4)

**Spec Anchor**: Domain specification (ReferenceSource aggregate defined from V1.0 data model; schema not yet created)
**Schema Status**: ❌ **No schema file exists yet** — `schemas/domain/reference-source.schema.json` is MISSING
**Generated Rust type**: ❌ Not generated
**Note**: **Local-only** — does NOT sync to platform. Shared excerpts go through `MemoryItem(memory_kind=research_material)`.

#### 2.8.1 Field Definitions (from spec only)

| # | Field | Type | Required | Default | Constraint | Spec Location |
|---|-------|------|----------|---------|-----------|---------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | (V1.0 spec) |
| 2 | `reference_source_id` | String | ✅ | — | `pattern: ^ref_[a-zA-Z0-9]+$` (convention) | (V1.0 spec) |
| 3 | `workspace_id` | `WorkspaceId` (String) | ✅ | — | `pattern: ^wrk_[a-zA-Z0-9]+$` | (V1.0 spec) |
| 4 | `source_type` | `String` | ✅ | — | enum: file/pdf/url/note | (V1.0 spec) |
| 5 | `uri` | `String` | ✅ | — | File path or URL | (V1.0 spec) |
| 6 | `title` | `String` | ✅ | — | Display title | (V1.0 spec) |
| 7 | `tags` | `Option<Vec<String>>` | ❌ | None | Classification tags | (V1.0 spec) |
| 8 | `content_hash` | `Option<String>` | ❌ | None | `sha256:xxx` | (V1.0 spec) |
| 9 | `scan_status` | `String` | ✅ | — | enum: pending/scanned/failed/ignored | (V1.0 spec) |
| 10 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | (V1.0 spec) |
| 11 | `updated_at` | `Option<Timestamp>` | ❌ | None | RFC 3339 UTC | (V1.0 spec) |

#### 2.8.2 Revised Domain Logic Methods

```rust
impl ReferenceSource {
    /// Register a new reference source.
    pub fn register(
        workspace_id: &str,
        source_type: ReferenceSourceType,
        uri: &str,
        title: &str,
    ) -> Self;

    /// Mark as scanned successfully.
    pub fn mark_scanned(&mut self, content_hash: Option<&str>);

    /// Mark scan as failed.
    pub fn mark_scan_failed(&mut self);

    /// Ignore this reference source.
    pub fn ignore(&mut self);

    /// Convert an excerpt to a MemoryItem for platform sync.
    /// The ReferenceSource itself stays local; only the MemoryItem syncs.
    pub fn extract_to_memory_item(&self, creator_id: &str, world_id: &str, excerpt: &str) -> MemoryItem;

    /// Validate URI format based on source_type.
    pub fn validate_uri(&self) -> Result<(), DomainError>;
}
```

#### 2.8.3 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-RS-1 | `test_register_file_reference` | Register file:// reference | `source_type: "file"`, `scan_status: "pending"` |
| TC-RS-2 | `test_mark_scanned` | Mark as scanned with hash | `scan_status` → `"scanned"`, `content_hash` set |
| TC-RS-3 | `test_extract_to_memory` | Extract excerpt to MemoryItem | Returns MemoryItem with `memory_kind: "research_material"` |
| TC-RS-4 | `test_all_source_types` | Register file, pdf, url, note | All 4 types serialize correctly |

---

### 2.9 TimelineEvent (P2 — G5)

**Spec Anchor**: `schemas/domain/timeline-event.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/timeline_event.rs` → `TimelineEvent`

#### 2.9.1 Current Plan Deviations

| Aspect | Plan (Wrong) | Spec/Schema (Correct) |
|--------|-------------|----------------------|
| Struct name | `Timeline` | `TimelineEvent` |
| Approach | Struct with `kb_refs`, `timeline_type`, `branch_parent_timeline_id` | Full aggregate with `timeline_event_id`, `branch_id`, `event_type`, `status`, `sequence_no`, causality |
| ID scheme | N/A | `timeline_event_id` with pattern `^evt_[a-zA-Z0-9]+$` |
| Causality | Missing | `caused_by_event_ids`, `affected_key_block_ids` |
| Sequence | `current_sequence: u64` | `sequence_no: u64` (per branch) |
| Status | Missing | canon/provisional/rejected |

#### 2.9.2 Revised Field Definitions

| # | Field | Type | Required | Default | Constraint | Schema Location |
|---|-------|------|----------|---------|-----------|-----------------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | — | `minimum: 1` | timeline-event.schema.json |
| 2 | `timeline_event_id` | `TimelineEventId` (String) | ✅ | — | `pattern: ^evt_[a-zA-Z0-9]+$` | common.schema.json |
| 3 | `world_id` | `WorldId` (String) | ✅ | — | `pattern: ^wld_[a-zA-Z0-9]+$` | timeline-event.schema.json |
| 4 | `branch_id` | `String` | ✅ | — | Fork branch ID (root or specific fork) | timeline-event.schema.json L22-23 |
| 5 | `event_type` | `String` | ✅ | — | enum: story_advance/state_update/fork_marker/official_progression/publish_marker | timeline-event.schema.json L26-28 |
| 6 | `status` | `String` | ✅ | — | enum: canon/provisional/rejected | timeline-event.schema.json L31-33 |
| 7 | `sequence_no` | `u64` | ✅ | — | `minimum: 0`; within branch | timeline-event.schema.json L36-38 |
| 8 | `title` | `Option<String>` | ❌ | None | `minLength: 1, maxLength: 200` | timeline-event.schema.json L41-43 |
| 9 | `summary` | `Option<String>` | ❌ | None | Event summary | timeline-event.schema.json L46-47 |
| 10 | `caused_by_event_ids` | `Option<Vec<TimelineEventId>>` | ❌ | None | Preceding events | timeline-event.schema.json L51-55 |
| 11 | `affected_key_block_ids` | `Option<Vec<KeyBlockId>>` | ❌ | None | Affected KBs | timeline-event.schema.json L58-62 |
| 12 | `source_command_id` | `Option<CommandId>` | ❌ | None | Triggering command | timeline-event.schema.json L65-66 |
| 13 | `created_at` | `Timestamp` (String) | ✅ | — | RFC 3339 UTC | timeline-event.schema.json |

#### 2.9.3 Revised Domain Logic Methods

```rust
impl TimelineEvent {
    /// Create a new timeline event on a branch.
    pub fn new(
        world_id: &str,
        branch_id: &str,
        event_type: TimelineEventType,
        sequence_no: u64,
    ) -> Self;

    /// Promote provisional → canon.
    /// Gate requirements:
    /// - Must not reorder existing canon sequence
    /// - Must revalidate branch_id, causality, sequence constraints, permissions, current head
    /// - Any promotion causing canon order conflict → return timeline conflict
    /// - Default promotion: append as new canon head
    pub fn promote_to_canon(
        &mut self,
        membership: &WorldMembership,
        current_head: &TimelineEventId,
        branch_events: &[TimelineEvent],
    ) -> Result<(), DomainError>;

    /// Reject a provisional or canon event.
    pub fn reject(&mut self) -> Result<(), DomainError>;

    /// Add causal predecessor.
    pub fn add_cause(&mut self, event_id: &str);

    /// Add affected KeyBlock reference.
    pub fn add_affected_kb(&mut self, kb_id: &str);

    /// Validate causality: caused_by_event_ids must reference same world.
    pub fn validate_causality(&self, world_id: &str) -> Result<(), DomainError>;

    /// Validate sequence is monotonic within branch.
    pub fn validate_sequence(&self, prev_sequence: u64) -> Result<(), DomainError>;
}
```

#### 2.9.4 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-TE-1 | `test_create_story_advance` | Create story_advance event | `event_type: "story_advance"`, `status: "provisional"` |
| TC-TE-2 | `test_promote_to_canon` | Promote with valid permissions and head | `status` → `"canon"` |
| TC-TE-3 | `test_promote_causes_order_conflict` | Promote event that would reorder canon | Returns `DomainError::TimelineConflict` |
| TC-TE-4 | `test_causality_validation_same_world` | Add cross-world causal ref | Returns `DomainError::CausalityViolation` |
| TC-TE-5 | `test_all_event_types` | Create with all 5 event types | All serialize correctly |
| TC-TE-6 | `test_matches_generated_type` | Domain TimelineEvent → `contracts::TimelineEvent` | Compile-time match |

---

### 2.10 ManuscriptPhase / ManuscriptState (P2 — G6)

**Spec Anchor**: `common.schema.json` (ManuscriptPhase enum)
**Note**: `ManuscriptState` is a **local-only aggregate** — platform does not own it in V1.0.

#### 2.10.1 Current Plan Deviations

| Aspect | Plan (Wrong) | Spec/Schema (Correct) |
|--------|-------------|----------------------|
| Phase names | Brainstorm→Write→Review→Provisional→Canon | brainstorm→draft→review→finalize→published |
| Phase state | ManuscriptState enum (Draft/Proposed/Confirmed/Published) | ManuscriptState is a **full aggregate**, not just an enum |
| Transition logic | Hardcoded valid_transitions with wrong names | Must match spec phases exactly |

#### 2.10.2 ManuscriptPhase Enum (from `common.schema.json` L59-63)

| Value | Plan Equivalent | Spec Meaning |
|-------|----------------|-------------|
| `brainstorm` | ✅ Brainstorm | Initial ideation phase |
| `draft` | ~~Write~~ | Active writing phase |
| `review` | ~~Provisional~~ | Review and revision phase |
| `finalize` | ~~Canon~~ | Final approval phase |
| `published` | ❌ Missing | Published state |

#### 2.10.3 ManuscriptState Aggregate Fields (from V1.0 spec)

| # | Field | Type | Required | Constraint |
|---|-------|------|----------|-----------|
| 1 | `schema_version` | `SchemaVersion` (u32) | ✅ | `minimum: 1` |
| 2 | `manuscript_state_id` | String | ✅ | `pattern: ^mss_[a-zA-Z0-9]+$` (convention) |
| 3 | `workspace_id` | `WorkspaceId` (String) | ✅ | `pattern: ^wrk_[a-zA-Z0-9]+$` |
| 4 | `world_id` | `WorldId` (String) | ✅ | `pattern: ^wld_[a-zA-Z0-9]+$` |
| 5 | `creator_id` | `CreatorId` (String) | ✅ | `pattern: ^ctr_[a-zA-Z0-9]+$` |
| 6 | `manuscript_phase` | `ManuscriptPhase` (enum) | ✅ | brainstorm/draft/review/finalize/published |
| 7 | `active_manifest_id` | `Option<StoryManifestId>` | ❌ | Currently active manifest |
| 8 | `last_confirmed_delta_sequence` | `Option<u64>` | ❌ | For conflict detection |
| 9 | `updated_at` | `Timestamp` (String) | ✅ | RFC 3339 UTC |

#### 2.10.4 Revised Domain Logic Methods

```rust
impl ManuscriptStateMachine {
    /// Create new manuscript state in brainstorm phase.
    pub fn new(workspace_id: &str, world_id: &str, creator_id: &str) -> Self;

    /// Transition to next phase.
    /// Valid transitions: brainstorm→draft→review→finalize→published
    pub fn promote(&mut self) -> Result<(), DomainError>;

    /// Set active manifest.
    pub fn set_active_manifest(&mut self, manifest_id: &str);

    /// Get current phase.
    pub fn current_phase(&self) -> ManuscriptPhase;

    /// Check if phase transition is valid.
    pub fn can_transition_to(&self, target: ManuscriptPhase) -> bool;

    /// Validate provisional cleanup before finalize/published gate.
    /// Provisional records must be promoted or cleaned before entering finalize/published.
    pub fn validate_pre_gate_cleanup(&self, provisional_count: usize) -> Result<(), DomainError>;
}
```

#### 2.10.5 TDD Test Cases

| # | Test Name | Scenario | Expected |
|---|-----------|----------|----------|
| TC-MS-1 | `test_full_phase_progression` | Promote from brainstorm through published | All 4 transitions succeed |
| TC-MS-2 | `test_invalid_transition` | Attempt brainstorm→finalize | Returns `DomainError::InvalidPhaseTransition` |
| TC-MS-3 | `test_published_is_final` | Attempt promote from published | Returns `DomainError::InvalidPhaseTransition` |
| TC-MS-4 | `test_pre_gate_cleanup_with_provisionals` | Validate cleanup with outstanding provisionals | Returns `DomainError::ProvisionalRecordsExist` |
| TC-MS-5 | `test_phase_serialize_roundtrip` | All 5 phases serialize/deserialize | snake_case JSON roundtrip matches |

---

### 2.11 World (Existing — Alignment Check)

**Spec Anchor**: `schemas/domain/world.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/world.rs` → `World`

#### 2.11.1 Schema Alignment Status: ✅ ALIGNED

All fields in `schemas/domain/world.schema.json` match exactly. No plan deviations detected.

Key fields: `world_id`, `owner_creator_id`, `title`, `slug`, `status`, `visibility`, `time_policy`, `canon_revision`, `current_timeline_head_id`, `current_time_pointer`, `root_fork_branch_id`, `world_rules`, `created_at`, `updated_at`.

#### 2.11.2 Domain Logic Methods (supplementary)

```rust
impl World {
    /// Create a new world.
    pub fn new(owner_creator_id: &str, title: &str, slug: &str, visibility: Visibility, time_policy: TimePolicy) -> Self;

    /// Update time pointer (world progression).
    pub fn advance_time_pointer(&mut self, event_id: &str) -> Result<(), DomainError>;

    /// Fork this world.
    /// Creates a new child world and ForkBranch record.
    pub fn fork(&self, creator_id: &str, forked_from_event_id: &str) -> Result<(World, ForkBranch), DomainError>;

    /// Update visibility.
    pub fn set_visibility(&mut self, visibility: Visibility) -> Result<(), DomainError>;

    /// Validate time policy: history mutation requires fork.
    /// Per world_rules.history_mutation_requires_fork.
    pub fn validate_time_policy(&self) -> Result<(), DomainError>;
}
```

---

### 2.12 WorldMembership (Existing — Alignment Check)

**Spec Anchor**: `schemas/domain/world-membership.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/world_membership.rs` → `WorldMembership`

#### 2.12.1 Schema Alignment Status: ✅ ALIGNED

All fields match. Permissions object includes: `can_sync_kb`, `can_publish`, `can_fork`, `can_invite_official_creator`, `can_confirm_canon`.

---

### 2.13 DeltaBundle (Existing — Alignment Check)

**Spec Anchor**: `schemas/domain/bundle.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/bundle.rs` → `DeltaBundle`

#### 2.13.1 Schema Alignment Status: ✅ ALIGNED

All fields match. Notable additions in schema vs spec: `submitting_creator_id`, `manuscript_phase`, `last_confirmed_delta_sequence`, `bundle_apply_status`, `delta_results` (server-side write-back fields).

---

### 2.14 SyncCommand (Existing — Alignment Check)

**Spec Anchor**: `schemas/domain/sync-command.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/sync_command.rs` → `SyncCommand`

#### 2.14.1 Schema Alignment Status: ✅ ALIGNED

All fields match. Command types: `advance_world`, `inject_future_event`, `extract_kb`, `sync_push`, `sync_pull`, `fork_world`, `publish_story`.

---

### 2.15 OutboxEntry (Existing — Alignment Check)

**Spec Anchor**: `schemas/domain/outbox-entry.schema.json`
**Generated Rust type**: `crates/nexus-contracts/src/generated/outbox_entry.rs` → `OutboxEntry`

#### 2.15.1 Schema Alignment Status: ✅ ALIGNED

All fields match. Delivery states: `staged`, `ready`, `sent`, `acked`, `conflicted`, `failed`.

---

## 3. Dependency Order

The following diagram shows the recommended implementation order based on aggregate dependencies:

```
Layer 0 (No dependencies — Foundation):
├── Creator
├── World
└── Common types (enums, SourceAnchor, VersionRef)

Layer 1 (Depends on Layer 0):
├── Pairing (depends on: Creator)
├── WorldMembership (depends on: World, Creator)
├── ManuscriptState (depends on: World, Creator, ManuscriptPhase enum)
└── ReferenceSource (depends on: WorkspaceId, local-only)

Layer 2 (Depends on Layers 0-1):
├── KeyBlock (depends on: World, SourceAnchor, WorldMembership for permissions)
├── StoryManifest (depends on: World, Creator)
├── ForkBranch (depends on: World, Creator, TimelineEvent)
└── TimelineEvent (depends on: World, ForkBranch for branch_id)

Layer 3 (Depends on Layers 0-2):
├── MemoryItem (depends on: Creator, World, StoryManifest via source_refs)
├── SyncCommand (depends on: World, Creator, Workspace)
└── DeltaBundle (depends on: World, Creator, SyncCommand, all delta types)

Layer 4 (Depends on all above):
├── OutboxEntry (depends on: DeltaBundle)
├── Consistency rules (cross-aggregate validation)
└── ManuscriptStateMachine (cross-cutting gate logic)
```

**Recommended implementation sequence**:
1. Common types and enums (already generated)
2. Creator + World (foundation aggregates)
3. Pairing + WorldMembership (relationship aggregates)
4. KeyBlock + SourceAnchor (core knowledge unit)
5. TimelineEvent (core timeline unit)
6. ForkBranch (depends on TimelineEvent — **requires schema creation first**)
7. StoryManifest (platform summary authority)
8. MemoryItem (memory system)
9. ReferenceSource (local-only research — **requires schema creation first**)
10. SyncCommand + DeltaBundle (sync infrastructure)
11. OutboxEntry (delivery queue)
12. ManuscriptState + ManuscriptStateMachine (phase gates)
13. Consistency rules (cross-aggregate validation)
14. Integration tests (domain ↔ nexus-contracts type matching)

---

## 4. Integration with nexus-contracts

### 4.1 Strategy

The domain crate (`nexus-domain`) builds business logic on top of generated contract types from `nexus-contracts`. The integration must guarantee **zero drift** between domain types and wire contracts.

### 4.2 Compile-Time Assertions

```rust
// crates/nexus-domain/src/contract_assertions.rs

/// Compile-time assertion: domain types must be convertible to/from contract types.
/// These will fail to compile if field names or types diverge.

// 1. SourceAnchor domain → contract
fn _assert_source_anchor_match() {
    fn check<'a>(domain: &'a super::source_anchor::SourceAnchor) -> nexus_contracts::generated::SourceAnchor {
        nexus_contracts::generated::SourceAnchor {
            story_summary_refs: domain.story_summary_refs.as_ref().map(|refs| {
                refs.iter().map(|r| nexus_contracts::generated::SourceSummaryRef {
                    story_manifest_id: r.story_manifest_id.clone(),
                    summary_unit_id: r.summary_unit_id.clone(),
                    unit_kind: r.unit_kind.clone(),
                }).collect()
            }),
            excerpt: domain.excerpt.clone(),
            summary: domain.summary.clone(),
        }
    }
}

// 2. Enum variant count assertion (via const)
const BLOCK_TYPE_COUNT: usize = 8; // Must match common.schema.json
const MEMORY_TYPE_COUNT: usize = 3; // Must match common.schema.json
const MANUSCRIPT_PHASE_COUNT: usize = 5; // Must match common.schema.json
```

### 4.3 Serde Roundtrip Tests

Every domain aggregate must have a serde roundtrip test that validates against the corresponding JSON Schema:

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test that domain KeyBlock serializes to JSON that validates against key-block.schema.json
    #[test]
    fn test_keyblock_json_schema_conformance() {
        let kb = KeyBlock::new("wld_test123", BlockType::Character, "Test Character");
        let json = serde_json::to_value(&kb).unwrap();

        // Load schema and validate
        let schema_str = include_str!("../../../schemas/domain/key-block.schema.json");
        let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
        let validator = jsonschema::JSONSchema::compile(&schema).unwrap();

        let result = validator.validate(&json);
        assert!(result.is_ok(), "KeyBlock JSON does not conform to schema: {:?}", result.err());
    }

    /// Test that contract KeyBlock deserializes from same JSON domain produces
    #[test]
    fn test_keyblock_domain_contract_equivalence() {
        let domain_kb = KeyBlock::new("wld_test", BlockType::Event, "Test Event");
        let json = serde_json::to_string(&domain_kb).unwrap();

        let contract_kb: nexus_contracts::generated::KeyBlock =
            serde_json::from_str(&json).unwrap();

        assert_eq!(contract_kb.key_block_id, domain_kb.key_block_id);
        assert_eq!(contract_kb.block_type, nexus_contracts::generated::BlockType::Event);
        assert_eq!(contract_kb.canonical_name, domain_kb.canonical_name);
    }
}
```

### 4.4 Type Correspondence Table

| Domain Type | Contract Type (Rust) | Contract Type (TS) | Schema File |
|-------------|---------------------|--------------------|-------------|
| `KeyBlock` | `contracts::generated::KeyBlock` | `KeyBlock` | `key-block.schema.json` |
| `MemoryItem` | `contracts::generated::Memory` | `Memory` | `memory.schema.json` |
| `TimelineEvent` | `contracts::generated::TimelineEvent` | `TimelineEvent` | `timeline-event.schema.json` |
| `Creator` | `contracts::generated::Creator` | `Creator` | `creator.schema.json` |
| `Pairing` | `contracts::generated::Pairing` | `Pairing` | `pairing.schema.json` |
| `StoryManifest` | `contracts::generated::StoryManifest` | `StoryManifest` | `story-manifest.schema.json` |
| `World` | `contracts::generated::World` | `World` | `world.schema.json` |
| `WorldMembership` | `contracts::generated::WorldMembership` | `WorldMembership` | `world-membership.schema.json` |
| `SyncCommand` | `contracts::generated::SyncCommand` | `SyncCommand` | `sync-command.schema.json` |
| `DeltaBundle` | `contracts::generated::DeltaBundle` | `DeltaBundle` | `bundle.schema.json` |
| `OutboxEntry` | `contracts::generated::OutboxEntry` | `OutboxEntry` | `outbox-entry.schema.json` |
| `SourceAnchor` | `contracts::generated::SourceAnchor` | (embedded) | `source-anchor.schema.json` |
| `ForkBranch` | ❌ Not yet generated | ❌ Not yet generated | ❌ **Schema missing** |
| `ReferenceSource` | ❌ Not yet generated | ❌ Not yet generated | ❌ **Schema missing** |
| `ManuscriptState` | ❌ Not yet generated | ❌ Not yet generated | ❌ **Schema missing** |

### 4.5 Prerequisite: Missing Schema Files

Before domain logic can be implemented for ForkBranch, ReferenceSource, and ManuscriptState, their JSON Schema files must be created:

| Schema File | Spec Source | Priority |
|-------------|-------------|----------|
| `schemas/domain/fork-branch.schema.json` | Fork branch aggregate (V1.0 mandatory) | **P1** — blocks domain implementation |
| `schemas/domain/reference-source.schema.json` | Reference source aggregate (V1.0 mandatory) | **P1** — blocks domain implementation |
| `schemas/domain/manuscript-state.schema.json` | Manuscript state aggregate (local-only) | **P2** — local-only, can defer |

After schema creation, the codegen pipeline (`pnpm run codegen`) must be re-run to generate Rust and TypeScript types.

---

## 5. Revised Task Breakdown (TDD)

> These tasks replace ALL tasks in the existing plan. Each task follows TDD discipline.

### Task 0: Prerequisites — Create Missing Schemas

- [ ] **Step 1**: Create `schemas/domain/fork-branch.schema.json` from §2.7 field definitions
  - Validate: `npx ajv validate -s schemas/common/common.schema.json -d schemas/domain/fork-branch.schema.json`
- [ ] **Step 2**: Create `schemas/domain/reference-source.schema.json` from §2.8 field definitions
  - Validate: same as above
- [ ] **Step 3**: Create `schemas/domain/manuscript-state.schema.json` from §2.10 field definitions
  - Validate: same as above
- [ ] **Step 4**: Run codegen pipeline: `pnpm run codegen`
  - Expected: New Rust types in `crates/nexus-contracts/src/generated/`
  - Expected: New TS types in `packages/nexus-contracts/src/generated/`
- [ ] **Step 5**: Verify workspace compiles: `cargo check --workspace`
- [ ] **Step 6**: Commit: `feat(schemas): add fork-branch, reference-source, manuscript-state schemas`

### Task 1: Initialize Domain Crate (Revised)

- [ ] **Step 1**: Create feature branch `feature/v1.0-domain-models` from `main`
- [ ] **Step 2**: Create `crates/nexus-domain/Cargo.toml` with dependency on `nexus-contracts`
  - Add dev-dependencies: `jsonschema`, `serde_json`
- [ ] **Step 3**: Create `crates/nexus-domain/src/lib.rs` with module declarations:
  ```rust
  pub mod creator;
  pub mod pairing;
  pub mod world;
  pub mod world_membership;
  pub mod key_block;
  pub mod timeline_event;
  pub mod fork_branch;
  pub mod story_manifest;
  pub mod memory_item;
  pub mod reference_source;
  pub mod source_anchor;
  pub mod manuscript_state;
  pub mod sync_command;
  pub mod delta_bundle;
  pub mod outbox_entry;
  pub mod consistency;
  pub mod errors;
  pub mod contract_assertions;
  ```
- [ ] **Step 4**: Update workspace `Cargo.toml` to include `nexus-domain`
- [ ] **Step 5**: Verify compilation: `cargo check -p nexus-domain`
- [ ] **Step 6**: Commit: `feat(domain): initialize domain logic crate`

### Task 2: Implement Creator + Pairing (Layer 0-1)

- [ ] **Step 1**: Write failing tests for Creator (TC-CR-1 through TC-CR-8)
  - File: `crates/nexus-domain/src/creator.rs`
  - Run: `cargo test -p nexus-domain --lib creator::tests` → FAIL
- [ ] **Step 2**: Implement Creator domain logic
  - Fields must match `schemas/domain/creator.schema.json` exactly
  - Enum values must match the domain specification (§7)
- [ ] **Step 3**: Run Creator tests → PASS
- [ ] **Step 4**: Write failing tests for Pairing (TC-PR-1 through TC-PR-6)
  - File: `crates/nexus-domain/src/pairing.rs`
  - Run: `cargo test -p nexus-domain --lib pairing::tests` → FAIL
- [ ] **Step 5**: Implement Pairing domain logic
- [ ] **Step 6**: Run Pairing tests → PASS
- [ ] **Step 7**: Integration test: Creator+Pairing contract type matching
  - File: `crates/nexus-domain/src/contract_assertions.rs`
- [ ] **Step 8**: Commit: `feat(domain): implement Creator and Pairing aggregates`

### Task 3: Implement KeyBlock + SourceAnchor (Layer 2 — P1 Critical)

- [ ] **Step 1**: Write failing tests for SourceAnchor (TC-SA-1 through TC-SA-7)
  - File: `crates/nexus-domain/src/source_anchor.rs`
  - Run: `cargo test -p nexus-domain --lib source_anchor::tests` → FAIL
- [ ] **Step 2**: Implement SourceAnchor domain logic
  - Must use `SourceSummaryRef` sub-structure
  - Must NOT have `anchor_ref` or `story_ref` fields
- [ ] **Step 3**: Run SourceAnchor tests → PASS
- [ ] **Step 4**: Write failing tests for KeyBlock (TC-KB-1 through TC-KB-8)
  - File: `crates/nexus-domain/src/key_block.rs`
  - Must use `BlockType` enum with 8 values from common.schema.json
  - Must use `key_block_id`, `canonical_name` (not `kb_ref`, `title`)
  - `confirm()` must implement full multi-gate check (role, base_versions, completeness, traceability, no conflicts)
- [ ] **Step 5**: Implement KeyBlock domain logic
- [ ] **Step 6**: Run KeyBlock tests → PASS
- [ ] **Step 7**: Integration test: KeyBlock JSON validates against `key-block.schema.json`
- [ ] **Step 8**: Integration test: KeyBlock ↔ `contracts::generated::KeyBlock` roundtrip
- [ ] **Step 9**: Commit: `feat(domain): implement KeyBlock and SourceAnchor`

### Task 4: Implement TimelineEvent + ForkBranch (Layer 2)

- [ ] **Step 1**: Write failing tests for TimelineEvent (TC-TE-1 through TC-TE-6)
  - File: `crates/nexus-domain/src/timeline_event.rs`
  - Replace `Timeline` struct with `TimelineEvent` aggregate
- [ ] **Step 2**: Implement TimelineEvent domain logic
  - Causality validation: caused_by_event_ids must reference same world
  - Provisional → canon gate: no reorder, revalidate branch/sequence/permissions/head
- [ ] **Step 3**: Run TimelineEvent tests → PASS
- [ ] **Step 4**: Write failing tests for ForkBranch (TC-FB-1 through TC-FB-5)
  - File: `crates/nexus-domain/src/fork_branch.rs`
  - **Requires**: Task 0 schema creation to be complete
- [ ] **Step 5**: Implement ForkBranch domain logic
  - Write scope validation: structured writes only to child world/branch
- [ ] **Step 6**: Run ForkBranch tests → PASS
- [ ] **Step 7**: Commit: `feat(domain): implement TimelineEvent and ForkBranch`

### Task 5: Implement StoryManifest + MemoryItem (Layer 2-3)

- [ ] **Step 1**: Write failing tests for StoryManifest (TC-SM-1 through TC-SM-7)
  - File: `crates/nexus-domain/src/story_manifest.rs`
- [ ] **Step 2**: Implement StoryManifest domain logic
- [ ] **Step 3**: Run StoryManifest tests → PASS
- [ ] **Step 4**: Write failing tests for MemoryItem (TC-MI-1 through TC-MI-8)
  - File: `crates/nexus-domain/src/memory_item.rs`
  - Must use `memory_item_id` (not `memory_id`)
  - Must use `MemoryType` enum: canon/working/experience (not Experience/Soul/Knowledge/ReferenceExcerpt)
  - Must include all 15 fields from schema
- [ ] **Step 5**: Implement MemoryItem domain logic
  - Scope and quota validation: creator/world scope, quota limits
- [ ] **Step 6**: Run MemoryItem tests → PASS
- [ ] **Step 7**: Integration test: MemoryItem ↔ `contracts::generated::Memory` roundtrip
- [ ] **Step 8**: Commit: `feat(domain): implement StoryManifest and MemoryItem`

### Task 6: Implement ReferenceSource (Layer 1 — Local-Only)

- [ ] **Step 1**: Write failing tests for ReferenceSource (TC-RS-1 through TC-RS-4)
  - File: `crates/nexus-domain/src/reference_source.rs`
  - **Requires**: Task 0 schema creation to be complete
- [ ] **Step 2**: Implement ReferenceSource domain logic
  - Extract-to-MemoryItem conversion
  - URI validation by source_type
- [ ] **Step 3**: Run ReferenceSource tests → PASS
- [ ] **Step 4**: Commit: `feat(domain): implement ReferenceSource (local-only)`

### Task 7: Implement ManuscriptState + Phase Machine (Layer 4)

- [ ] **Step 1**: Write failing tests for ManuscriptStateMachine (TC-MS-1 through TC-MS-5)
  - File: `crates/nexus-domain/src/manuscript_state.rs`
  - Must use spec phase names: brainstorm/draft/review/finalize/published
  - Must NOT use: write/provisional/canon
- [ ] **Step 2**: Implement ManuscriptState aggregate and ManuscriptStateMachine
  - Pre-gate provisional cleanup: provisional records must be promoted or cleaned before finalize/published
- [ ] **Step 3**: Run ManuscriptState tests → PASS
- [ ] **Step 4**: Commit: `feat(domain): implement ManuscriptState and phase machine`

### Task 8: Implement Consistency Rules + Domain Errors

- [ ] **Step 1**: Implement domain error types
  - File: `crates/nexus-domain/src/errors.rs`
  - Cover all error variants from consistency rules (G1-G6 inlined in §2)
- [ ] **Step 2**: Implement consistency validator
  - File: `crates/nexus-domain/src/consistency.rs`
  - Global invariants G1-G6
  - Domain invariants per aggregate (§3.1-§3.6)
  - Provisional TTL validation (30 days)
  - Phase transition validation
- [ ] **Step 3**: Write and run consistency tests
- [ ] **Step 4**: Commit: `feat(domain): implement consistency rules and domain errors`

### Task 9: Contract Integration Tests

- [ ] **Step 1**: Write compile-time type assertions
  - File: `crates/nexus-domain/src/contract_assertions.rs`
  - Every domain aggregate → contract type conversion
- [ ] **Step 2**: Write serde roundtrip tests for ALL aggregates
  - Domain → JSON → Contract type → JSON → Domain
- [ ] **Step 3**: Write JSON Schema validation tests for ALL aggregates
  - Domain → JSON → validate against `schemas/domain/*.schema.json`
  - Requires `jsonschema` crate as dev-dependency
- [ ] **Step 4**: Run all integration tests → PASS
  - `cargo test -p nexus-domain`
- [ ] **Step 5**: Verify full workspace builds
  - `cargo build --workspace`
- [ ] **Step 6**: Commit: `test(domain): add contract integration and schema validation tests`

### Task 10: Final Verification and Documentation

- [ ] **Step 1**: Run full test suite: `cargo test -p nexus-domain`
  - Expected: All tests pass
- [ ] **Step 2**: Run clippy: `cargo clippy -p nexus-domain -- -D warnings`
  - Expected: No warnings
- [ ] **Step 3**: Check format: `cargo fmt -p nexus-domain -- --check`
  - Expected: No formatting issues
- [ ] **Step 4**: Create domain crate README
  - File: `crates/nexus-domain/README.md`
  - Document all modules, design decisions, and integration strategy
- [ ] **Step 5**: Commit: `docs(domain): add domain crate README`

---

## 6. P1 Gap Remediation Tracker

| Gap ID | Aggregate | Issue | Remediation | Task Ref |
|--------|-----------|-------|-------------|----------|
| G1 | KeyBlock | BlockType enum wrong (6 vs 8) | Replace with schema's BlockType enum | Task 3 |
| G1 | KeyBlock | Field names wrong (kb_ref, title, etc.) | Rename all fields to match schema | Task 3 |
| G1 | KeyBlock | confirm() oversimplified | Rewrite with multi-gate logic | Task 3 |
| G2 | SourceAnchor | Completely wrong structure | Rewrite with StorySummaryRef sub-structure | Task 3 |
| G3 | MemoryItem | Missing 8 fields | Add all fields from schema | Task 5 |
| G3 | MemoryItem | 2 of 4 enum values wrong | Replace MemoryType with spec's canon/working/experience | Task 5 |
| G4 | Creator | Not in plan | Add new aggregate with full field definitions | Task 2 |
| G4 | Pairing | Not in plan | Add new aggregate | Task 2 |
| G4 | StoryManifest | Not in plan | Add new aggregate | Task 5 |
| G4 | ForkBranch | Not in plan + schema missing | Create schema + add aggregate | Task 0 + Task 4 |
| G4 | ReferenceSource | Not in plan + schema missing | Create schema + add aggregate | Task 0 + Task 6 |
| G5 | TimelineEvent | Plan has Timeline, not TimelineEvent | Replace struct with aggregate | Task 4 |
| G6 | ManuscriptPhase | Phase names wrong | Align to spec: brainstorm/draft/review/finalize/published | Task 7 |

---

## 7. Implementation Effort

- **Complexity**: M (medium)
- **Agent session band**: ~2-3 focused sessions for full domain crate implementation
  - Session 1: Tasks 0-3 (schemas, crate init, Creator, Pairing, KeyBlock, SourceAnchor)
  - Session 2: Tasks 4-7 (TimelineEvent, ForkBranch, StoryManifest, MemoryItem, ReferenceSource, ManuscriptState)
  - Session 3: Tasks 8-10 (consistency, integration tests, verification, documentation)

---

## 8. Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| ForkBranch/ReferenceSource schemas not yet created | Blocks domain implementation for those aggregates | Task 0 is prerequisite; schema creation follows existing schema patterns |
| Domain logic diverges from generated types | Runtime serialization failures | Compile-time assertions (Task 9 Step 1) + serde roundtrip tests |
| Consistency gate logic is complex | confirm()/promote() methods may be incomplete | Start with simplified gates; mark TODO for full implementation; add integration tests |
| `jsonschema` crate adds build complexity | CI/CD impact | Use only in dev-dependencies; validate in tests only |
| ManuscriptState is local-only but needs schema for consistency | Schema creation may not align with local-only intent | Create schema but mark as `local_authority: true`; platform sync excludes this type |

---

*End of Revised Domain Models Specification v1*
