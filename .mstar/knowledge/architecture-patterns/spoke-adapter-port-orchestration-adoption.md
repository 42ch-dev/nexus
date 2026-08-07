---
module: nexus-spoke-adapter
date: 2026-07-28
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-28-v1.141-p1-adapter-port-architecture-adoption
tags: [spoke, adapter, baseline-ports, orchestration, surface-b, cas, occ, call-boundary-invariant, mock-test-pattern]
last_updated: 2026-07-28
applies_when: Consuming SPOKE ≥ 0.3.0 adapter-port + injection-orchestration architecture from a product adapter boundary (Surface B adoption); deciding between pure delegates (Surface A) and port+orchestrator (Surface B); verifying CAS through orchestrators; enforcing call-boundary invariant mechanically
---

# SPOKE Adapter Port + Orchestration Adoption (Surface B)

Companion to [`spoke-adapter-conversion-seam.md`](spoke-adapter-conversion-seam.md) (Surface A — V1.139). Read both: Surface A and Surface B coexist on the same adapter boundary.

## Context

Spoke `0.3.0` introduced a major architectural layer beyond the pure-helper surface nexus consumed at `0.2.0`: **capability-sliced adapter port traits** (`KnowledgeEntryPort`, `RelationPort`, `ScopeQueryPort`, `FindingPort`, `RuleQueryPort`, `HostManifestPort`; optional `ComputablePort` + `ForkTimelineQueryPort`) and **injection orchestration entrypoints** (`orchestrate_upsert` / `_promote` / `_relate` / `_check` / `_assemble` / `_project` / `_compute` / `_fork_check` / `_fork_assemble`). Orchestrators compose the pure helpers with port I/O — they sequence validate → load stored → apply → put-with-CAS in one call, with the adapter (product) owning persistence behind the port traits.

This creates a **dual public surface** at the adapter boundary (spec `spoke-adapter-architecture.md` §7.3):

- **Surface A — Pure delegates** (`ops::*`, `extensions::*`): stateless pass-throughs. Caller manages its own storage and calls helpers one at a time.
- **Surface B — Injection orchestration** (`orchestrate_*` + port traits): caller implements the port families once and delegates lifecycle composition to the orchestrators.

The challenge: how does a product adapter boundary (like `nexus-spoke-adapter`) expose both surfaces without (a) reimplementing spoke invariants, (b) widening its dependency footprint, or (c) confusing consumers about when to pick which surface?

## Guidance

### Adapter boundary exposes both surfaces via re-exports

The adapter crate flat-re-exports the full spoke `adapter` module — all port traits, composition aliases (`BaselinePorts` / `ComputablePorts` / `ForkPorts` / `FullPorts`), `BaselineAdapter` / `ComputableAdapter` / `ForkAdapter` / `FullAdapter` marker traits, the 9 `orchestrate_*` entrypoints, `CheckRunInput`, plus the wire types referenced by orchestrators (request/response envelopes, `HostCapabilityManifest`, `Rule`, `TimelineEvent`, `Relation`, `Scope`). Consumers depend on the adapter for both surfaces — no direct `spoke-operations` dep elsewhere.

```rust
// crates/nexus-spoke-adapter/src/lib.rs (excerpt)
pub use spoke_operations::{
    KnowledgeEntryPort, RelationPort, ScopeQueryPort, /* ... 5 more port traits */,
    BaselinePorts, /* ... 3 more composition traits */,
    BaselineAdapter, /* ... 3 more adapter aliases */,
    orchestrate_upsert, /* ... 8 more entrypoints */,
    CheckRunInput,
};
pub use spoke_schemas::{
    HostCapabilityManifest, UpsertRequest, UpsertResponse, /* ... etc. */,
};
```

### When to pick Surface A vs Surface B

| Situation | Pick |
|-----------|------|
| Caller already has a transaction open; mutation is a single helper call | **Surface A** (simpler) |
| Mutation composes multiple helpers across port families + OCC is required | **Surface B** (encapsulates sequencing) |
| Caller is the storage owner (implements the port traits) and wants lifecycle composition | **Surface B** |
| Caller just needs a pure check (validate, transition) with no I/O | **Surface A** |

General rule: Surface A is the default for one-shot helpers; Surface B pays off when the mutation sequence is non-trivial or when OCC discipline matters. Both surfaces coexist — adopting B does not require migrating A call sites.

### CAS contract verification through orchestrators (not via the mock directly)

The orchestrators internally call `assert_revision_match` on the `expected_base_revision` derived from `get_knowledge_entry` vs the caller-supplied request. To **prove** the CAS gate is reachable end-to-end from a product adapter, drive the reject path **through the orchestrator** (`orchestrate_upsert` or `orchestrate_promote`), not by calling the mock's `put_knowledge_entry` directly.

```rust
// In tests/orchestration_adoption.rs
// Happy create: expected_base_revision = None, entry absent → ok
// Stale revision: stored > expected → SpokeResult::Reject { code: STORED_REVISION_STALE }
// Conflict:       stored < expected → SpokeResult::Reject { code: REVISION_CONFLICT }
```

The mock's own CAS branches are adapter-owned (spec: "adapters own transport, persistence, transactions; library stays I/O-free") — but the test exercises them through the orchestrator path because that is the production caller pattern.

### Mock-test pattern for orchestrator adoption

A reference `BaselinePorts` mock that satisfies all 6 baseline port families lives in `crates/nexus-spoke-adapter/examples/baseline_adapter.rs` (runnable via `cargo run --example baseline_adapter`). The programmatic test twin lives in `tests/orchestration_adoption.rs`. Both reuse the same mock via `#[path = "../examples/baseline_adapter.rs"] mod baseline_adapter;` (compact) or by duplicating (decoupled, more verbose).

The mock is **deliberately thin**: `HashMap`/`Vec`-backed storage; CAS is the only non-trivial logic; each port method body is a few lines. No ranking, retrieval, scoring, or product logic. The mock calls `nexus_spoke_adapter::orchestrate_*` — it does NOT reimplement any spoke invariant (the call-boundary invariant §7 applies to the mock too).

### Optional `ComputablePort` / `ForkTimelineQueryPort` capability-missing path

The orchestrators that require an optional capability (`orchestrate_project` / `orchestrate_compute` require `ComputablePort`; `orchestrate_fork_*` require `ForkTimelineQueryPort`) emit `CAPABILITY_PORT_MISSING` if a baseline-only adapter is passed at a dynamic optional boundary. To verify this in tests, construct a `BaselineOnlyPorts` wrapper that manually impls `ComputablePorts::as_computable() → None`, then call `orchestrate_project` against it — expect `CAPABILITY_PORT_MISSING`.

## Why this matters

- **No second boundary:** the adapter crate is the single import surface for both helpers and orchestrators. Downstream crates (e.g. `nexus-knowledge`) do not pull `spoke-operations` directly.
- **No invariant reimplementation:** orchestrators call the pure helpers internally — products that adopt Surface B get OCC / promote gate / status transition correctness for free.
- **CAS discipline:** `put_knowledge_entry(entry, expected_base_revision)` is the OCC contract. Adapters that back this with real compare-and-put (e.g. nexus's V1.73 `kb_key_blocks` SQLite CAS pattern) get true concurrent safety; adapters that ignore `expected_base_revision` are defective.

## When to Apply

- Consuming SPOKE ≥ 0.3.0 from a product adapter boundary
- Deciding between one-shot pure helpers (Surface A) vs. orchestrator composition (Surface B)
- Verifying CAS end-to-end through orchestrators (not via mock direct calls)
- Implementing a baseline port family against product storage (the mock is the reference shape)

## Examples

### Reference layout in `nexus-spoke-adapter`

```
crates/nexus-spoke-adapter/
  src/lib.rs                          # Surface A + Surface B re-exports
  src/ops.rs                          # Surface A pure delegates (frozen)
  src/extensions.rs                   # extensions.nexus accessors (Surface A)
  examples/baseline_adapter.rs        # Reference BaselinePorts mock (runnable)
  tests/adapter_surface.rs            # Parity test (P0 — re-exports reachable)
  tests/orchestration_adoption.rs     # Adoption test (P1 — 10 scenarios, CAS + capability-missing)
  tests/call_boundary_invariant.rs    # Static check (P1 — 4 assertions enforcing §7)
```

### Two CAS reject codes (do not collapse)

```rust
// In the mock's put_knowledge_entry:
match (expected_base_revision, stored_revision) {
    (None, None) => /* create ok */,
    (None, Some(_)) => /* reject: KnowledgeEntryAlreadyExists */,
    (Some(expected), Some(stored)) if stored == expected => /* update ok, bump */,
    (Some(expected), Some(stored)) if stored > expected => /* reject: STORED_REVISION_STALE */,
    (Some(expected), Some(stored)) if stored < expected => /* reject: REVISION_CONFLICT */,
    (Some(_), None) => /* reject: stored absent but expected present — version mismatch */,
}
```

A common mistake (made and corrected during V1.141 P1 T2): collapse both mismatches into `STORED_REVISION_STALE`. They are distinct codes — spoke's `assert_revision_match` emits them in opposite directions; tests must verify both.

## Common pitfalls

- **Reimplementing spoke invariants in the mock** — violates call-boundary §7. The mock does storage I/O only; orchestrators own lifecycle composition.
- **T4-style static check too strict** — `assert extensions.rs has zero spoke_operations references` fails because `ExtensionMap` is defined in `spoke-operations` (not `spoke-schemas`) and is a wire type, not a lifecycle operation. The correct static check forbids `spoke_operations::[a-z_][a-z0-9_]*\(` (function-call pattern) while admitting type imports.
- **Pulling `nexus-local-db` into the adapter crate** to implement a "real" `KnowledgeEntryPort` — violates the dependency graph (spec §8: adapter stays `spoke-schemas` + `spoke-operations` only). Concrete product port impls live downstream (e.g. `nexus-knowledge`, which already depends on both).
- **Testing CAS by calling the mock directly** — proves the mock's CAS works, not that the orchestrator path works. Drive rejects through `orchestrate_*`.

## Production boundary (shipped V1.142; updated V1.146)

> **Update (V1.142):** The production `BaselinePorts` implementation (`NexusAdapter`, V1.146 rename) was originally landed in `nexus-local-db/src/spoke_adapter/`, **not** "downstream in nexus-knowledge" as the V1.141 doc speculated. `nexus-knowledge` is a domain-types-and-traits crate with no SQLite dependency — it cannot be the production port home. V1.145 P1b rehomed the adapter to `nexus-spoke-adapter/src/adapter/` (spec §7.4 / §8 dep-graph reversal). See spec [`spoke-adapter-architecture.md`](../../../specs/spoke-adapter-architecture.md) §7.4 for the production-vs-stub matrix and dependency rationale.

V1.142 shipped the production adapter in `nexus-local-db` implementing all 6 baseline port families against existing SQLite storage (V1.145 P1b rehome to `nexus-spoke-adapter`):

| Family | Status | Backing |
|--------|--------|---------|
| `KnowledgeEntryPort` | **Production (CAS)** | `kb_key_blocks` via V1.73 CAS path; both `STORED_REVISION_STALE` + `REVISION_CONFLICT` directions |
| `RelationPort` | **Production** | `kb_relationships` |
| `FindingPort` | **Production (batch tx)** | `findings` (V1.142 QC fix: batch wrapped in explicit transaction) |
| `ScopeQueryPort.list_knowledge_entries` | **Production** | `kb_key_blocks` by `world_id` |
| `ScopeQueryPort.list_timeline_events` | **Stub (empty)** | no persisted `TimelineEvent` storage; nexus-narrative holds in-memory |
| `RuleQueryPort` | **Stub (empty)** | no persisted spoke `Rule`; rules come from works config |
| `HostManifestPort` | **Stub (static self)** | local-first nexus; static self manifest; no peers |

Stub families are documented per spec §7.4 with roadmap triggers for full versions.

### Production adapter patterns (V1.142 learnings)

1. **Wire conversion reuse:** the production `KnowledgeEntryPort` reuses the V1.139 `WorldKbEntry ↔ SpokeKnowledgeEntry` `From` impls (single seam). `Relation` / `Finding` map inline at the boundary (no parallel types) with vocabulary mapping (spoke ↔ nexus severity/status) + `INVALID_INPUT` reject on unknown values.
2. **CAS mapping per spec §7.4 (6 outcomes):** `stored > expected → STORED_REVISION_STALE`; `stored < expected → REVISION_CONFLICT`; `absent + Some(_) → RevisionConflict` (NOT `StoredRevisionStale` as the V1.141 mock returned — spec production table is authority).
3. **Async→sync bridge:** `tokio::task::block_in_place` wraps async sqlx behind the sync port trait methods. Requires a multi-threaded tokio runtime; `debug_assert!` on `Handle::runtime_flavor() == MultiThread` at adapter construction catches wrong-flavor early in dev/CI. **V1.154 update:** the *Connect invoke* path (a different surface — sync handler on the spoke-connect event loop) moved OFF `block_in_place` onto a bounded executor (Semaphore + spawn_blocking + deadline + response cap) — see [bounded-sync-async-bridge-event-loop.md](bounded-sync-async-bridge-event-loop.md). The port-trait bridge above remains for the daemon/CLI paths.
4. **Batch transaction:** `FindingPort.put_findings` wraps the batch loop in an explicit SQLite transaction (`pool.begin() → loop → tx.commit()`) so mid-batch failure rolls back the whole batch.
5. **Per-request construction:** `NexusAdapter::new(pool.clone())` is cheap (pool handle clone); construct per request, not per application.

### Orchestrator cutover on write paths (V1.142 P2)

The first production orchestrator consumer is `promote_adopt` in `nexus-daemon-runtime/src/api/handlers/world_kb.rs`, routed through `orchestrate_promote(&NexusAdapter, PromoteRequest)`. Key patterns:

1. **`orchestrate_promote` handles `stored = None`:** the orchestrator can promote a NEW candidate (not just an existing provisional entry). It skips stored-entry terminal-status checks when `stored = None`, validates `candidate.status == "provisional"`, applies acceptance (`→ confirmed`, revision bump), and calls `put_knowledge_entry(None)` (create path). This means the nexus adopt flow (create + confirm in one step) maps cleanly: build candidate with `status = "provisional"`, let the orchestrator flip it.
2. **Single-transaction adopt (V1.142 P3):** `promote_adopt` binds a handler-owned `sqlx::Transaction` into `NexusAdapter` via a shared `Arc<std::sync::Mutex<Option<Transaction>>>` for the synchronous `orchestrate_promote` bridge, then flips the extract job in the same transaction before one `COMMIT`. Rollback on any failure — no greploop soft-delete compensation.
3. **Retry-safe idempotency:** when a prior attempt committed but returned an error (commit-ack ambiguity), retry hits `KnowledgeEntryAlreadyExists` → handler checks job status → returns success if already confirmed and attributed.
4. **SpokeRejectCode → NexusApiError mapping:** OCC codes → 409 with re-read version; validation codes → 422; remaining → 500. Preserve existing error semantics.

### Remaining production boundary work (roadmap)

- Remaining write paths cut over as product features need them: **`upsert` shipped V1.143** (P1 — `patch_entity` → `orchestrate_upsert`); `relate` **deferred to V1.144** (spoke `Relation` lacks `revision` — see V1.143 lesson below).
- Stub family upgrades: `RuleQueryPort` (persisted Rules), `HostManifestPort.list_peer` (multi-host collaboration), `ScopeQueryPort.list_timeline_events` (persisted `TimelineEvent` storage; also unblocks T3b full timeline helper migration in nexus-narrative — **partial progress V1.143** with `get_timeline_ordered()` on both gateways).
- Transaction-boundary unification for `promote_adopt`: **shipped** (V1.142 P3 — adapter-local TX bind via `NexusAdapter::with_tx_cell`).
- TS app orchestrator pattern adoption (separate product surface).

## V1.143 — Deep SPOKE integration (roadmap step 1 of 3)

V1.143 took the V1.141/V1.142 adapter-port foundation and deepened it across three fronts — Timeline wire unification, the second production orchestrator cutover, and Surface A outcome resolution — while discovering a structural constraint that defers the third cutover.

### Production cutover count: 3 paths (2 shipped, 1 deferred)

The orchestrator-cutover-on-write-paths pattern is now proven on the two highest-traffic KB write paths:

| Cutover | Version | Status | Notes |
|---------|---------|--------|-------|
| `promote_adopt` → `orchestrate_promote` | V1.142 | **Shipped** | First Surface B production consumer; single-TX adopt flow with retry-safe idempotency |
| `patch_entity` → `orchestrate_upsert` | V1.143 | **Shipped** | Second production consumer; `with_bound_tx` for transaction binding; C1 merged-terminal behavior-diff (see below) |
| `relate` → `orchestrate_relate` | V1.144 | **Deferred** | Blocked by structural mismatch (see below) |

### `From`/`Into` conversion-seam pattern generalized

V1.143 applied the same `From`/`Into` conversion-seam pattern that V1.139 established for `WorldKbEntry`↔`KnowledgeEntry` to a second type pair: **`TimelineEvent` (nexus-narrative) ↔ spoke `TimelineEvent`**. This is now a proven reusable pattern:

- Each product domain type that has a spoke counterpart gets a dedicated `From`/`Into` impl pair (one direction, or both as needed).
- The adapter crate re-exports the spoke wire type; product crates convert at the boundary.
- The conversion seam is the **sole point of evolution** when the spoke schema changes — product internals are insulated.

**Dual-path design for timeline ordering:** Both `InMemoryNarrativeGateway` and `SqliteNarrativeGateway` now expose an inherent `get_timeline_ordered()` method (adopted from `order_timeline_events_by_ids`, which was test-only in V1.142). The `sequence_no` default sort is retained as the data-collection fallback — the spoke ordering is applied when timeline IDs are available. This allows both gateways to serve ordered timelines without coupling to the spoke ordering API at the call site.

### Structural-mismatch discovery: spoke wire types are NOT uniform (high-value lesson)

This is the most important V1.143 learning for V1.144 planning:

**`KnowledgeEntry` carries `revision: Option<u64>`** — this enables `orchestrate_upsert`'s create-or-update OCC fork: the orchestrator calls `get_knowledge_entry`, reads the stored revision, feeds it as `expected_base_revision` to `put_knowledge_entry`, and the port implementation rejects stale/conflicting writes. This is what makes the V1.142/V1.143 cutovers work.

**`Relation` has NO `revision` field** — spoke's `RelationPort` defines `get_relations()` / `put_relations()` / `remove_relations()` but `Relation` carries no OCC field. The `orchestrate_relate` entrypoint expects the caller to supply an `expected_base_revision` per relation, but there is no stored revision to compare against. The production `RelationPort` in `nexus-local-db` is currently insert-only (no CAS gate).

**Consequence:** the OCC-mirror pattern from `KnowledgeEntry` does **not** transfer to `Relation`. Each port family's orchestrability must be verified against the spoke wire type's actual fields, not assumed. This is why `relate` is deferred to V1.144 — it needs a first-class adapter-extension plan (e.g. nexus-side `revision` column in `kb_relationships`, surfaced through the port impl, not through the spoke `Relation` type itself).

> **Rule:** before cutting over a port family to its orchestrator, check whether the spoke wire type carries a `revision` (or other OCC-relevant field). If it doesn't, the cutover may require nexus-side extension rather than a pure spoke adoption.

### C1 accepted-behavior-diff pattern

When V1.143 cut over `patch_entity` → `orchestrate_upsert`, a latent behavior change surfaced: **spoke's `upsert` marks merged-terminal KnowledgeEntries (e.g. `merged-away`) as `Rejected`** — the orchestrator validates the entry's status against spoke's lifecycle and rejects entries that are in terminal states not eligible for update. The existing nexus `patch_entity` handler did **not** enforce this check for the `merged` status.

The decision was to **accept the behavior diff** (C1 finding: `merged`-terminal entries now return an error when patched, rather than silently updating) because:
1. The spoke lifecycle is the normative authority — keeping nexus' laxer behavior would be incorrect.
2. The regression test was updated to assert the new behavior.
3. The diff was documented in the review bundle and spec note.

**Pattern:** when a cutover adopts spoke lifecycle validation that is stricter than the legacy nexus behavior, decide deliberately whether to accept the diff or add a nexus override. Document + regression-test either way. Do NOT silently change behavior.

## V1.144 — Deep SPOKE integration (roadmap step 2 of 3)

### Structural-mismatch resolution: spoke 0.5.0 closes the `revision` gap

The V1.143 `R-V1143P2-DEFER-RELATE` finding — spoke 0.4.1's `Relation` type lacked a `revision` field, blocking CAS-based `orchestrate_relate` — is **resolved at the protocol level** by spoke 0.5.0:

- `Relation` now carries `revision: Option<u64>` (additive, optional for backward compat).
- `RelationPort` gains `get_relation(id) → Option<Relation>` + `put_relation(relation, expected_base_revision: Option<u64>)` — the OCC-aware trait methods that the V1.143 structural-mismatch analysis identified as necessary.
- `orchestrate_relate` deep OCC uses `expected_base_revision` internally, with `RelationAlreadyExists`/`RelationNotFound` reject codes added.
- The nexus `kb_relationships` table already had a `revision` column (V1.74) — no migration needed.

**Lesson reinforced:** the V1.143 "verify spoke wire fields before claiming clean mapping" discipline caught the gap before cutover; the upstream fix (0.5.0) unblocked the relate cutover without any nexus-side schema change. The discipline is now a repeatable pre-cutover checklist item.

### Production cutover count: 3 paths, all shipped

The orchestrator-cutover-on-write-paths pattern is now proven across **all three** storage-backed entity write families:

| Cutover | Version | Status | Notes |
|---------|---------|--------|-------|
| `promote_adopt` → `orchestrate_promote` | V1.142 | **Shipped** | First Surface B production consumer; single-TX adopt flow with retry-safe idempotency |
| `patch_entity` → `orchestrate_upsert` | V1.143 | **Shipped** | Second production consumer; C1 merged-terminal behavior-diff accepted and regression-tested |
| `patch_relationship_add/update` → `orchestrate_relate` | V1.144 | **Shipped** | Third consumer; `remove` stays Surface A; `patch_relationship` handler routes add/update through `orchestrate_relate` |

With three cutovers shipped, the pattern is clearly reproducible. The emerging next-refactor target is reject-mapper DRY — all four ports share the same `SpokeRejectCode → NexusApiError` mapping logic, currently duplicated per port (see `R-V1144P2-INVALIDINPUT-400` below).

### OCC port-extension pattern (insert-only → OCC-aware)

V1.144 P1 upgraded the production `RelationPort` from V1.142's insert-only implementation to OCC-aware, following a repeatable pattern:

1. **Reuse existing column:** `kb_relationships.revision` already existed (V1.74) — no migration needed.
2. **Spoke convention seeds revision=1:** new relations start at revision 1 (not 0), matching spoke's internal seed convention.
3. **CAS guard on write:** `UPDATE ... WHERE revision = ?` — if the stored revision doesn't match `expected_base_revision`, the SQLite `changes()` count is 0 → `StoredRevisionStale`.
4. **Spoke reject codes:** map `RelationAlreadyExists` → 409; `StoredRevisionStale` → 409 with current version in response; `RelationNotFound` → 404.
5. **Backward compat:** the insert-only code path from V1.142 is preserved for callers that don't supply `expected_base_revision` (legacy Surface A consumers).

This pattern is generic: any `*Port` that starts with insert-only storage and later needs OCC can follow the same steps — verify the spoke wire type now carries a revision field, check an existing `kb_*` revision column (or add one), add the CAS guard, and map reject codes.

### Known gaps

- **`R-V1144P1-001` — `extensions.nexus` keys don't round-trip on Relation:** unknown `extensions.nexus` keys are silently dropped when a Relation is stored and re-read, because there is no `extras` JSON column for `kb_relationships` (unlike `kb_key_blocks` which stores full body fidelity via the V1.143 Greptile P1 fix). The `extras` column pattern from `kb_key_blocks` would be the model for a future fix.
- **`R-V1144P2-INVALIDINPUT-400` — no spoke 500-class reject code:** storage errors (e.g. SQLite constraint failures) are currently classified as `INVALID_INPUT` (400) because spoke's `RejectCode` enum has no server-error / 500-class variant. This misclassification is shared across all four cutover ports (promote, upsert, relate, remove). Tracked for a future iteration when spoke adds the reject code or nexus implements a local mapping fallback.

## V1.145 — Spoke consumer alignment (adapter归位 + scope.extensions + timeline port production)

V1.145 (XL) shipped the adapter topology correction that the V1.141–V1.144 roadmap had been building toward. Three structural changes (P1a/P1b rehome, P2 scope-pushdown redo, P3 timeline port production) plus a P4 docs-correction pass.

### Adapter rehome + dep reversal (P1a/P1b)

The conversion seam and production adapter (`NexusAdapter`, V1.146 rename) both moved to `nexus-spoke-adapter`, reversing the dependency graph:

**Conversion seam (P1a):** The `WorldKbEntry↔KnowledgeEntry` `From` impls were in `nexus-knowledge`. They needed to move to `nexus-spoke-adapter` so the adapter owns the sole boundary. But the **orphan rule** (E0117) prevented implementing `From<WorldKbEntry>` for `KnowledgeEntry` (or vice versa) in a third crate where both types are foreign. Solution: **free functions** `world_kb_to_spoke` / `spoke_to_world_kb` (the compiler's own suggestion — `rustc --explain E0117` explicitly recommends free functions over orphan-compatible newtypes) + a local **trait** `WorldKbEntrySpokeExt` for lifecycle methods (`confirm`/`deprecate`/`merge_into`/`delete`). This is the **proven pattern** for conversion seams across crates: when orphan rule blocks `From`/`Into`, use a free function pair + local trait on the product type.

**Production adapter (P1b):** `NexusAdapter` (V1.146 rename) + 6 baseline port impls moved from `nexus-local-db/src/spoke_adapter/` to `nexus-spoke-adapter/src/adapter/`. The adapter crate becomes the **capability aggregation** layer — it depends on `nexus-local-db` storage primitives (`SqliteKbStore`, `open_pool`, `run_migrations`), `sqlx`, and `tokio`, but the port families are adapter-owned.

**`nexus-local-db`** becomes **pure storage**: no `nexus-spoke-adapter` dep, no spoke types in its public API. It still depends on `nexus-narrative` (for `TimelineEvent` type) and `nexus-knowledge` (for `WorldKbEntry` type), but these are domain-type deps, not adapter deps.

**Import path change (V1.145→V1.146 rename):** `nexus_spoke_adapter::NexusAdapter` — V1.145 P1b rehomed the adapter from `nexus-local-db` to `nexus-spoke-adapter` (intermediate path: `nexus_spoke_adapter::adapter::NexusAdapter`); V1.146 renamed the struct and upgraded it to `FullAdapter`.

**Dep graph reversal (before → after):**

```
Before V1.145:
  nexus-local-db (hosted adapter, depended on spoke-adapter)
    → nexus-spoke-adapter
    → nexus-knowledge (hosted conversion seam)

After V1.145:
  nexus-spoke-adapter (capability aggregation, hosts adapter + conversion seam)
    → nexus-local-db (pure storage, no spoke-adapter dep)
    → nexus-knowledge
```

**Cycle break:** The reversal created a transitive `local-db → narrative-gateway → spoke-operations (via order_timeline_events_by_ids) → ... → spoke-adapter → local-db` cycle. Resolved by having `narrative_gateway` (local-db) and `gateway` (nexus-narrative) take a **direct** `spoke-operations` dep for the `order_timeline_events_by_ids` helper. P4 reframed this from "temporary cycle-break workaround" to **legitimate standard-leaf-library usage**: `spoke-operations` is a leaf crate with zero nexus transitive edges, so there is no cycle. The narrative-read-via-spoke-adapter goal remains deferred to V1.146.

### Scope.extensions (spoke 0.6.0) — P2 typed-carrier → native extensions redo

The original architect scope-pushdown design (§7.5) called for nexus-specific KB query filters (text_search, canonical_name, limit, offset, computable) to ride `scope.extensions["nexus"]`. But **spoke 0.5.0 had no `Scope.extensions` field** — the spec was based on an incorrect assumption about the published spoke version.

P2 shipped a **typed `KbScopeFilters` carrier** as a workaround: a nexus-only struct that carried the 5 nexus filters alongside a spoke `Scope` (which carried only native fields like `entry_types`). This worked but introduced a parallel carrier path — every conversion site wrapped/unwrapped the extra struct.

**spoke 0.6.0** (bumped mid-iteration, commit `c641a99a`) added `Scope.extensions: ExtensionMap` — the gap was real, and the upstream fix closed it. P2 **redid** the mechanism in commit `77f91067`: removed `KbScopeFilters` + `SqliteKbStore::query_scoped` entirely and rebuilt the query path on spoke-native `scope.extensions["nexus"]` (looked up via `ScopeExtensionsKey`). `NexusAdapter::list_knowledge_entries_scoped` reads the scope, reconstructs `KbQuery` from extensions, and delegates to `SqliteKbStore::query` — byte-identical output proven by comparison test.

**Lesson:** When spoke has a gap, the best-practice fix is **upstream** (advance spoke), not nexus workarounds. The typed carrier was the correct work-for-today, but the 0.6.0 upgrade made it obsolete within the same iteration. This reinforces the V1.143 "verify spoke source before claiming clean mapping" discipline multiply: spec authors must check the **actual published spoke version** for the fields they depend on.

### Timeline port production (P3)

`ScopeQueryPort::list_timeline_events` was a stub returning `Ok(Vec::new())` — the V1.142 spec claimed "nexus-narrative holds events in-memory" as justification. **This was incorrect.** Timeline data IS persisted in `narrative_timeline_events` (V1.26 migration `20260524_narrative_worlds.sql`). The port just never queried it.

P3 (commit `4ccaf8bd`) replaced the stub with a production implementation that:
1. Queries `narrative_timeline_events` by `world_id` (from `scope.scope_id`), `branch_id` (via `scope.extensions["nexus"]["branch_id"]`), and `timeline_event_ids` (from `scope.timeline_event_ids`).
2. Converts each nexus `TimelineEvent` row to the spoke `TimelineEvent` wire type via the V1.143 `From<nexus_narrative::TimelineEvent>` conversion seam.
3. Adds an additive column `extensions_nexus_json TEXT` (migration `20260729_000001`) for future spoke-round-trip write paths.
4. Replaces the single stub-returns-empty test with 4 integration tests (unfiltered, branch filter, event-ids filter, empty-world).

No new table and no write migration — the SSOT for timeline data already lived in `narrative_timeline_events`.

### Narrative-read deferral (P4)

P4 was re-scoped to docs-only (commit `63037b4f`). Key outcomes:

1. **Direct `spoke-operations` deps accepted as standard library usage.** All "TEMP P4" / "cycle-break" annotations in source comments, Cargo descriptions, and AGENTS.md were corrected to "standard spoke-operations library usage".
2. **Spec §7.4/§8 corrected** to match the post-reversal dep graph (architect commit `67c45629`).
3. **Narrative-read-via-spoke-adapter deferred to V1.146** (`R-V1145P4-001`). The `get_timeline_ordered` ordering helper is called from `narrative_gateway` (local-db, pure storage) and `gateway` (nexus-narrative), neither of which can depend on `nexus-spoke-adapter` post-P1b reversal without re-introducing the reversed edge. The ordering logic needs to move to a spoke-adapter-dependent layer (or be called from the daemon-runtime handler layer instead). Until then, the direct `spoke-operations` dep is the correct intermediate.

### The 3 architect Phase-1 errors caught at implement time

V1.145 surfaced three spec-vs-reality gaps during implement (not QC) — all caught because the implementer read the spoke source to verify. This reinforces the V1.143 "verify spoke source before claiming clean mapping" discipline as a **required pre-implement step**.

| Error | Spec claimed | Reality at implement time | Was caught when | Fix |
|-------|-------------|---------------------------|-----------------|-----|
| **P0 — non-existent fn** | An opaque-JSON INSERT primitive existed for `kb_key_blocks` | No such primitive — spoke-adapter built `extensions_nexus_json` inline at the INSERT site (the UPDATE path had one; INSERT did not) | T1: implementer found no call target | P0 added `insert_key_block_with_extensions_in_tx` to `SqliteKbStore` |
| **P1 — orphan rule + cycle** | `From` impls move cleanly to spoke-adapter; dep graph reversal is a pure file move | Orphan rule (E0117) blocks `From<WorldKbEntry> for KnowledgeEntry` in a third crate; reversal creates `local-db→narrative→spoke-adapter→local-db` cycle | T1: `rustc` rejects orphan impls; T2: `cargo tree` reveals cycle | Free functions + local trait for conversion seam; direct `spoke-operations` leaf dep for timeline ordering helper |
| **P2 — Scope.extensions fiction** | `Scope.extensions` exists at spoke 0.5.0, enabling native scope-pushdown via `extensions["nexus"]` | spoke 0.5.0 Scope has **no** `extensions` field — field was added in spoke 0.6.0 | P2 T1: implementer reads spoke `Scope` source | Typed carrier workaround (P2) → spoke 0.6.0 upgrade (mid-iteration) → redo on native extensions (P2 redo) |

**Pattern:** architect spec errors that survive QC only surface at implement time when the implementer reads the actual spoke crate source at the pinned version. Before assuming a spoke field, function, or enum variant exists, **grep the spoke crate** at the `Cargo.lock`-resolved version. The V1.1139/V1.143 lesson ("verify before claiming") is now codified as a mandatory pre-implement step for any spec that asserts a spoke behavior.
