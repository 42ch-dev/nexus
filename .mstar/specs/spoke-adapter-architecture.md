# Spoke Adapter Architecture

> **Status:** Normative (v0.4 — V1.143 orchestrator cutover on upsert/relate + timeline wire-type unification + promote-reject/merge resolution; v0.3 was V1.142 SPOKE 0.4.1 pin + production adapter home + first orchestrator cutover; v0.2 was V1.141 0.4.0 adapter-port architecture; v0.1 was V1.139 SPOKE adoption baseline)
> **Document class:** Master
> **Scope:** The `nexus-spoke-adapter` crate boundary, `extensions.nexus` namespace contract, spoke-operations delegation rules, daemon-api envelope strategy, drift detection adaptation, and the `/kb/` HTTP route stability decision.
> **Related:** [entity-scope-model.md](entity-scope-model.md), [local-db-schema.md](local-db-schema.md), [schemas-directory-layout.md](schemas-directory-layout.md), spoke `CONCEPTS.md`, spoke `.mstar/specs/spoke-data-model.md`, spoke `.mstar/specs/spoke-operations.md`

## 0. Document Position

This spec is the durable, tracked architectural SSOT for the SPOKE consumption boundary in nexus. It records locked architecture facts — not iteration archaeology, delivery history, or grill-me dialog. The upstream locked decisions (Q1–Q6, Q11) are restated as architecture invariants; Q7–Q10 + Q12 are the architect's resolved decisions.

## 1. Architecture Facts (Q1–Q6 restated as invariants)

These are the architecture bedrock — do not re-litigate.

### 1.1 Consume spoke packages directly

nexus depends on spoke's published packages directly:
- **Rust:** `spoke-schemas` + `spoke-operations` (crates.io, lockstep **`0.4.1`** exact pin)
- **TypeScript:** `@42ch/spoke-schemas` + `@42ch/spoke-operations` (npm, lockstep **`0.4.1`** exact pin)

> **Historical:** V1.139 shipped at `0.1.1`; V1.140 bumped to `0.2.0`. V1.141 jumps to `0.4.0` (covering both the `0.3.0` capability-sliced port architecture and `0.4.0` additive `HostCapabilityManifest` + body helpers + UTF-8 peer sort).

The bespoke `schemas/domain/key-block.schema.json` is deleted. No nexus-local copy of spoke schemas exists. The atomic KB wire type is `KnowledgeEntry` from spoke.

### 1.2 Prefer spoke; minimal nexus customization

Where spoke already provides a type, field, op, or lifecycle invariant, nexus uses it directly. The discipline applies across the iteration:

- **Lean `extensions.nexus`** — carry only fields that spoke genuinely has no equivalent for (e.g. `world_id`, `created_from_command_id`). Before adding a nexus-local extension, verify spoke has no parallel concept (e.g. prefer spoke `SourceAnchor` over nexus-local `source_*` fields where they overlap).
- **Thin adapter** — `nexus-spoke-adapter` is a delegation facade (re-export / pass-through), not a thick mapping layer. No parallel nexus types where spoke already provides them.
- **Direct TS imports** — apps import `KnowledgeEntry` and helpers directly from `@42ch/spoke-schemas` / `@42ch/spoke-operations`; no nexus wrapper package on the TS side.

### 1.3 Full terminology rename

`KeyBlock` is retired across the codebase, schemas, specs, and docs. The wire type is `KnowledgeEntry` from spoke. The `nexus-platform` private repo's consumer concerns are out of scope for this OSS repo.

### 1.4 Crate topology

- **New:** `crates/nexus-spoke-adapter/` — the only boundary that constructs spoke objects with a **lean** `extensions.nexus` populated and delegates lifecycle ops to `spoke-operations`. Thin facade (Q13).
- **Merge:** `crates/nexus-kb/` is merged INTO the **existing** `crates/nexus-knowledge/` (which today owns User-scoped global knowledge + reference sources). After merger, `nexus-knowledge` consolidates three knowledge tiers in one crate:
  1. **World KnowledgeEntry** (formerly `nexus-kb`'s domain — narrative KB entries tied to a World)
  2. **User knowledge** (existing — tag-driven global knowledge entries indexed per `user_id`)
  3. **reference sources** (existing — local-only research/reference registration)
  
  `crates/nexus-kb/` is deleted after merger. spoke-provided standard lifecycle invariants live in `nexus-spoke-adapter`, **not** in the merged `nexus-knowledge`. The merged crate's AGENTS.md is rewritten to reflect the expanded scope (the current AGENTS.md line "does not own World/narrative KeyBlocks" is inverted).

### 1.5 Full operations delegation

All standard lifecycle invariants (promote gate, status transitions, AssemblePacket builder, extension merge) are delegated to `spoke-operations`. **Every spoke-operations invocation takes only spoke standard objects** (`KnowledgeEntry`, `Finding`, `Scope`, `PromoteRequest`, `AssemblePacket`). Nexus wrapper types never reach a `spoke-operations` function signature. The `nexus-spoke-adapter` is the sole boundary that crosses this line.

### 1.6 Author-facing product label (Q11)

| Surface | Term |
|---------|------|
| Wire / code / schemas / specs (technical identifiers) | `KnowledgeEntry` |
| EN author-visible copy | **Knowledge entry** / **Knowledge entries** (sentence case); **Knowledge Entry** only where DESIGN.md Title Case applies |
| zh-CN author-visible copy | **知识条目** |
| Never in author-visible UI | camelCase `KnowledgeEntry`, legacy `KeyBlock`, bare "key block(s)" |

## 2. `extensions.nexus` Namespace Contract (Q7)

### 2.1 Field inventory

The `extensions.nexus` namespace carries all nexus-local fields that spoke deliberately keeps out of its core `KnowledgeEntry` schema. The namespace key is `"nexus"` (lowercase, matches spoke `^[a-z][a-z0-9_-]*$` namespace convention).

| Field | JSON type | Required | Semantics | Source in current `KeyBlock` |
|-------|-----------|----------|-----------|------------------------------|
| `world_id` | string | yes | World this entry belongs to. Prefix `wld_`. | `KeyBlock.world_id` |
| `created_from_command_id` | string | no | SyncCommand that originated this entry. Prefix `cmd_`. | `KeyBlock.created_from_command_id` |
| `source_work_id` | string | no | Work that produced this entry (V1.52 provenance). Prefix `wrk_`. | `KeyBlock.source_work_id` |
| `source_chapter` | integer | no | Chapter number where the entry was extracted. | `KeyBlock.source_chapter` |
| `source_provenance_kind` | string | no | How the entry entered the KB graph. Values: `manual`, `review_time_extract`, `finalize_time_extract`, `cross_chapter_rescan`, `author_explicit`. | `KeyBlock.source_provenance_kind` |

**Wire example:**
```json
{
  "extensions": {
    "nexus": {
      "world_id": "wld_abc",
      "created_from_command_id": "cmd_xyz",
      "source_work_id": "wrk_def",
      "source_chapter": 3,
      "source_provenance_kind": "review_time_extract"
    }
  }
}
```

### 2.2 Round-trip preservation rules

1. **Unknown namespaces** in `extensions` are preserved verbatim on read→modify→write cycles.
2. **Unknown keys** inside `extensions.nexus` are preserved verbatim.
3. **Empty `extensions.nexus`** (`{}`) is valid and is not dropped.
4. Nexus never writes to namespaces other than `"nexus"`; it preserves all others.
5. The adapter's typed accessors (`get_world_id()`, `set_world_id()`, etc.) operate only on known keys within `extensions.nexus` — they silently pass through unknown keys.

### 2.3 SQLite storage shape

**Decision: keep existing columns (additive migration).**

The SQLite `kb_key_blocks` table retains its current columns (`world_id`, `created_from_command_id`, `source_work_id`, `source_chapter`, `source_provenance_kind`) as-is. The `nexus-spoke-adapter` populates `extensions.nexus` from these columns when constructing a spoke `KnowledgeEntry`, and extracts them back when persisting.

| Rationale | Detail |
|-----------|--------|
| Query efficiency | `list_by_world(world_id)` filters on the indexed `world_id` column directly — no JSON extraction at query time |
| Migration safety | Additive-only: no DDL changes to existing columns; new rows populate existing columns |
| Round-trip fidelity | Known fields have typed columns; unknown extensions.nexus keys are serialized into a `extensions_nexus_json TEXT` column (additive, added by migration) for round-trip preservation |

**Migration path:**

1. **Add column:** `ALTER TABLE kb_key_blocks ADD COLUMN extensions_nexus_json TEXT;` — stores the full `extensions.nexus` JSON for round-trip preservation of unknown keys.
2. **Backfill:** for existing rows, `extensions_nexus_json` is populated from the existing columns on next read/write cycle.
3. **Read:** `KeyBlockRow` → adapter constructs `extensions.nexus` from typed columns + parses `extensions_nexus_json` for unknown keys (merged), then populates `KnowledgeEntry.extensions`.
4. **Write:** adapter extracts known fields from `extensions.nexus` into typed columns; serializes the full `extensions.nexus` into `extensions_nexus_json` for the round-trip guarantee.

The `world_id` column is **retained as a required SQLite column** (FK to `narrative_worlds`). This preserves the active unique index `idx_kb_key_blocks_active_unique (world_id, block_type, canonical_name)` without rewriting the uniqueness constraint.

**Adaptive migration note:** pre-1.0 allows DB wipes. If the additive migration causes issues, a clean migration (rename table, recreate, re-insert) is acceptable — but additive is preferred.

## 3. Daemon-API Envelope Strategy (Q8)

### 3.1 Decision: `$ref` spoke schema URIs

Daemon-api envelope schemas reference spoke types via their published `$id` URIs. The spoke `knowledge-entry.schema.json` declares `"$id": "https://spoke42.invalid/schemas/data/knowledge-entry.schema.json"`. Nexus daemon-api schemas use:

```json
{
  "properties": {
    "knowledge_entry": {
      "$ref": "https://spoke42.invalid/schemas/data/knowledge-entry.schema.json"
    }
  }
}
```

### 3.2 Codegen resolution

The codegen tooling (`json-schema-to-typescript` + `typify`) resolves `$ref` URIs by loading schema files from a known filesystem path. The spoke-schemas package (npm: `@42ch/spoke-schemas`, cargo: `spoke-schemas`) ships its `schemas/` directory. The codegen schema loader is configured with a resolver that maps `https://spoke42.invalid/schemas/` → the spoke package's `schemas/` directory on disk.

### 3.3 Schema migration impact

| File | Before | After |
|------|--------|-------|
| `schemas/domain/key-block.schema.json` | Exists; owns `KeyBlock` type | **Deleted** |
| `schemas/common/common.schema.json` | Defines `KeyBlockId`, `BlockType`, `KeyBlockStatus` | `KeyBlockId` removed (spoke uses `entry_id`); `BlockType` and `KeyBlockStatus` **retained** as nexus-local definitions for daemon-api envelope fields and query parameters that still carry the legacy enum values (status transition maps to spoke core vocab internally) |
| `schemas/daemon-api/canvas/world-kb/world-kb-graph-response.schema.json` | References `KeyBlock` from `key-block.schema.json` | References `KnowledgeEntry` from spoke via `$ref` |
| `schemas/daemon-api/compute/compute-input.schema.json` | Carries `KeyBlock` in `key_blocks` array | Carries `KnowledgeEntry` from spoke via `$ref` |
| `schemas/daemon-api/compute/compute-output.schema.json` | References `KeyBlock` in state_delta target | References `KnowledgeEntry` from spoke |

### 3.4 Fallback path

If the codegen tooling cannot resolve external `$ref` URIs for spoke schemas within the P0 slice, the daemon-api schemas annotate the spoke type dependency in their `description` field, and the generated types import spoke types as external crate/package imports. This is treated as a temporary bridge — the target state is `$ref`-based resolution.

## 4. `nexus-contracts` Package Boundary (Q9)

### 4.1 Decision: reference only — no re-export

`@42ch/nexus-contracts` (npm) and `nexus-contracts` (Rust, monorepo-internal) do **not** re-export spoke types. Consumers import `KnowledgeEntry` directly from the spoke packages.

| Package | Contains |
|---------|----------|
| `@42ch/spoke-schemas` | `KnowledgeEntry`, `Relation`, `SourceAnchor`, `Finding`, `Scope`, `AssemblePacket`, `Rule`, `TimelineEvent` — all spoke data types |
| `@42ch/spoke-operations` | Lifecycle helpers (`validatePromoteRequest`, `transitionKnowledgeEntryStatus`, `buildAssemblePacket`, …) |
| `@42ch/nexus-contracts` | Nexus-specific daemon-api envelopes (route DTOs), compute ABI types, common nexus identifiers |

### 4.2 App import pattern

```typescript
// KB entry data type — from spoke
import { KnowledgeEntry } from "@42ch/spoke-schemas";

// Nexus-specific route DTO — from nexus-contracts
import { WorldKbGraphResponse, PatchEntityRequest } from "@42ch/nexus-contracts";

// Lifecycle helpers — from spoke-operations
import { validatePromoteRequest } from "@42ch/spoke-operations";
```

### 4.3 SemVer bump

`@42ch/nexus-contracts` bumps to the next minor in the pre-1.0 series (e.g., `0.8.0` → `0.9.0`). The package no longer exports `KeyBlock` — this is a breaking change per pre-1.0 SemVer; a minor bump is sufficient (not a major to 1.0, since the package is pre-1.0 and the spire speaks 0.x).

## 5. Drift Detection Adaptation (Q10)

### 5.1 Decision: remove local drift for spoke-sourced types; add spoke version conformance

The `schema_drift_detection.rs` `build_schema_map()` removes the `key-block.schema.json` entry:

```rust
// REMOVED (was line 117):
// entry!("schemas/domain/key-block.schema.json", Strict, KeyBlock),
```

### 5.2 Spoke conformance: lightweight version check

`check-wire-drift.sh` gains a new spoke-conformance step (P0 T4):

1. Verify `spoke-schemas` crate version matches pinned **`0.4.1`** in `Cargo.toml`.
2. Verify `@42ch/spoke-schemas` npm version matches pinned **`0.4.1`** in `package.json`.
3. Construct a `KnowledgeEntry` from spoke fixture JSON, deserialize via nexus's serde path, serialize back — verify structural round-trip. This catches type-mapping regressions without requiring a local schema.

### 5.3 Daemon-api envelopes that `$ref` spoke types

Schemas in `schemas/daemon-api/**` that `$ref` spoke types continue to have local drift detection for their **own** fields (the envelope wrapper). The spoke fields that come from `$ref` are validated by the codegen tool at generation time — if the `$ref` resolution fails, codegen fails, which CI's `verify-codegen` gate catches.

## 6. HTTP Route Path Stability (Q12)

### 6.1 Decision: keep `/kb/` and `/kb/key-blocks/`

The daemon API HTTP route paths under `/v1/daemon/.../kb/` are **not renamed** in V1.139. Rationale:

| Reason | Detail |
|--------|--------|
| Path stability | `kb` = knowledge base — semantically accurate. Changing it breaks other consuming clients unnecessarily. |
| Deferred concern | Route path renaming is a separate CLI-IA concern, not a data-model refactor. A holistic route-path review across all surfaces belongs in a dedicated iteration. |
| Product alignment | The `nexus42 kb ...` CLI subcommand already stays as `kb` — consistent with the daemon path. |
| Client impact | The daemon API is consumed by the bundled web UI (same repo) and potentially by the Tauri desktop shell. Renaming paths would cascade into client-side URL builders for no data-model benefit. |

**Affected paths (keep as-is):**

| Current path | V1.139 behavior |
|--------------|-----------------|
| `/v1/daemon/kb/entries` | Keep |
| `/v1/daemon/kb/entries/:entry_id` | Keep |
| `/v1/daemon/worlds/:world_id/kb/patch-entity` | Keep |
| `/v1/daemon/worlds/:world_id/kb/patch-relationship` | Keep |
| `/v1/daemon/worlds/:world_id/kb/promote-candidate` | Keep |
| `/v1/daemon/worlds/:world_id/kb/graph` | Keep |
| `/v1/daemon/worlds/:world_id/kb/candidates` | Keep |
| `/v1/daemon/worlds/:world_id/kb/key-blocks/:key_block_id/state` | Keep |

DTO shapes, handler type signatures, and generated code update to `KnowledgeEntry` — only URL path segments remain stable.

## 7. Spoke-Operations Call-Boundary Invariant (HARD)

This is the single most important architectural rule in this spec. It is restated from Q4 and must be visible in every implementer's dispatch.

> **Every call to a `spoke-operations` function takes only spoke standard objects** (`KnowledgeEntry`, `Finding`, `Scope`, `PromoteRequest`, `AssemblePacket`, `Relation`, `Rule`, `TimelineEvent`). Nexus domain types are never passed as operands to `spoke-operations`.

### 7.1 Enforcement

| Layer | Enforcement mechanism |
|-------|----------------------|
| **`nexus-spoke-adapter`** | All public functions accept/return spoke types only. The adapter's internal mapping layer converts nexus DB rows ↔ spoke objects — but the public API surface is spoke-only. |
| **Rust type system** | `spoke-operations` functions take `spoke_schemas::KnowledgeEntry`, not `nexus_knowledge::KnowledgeEntry`. The adapter constructs the spoke type before calling spoke-operations. |
| **Code review** | P1 implement AC-P1-3: static check (grep) confirms no spoke-operations call site passes a nexus-wrapper type. |

### 7.2 Adapter public API surface

The `nexus-spoke-adapter` crate exports:

```rust
// ── Extensions accessors ──────────────────────────────────────────────

/// Read `extensions.nexus.world_id` from a KnowledgeEntry.
pub fn get_world_id(entry: &KnowledgeEntry) -> Option<&str>;

/// Set `extensions.nexus.world_id` on a KnowledgeEntry (mutates in place,
/// preserves unknown keys in extensions.nexus).
pub fn set_world_id(entry: &mut KnowledgeEntry, world_id: String);

/// Read `extensions.nexus.created_from_command_id`.
pub fn get_created_from_command_id(entry: &KnowledgeEntry) -> Option<&str>;

/// Set `extensions.nexus.created_from_command_id`.
pub fn set_created_from_command_id(entry: &mut KnowledgeEntry, command_id: String);

/// Read provenance fields: (source_work_id, source_chapter, source_provenance_kind).
pub fn get_provenance(entry: &KnowledgeEntry)
    -> (Option<&str>, Option<i64>, Option<&str>);

/// Set provenance fields.
pub fn set_provenance(
    entry: &mut KnowledgeEntry,
    source_work_id: Option<String>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<String>,
);

/// Build `extensions.nexus` from typed nexus fields. Preserves any unknown
/// keys already present in extensions.nexus. Returns the full ExtensionMap
/// value for the "nexus" namespace key.
pub fn build_extensions_nexus(
    world_id: &str,
    created_from_command_id: Option<&str>,
    source_work_id: Option<&str>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<&str>,
    existing_extensions: &ExtensionMap,
) -> serde_json::Value;

// ── Ops delegation wrappers (spoke-operations pass-through) ────────────

/// Delegate to `spoke_operations::validate_promote_request`.
/// Operand: spoke `PromoteRequest` only.
pub fn validate_promote(request: &PromoteRequest) -> SpokeResult<()>;

/// Delegate to `spoke_operations::apply_promote_acceptance`.
/// Operand: spoke `PromoteRequest` only. Returns promoted `KnowledgeEntry`.
pub fn apply_promote(request: &PromoteRequest) -> SpokeResult<KnowledgeEntry>;

/// Delegate to `spoke_operations::transition_knowledge_entry_status`.
/// Operand: spoke `KnowledgeEntry` only.
pub fn transition_status(
    entry: &KnowledgeEntry,
    to: &str,
) -> SpokeResult<KnowledgeEntry>;

/// Delegate to `spoke_operations::transition_finding_status`.
/// Operand: spoke `Finding` only.
pub fn transition_finding_status(
    finding: &Finding,
    to: &str,
) -> SpokeResult<Finding>;

/// Delegate to `spoke_operations::build_assemble_packet`.
/// Operands: spoke `KnowledgeEntry` slice only.
pub fn build_assemble_packet(
    packet_id: &str,
    entries: &[KnowledgeEntry],
    max_entries: Option<usize>,
) -> SpokeResult<AssemblePacket>;

/// Delegate to `spoke_operations::merge_extension_maps`.
/// Operands: spoke `ExtensionMap` only.
pub fn merge_extensions(
    base: &ExtensionMap,
    overlay: &ExtensionMap,
) -> ExtensionMap;

/// Delegate to `spoke_operations::assert_revision_match`.
pub fn assert_revision(expected: u64, actual: u64) -> SpokeResult<()>;
```

All delegation wrappers are thin — they enforce the boundary that operands must already be spoke types at the call site. They do not transform types internally.

### 7.3 Adapter port + injection-orchestration surface — Surface B (spoke ≥ 0.3.0)

As of spoke 0.3.0, `spoke-operations` ships a **capability-sliced adapter port** architecture with **injection orchestration**. The `nexus-spoke-adapter` crate re-exports the port traits and orchestration entrypoints so that consumers can participate in spoke's injection-orchestration model through the same boundary crate — without a direct `spoke-operations` dependency.

**What changes:** the adapter crate's public API gains a second surface (Surface B) alongside the existing pure-delegate helpers (Surface A). Consumers that currently call `ops::validate_promote` / `ops::apply_promote` directly (Surface A) can **optionally** adopt the port+orchestrator pattern (Surface B) by implementing spoke port traits and calling `orchestrate_*` entrypoints. Surface A is **frozen and unchanged** — no existing call site must migrate.

#### Surface A — Pure delegates (unchanged)

| Category | Examples | Status |
|---|---|---|
| Extensions accessors | `get_world_id`, `set_world_id`, `build_extensions_nexus`, `get_nexus_extras`, `set_nexus_extras` | **Frozen.** No behavior change at any spoke version. |
| Ops delegation wrappers | `validate_promote`, `apply_promote`, `transition_status`, `build_assemble_packet`, `merge_extensions`, `assert_revision` | **Frozen.** Thin pass-throughs to `spoke-operations` pure helpers. |

Consumers that manage their own storage and only need spoke lifecycle validation should remain on Surface A. Surface A is the **permanent integration surface** for all cases where the consumer wants to control its own persistence transaction boundaries.

#### Surface B — Injection orchestration (new)

Consumers **implement** spoke port traits (at minimum the six `BaselinePorts` families) and **call** `orchestrate_*` entrypoints. The orchestration composes pure helpers with port I/O — consumers supply the ports; spoke supplies the lifecycle sequencing.

**Port traits (capability-sliced):**

| Family | Trait | Required for | Key method |
|---|---|---|---|
| Knowledge entry persistence | `KnowledgeEntryPort` | Baseline (all) | `put_knowledge_entry(entry, expected_base_revision: Option<u64>)` → `SpokeResult<KnowledgeEntry>` |
| Relation persistence | `RelationPort` | Baseline (all) | `put_relation(relation)` → `SpokeResult<Relation>` |
| Scope query | `ScopeQueryPort` | Baseline (all) | `list_knowledge_entries(scope)`, `list_timeline_events(scope)` |
| Finding persistence | `FindingPort` | Baseline (all) | `put_findings(findings)` → `SpokeResult<Vec<Finding>>` |
| Rule query | `RuleQueryPort` | Baseline (all) | `list_rules(rule_refs)` → `SpokeResult<Vec<Rule>>` |
| Host manifest | `HostManifestPort` | Baseline (all) | `get_host_capability_manifest()`, `list_peer_host_capability_manifests()` |
| Computable session | `ComputablePort` | Optional (`l2-computable`) | `project(request)`, `compute(request)` |
| Fork timeline query | `ForkTimelineQueryPort` | Optional (`l5-fork`) | `list_fork_timeline_events(scope)` |

**Composition traits:** `BaselinePorts` (blanket over all six baseline families), `ComputablePorts` (baseline + computable), `ForkPorts` (baseline + fork), `FullPorts` (all three).

**Orchestration entrypoints:**

| Entrypoint | Required ports | Sequence |
|---|---|---|
| `orchestrate_upsert` | `KnowledgeEntryPort` | Load update context → `validateUpsertKnowledgeEntry` → status/uniqueness helpers → `putKnowledgeEntry(entry, expectedBaseRevision)` |
| `orchestrate_promote` | `KnowledgeEntryPort` | Load stored entry → terminal/revision gates → `validatePromoteRequest` → `applyPromoteAcceptance` → `putKnowledgeEntry(promoted, expectedBaseRevision)` |
| `orchestrate_relate` | `RelationPort` | `validateRelateRequest` → `putRelation` |
| `orchestrate_check` | `ScopeQueryPort`, `RuleQueryPort`, `FindingPort` | Resolve refs → query scoped entries/events → caller-supplied `runChecker` callback → `putFindings` |
| `orchestrate_assemble` | `ScopeQueryPort` | Query scoped entries/events → `buildAssemblePacket` |
| `orchestrate_project` | `ComputablePort` | `validateProjectRequest` → `project` |
| `orchestrate_compute` | `ComputablePort` | `validateComputeRequest` → `compute` |
| `orchestrate_fork_check` | `ForkTimelineQueryPort` + baseline check | Validate `scope.fork_id` → fork-aware queries → `runChecker` → `putFindings` |
| `orchestrate_fork_assemble` | `ForkTimelineQueryPort` + baseline assemble | Validate `scope.fork_id` → fork-aware queries → `buildAssemblePacket` |

The upstream SSOT for trait method contracts, orchestration sequences, and capability matrices is spoke's `.mstar/specs/spoke-operations.md` "Adapter Interfaces" (§Port policy through §TS/Rust parity table) and "Injection Orchestration" (§Injection Orchestration through §Public export and module paths). **This spec cross-links; it does not restate normative port/orchestration behavior.**

#### `put_knowledge_entry` CAS contract

The `KnowledgeEntryPort::put_knowledge_entry(entry, expected_base_revision: Option<u64>)` method carries optimistic concurrency control structurally:

- `expected_base_revision = None` → **create.** The adapter MUST reject if an entry with `entry.entry_id` already exists in the store.
- `expected_base_revision = Some(rev)` → **conditional update.** The adapter MUST compare `rev` against the store's current revision for `entry.entry_id`. On mismatch:
  - `actual > expected` → reject with `STORED_REVISION_STALE` (caller read a stale base).
  - `actual < expected` → reject with `REVISION_CONFLICT` (caller expects an impossible future revision).
  - `actual == expected` → accept the write and persist the entry.

True concurrent safety requires atomic compare-and-put in the adapter implementation. The `spoke-operations` library stays I/O-free — it only calls the port; the adapter owns locking, transactions, and storage.

#### `CAPABILITY_PORT_MISSING` reject path

When a consumer calls an optional orchestrator (`orchestrate_project`, `orchestrate_compute`, `orchestrate_fork_check`, `orchestrate_fork_assemble`) through a `BaselinePorts`-only adapter that does **not** implement the optional family, the orchestrator returns `SpokeReject { code: "CAPABILITY_PORT_MISSING", ... }` rather than a compile error or panic. This is the dynamic dispatch path for `ComputablePort` / `ForkTimelineQueryPort`.

#### Adoption guide: when to use Surface A vs Surface B

| Surface | When to use | What consumer does |
|---|---|---|
| **A — Pure delegates** | Consumer manages its own storage transactions and only needs spoke lifecycle validation or extension accessors. Examples: `nexus-knowledge` V1.139 confirm() path (status transition only); pact CLI `kb entry list` (extension reads only). | Call `ops::transition_status(entry, to)`, `extensions::get_world_id(entry)`, etc. directly. Consumer owns the transaction, the DB row locking, and the before/after invariants. |
| **B — Injection orchestration** | Consumer wants spoke to compose the full OCC lifecycle — load, validate, apply, persist — in one call. The consumer implements port traits that map to its persistence layer, and spoke executes the sequencing. Examples: future `nexus-knowledge` write path adopters (compass Roadmap item 1–2); multi-host collaboration scenarios where a single OCC authority per `entry_id` is required. | Implement `BaselinePorts` (six families) on a struct that wraps nexus storage. Call `orchestrate_promote(ports, request)` and the orchestrator handles loading, validation, revision bump, and persistence through your ports. |

**General rule:** if the consumer already has a transaction open and the mutation is a single helper call, Surface A is simpler. If the mutation composes multiple helpers across port families and OCC is required, Surface B encapsulates the sequencing.

**Surface B production storage boundary (out of scope for V1.141):** wiring nexus SQLite tables behind the six `BaselinePorts` families is **next-iteration roadmap** (compass Roadmap items 1–2). V1.141 ships the port traits, orchestration re-exports, a reference in-memory mock, and adoption tests — sufficient to prove the boundary works. Production `KnowledgeEntryPort` implementations against `nexus-local-db` ship in a downstream iteration triggered by the first write-path cutover. See iteration compass "Roadmap Position" for the staged cutover plan.

**Production adapter shipped (V1.142):** the production `BaselinePorts` implementation (`NexusBaselineAdapter`) is now shipped in `nexus-local-db/src/spoke_adapter/`, replacing the V1.141 reference in-memory mock for the four storage-backed families. The mock remains the reference shape for tests. See §7.4 for the production adapter home, family matrix, and CAS contract reuse.

### 7.4 Production Adapter Home — `NexusBaselineAdapter` in `nexus-local-db`

The production `BaselinePorts` implementation (`NexusBaselineAdapter`) lives in **`nexus-local-db/src/spoke_adapter/`**, not in `nexus-knowledge`. Rationale: `nexus-local-db` already depends on both `nexus-knowledge` (for domain types + the `WorldKbEntry ↔ KnowledgeEntry` conversion seam) and `nexus-spoke-adapter` (for port traits + orchestrators), and it owns the SQLite persistence. `nexus-knowledge` is a domain-types-and-traits crate that does **not** depend on `nexus-local-db` — it cannot be the production port home (the V1.141 compound doc §"Production boundary" suggestion that production impls live "downstream in nexus-knowledge" is stale; the actual dependency graph is `nexus-local-db → nexus-knowledge → nexus-spoke-adapter` — see §8).

The adapter converts between nexus storage rows and spoke wire types using the existing V1.139 conversion seam in `nexus-knowledge::world_kb::knowledge_entry` (`impl From<WorldKbEntry> for SpokeKnowledgeEntry` + the reverse). This is reachable from `nexus-local-db` through its existing `nexus-knowledge` dependency. No second conversion path is added — the two `From` impls are the sole seam (spec §7.1).

#### Production-vs-stub matrix per port family

| Port family / method | Implementation class | Backing | Rationale |
|---|---|---|---|
| `KnowledgeEntryPort` | **Production** | `kb_key_blocks` via V1.73 CAS (`cas_update_key_block_fields`) | Existing storage with OCC |
| `RelationPort` | **Production** | `kb_relationships` | Existing storage |
| `FindingPort` | **Production** | `findings` | Existing storage |
| `ScopeQueryPort.list_knowledge_entries` | **Production** | `kb_key_blocks` scope-filtered by `extensions.nexus.world_id` | Existing storage |
| `ScopeQueryPort.list_timeline_events` | **Stub** — `Ok(Vec::new())` | None | No persisted `TimelineEvent` storage; nexus-narrative holds events in-memory |
| `RuleQueryPort` | **Stub** — `Ok(Vec::new())` | None | No spoke `Rule` persistence table |
| `HostManifestPort` | **Static-stub** — self manifest only | Static data | Multi-host / peer discovery not implemented |

**Stub behavior contract:** each stub is a documented empty/static return with a doc-comment referencing its roadmap trigger (iteration compass Roadmap Next rows 3–5) plus a residual row in `status.json` with owner, trigger, and target iteration. Stubs must never fabricate data — they return exactly what the backing storage would if it were empty/static.

#### CAS contract reuse

`KnowledgeEntryPort::put_knowledge_entry` routes through the existing V1.73 `cas_update_key_block_fields` CAS path in `nexus-local-db::kb_store`. The adapter maps CAS outcomes to spoke reject codes:

| CAS outcome (actual vs expected_revision) | Spoke reject code |
|---|---|
| `actual > expected` (stored revision is newer) | `STORED_REVISION_STALE` |
| `actual < expected` (caller expects future revision) | `REVISION_CONFLICT` |
| Entry absent + `expected_revision = Some(_)` | `REVISION_CONFLICT` |
| Entry present + `expected_revision = None` (create on existing) | `KnowledgeEntryAlreadyExists` |

True concurrent safety requires the CAS check to be atomic with the write. The V1.73 `cas_update_key_block_fields` function satisfies this through a `WHERE COALESCE(revision, 0) = ?` guard column inside the caller's SQLite transaction. The adapter does not add a second CAS layer.

#### Orchestrator cutover registry

Each orchestrator adoption on a daemon write path is registered here. The registry records the handler, the orchestrator, the adapter, and the cutover iteration.

| Orchestrator | Handler (symbol) | File | Adapter | Cutover | Status |
|---|---|---|---|---|---|
| `orchestrate_promote` | `promote_adopt()` | `world_kb.rs:608` | `NexusBaselineAdapter` | V1.142 | Shipped |
| `orchestrate_upsert` | `patch_entity()` | `world_kb.rs:286` | `NexusBaselineAdapter` | V1.143 | Planned |
| `orchestrate_relate` | `patch_relationship_add()` / `_update()` | `world_kb.rs:1669`/`1727` | `NexusBaselineAdapter` | V1.143 | Planned |

> **Note:** `patch_relationship_remove()` (line 1653) is **not** a cutover candidate — `orchestrate_relate` has no delete path (`RelationPort` exposes only `put_relation`). The `remove` action stays on Surface A via `delete_relationship_in_tx()`.

**Surface A retention (promote sub-outcomes):** spoke `orchestrate_promote` covers the accept/adopt lifecycle only. The following nexus-specific promote outcomes are retained on Surface A with explicit rationale:

| Outcome | Handler | Rationale |
|---|---|---|
| `promote_reject` | `promote_reject()` (line 1134) | Operates on `kb_extract_jobs.promotion_status`, not on `KnowledgeEntry`. spoke has no reject lifecycle on `PromoteRequest`. Handler is 20 lines of CAS UPDATE — no shared lifecycle logic to delegate. |
| `promote_merge` | `promote_merge()` (line 1183) | Compound multi-table SQLite transaction: CAS-update target `kb_key_blocks` body + CAS-reject candidate `kb_extract_jobs` in one atomic TX. spoke orchestrators compose a single port family per call — splitting merge into two orchestrator calls would break atomicity. |

Both decisions are accepted residuals (`R-V1143P2-ACCEPT-01`, `R-V1143P2-ACCEPT-02`) with rationale; pre-1.0, retaining narrow product-specific handlers on Surface A is acceptable when spoke has no equivalent concept.

#### Timeline wire-type unification (V1.143 P0)

`nexus-narrative::timeline_event::TimelineEvent` and `spoke_schemas::TimelineEvent` are unified via a `From`/`Into` conversion seam (two `From` impls in `nexus-narrative/src/timeline_event.rs`), mirroring the `WorldKbEntry`↔`KnowledgeEntry` pattern (§7.1). The types are structurally divergent (spoke: fork-oriented with 14 fields; nexus: branch/world-oriented with 13 fields including lifecycle state machine) — a type alias is not feasible.

**Conversion seam contract:**

| Nexus field | spoke field | Direction |
|---|---|---|
| `timeline_event_id` | `timeline_event_id` | bidirectional |
| `created_at` | `created_at` (String↔DateTime) | bidirectional |
| `title` | `canonical_name` | nexus→spoke |
| `summary` | `description` | bidirectional |
| `affected_key_block_ids` | `participant_entry_ids` | bidirectional |
| `caused_by_event_ids` | `extensions.nexus.caused_by_event_ids` | bidirectional |
| `world_id` | `extensions.nexus.world_id` | bidirectional |
| `branch_id` | `extensions.nexus.branch_id` | bidirectional |
| `event_type` | `extensions.nexus.event_type` | bidirectional |
| `status` | `extensions.nexus.timeline_status` | bidirectional |
| `sequence_no` | `sort_key` (`sequence_no.to_string()`) | nexus→spoke |
| `source_command_id` | `extensions.nexus.source_command_id` | bidirectional |

spoke-only fields (`fork_id`, `parent_fork_id`, `timeline_scale`, `source_anchor`, `computable_logs`) are lossily filled (`None`/empty) — nexus does not yet participate in spoke's fork model (V1.145 roadmap).

**Beat-assist helper adoption:** `order_timeline_events_by_ids` is the primary production adoption target (V1.143 P0 T2). The helper operates on `timeline_event_id` alone and requires no peripheral infrastructure. `order_timeline_events_by_precedes` is a stretch target (requires mapping `caused_by_event_ids` → spoke `Relation` objects + `extensions.spoke.timeline_entry_id` on each event).

**`ScopeQueryPort.list_timeline_events`** remains a stub (`Ok(Vec::new())`) — the conversion seam enables future production implementation when a product feature requires persisted timeline events (roadmap V1.145).

## 8. Crate Dependency Graph

```
nexus-local-db                       ← production adapter home (§7.4)
  ├── nexus-knowledge ──────────────┐
  │   ├── nexus-spoke-adapter ──┐   │
  │   │   ├── spoke-schemas     │   │ (spoke types)
  │   │   └── spoke-operations  │   │ (lifecycle helpers + orchestrators)
  │   ├── spoke-schemas         │   │ (native KnowledgeEntry type)
  │   └── nexus-contracts       │   │ (local DTOs)
  ├── nexus-spoke-adapter ──────────┘ (port traits + orchestrator entrypoints)
  └── sqlx, ...                       (persistence)
```

`nexus-daemon-runtime`, `nexus-orchestration`, `nexus-moment-context-assembly`, `nexus-cloud-sync`, `nexus-wasm-host`, and `apps/nexus42` depend on `nexus-knowledge` (and transitively on `nexus-spoke-adapter` for ops delegation). `nexus-local-db` is the **production `BaselinePorts` home** — it depends on `nexus-knowledge` for domain types + the conversion seam, and directly on `nexus-spoke-adapter` for the port traits + orchestrators it implements. No crate other than `nexus-spoke-adapter` directly depends on `spoke-operations`.

## 9. Migration Summary

| P0 | P1 | P2 | P3 |
|----|----|----|-----|
| Add spoke deps; create adapter crate; delete `key-block.schema.json`; repoint daemon-api envelope `$ref`; regenerate codegen; update drift detection; create this tracked spec | Rename `nexus-kb` → `nexus-knowledge`; migrate types to spoke `KnowledgeEntry`; route lifecycle ops through adapter; migrate SQLite storage; migrate all Rust consumers | Rename daemon-api DTOs; bump `@42ch/nexus-contracts`; migrate TS apps (import from `@42ch/spoke-schemas`); update UI strings to Q11 product label | Sweep specs/docs/knowledge/CLI/fixtures for terminology; add pattern doc via compound at iteration-close |
