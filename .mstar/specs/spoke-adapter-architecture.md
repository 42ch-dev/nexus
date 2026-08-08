# Spoke Adapter Architecture

> **Status:** Normative (v0.18 — V1.155 P0 N-C3 multi-host production: `HostManifestPort.list_peer_host_capability_manifests` is production — the last adapter stub is gone; `peer_hosts` table records manifest-backed outbound observations at `connect()` return (lock #1 fallback: inbound-only peers not recorded), empty → `Ok(vec![])` preserved, corrupt stored row → `InternalError`; adapter read `list_observed_peer_hosts` (manifest + `last_seen`) backs the `connect peers list` CLI; §7.3 matrix + §10.6 N-C3 row delivered; residual `R-V1142P1-002` closed; v0.17 — V1.154 P2 QC fix wave: module-id pin hardened to key-presence (any request-carried `computable.module_id` — differing string or non-string value — must equal the gated id else `module_not_scoped`), Connect compute reuses the `nexus_wasm_host::ModuleCache` compiled-module cache (id + bytes-hash keying, overwrite-on-change eviction), missing compute target entry maps to the `invalid_input` family, shared `is_safe_module_id` + `module_identity_missing` marker — §10.6 compute row; v0.16 — V1.154 P2 N-C2 compute half + world-aware CAS: `compute` served over Connect (host-local modules under `~/.nexus42/modules/`, per-peer `module_scope` fail-closed, read-only `settle:false`, module-id pin against request override) + fixed `world_conflict` wire code on world-aware CAS write predicates + semantic reasoning-complete milestone via roles `computable-engine` + capabilities `l2-computable` (literal string absent), §10.3/§10.4/§10.6); v0.15 — V1.154 P1 N-C2 read-half QC fix wave: response byte cap (2 MiB, envelope `response_too_large`) + scope-batch-array entry caps + concurrency-model/single-deadline clarity in the §10.6 bridge row, `connect_host_slice` → `"n-c2"` §10.3); v0.14 — V1.154 P0 spoke lockstep pin 0.9.1→0.9.2: Connect session-peer invoke identity (`InvokeHandlerV2`; payload `extensions.nexus.peer_id` informational-only, hard deny on mismatch, §10.4/§10.6); spoke-connect `mdns` feature removed upstream); v0.13 — V1.153 P0 spoke lockstep pin 0.8.2→0.9.1: `spoke-operations` port traits + `orchestrate_*` sync→async (adapter adapted signature-level; durable note §7.3); v0.12 — V1.152 DF-77 §11 Narrative Knowledge Pack I/O: shipped P0+P1; P2 dogfood-confirmed — additive daemon export/import routes + all three conflict policies (skip/rename/overwrite) + CLI↔daemon shared `import_pack` module + Control Room panel; v0.11 — V1.151 DF-76 §7.4 inspector packet field surface (shipped P0+P1; P2 dogfood-confirmed against the spoke assemble-module recipe handbook); v0.10 — V1.150 DF-75 §7.4 slot + Moment Directive + generation-stage matrix shipped at P2 close; v0.9 was V1.149 lore activation §7.4 production matrix: default-on engine + Relation hop expand; v0.8 was V1.148 spoke pin 0.6.1→0.8.2 + RuleQueryPort production + orchestrate_check daemon route + Connect Host N-C0 surface; v0.7 was V1.146 spoke InternalError reject code: pin bump 0.6.0→0.6.1; v0.6 was V1.145 spoke consumer alignment: adapter rehome to spoke-adapter + dep reversal + WorldKB/timeline read via ScopeQuery + scope-pushdown contract; v0.5 was V1.144 spoke 0.5.0 upgrade + RelationPort OCC extension + orchestrate_relate cutover)
> **Document class:** Master
> **Scope:** The `nexus-spoke-adapter` crate boundary, `extensions.nexus` namespace contract, spoke-operations delegation rules, daemon-api envelope strategy, drift detection adaptation, the `/kb/` HTTP route stability decision, the opt-in Connect Host N-C0 surface (DF-72), and the Narrative Knowledge Pack I/O product-transport surface (DF-77).
> **Related:** [entity-scope-model.md](entity-scope-model.md), [local-db-schema.md](local-db-schema.md), [schemas-directory-layout.md](schemas-directory-layout.md), spoke `CONCEPTS.md`, spoke `.mstar/specs/spoke-data-model.md`, spoke `.mstar/specs/spoke-operations.md`, spoke `.mstar/specs/spoke-connect.md`. Iteration product drafts (process): `.mstar/iterations/v1.148/specs/fl-r-connect-host-foundation.md`, `.mstar/iterations/v1.152/specs/fl-l-w7-knowledge-pack-productization.md`.

## 0. Document Position

This spec is the durable, tracked architectural SSOT for the SPOKE consumption boundary in nexus. It records locked architecture facts — not iteration archaeology, delivery history, or grill-me dialog. The upstream locked decisions (Q1–Q6, Q11) are restated as architecture invariants; Q7–Q10 + Q12 are the architect's resolved decisions.

## 1. Architecture Facts (Q1–Q6 restated as invariants)

These are the architecture bedrock — do not re-litigate.

### 1.1 Consume spoke packages directly

nexus depends on spoke's published packages directly:
- **Rust:** `spoke-schemas` + `spoke-operations` (crates.io, lockstep **`0.9.2`** exact pin)
- **TypeScript:** `@42ch/spoke-schemas` + `@42ch/spoke-operations` (npm, lockstep **`0.9.2`** exact pin)
- **Rust (opt-in Connect Host only):** `spoke-connect` (crates.io, lockstep **`0.9.2`** exact pin) — workspace dep consumed **only** behind cargo feature `connect-host` on `apps/nexus42`. Default `nexus42` / daemon builds MUST NOT link `spoke-connect`. See §10.

> **Historical:** V1.139 shipped at `0.1.1`; V1.140 bumped to `0.2.0`. V1.141 jumped to `0.4.0` (covering both the `0.3.0` capability-sliced port architecture and `0.4.0` additive `HostCapabilityManifest` + body helpers + UTF-8 peer sort). V1.144 bumped to `0.5.0` (additive `Relation.revision` + OCC-aware `RelationPort` + `RelationAlreadyExists`/`RelationNotFound` reject codes + relate-gate explicit mode). V1.145 bumped to `0.6.0` (additive `Scope.extensions` + `KnowledgeEntry.modules`). V1.146 bumped to `0.6.1` (additive `InternalError` 500-class reject code, PR #35). **V1.148 bumped to `0.8.2`** (spoke-connect surface 0.7.0–0.8.2 additive; 0.7.0 demote pack catalog from ModuleMap — pack catalog is product transport envelope, not `modules.pack` on KE/AssemblePacket; connect family schemas additive). **V1.153 bumped to `0.9.1`** (lockstep re-baseline on the connect v2 wire; 0.9.0's dial-bound hello + envelope-auth v2 are internal to `spoke-connect`; `spoke-operations` 0.9.1 additionally converted the adapter port traits + `orchestrate_*` to native async — nexus adapted signature-level, see §7.3).

The bespoke `schemas/domain/key-block.schema.json` is deleted. No nexus-local copy of spoke schemas exists. The atomic KB wire type is `KnowledgeEntry` from spoke.

### 1.2 Prefer spoke; minimal nexus customization

Where spoke already provides a type, field, op, or lifecycle invariant, nexus uses it directly. The discipline applies across the iteration:

- **Lean `extensions.nexus`** — carry only fields that spoke genuinely has no equivalent for (e.g. `world_id`, `created_from_command_id`). Before adding a nexus-local extension, verify spoke has no parallel concept (e.g. prefer spoke `SourceAnchor` over nexus-local `source_*` fields where they overlap).
- **Thin adapter** — `nexus-spoke-adapter` is a delegation facade (re-export / pass-through), not a thick mapping layer. No parallel nexus types where spoke already provides them.
- **Direct TS imports** — apps import `KnowledgeEntry` and helpers directly from `@42ch/spoke-schemas` / `@42ch/spoke-operations`; no nexus wrapper package on the TS side.

### 1.3 Full terminology rename

`KeyBlock` is retired across the codebase, schemas, specs, and docs. The wire type is `KnowledgeEntry` from spoke. The `nexus-platform` private repo's consumer concerns are out of scope for this OSS repo.

### 1.4 Crate topology

- **New:** `crates/nexus-spoke-adapter/` — the only boundary that constructs spoke objects with a **lean** `extensions.nexus` populated and delegates lifecycle ops to `spoke-operations`. Capability aggregation (Q13, refined V1.145).
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

1. Verify the three crate pins (`spoke-schemas`, `spoke-operations`, `spoke-connect`) all match the pinned **`0.9.2`** in `Cargo.toml`.
2. Verify the two npm pins (`@42ch/spoke-schemas`, `@42ch/spoke-operations`) both match the pinned **`0.9.2`** in `package.json`.
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
| **`nexus-spoke-adapter`** | All public functions accept/return spoke types only. The adapter owns the sole conversion seam (free fns `world_kb_to_spoke` / `spoke_to_world_kb` + `WorldKbEntrySpokeExt` in `src/conversion/`, V1.145 P1a) and the production `NexusAdapter` port impls in `src/adapter/` (V1.146 rename) — the public API surface is spoke-only. |
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

// ── Conversion seam (V1.145 P1a — sole WorldKbEntry ↔ KnowledgeEntry seam) ─

/// Forward: nexus domain `WorldKbEntry` → spoke standard `KnowledgeEntry`.
/// Borrows the domain entry (owned fields are cloned internally).
pub fn world_kb_to_spoke(entry: &WorldKbEntry) -> KnowledgeEntry;

/// Reverse: spoke standard `KnowledgeEntry` → nexus domain `WorldKbEntry`.
/// Consumes the spoke entry (extracts the body carrier + destructures the body).
pub fn spoke_to_world_kb(entry: KnowledgeEntry) -> WorldKbEntry;

/// Nexus lifecycle methods on `WorldKbEntry` that delegate status-transition
/// validity to `spoke_operations` (`confirm` / `deprecate` / `merge_into` /
/// `delete`). Local trait on a foreign type (orphan-rule compliant). Callers
/// must `use nexus_spoke_adapter::conversion::WorldKbEntrySpokeExt;`.
pub trait WorldKbEntrySpokeExt {
    fn confirm(
        &mut self,
        membership: &MembershipPermissionCheck,
        base_revision: u64,
        conflict_check: &ConflictCheckResult,
        visible_manifests: &[&str],
    ) -> Result<(), KbError>;
    fn deprecate(&mut self, replacement_kb_id: Option<&str>) -> Result<(), KbError>;
    fn merge_into(&mut self, target_kb_id: &str) -> Result<(), KbError>;
    fn delete(&mut self) -> Result<(), KbError>;
}
```

All delegation wrappers are thin — they enforce the boundary that operands must already be spoke types at the call site. They do not transform types internally.

### 7.3 Adapter port + injection-orchestration surface — Surface B (spoke ≥ 0.3.0)

As of spoke 0.3.0, `spoke-operations` ships a **capability-sliced adapter port** architecture with **injection orchestration**. The `nexus-spoke-adapter` crate re-exports the port traits and orchestration entrypoints so that consumers can participate in spoke's injection-orchestration model through the same boundary crate — without a direct `spoke-operations` dependency.

> **Async port surface (spoke-operations ≥ 0.9.1):** the adapter port traits are `#[async_trait] async fn` and the `orchestrate_*` entrypoints are native `async fn` — no sync compatibility shim exists (verified against the 0.9.1 registry source). Nexus port impls and every `orchestrate_*` call site use the async surface (`.await`); the adapter no longer captures a tokio runtime handle and can be constructed anywhere. Any future port impl or orchestrator call MUST use the async surface.

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

**Production adapter shipped (V1.142):** the production `BaselinePorts` implementation (`NexusAdapter`, V1.146 rename) was first shipped in `nexus-local-db/src/spoke_adapter/`, replacing the V1.141 reference in-memory mock for the four storage-backed families. The mock remains the reference shape for tests. See §7.4 for the production adapter home, family matrix, and CAS contract reuse.

### 7.4 Production Adapter Home — `NexusAdapter` in `nexus-spoke-adapter` (V1.146 rename)

The production `BaselinePorts` implementation (`NexusAdapter`, V1.146 rename) lives in **`nexus-spoke-adapter/src/adapter/`** (V1.145 rehome). The V1.142 placement in `nexus-local-db/src/spoke_adapter/` was a pragmatic ship for write-path speed; V1.145 corrects the topology to match the durable layering vision:

| Layer | Role | Dependency direction |
|-------|------|---------------------|
| `nexus-local-db` | **Pure storage** — DB CRUD primitives (`SqliteKbStore`, `open_pool`, `run_migrations`); no spoke types or dep on spoke-adapter | ← `nexus-spoke-adapter` depends on these primitives |
| `nexus-spoke-adapter` | **Capability aggregation** — owns `NexusAdapter` + 8 port impls; maps spoke ↔ storage primitives; re-exports Surface A/B | → depends on `nexus-local-db`, `nexus-knowledge`, `spoke-schemas`, `spoke-operations` |
| Business crates (`nexus-daemon-runtime`, `nexus-narrative`, MCA, …) | **Spoke consumers** — call adapter/orchestrators; do not host port impls or spoke serialization | → depend on `nexus-spoke-adapter` + `nexus-knowledge` |

The adapter converts between nexus storage rows and spoke wire types using the V1.145 P1a conversion seam, now owned by `nexus-spoke-adapter` as free functions in `src/conversion/` (`world_kb_to_spoke` / `spoke_to_world_kb`) plus the `WorldKbEntrySpokeExt` lifecycle trait. The seam moved out of `nexus-knowledge` (orphan rule: both `WorldKbEntry` and `KnowledgeEntry` are foreign to `nexus-knowledge`'s former `From` impls), reversing the `nexus-knowledge → nexus-spoke-adapter` edge to `nexus-spoke-adapter → nexus-knowledge`. The conversion seam remains the sole boundary — no second conversion path is added (spec §7.1).

**Module layout (V1.146):**
```
nexus-spoke-adapter/src/
  lib.rs                            ← Surface A (extensions accessors, delegate wrappers) + Surface B (port trait re-exports, orchestrator entrypoints) + production adapter re-export
  extensions.rs                     ← extensions.nexus accessors
  ops.rs                            ← Surface A delegation wrappers
  conversion/                       ← V1.145 P1a — WorldKbEntry↔KnowledgeEntry free fns + WorldKbEntrySpokeExt (sole conversion seam)
  adapter/
    mod.rs                          ← NexusAdapter struct + TX bridge (Arc<Mutex<Option<Transaction>>>) [V1.146 rename; V1.145 P1b rehome from nexus-local-db]
    ── Spoke port trait impls ──
    knowledge_entry_port.rs         ← impl KnowledgeEntryPort for NexusAdapter (CAS + modules_json serialization)
    relation_port.rs                ← impl RelationPort for NexusAdapter (OCC-aware + extensions_nexus_json round-trip)
    scope_query_port.rs             ← impl ScopeQueryPort for NexusAdapter (list_knowledge_entries + list_timeline_events, both production)
    finding_port.rs                 ← impl FindingPort for NexusAdapter
    rule_query_port.rs              ← impl RuleQueryPort for NexusAdapter (production, V1.148 P1)
    host_manifest_port.rs           ← impl HostManifestPort for NexusAdapter (self manifest + list_peer, both production; V1.155 P0 N-C3 closes the last stub)
    computable_port.rs              ← impl ComputablePort for NexusAdapter (production, V1.146 P2 T2)
    fork_port.rs                    ← impl ForkTimelineQueryPort for NexusAdapter (production, V1.146 P2 T3)
    ── Adapter-internal capabilities ──
    activation.rs                   ← Lore activation engine (V1.149 / DF-74) — default-on; full spoke `modules.activation` dialect + Relation hop expand (see §7.4 Lore activation)
    mca_read.rs                     ← SpokeBackedKbStore wrapper (V1.145 P2) — implements KbStore by translating KbQuery → spoke Scope + NexusAdapter::list_knowledge_entries_scoped
    narrative_read.rs               ← Timeline ordering through adapter boundary (V1.146 P1) — NexusAdapter::list_timeline_events_ordered
examples/
  baseline_adapter.rs               ← reference in-memory mock (for port shape reference)
```

**V1.146 rename:** adapter struct renamed to `NexusAdapter`. Import path: `nexus_spoke_adapter::NexusAdapter`. (V1.145: `nexus_spoke_adapter::adapter::NexusAdapter`; V1.142–V1.144: `nexus_local_db::spoke_adapter::NexusAdapter` before the rename.)

#### Lore activation engine — default-on + Relation hop expand (V1.149 / DF-74)

| Aspect | Contract |
|--------|----------|
| **Status** | **Production default-on** (V1.149 — in-flight; P2 dogfood confirms before ship). Supersedes V1.146 P4 flag-gated spike (`NEXUS_MCA_LORE_ACTIVATION=1` opt-in). |
| **Owner** | `nexus-spoke-adapter/src/adapter/activation.rs` (pure match + hop). MCA calls the engine; CLI loads hop edges. **No** matching/hop code in `spoke-operations`. |
| **Dialect** | Consumer-only over spoke handbook `domain-profile-lore-activation.md` `modules.activation`: `keys`, `secondary_keys`, `logic`, `constant`, `order`, `priority`, `position_hint`, `outlet`, `match`. Inner shapes are handbook-defined under open `ModuleMap` (KE schema does not enumerate activation fields). Logic per handbook truth table — `secondary_keys` absent/empty ⇒ primary-any, `logic` ignored (replaces V1.146 primary-only). Unknown keys ignored + round-trip safe. `position_hint`/`outlet` parsed-not-actioned until DF-75. |
| **Default / off-switch** | Runs on every `assemble_moment` with `world_id` + non-empty World-KB. Escape hatch: `NEXUS_MCA_LORE_ACTIVATION=off` (kept ≥ one minor). |
| **Neutral-only guarantee** | Worlds with no `modules.activation` produce assembled output **byte-equivalent** to V1.146 flag-off (hard ship gate). |
| **Scan** | Stage-0 full string + outline beats = timeline `title`/`summary` from existing narrative fetch. Per-entry `canonical_name` + `body.summary` self-match. Raw manuscript body **not** on MCA path (documented Stage-0 fallback). |
| **Ordering** | Constant seeds first; then `priority` descending (higher wins) → `order` ascending → stable index. |
| **Hops** | On primary/constant fire, expand ≤ **2** hops over undirected adjacency built from confirmed `kb_relationships` via inherent `NexusAdapter::list_hop_edges_for_world` (`list_relationships_for_world(..., include_suggested=false)`). spoke `RelationPort` remains get/put only — **no** list-by-entity on the trait. Hop-pulled entries do **not** re-fire keyword activation. Token budget = remaining MCA budget after primary match when `max_tokens` set; personality never truncated. Cycle-safe (`visited` on `entry_id`). |
| **Trace** | Per-entry reason + hop-origin / hop-depth / source-relation for hopped rows (`--emit-packet`; Control Room inspector = DF-76). |

Product behavior detail (author story, DF mapping, Prepare locks): iteration draft [`../iterations/v1.149/specs/fl-l-w4-activation.md`](../iterations/v1.149/specs/fl-l-w4-activation.md) (process path until P2 closeout promotes any durable deltas here). DF mapping + tracker: [`../knowledge/deferred-features-cross-version-tracker.md`](../knowledge/deferred-features-cross-version-tracker.md) — DF-74 delivered (V1.149) / DF-75 delivered (V1.150) / DF-76 next (V1.151 — assembly inspector).

#### Slot vocabulary + Moment Directive + generation-stage gates (V1.150 / DF-75 — shipped)

V1.150 (DF-75, plans P0+P1+P2) adds **product-local** assembly shaping on top of the V1.149 activation engine. **Consumer-only discipline holds:** no new spoke wire, no `spoke-operations` changes, no new `schemas/` DTOs (`wire_contracts_changed: false`). Product behavior SSOT: iteration draft [`../iterations/v1.150/specs/fl-l-w5-prompt-control-plane.md`](../iterations/v1.150/specs/fl-l-w5-prompt-control-plane.md) (promoted to tracked status here at P2 close).

| Aspect | Contract |
|--------|----------|
| **Slot engine** | `crates/nexus-moment-context-assembly/src/slots.rs` — thin **post-activation** step consuming the V1.149 matched candidate list (no new matching logic; source entries never mutated). Routes each entry by its parsed `position_hint`/`outlet` into named, ordered slots **within** `## World Knowledge Base`. |
| **Slot vocabulary** | `world.before` (`### World (Before)`; `position_hint:"before_defs"`) → default fallback (no-hint / `depth` / unknown hint — the V1.149 flat block, byte-equivalence anchor) → `world.after` (`### World (After)`; `after_defs`) → `kb.outlet.<name>` (`### Outlet: <name>`, open outlets sorted by name) → `style.post_history` (`### Style (Post-History)`; the one reserved well-known outlet name, tail). `position_hint:"depth"` stays parsed-not-actioned (locked Non-Goal). Unknown outlet names are **not** errors — they open a `kb.outlet.<name>` slot (round-trip safe). |
| **Emit order** | Within World KB (locked): `### World (Before)` → default fallback → `### World (After)` → `### Outlet: <name>` (sorted) → `### Style (Post-History)`. `## Moment Directive` is a **top-level** section between `## Timeline` and `## World Knowledge Base`. Does not reorder `## World State` / `## Timeline` / `## User Knowledge`. Within-slot order = V1.149 priority-then-order (constant band first) — unchanged. |
| **Moment Directive** | P1: product-local short-horizon author instruction (`creator moment-directive set|show|clear`; body + `insert_depth` head/mid/tail + TTL `generations`/`chapters` + optional `clear_on_scene_change`). Persistence: `moment_directives` table in `nexus-local-db` (migration + repository `moment_directives.rs`, unique partial index `(creator_id, scope_kind, scope_id) WHERE status='active'`). Scope: per-Work with optional World override; closest scope wins; **not** creator-global, **never** on the spoke wire (AC-I3 — not a `modules.*` object, not a KnowledgeEntry, never in AssemblePacket `placement[]`/`activation_trace[]`). Lifecycle: inject → TTL decrement (one injecting assemble = one generation) / chapter-advance / scene-change clear (`MomentRequest.event_id` change proxy, guide Q7) → soft-delete on expiry. |
| **Generation-stage gates** | P2: `MomentRequest.generation_stage: Option<GenerationStage>` (internal MCA enum: workflow stages `intake`/`research`/`produce`/`review`/`persist` + maintenance run-intents `work_maintenance`/`system_maintenance` + `unspecified`; `run_intent` derivable from stage — no separate field). `slots::apply_stage_gate` implements the spec §4 fill matrix: `world.before`/default/`world.after`/`kb.outlet.*` fill for all narrative stages + `work_maintenance` + `unspecified` (current-behavior cells); `style.post_history` fills **only** for `produce`/`review` (tested gate, AC-I4); `system_maintenance` runs **no lore slots** (tested gate, `_system.*` isolation); `unspecified` (direct `assemble-moment` CLI / inspector path) keeps all slots on. `moment.directive` is **not** stage-gated — TTL governs lifetime, not stage. Wire: CLI `assemble-moment --stage <stage>` (default unspecified); see [`../iterations/v1.150/guides/generation-trigger-wiring.md`](../iterations/v1.150/guides/generation-trigger-wiring.md). |
| **Neutral-only guarantee** | Unchanged (AC-I1b, HARD): no activation entries + no active directive + `unspecified` ⇒ byte-identical to V1.149. Golden suites (`golden_neutral_only_default_on`, `golden_slots_neutral_only`, `golden_flag_off`) green at V1.150 P2 close. |
| **Off-switch interaction** | Slot routing + stage gate are **activation-product shaping steps**: with `NEXUS_MCA_LORE_ACTIVATION=off` every candidate entry is emitted unchanged as the V1.149 flat block (no sub-headings, no gating). |

DF mapping + tracker: [`../knowledge/deferred-features-cross-version-tracker.md`](../knowledge/deferred-features-cross-version-tracker.md) — DF-74 delivered (V1.149) / **DF-75 delivered (V1.150, P0+P1+P2)** / DF-76 next (assembly inspector + thin Control Room Moment Directive input).

#### Inspector packet field surface (V1.151 / DF-76 — shipped; P2 dogfood-confirmed)

V1.151 (DF-76, plans P0+P1+P2) makes the V1.149–V1.150 traces **author-visible** via an **enriched inspector packet** — a **separate emission path** from `to_full_context()` (AC-I6: assembled bytes are byte-identical to V1.150). **Consumer-only discipline holds:** the `modules.*` section keeps the spoke assemble-module recipe vocabulary (presented, not cloned); `slot_map`/`budget`/`moment_directive` are **product-local** sections, explicitly outside the spoke recipe. No new spoke wire, no `schemas/` spoke DTOs; the daemon wire DTOs are nexus-product-local (`schemas/daemon-api/inspector/`). Product behavior SSOT: iteration draft [`../iterations/v1.151/specs/fl-l-w6-assembly-inspector.md`](../iterations/v1.151/specs/fl-l-w6-assembly-inspector.md) (promoted to tracked status at P2 close).

**P2 confirmation (2026-08-05, dogfood + recipe sweep):** the `modules.*` surface was verified against the spoke assemble-module recipe handbook (`assemble-module-recipes.md`) and the shipped `build_inspector_packet` (`nexus-moment-context-assembly/src/inspector.rs`, exercised by the T1 dogfood `dogfood_inspector_packet_stages` across `produce`/`review`/`persist`). Nexus is **consumer-only**: it presents the trace in the recipe **vocabulary** (`placement` = accepted-entries snapshot; `activation_trace` = full fire/miss provenance with `reason`), and does **not** clone the spoke `AssemblePacket.modules` wire — no `position_hint`/`depth`/`outlet`/`matched_key`/`hop_count` inner fields, no `packet_id`/`entries[]` envelope; nexus adds product fields (`canonical_name`, `accepted`) to the per-entry rows. `slot_map`/`budget`/`moment_directive` are top-level **product-local** sections, explicitly outside the spoke recipe (never under `modules.*`, AC-I3).

| Aspect | Contract (architect lock — grounded) |
|--------|--------------------------------------|
| **Packet shape** | `{ "modules": { "placement", "activation_trace" }, "slot_map": [...], "budget": {...}, "moment_directive": {...} }`. `modules.*` **unchanged** from the V1.150 packet (former builder `apps/nexus42/src/commands/platform/context.rs:810-845`; now `nexus-moment-context-assembly/src/inspector.rs`): `placement[]` = accepted entries `{entry_id, canonical_name, reason}`; `activation_trace[]` = full trace `{entry_id, canonical_name, reason, accepted}` (`ActivationTraceEntry`, `nexus-spoke-adapter/src/adapter/activation.rs:156-167`). |
| **`slot_map`** | `[{ "entry_id", "slot" }]` — every accepted entry that survived the stage gate → its slot id. Slot ids = `SlotRouting` field names (`nexus-moment-context-assembly/src/slots.rs:92-109`): `world.before` / `default` / `world.after` / `kb.outlet.<name>` / `style.post_history`, plus `moment.directive` when an active directive renders (top-level section). **Derived from the V1.150 routing output, never re-routed**; captured **post stage-gate** (the gate drops `style.post_history` for non-produce/review stages, `slots.rs:257`). **Additive capture:** `SlotMapEntry` + `SlotRouting::to_slot_map()`; `MomentContext.slot_map: Option<Vec<SlotMapEntry>>` populated at `assemble_moment_with_directive` (`moment.rs:714`). |
| **`budget`** | `{ "primary_tokens_est", "hop_tokens_est", "cap", "remaining" }` (u | null) — chars/4 estimator (`estimate_tokens`, `activation.rs:778`, summary-or-name / 4). **Additive:** `ActivationBudget` on `ActivationResult` (values already computed inside `apply_activation_with_hops` at `activation.rs:576` and `expand_relation_hops` at `activation.rs:698`; surface via `HopExpandResult.tokens_consumed`); `MomentContext.activation_budget` captured at `moment.rs:695`. `cap` = effective hop cap (`None` ⇒ depth+cycle only); `remaining` = budget left after primary + hops (`None` when no cap). No-hops path: primary estimate only, `cap`/`remaining` = `None`. |
| **`moment_directive`** | Status/metadata only — `{ "scope": "work"|"world"|null, "scope_id", "insert_depth": "head"|"mid"|"tail"|null, "ttl_kind": "generations"|"chapters"|null, "ttl_remaining": u|null, "clear_on_scene_change": bool, "status": "active"|"none" }`. Sources: persisted `MomentDirectiveRow` (`nexus-local-db/src/moment_directive.rs:59-87` — `scope_kind`, `scope_id`, `insert_depth`, `ttl_kind`, `ttl_remaining`, `clear_on_scene_change`, `status`). **Additive:** `ActiveDirective` (`nexus-moment-context-assembly/src/directive.rs:103-134`) gains `ttl_remaining/status/scope_kind/scope_id`; `MomentContext.moment_directive_meta` captured at `moment.rs:779`. `status` = `"active"` when injected this assembly (active-row lookups only, `moment_directive.rs:224-243`), `"none"` when absent. |
| **Directive-body exclusion (AC-I3, by construction)** | The packet builder reads `MomentContext.moment_directive_meta` and **never** `MomentContext.moment_directive` (the body, `moment.rs:292`) nor the `body` column. The directive body is author content surfaced only via the directive `show` surface / the assembled prompt. Enforced by the extended `inspector_packet_never_carries_moment_directive` test. |
| **Builder home** | Packet building stays **product-local**: `build_inspector_packet` relocated from `apps/nexus42` (private fn, `context.rs:810-845`) to MCA module `nexus-moment-context-assembly/src/inspector.rs` (`pub fn build_inspector_packet(ctx: &MomentContext) -> serde_json::Value`, shipped P0) — shared by the CLI (`emit_inspector_packet`, `context.rs:785`), the daemon route (`POST /v1/daemon/inspector/moment`, tier2, ownership-guarded via `is_world_owned` — `handlers/check.rs:98-184` pattern), and the Control Room panel. No inspector logic in `spoke-operations`. |
| **Directive daemon route (P0, H5)** | `POST /v1/daemon/moment-directive` — thin set/show/clear HTTP wrapper over `nexus_local_db::moment_directive::{set_active, replace_active, get_active_for_work, get_active_for_world, clear}` (CLI precedent `apps/nexus42/src/commands/creator/moment_directive.rs:145/247/273`); validation mirrors the CLI (`handle_set`, `apps/nexus42/src/commands/creator/moment_directive.rs:145`; explicit `replace` via the unique partial index, `local-db moment_directive.rs:126` — no silent overwrite). `LocalDirectiveStore` (now `nexus-daemon-runtime/src/directive_store.rs:37`) relocated so CLI and route share the composition root (cannot live in `nexus-local-db` — cycle MCA → spoke-adapter → local-db). |

#### Production-vs-stub matrix per port family (V1.148)

| Port family / method | Implementation class | Backing | Rationale |
|---|---|---|---|
| `KnowledgeEntryPort` | **Production** | `kb_key_blocks` via `SqliteKbStore` primitives + V1.73 CAS + `modules_json` serialization (spoke `KnowledgeEntry.modules`, V1.146 P4 T1) | Existing storage with OCC; adapter in spoke-adapter, storage primitives in local-db |
| `RelationPort` | **Production** (OCC-aware, V1.144 P1) | `kb_relationships` via `SqliteKbStore` primitives + CAS (`WHERE revision = ?`) + `extensions_nexus_json` round-trip (V1.146 P3) | Existing storage; OCC added V1.144 P1 per spoke 0.5.0 `RelationPort` trait |
| `FindingPort` | **Production** | `findings` table | Existing storage |
| `ScopeQueryPort.list_knowledge_entries` | **Production** | `kb_key_blocks` scope-filtered by `scope_id` + `entry_ids`/`entry_types`; unfiltered full-world listings reject on >`LIST_BY_WORLD_LIMIT` overflow (no silent truncation for orchestrators). The MCA read path does NOT use this trait method — it uses `SpokeBackedKbStore` → `NexusAdapter::list_knowledge_entries_scoped` reading `scope.extensions["nexus"]` (see §7.4 scope-pushdown contract) | Existing storage; P2 production read via `SpokeBackedKbStore` (MCA only) |
| `ScopeQueryPort.list_timeline_events` | **Production** (V1.145 P3) | `narrative_timeline_events` table (V1.26); scope-filtered by `scope_id` → `world_id`, `extensions.nexus.branch_id`, `timeline_event_ids` | Timeline IS persisted in `narrative_timeline_events` (V1.26 migration); the V1.142 stub was incorrect — data exists, the port just didn't query it |
| `RuleQueryPort` | **Production** (V1.148 P1) | New `spoke_rules` table (`nexus-local-db`); `list_rules(rule_refs)` maps `rule_id` → spoke `Rule`; unknown refs omitted (not error); empty ref-set → empty | Closes `R-V1142P1-001`. Substrate for Daemon HTTP `POST /v1/daemon/check` (P2). Does **not** imply Connect `check` dispatch (N-C0 refuses all ops). |
| `HostManifestPort.get_host_capability_manifest` | **Production** (self) | Shared builder: installation `host_id` from `~/.nexus42/device-id`; roles `["data-store"]`; capabilities `spoke-baseline` + `l2-computable` + `l5-fork`; namespaces `["nexus"]` | Honest self-description for adapter + Connect Host hello (`ConnectConfig.local_manifest`) |
| `HostManifestPort.list_peer_host_capability_manifests` | **Production** (V1.155 P0 N-C3) | `peer_hosts` table (workspace DB, migration `20260808120000_peer_hosts.sql`); `nexus_local_db::list_peer_manifests` rows → typed `HostCapabilityManifest` (`last_seen` DESC, `host_id` ASC) | Multi-host production (DF-72 N-C3): peers recorded ONLY from observed Connect sessions — the outbound `connect()` return observation point (`record_peer_manifest`, manifest-backed, lock #1); inbound-only peers not recorded (spoke-connect API change, out of nexus scope). Empty table → `Ok(vec![])` (stub contract preserved); corrupt stored row → `InternalError` (never skipped). Closes `R-V1142P1-002`. Adapter-level CLI read: `NexusAdapter::list_observed_peer_hosts` (manifest + `last_seen`) backs `connect peers list`. |
| `ComputablePort` | **Production** (V1.146 P2 T2) | `compute_sessions` table via `nexus-wasm-host` session store | WASM compute sessions; bridges spoke `project`/`compute` requests to the stateless WASM runtime |
| `ForkTimelineQueryPort` | **Production** (V1.146 P2 T3) | `narrative_timeline_events` + `narrative_branches` fork-filtered | Fork-scoped timeline queries; fork has no relation to fork-timeline precedes ordering (precedes is Relation-DAG, not fork-port scope) |

**Stub behavior contract:** each stub is a documented empty/static return with a doc-comment referencing its roadmap trigger and residual. Stubs must never fabricate data — they return exactly what the backing storage would if it were empty/static. **V1.155 P0 (N-C3): the last stub is gone** — `HostManifestPort.list_peer_host_capability_manifests` is production (see the matrix row); the adapter has zero stubs. See `host_manifest_port.rs` module-level docs and §10.

#### CAS contract reuse

`KnowledgeEntryPort::put_knowledge_entry` routes through the existing V1.73 `cas_update_key_block_fields` CAS path in `nexus-local-db::kb_store`. The adapter maps CAS outcomes to spoke reject codes:

| CAS outcome (actual vs expected_revision) | Spoke reject code |
|---|---|
| `actual > expected` (stored revision is newer) | `STORED_REVISION_STALE` |
| `actual < expected` (caller expects future revision) | `REVISION_CONFLICT` |
| Entry absent + `expected_revision = Some(_)` | `REVISION_CONFLICT` |
| Entry present + `expected_revision = None` (create on existing) | `KnowledgeEntryAlreadyExists` |

True concurrent safety requires the CAS check to be atomic with the write. The V1.73 `cas_update_key_block_fields` function satisfies this through a `WHERE COALESCE(revision, 0) = ?` guard column inside the caller's SQLite transaction. The adapter does not add a second CAS layer.

`RelationPort::put_relation` routes through the existing V1.74 `update_relationship_in_tx` CAS path in `nexus-local-db::kb_relationships`. The `kb_relationships` table already has a `revision` column (V1.74; verified in `nexus-local-db/src/kb_relationships.rs` line 45), and `update_relationship_in_tx` already implements `WHERE revision = ?` CAS guard + `revision = expected + 1` bump (lines 171–176). The adapter maps CAS outcomes to spoke reject codes:

| CAS outcome (actual vs expected_revision) | Spoke reject code |
|---|---|
| `actual > expected` (stored revision is newer) | `STORED_REVISION_STALE` |
| `actual < expected` (caller expects future revision) | `REVISION_CONFLICT` |
| Relation absent + `expected_revision = Some(_)` | `STORED_REVISION_STALE` |
| Relation present + `expected_revision = None` (create on existing) | `RELATION_ALREADY_EXISTS` |
| Relation absent + `expected_revision = None` (create) | Accept; adapter seeds `revision = 1` (spoke convention) |

On create, the adapter seeds `revision = 1` (spoke convention), not `0` (nexus V1.74 legacy). The spoke `Relation.revision` field is `Option<u64>`, so consumers already handle optionality — no wire break.

`RelationPort::get_relation` reads from `kb_relationships` via the existing `KbRelationshipRow` → spoke `Relation` conversion. On not-found it returns `SpokeRejectCode::RelationNotFound` (available in spoke 0.5.0; verified in `result.rs` line 28). The conversion mapping is identical to the `put_relation` path (see `relation_port.rs` header table, verified for 0.5.0 field names).

#### Scope-pushdown contract — Nexus query filters alongside `Scope` (V1.145 P2)

> **V1.145 P2 (spoke-native extensions, ≥ 0.6.0):** the nexus-specific filters
> ride `scope.extensions["nexus"]` (looked up via the typify
> `ScopeExtensionsKey` newtype). This is the original scope-pushdown design;
> the prior typed `KbScopeFilters` carrier was a 0.5.0 workaround (0.5.0's
> `Scope` had no `extensions` field) and is **removed** in the 0.6.0 redo. See
> `nexus-spoke-adapter/src/adapter/mca_read.rs` (`scope_from_kb_query` /
> `kb_query_from_scope`). The round-trip is proven by the
> `scope_extensions_round_trip` smoke test.

spoke `Scope` supports `entry_ids`, `entry_types`, `source_id`, `fork_id`, `timeline_event_ids`, `timeline_scale`. Nexus WorldKB read paths (MCA `assemble_moment`) need additional filters that spoke Scope does not natively provide: `text_search`, `canonical_name`, `limit`, `offset`, `computable`. Since spoke 0.6.0 these ride the spoke-native `scope.extensions["nexus"]` namespace (a typed `ExtensionMap`), so the WorldKB read crosses the spoke boundary carrying every filter on a spoke `Scope` — no separate nexus-side carrier struct.

| Nexus query field | Where it rides | Adapter handling |
|---|---|---|
| `world_id` | `Scope.scope_id` | `WHERE world_id = ?` |
| `block_type` | `Scope.entry_types` | maps to `KbQuery.block_type` (MCA sends at most one) |
| `canonical_name` | `scope.extensions["nexus"].canonical_name` | in-memory filter (matches `KbStore::query`) |
| `text_search` | `scope.extensions["nexus"].text_search` | in-memory filter (matches `KbStore::query`) |
| `limit` | `scope.extensions["nexus"].limit` | in-memory pagination (matches `KbStore::query`) |
| `offset` | `scope.extensions["nexus"].offset` | in-memory pagination (matches `KbStore::query`) |
| `computable` | `scope.extensions["nexus"].computable` | in-memory filter (matches `KbStore::query`) |

**Behavior preservation (HARD):** `SpokeBackedKbStore::query` builds a spoke `Scope` (native `entry_types` + `extensions["nexus"]`), then `NexusAdapter::list_knowledge_entries_scoped` extracts the nexus filters from the scope, reconstructs the equivalent `KbQuery`, and delegates to `SqliteKbStore::query` — so it produces a byte-identical `KbQueryResult` to the direct `query` path (same silent 500-row window, same in-memory filter + pagination, **no** reject-on-overflow). The MCA inherent method and the spoke `ScopeQueryPort::list_knowledge_entries` (whose reject-on-overflow serves orchestrators) stay **separate** — unifying them would break one limit contract or the other. The body round-trips losslessly: the adapter stashes the `_nexus_body` carrier on the read path so `spoke_to_world_kb` recovers the exact body (V1.143 body-fidelity mechanism).

#### Read-path ScopeQuery adoption (V1.145)

**P2 — MCA WorldKB read:** MCA's `fetch_world_kb` (in `nexus-moment-context-assembly/src/moment.rs`) switches from `SqliteKbStore` to a `SpokeBackedKbStore` wrapper (`nexus-spoke-adapter/src/adapter/mca_read.rs`) that implements `KbStore` by translating `KbQuery` → spoke `Scope` (native `entry_types` from `block_type` + the nexus-specific filters under `scope.extensions["nexus"]`) → `NexusAdapter::list_knowledge_entries_scoped` (an async inherent method, NOT the spoke `ScopeQueryPort` trait method, so MCA does not inherit the spoke port's reject-on-overflow). The wrapper converts spoke `KnowledgeEntry` → nexus `WorldKbEntry` via the free function `spoke_to_world_kb` (V1.145 P1a conversion seam; lossless body carrier preserves summary/tags/attributes). The MCA read is wired at `apps/nexus42/src/commands/platform/context.rs::run_assemble_moment` (the single production `assemble_moment` KB-store call site). MCA's generic `K: KbStore` signature is unchanged — only the injected implementation changes.

**P2 scope boundary (explicit):** MCA is the only production consumer cut over in V1.145. Daemon CRUD read paths (`get_graph`, `get_candidates`) stay on `SqliteKbStore` directly — these are UI views, not spoke integration concerns. Evaluation deferred to V1.146+.

**Narrative timeline ordering through the adapter (deferred to V1.146):** P3 shipped production `ScopeQueryPort.list_timeline_events` (table query + scope filter). The remaining goal — routing the narrative gateway's `get_timeline_ordered` ordering through the adapter boundary so the spoke `order_timeline_events_by_ids` helper is called via adapter re-export rather than a direct `spoke-operations` import in `narrative_gateway.rs` — is **deferred to V1.146**. It is a real refactor: `narrative_gateway` lives in `nexus-local-db` (pure storage, no spoke-adapter dep post-P1b reversal) and in `nexus-narrative`, neither of which can depend on `nexus-spoke-adapter` without re-introducing the reversed edge. The ordering therefore needs to move to a spoke-adapter-dependent layer. Until then, `nexus-local-db` and `nexus-narrative` take `spoke-operations` directly as a **standard leaf library dep** (no cycle) for the ordering helper.

#### Timeline storage — existing table reuse (V1.145 P3)

The `narrative_timeline_events` table already exists (V1.26 migration `20260524_narrative_worlds.sql`). The production write path (`narrative_write.rs::append_event`) already writes to it. The V1.143 `TimelineEvent` ↔ spoke `TimelineEvent` conversion seam already maps the stored columns. P3 adds one additive column for spoke round-trip (`extensions_nexus_json TEXT`) and replaces the stub `list_timeline_events` with a production query. No new table, no write migration needed — the SSOT for timeline data already lives in `narrative_timeline_events`.

**Timeline Scope filter alignment:** Scope for timeline:
- `scope.scope_id` → `world_id` (SQL `WHERE world_id = ?`)
- `scope.extensions["nexus"]["branch_id"]` → `branch_id` (for fork-scoped queries)
- `scope.timeline_event_ids` → `WHERE timeline_event_id IN (...)`
- `scope.timeline_scale`, `scope.fork_id` → no-op filter (nexus doesn't use these; spoke pass-through)

#### Orchestrator cutover registry

Each orchestrator adoption on a daemon write path is registered here. The registry records the handler, the orchestrator, the adapter, and the cutover iteration.

| Orchestrator | Handler (symbol) | File | Adapter | Cutover | Status |
|---|---|---|---|---|---|---|
| `orchestrate_promote` | `promote_adopt()` | `world_kb.rs:608` | `NexusAdapter` | V1.142 | Shipped |
| `orchestrate_upsert` | `patch_entity()` | `world_kb.rs:286` | `NexusAdapter` | V1.143 | Shipped |
| `orchestrate_relate` | `patch_relationship_add()` / `_update()` | `world_kb.rs:2156`/`2211` | `NexusAdapter` | V1.144 | Shipped (`e17b9a34`; `remove` stays Surface A) |

> **Note:** `patch_relationship_remove()` (line 1653) is **not** a cutover candidate — `orchestrate_relate` has no delete path (`RelationPort` exposes only `put_relation`). The `remove` action stays on Surface A via `delete_relationship_in_tx()`.

> **V1.143 structural mismatch resolved by spoke 0.5.0:** V1.143 `R-V1143P2-DEFER-RELATE` identified that spoke 0.4.1's `Relation` type lacked a `revision` field, making `RelationPort::put_relation` insert-only with no CAS guard. spoke 0.5.0 ships `Relation.revision: Option<u64>` (verified in `../spoke/crates/spoke-schemas/src/generated/data/relation.rs`) + OCC-aware `RelationPort` trait with `get_relation` + `put_relation(relation, expected_base_revision: Option<u64>)` (verified in `../spoke/crates/spoke-operations/src/adapter/ports.rs` lines 30–52) + `orchestrate_relate` deep OCC (verified in `../spoke/crates/spoke-operations/src/adapter/orchestrate.rs` lines 300–338). The nexus `kb_relationships` table already has a `revision` column (V1.74; verified in `nexus-local-db/src/kb_relationships.rs` line 45). The cutover plan now spans three P0→P2 plans in V1.144:
> - **P0** — Pin to 0.5.0 + compile-gate stubs (`get_relation`/OCC `put_relation` signature).
> - **P1** — Production `RelationPort` OCC impl (CAS create/update, stale-reject, `RelationAlreadyExists`, `RelationNotFound`).
> - **P2** — Daemon handler cutover (`patch_relationship_add`/`_update` → `orchestrate_relate`).

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

**`ScopeQueryPort.list_timeline_events`** is now **production** (V1.145 P3) — queries `narrative_timeline_events` table, converts via the V1.143 conversion seam, filters by Scope. The V1.142 stub (`Ok(Vec::new())`) is replaced.

## 8. Crate Dependency Graph (V1.146)

```
nexus-spoke-adapter                    ← capability aggregation (PRODUCTION ADAPTER HOME)
  ├── nexus-local-db ──────────────┐   ← depends on storage primitives (SqliteKbStore, open_pool, …) [V1.145 P1b]
  │   ├── nexus-narrative ───────┐ │   ← local-db implements NarrativeGateway over SQLite
  │   │   ├── nexus-knowledge ───┤ │
  │   │   └── spoke-schemas      │ │   (TimelineEvent wire type, V1.143)
  │   ├── nexus-knowledge ───────┤ │
  │   │   └── spoke-schemas      │ │   (native KnowledgeEntry type)
  │   ├── sqlx, …                │ │   (persistence)
  ├── nexus-wasm-host ───────────────┘ ← ComputablePort bridges spoke compute to WASM runtime (V1.146 P2 T2)
  ├── nexus-knowledge                    ← domain types (WorldKbEntry); conversion seam moved OUT to spoke-adapter (V1.145 P1a)
  ├── spoke-schemas                      ← native spoke types
  └── spoke-operations                   ← orchestrators + helpers + port traits

nexus-local-db                       ← pure storage (NO spoke-adapter dep after V1.145 P1b)
  ├── nexus-knowledge                 ← domain types
  ├── nexus-narrative                 ← NarrativeGateway trait impl over SQLite
  └── sqlx, …                         ← persistence only

nexus-narrative                      ← narrative domain: worlds, forks, timelines (NO spoke-operations dep after V1.146 P1)
  ├── nexus-knowledge                 ← knowledge entries, source anchors
  ├── nexus-contracts                 ← generated wire types
  └── spoke-schemas                   ← TimelineEvent wire type (V1.143 conversion seam)
```

**Key changes from V1.145:**
- `nexus-spoke-adapter` gains a `nexus-wasm-host` dep (ComputablePort, V1.146 P2 T2).
- `nexus-local-db` no longer depends on `spoke-operations` (timeline ordering moved to `nexus-spoke-adapter::narrative_read`, V1.146 P1).
- `nexus-narrative` no longer depends on `spoke-operations` (same refactor, V1.146 P1).
- The ordered-timeline facet now lives on the adapter boundary as `NexusAdapter::list_timeline_events_ordered`.

**Historical V1.145 context:** prior to V1.146 P1, `nexus-narrative` and `nexus-local-db` depended on `spoke-operations` directly as a standard leaf library for the `order_timeline_events_by_ids` timeline ordering helper. The V1.146 P1 refactor moved timeline ordering to `nexus-spoke-adapter::narrative_read` (`NexusAdapter::list_timeline_events_ordered`), removing the last `spoke-operations` direct deps from both `nexus-local-db` and `nexus-narrative`. The conversion seam (`world_kb_to_spoke` / `spoke_to_world_kb`) remains owned by `nexus-spoke-adapter` (V1.145 P1a).

## 9. Migration Summary

| P0 | P1 | P2 | P3 |
|----|----|----|-----|
| Add spoke deps; create adapter crate; delete `key-block.schema.json`; repoint daemon-api envelope `$ref`; regenerate codegen; update drift detection; create this tracked spec | Rename `nexus-kb` → `nexus-knowledge`; migrate types to spoke `KnowledgeEntry`; route lifecycle ops through adapter; migrate SQLite storage; migrate all Rust consumers | Rename daemon-api DTOs; bump `@42ch/nexus-contracts`; migrate TS apps (import from `@42ch/spoke-schemas`); update UI strings to Q11 product label | Sweep specs/docs/knowledge/CLI/fixtures for terminology; add pattern doc via compound at iteration-close |

## 10. Connect Host N-C0 (V1.148 / DF-72)

Normative architectural surface for the first FL-R Connect Host slice. Product behavior detail (integrator persona, dogfood story, non-goals) lives in the iteration draft `.mstar/iterations/v1.148/specs/fl-r-connect-host-foundation.md` until promoted; this section locks durable topology and honesty rules for implementers and QC.

### 10.1 Opt-in boundary

| Rule | Norm |
|------|------|
| Cargo feature | `connect-host` on `apps/nexus42` (default **off**) |
| CLI entrypoint | `nexus42 connect start` only (feature-gated) |
| Dependency | `spoke-connect = "=0.9.2"` workspace dep; optional on `nexus42` |
| Default daemon | Feature-off build does **not** link `spoke-connect`. `nexus42 daemon start` never opens a Connect listener (even if feature-on binary is used as daemon). |
| mDNS | spoke-connect exposes no `mdns` feature as of 0.9.2 (removed upstream); hickory/libp2p-mdns stay lockfile-only via libp2p 0.56 optional deps, never compiled |

### 10.2 Topology

- Connect Host runs as a **separate OS process** (`connect start`), not a tokio task inside the daemon.
- N-C0 does **not** share `Arc<NexusAdapter>` with the daemon process (no invoke path needs the adapter).
- Coexistence with Daemon HTTP is process-level (both may run); Connect does not proxy creator HTTP routes.
- N-C1+ may open workspace DB inside the Connect process and construct `NexusAdapter` there.

### 10.3 Manifest honesty

- **Single builder** shared by `HostManifestPort::get_host_capability_manifest` and `ConnectConfig.local_manifest`.
- Wire type: spoke `HostCapabilityManifest` (`schemas/data/host-capability-manifest.schema.json`); hello embeds field-identical `connect_hello::HostCapabilityManifest`.
- N-C0 field contract: `schema_version = 1`; `host_id` = `~/.nexus42/device-id` UUID; `roles = ["data-store"]`; `capabilities = ["spoke-baseline", "l2-computable", "l5-fork"]`; `namespaces = ["nexus"]`; `authority` absent; `extensions.nexus = { connect_host_slice: "n-c0", daemon_http_coexists: true }`.
- N-C1 field extension (V1.153, delivered): `extensions.nexus = { connect_host_slice: "n-c1", served_ops: ["upsert", "promote", "relate"], daemon_http_coexists: true }` — `served_ops` advertises **exactly** the write ops the Connect invoke dispatcher serves; the honesty tests machine-check both directions (advertised ⇔ served) so the manifest cannot drift from the dispatch. `roles` / `capabilities` / `namespaces` unchanged.
- N-C2 read-half field extension (V1.154 P1, delivered): `extensions.nexus = { connect_host_slice: "n-c2", served_ops: ["upsert", "promote", "relate", "check", "assemble"], daemon_http_coexists: true }` — the slice marker advances with the served surface (`served_ops` remains the authoritative op list); `roles` add `checker` / `assembler`; `capabilities` / `namespaces` unchanged.
- N-C2 compute-half field extension (V1.154 P2, delivered): the slice marker stays `"n-c2"`; `served_ops` → `["upsert", "promote", "relate", "check", "assemble", "compute"]` (the full E2 served set); `roles` add `computable-engine` — the semantic reasoning-complete milestone wire form (`l2-computable` has been advertised since N-C0; the literal `"reasoning-complete"` string stays absent — the host-capability-manifest schema defines open string arrays, not that enum); `capabilities` / `namespaces` unchanged. The honesty machine-check covers both directions over the full E2 set (advertised `served_ops` ⇔ dispatch `SERVED_OPS`, roles ⇔ served dispatch).
- MUST NOT advertise `"reasoning-complete"` or any role beyond `data-store` until Connect dispatch exists for that role.
- Capability strings must map to production adapter ports (machine-checked in P3 honesty test).

### 10.4 Handshake, allowlist, op-refusal

- Normative spoke-connect hello (`spoke-connect-hello-jcs-v1`): JCS + Ed25519 over `{protocol_version, peer_id, nonce, host}`; `PROTOCOL_VERSION = 1`.
- Allowlist: `~/.nexus42/connect/allowlist.json` + CLI `--allow-peer`; empty ⇒ fail-closed (spoke semantics).
- Capability-token: structural gate only — production defaults `trusted_issuers` empty, `capability_token_provider = None`, `require_capability_token = false`.
- **Op refusal (no handler):** a host without any invoke handler (`invoke_handler = None` and `invoke_handler_v2 = None`) refuses every inbound invoke with `ErrorEnvelope.code = "op_unsupported"` and no side effects — the crate default. No `NexusAdapter` call from Connect invoke in N-C0.
- **N-C1 refusal contract (extends):** the connect host wires `config.invoke_handler_v2 = Some(invoke::build_handler(...))` — the session-peer dispatch handler (spoke-connect 0.9.2 `InvokeHandlerV2`, `Fn(&PeerId, &str, Value)`, caller identity = the authenticated session peer) — and leaves the legacy `invoke_handler = None` (clean cutover: the payload-identity path is not selected). N-C2 (E2): `check` / `assemble` / `compute` served alongside `upsert` / `promote` / `relate`; non-served ops (`project` / unknown) still return `ErrorEnvelope.code = "op_unsupported"` with zero side effects. The no-handler rule above remains the crate default for hosts without a handler.

### 10.5 Daemon HTTP `check` (related, not Connect)

- V1.148 also lands **`POST /v1/daemon/check`** (tier2, `is_world_owned`, adapter `orchestrate_check`, no KB auto-apply beyond FindingPort persistence). That route is **creator Daemon HTTP only** — it does not make Connect `check` available. See plan `2026-08-04-v1.148-p2-orchestrate-check-daemon-cutover`.

### 10.6 Phased DF-72 roadmap (durable)

| Slice | Content | Tracker |
|-------|---------|---------|
| **N-C0** (V1.148) | Opt-in host, hello + allowlist, honest manifest, all ops refused | DF-72 partial |
| **N-C1** (V1.153, delivered) | Inbound write ops `upsert` / `promote` / `relate` over Connect + OCC (`expected_base_revision`, locked reject-code mapping) + fail-closed world scoping (allowlist `world_scope` + stored-world gate); coexistence = SQLite WAL (no `runtime_lock` on the invoke path); caller identity = the noise-authenticated **session peer** (spoke-connect 0.9.2 `InvokeHandlerV2`; payload `extensions.nexus.peer_id` informational only — present ⇒ must equal the session peer, hard deny on mismatch, zero side effects); spoke-connect exposes no `mdns` feature (hickory/libp2p-mdns stay lockfile-only, never compiled) | DF-72 |
| **N-C2** (read half, delivered) | `check` / `assemble` over Connect — served-op set = `upsert` / `promote` / `relate` / `check` / `assemble` (`SERVED_OPS` ⇔ manifest `extensions.nexus.served_ops`, honesty machine-checked both directions over the enlarged set, literal `"reasoning-complete"` asserted absent); roles = `["data-store", "checker", "assembler"]` (open-string vocabulary; `checker` / `assembler` back the served read ops); read ops ride the same fail-closed world/op scoping as writes + the bounded async bridge (architect-locked `BridgeLimits`: **8** permits bounding BLOCKING-pool orchestrator work per process, a single **30,000 ms** per-invoke deadline shared by the permit acquire AND the result wait (`invoke_busy` fires only after the full budget), **500** logical collection entries or **2 MiB** serialized request bytes — whichever is reached first — and a **2 MiB** serialized **response** byte cap measured after the orchestrator returns (envelope codes `invoke_busy` / `invoke_deadline_exceeded` / `payload_too_large` / `response_too_large`). Concurrency model: the wire path is **serialized on the node's single event loop** — the handler parks the loop during an invoke — so "8 concurrent" means bounded orchestrator work, NOT parallel wire processing. The 2 MiB request byte cap is defense-in-depth: the libp2p request-response codec's inbound-request cap (1 MiB, spoke-connect defaults) is the tighter wire bound, so the 2 MiB check can only fire on locally-built payloads.); the literal `"reasoning-complete"` string stays absent — the semantic reasoning-complete milestone is advertised via `computable-engine` + `l2-computable` with `compute` served (N-C2 compute row) | DF-72 (read half) |
| **N-C2 compute** (E2 compute half, delivered) | `compute` served over Connect — served-op set = `upsert` / `promote` / `relate` / `check` / `assemble` / `compute` (`SERVED_OPS` ⇔ manifest `extensions.nexus.served_ops`, honesty machine-checked both directions over the full E2 set; `project` remains `op_unsupported`). Envelope = spoke `ComputeRequest` / `ComputeResponse` through `orchestrate_compute` → `ComputablePort::compute` (no third wrapper; the V1.147 HTTP `RunRequest` / `RunResponse` pair stays the canonical HTTP mapping reference). **Host-local modules:** the peer names only a module already installed under `~/.nexus42/modules/` — module bytes are never peer-supplied; `module_id` is resolved host-side by the locked precedence (staged session state, then entry `body.computable.module_id`) and **pinned against request override on key presence** — any request-carried `computable.module_id` key (a differing string or a non-string value like `42` / `{}` / `null`) must be a JSON string equal to the gated id, else the defined `module_not_scoped`; neither source ⇒ `module_not_found`. **Missing entry:** a compute request targeting a non-existent `entry_id` is denied with the defined `invalid_input` envelope (client-input family, consistent with the check/assemble mapping) — never `internal_error`. **Compiled-module cache:** the adapter's module load reuses `nexus_wasm_host::ModuleCache` keyed by `(module id, bytes hash)` — the wasmtime compile runs once per distinct module content (repeated invokes of unchanged bytes hit the cache, no recompile); an operator update to the module files (new bytes hash) recompiles on the next invoke and overwrites the cached entry, the cache's only eviction (entries live for the process lifetime; the Connect host's single per-process adapter makes the cache process-wide). A **manifest-only** change (same `.wasm` bytes) is not observed until the `.wasm` changes or the host restarts — the cached manifest is served (matches the daemon's warm-once semantics; fail-safe for operator-installed modules). **`module_scope` per-peer fail-closed** (architect lock): absent/empty scope denies ALL compute with `module_not_scoped` before any WASM execution; the resolved module must be in the peer's module allowlist. **Read-only compute:** `settle: true` ⇒ `settle_not_enabled`; no `project` / accept route on Connect; any future compute write surface must inherit the world-aware CAS before advertisement. **Execution lane:** shared P1 bounded bridge (`spawn_blocking` + 8-permit semaphore + per-engine `compute_serializer` permit — one compute invocation at a time under the global epoch watchdog) + the single 30,000 ms invoke deadline; WASM `DEFAULT_FUEL` / `DEFAULT_MEMORY_MIB` / `DEFAULT_WALL_TIME` stay inner guards; module faults / fuel / memory / epoch traps / deadline expiry ⇒ defined `ErrorEnvelope` codes, no panic crosses Connect. **World-aware CAS (R3 closure):** the write-orchestrator CAS predicates now carry the stored `world_id` — `kb_key_blocks`: `WHERE key_block_id = ? AND COALESCE(revision, 0) = ? AND world_id = ?`; `kb_relationships`: `WHERE relationship_id = ? AND revision = ? AND world_id = ?` — a zero-row CAS caused by the row living in another world surfaces as the fixed `ErrorEnvelope.code = "world_conflict"` (adapter `InternalError` carrier + `world_conflict: true` details marker, remapped by hosts via `is_world_conflict_reject`), never collapsed into `revision_conflict` / `stored_revision_stale` (a same-world stale revision keeps its existing code); the relate **create** path additionally verifies both endpoints' stored worlds; no schema migration (predicate/bind change only); the invoke-gate stored-world check stays as defense-in-depth — the storage-layer CAS is the atomic source of truth across processes (Connect ∥ daemon on the same workspace DB). **Semantic reasoning-complete milestone** (product lock): roles = `["data-store", "checker", "assembler", "computable-engine"]`; capabilities = `["spoke-baseline", "l2-computable", "l5-fork"]` — the literal `"reasoning-complete"` string **never appears in wire** (host-capability-manifest schema: open string arrays, not that enum); the honesty machine-check covers both directions (advertised ⇔ served; roles ⇔ dispatch) over the full E2 served set | DF-72 (compute half) |
|| **N-C3** (V1.155, delivered) | `list_peer_host_capability_manifests` production / multi-host — `peer_hosts` table (workspace DB, migration `20260808120000_peer_hosts.sql`; `manifest_json` is the single manifest source of truth — no denormalized column, fix wave F-002); recording at the outbound `connect()` return (single atomic manifest-backed upsert, `last_seen` RFC 3339 UTC fixed millis, lock #1 fallback: inbound-only peers not recorded — spoke-connect inbound-manifest API change, out of nexus scope); **production dial trigger = the `connect dial <multiaddr>` CLI** (fix wave F-001: dials via `SpokeConnectNode::connect()`, records via `record_dialed_peer`, fail-closed on dial/record errors; `connect start` / `nexus-runtime` boot never dials; the session allowlist is mutual — the dialed peer must be allowlisted via `--allow-peer` / allowlist.json); empty → `Ok(vec![])` (stub contract preserved); corrupt stored row → `InternalError`; honesty: only observed peers, never fabricated; operator read = `connect peers list` via `NexusAdapter::list_observed_peer_hosts` (manifest + `last_seen`). Zero adapter stubs remain. **Roadmap notes:** F-004 — record the session libp2p `peer_id` alongside the claimed `host_id` in a future iteration so host_id spoof/collision is detectable (`last_seen`/`peer_id` drift; impact today is operator-visibility data only, documented contract); S-002 — no `last_seen` index needed at PK-bounded scale; add one when peer count growth demands (documented trigger). | DF-72; closes `R-V1142P1-002` |
| **DF-73** | Headless `nexus-runtime` binary | Separate backlog; after N-C0 dogfood |

**Denial signaling (N-C1):** identity denials and non-served ops share the `op_unsupported` refusal family (`ErrorEnvelope.code = "op_unsupported"`), but the message distinguishes them. An invalid payload identity claim (present `extensions.nexus.peer_id` that does not resolve to the session peer — mismatch, non-string, unparseable, or >128 chars) returns `message = "op denied: extensions.nexus.peer_id does not match the session peer; caller identity comes from the authenticated session"` (invoke.rs `denied()`); a non-served op returns `message = "op <op> is not supported: …"` (invoke.rs `unsupported()`). Integrators match on the `op denied:` prefix to tell "identity spoofed / claim invalid" from "op not served".

## 11. Narrative Knowledge Pack I/O (V1.152 / DF-77)

A **Narrative Knowledge Pack** is a single JSON file that transports one
World's lore — ordered `KnowledgeEntry` records, their `Relation`s, optional
`SourceAnchor`s, and pack-level catalog metadata — between narrative hosts.
Pack I/O is a **product transport envelope**, not a spoke-operations surface:
it moves spoke-standard atoms in bulk using the adapter's `pack` helpers +
orchestrators, but it adds no spoke wire dialect and no spoke-operations
contract. nexus is **consumer-only** for the pack handbook shape (spoke
`domain-profile-narrative-knowledge-pack.md`).

**Supersedes** iteration-scoped
`.mstar/iterations/v1.146/specs/pack-io-product-behavior.md` (V1.146 P3 shipped
the CLI transport; V1.152 ships the author-facing surface: daemon routes, all
three conflict policies, Control Room UI).

### 11.1 Envelope shape (handbook-conformant)

```jsonc
{
  "modules":      { "pack": { "title", "version", "creator", "description?" } },
  "entries":      [ /* KnowledgeEntry[] — ordered canonical_name ASC */ ],
  "relations":    [ /* Relation[] — ordered relationship_id ASC */ ],
  "source_anchors": [ /* optional SourceAnchor[] — only when include_anchors */ ]
}
```

Build/parse helpers: `nexus_spoke_adapter::pack::{build_pack, parse_pack,
parse_pack_str}` (`crates/nexus-spoke-adapter/src/pack.rs`). `parse_pack`
returns a `ParsedPack { pack_metadata, entries, relations, source_anchors,
extra_modules }`.

**Round-trip invariants (HARD):**

| Invariant | Mechanism |
|-----------|-----------|
| `modules.pack` is **product-envelope metadata** (spoke 0.7.0 pack-catalog demote) — never written into KE atoms' `modules` | `build_pack` writes it at the pack root; `parse_pack` stores it in `ParsedPack::pack_metadata` |
| Unknown `modules.*` keys round-trip verbatim | `ParsedPack::extra_modules` (always includes `"pack"` for re-emission); `build_pack(extra_modules=…)` merges them |
| Unknown `extensions.*` namespaces on atoms round-trip verbatim | spoke `KnowledgeEntry.extensions` maps carry them natively; the adapter preserves them (§2.2) |
| `modules.activation` fire-conditions travel with lore | Export preserves per-entry `modules.activation`; import re-persists it (no force-enable — §7.4 Lore activation engine is default-on, activation travels but is not flipped on import) |

### 11.2 Daemon routes (additive, ownership-guarded)

| Route | Method | Product input | Product output |
|-------|--------|---------------|----------------|
| `/v1/daemon/worlds/:world_id/kb/pack/export` | POST | `PackExportRequest` `{include_deprecated?, include_anchors?, title?, pack_version?, description?}` | handbook pack envelope JSON (opaque items — entries/relations are spoke objects per the V1.139 `$ref` fallback §3.4) |
| `/v1/daemon/worlds/:world_id/kb/pack/import` | POST | `PackImportRequest` `{pack: <opaque handbook pack>, conflict: "skip"\|"rename"\|"overwrite", include_anchors?}` | `PackImportResponse` `{entries: AtomCounts, relations: AtomCounts, details: ImportDetail[]}` |

Handler: `crates/nexus-daemon-runtime/src/api/handlers/world_kb_pack.rs`.
Registered via a `pack_routes()` fn merged into `tier2_routes()` (`api/mod.rs`),
mirroring the V1.151 `inspector_routes()` pattern.

**Auth/guard (HARD):** both routes are tier2 (`require_api_key` +
`require_active_creator` middleware) and additionally call `require_creator` +
`require_world_owner` (`world_kb.rs:113-149`) inside the handler. Guard order:
tier2 middleware → `require_world_owner` (403 cross-author / 404 missing world)
→ business logic. Non-owners cannot export or import.

**Wire schemas:** `schemas/daemon-api/kb/pack-{export,import}-{request,response}.schema.json`.
All DTOs carry `additionalProperties: false`. Item arrays (entries, relations)
are opaque `object` per the V1.139 fallback — description cites the spoke
canonical URIs (`https://spoke42.invalid/schemas/data/{knowledge-entry,relation}.schema.json`).
Codegen → `nexus-contracts` (Rust + TS). Contracts minor bump;
`wire_contracts_changed: true`.

### 11.3 Conflict-policy matrix

On collision (matching `entry_id` **or** `canonical_name` + `entry_type` for
entries; matching `relationship_id` for relations), the selected policy
applies **uniformly** to entries and relations:

| Policy | On collision | Entry mechanics | Count |
|--------|-------------|-----------------|-------|
| **skip** (default) | Keep existing; imported atom not written | Skip; remap pack `entry_id` → existing for relation endpoints (F-002) | `skipped` |
| **rename** | Bring in both; imported atom is disambiguated + created | `canonical_name` ← `<original> imported` with numeric tiebreak (` imported 2`, …); fresh `entry_id` minted (`kb_<uuid>`, matching `WorldKbEntry::new()`); remap for relations | `renamed` |
| **overwrite** | Replace exactly one colliding atom (body via upsert; lifecycle + revision preserved) | `orchestrate_upsert` on collided `entry_id` with imported body, `expected_base_revision = existing.revision`, `status` preserved; revision bumped by orchestrator; **never** raw DELETE | `overwritten` |

**Create-path revision normalization:** new entries (no collision) clear `revision` to `None` before `orchestrate_upsert` (`prepare_create_entry` in `pack_import.rs`) so pack rows that carried `revision >= 1` from export still pass the spoke create gate. Overwrite preserves the existing row's `status` and sets `revision` to the collided row's current revision for CAS upsert.

**Additive-only at the policy level:** skip and rename never delete. Overwrite
replaces via `orchestrate_upsert` / `orchestrate_relate` (CAS body replace), never
a raw DELETE. The V1.146 "don't clobber author work" invariant holds for the two
safe policies.

**Control Room confirmation:** overwrite in the Control Room (P1) requires an
explicit confirmation dialog before any write (data-loss path). CLI overwrite is
opt-in via `--conflict overwrite`, never the default; no interactive CLI confirm
required this iteration.

### 11.4 Provenance

Created, renamed, and overwritten rows carry
`source_provenance_kind = "pack_import"` (stamped via
`extensions::set_provenance(..., Some("pack_import"))`). Skipped rows are
unchanged. The constant `IMPORT_PROVENANCE = "pack_import"` is shared by CLI
and daemon (V1.146 lock; DB CHECK includes this value since migration
`20260731000001`).

### 11.5 Shared import-orchestration module

The import-orchestration logic (conflict detection per policy, orchestrator
calls, provenance stamp, endpoint remap) lives in **one** shared module:
`crates/nexus-daemon-runtime/src/pack_import.rs` (`pub async fn import_pack`).
Both the CLI (`apps/nexus42/.../pack.rs::import` — thin caller) and the daemon
route (`world_kb_pack.rs::pack_import` — thin HTTP caller) consume it. This
follows the V1.151 `LocalDirectiveStore` relocation precedent
(`directive_store.rs:1-12`): a composition-root module in `nexus-daemon-runtime`
that both the CLI (which depends on `nexus-daemon-runtime`,
`apps/nexus42/Cargo.toml:40`) and the daemon consume.

`import_pack` does **not** perform the owner gate — each caller calls
`require_world_owner` before invoking it (auth is the caller's job, matching the
`LocalDirectiveStore` precedent). The pack round-trip guarantee is not broken:
`import_pack` consumes `ParsedPack` (which carries `extra_modules`) and writes
atoms via orchestrators — it never re-builds the pack.

### 11.6 CLI surface (LOCKED — V1.146 placement, V1.152 completeness)

User-facing path: `creator world kb pack export|import` (Pack is World-lore
transport). All three conflict policies are implemented (V1.146 shipped `skip`;
V1.152 implements `rename` + `overwrite`, removing the "not yet implemented"
stubs). `skip` is the default. `--dry-run` covers all three policies. CLI +
daemon share the single `import_pack` path.

### 11.7 Non-goals (product)

Pack I/O does **not** ship: an ST lorebook → Pack importer; a seed pack /
community Pool / marketplace / registry / signing / remote pull; multi-pack
compose / stack; automatic post-import activation flip (activation travels with
lore but is not force-enabled); or a new spoke wire dialect (pack is a product
transport envelope, not a `spoke-operations` surface).

### 11.8 Dogfood gate

P2 ships `dogfood_pack_round_trip_preserves_activation_and_relations`
(`apps/nexus42/.../pack.rs::tests`) — seeds World A (entries with
`modules.activation` + ≥1 relation) → export → import into fresh World B under
`skip` → asserts `entries.created` matches seeded count, `pack_import`
provenance on all B entries, `modules.activation` deep-equal A→B,
`relations.created >= 1`, then re-import under `skip` with
`entries.created == 0` (idempotency). Separate tests cover rename/overwrite
policies and activation preservation on collision paths.
