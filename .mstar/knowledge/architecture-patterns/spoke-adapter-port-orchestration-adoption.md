---
module: nexus-spoke-adapter
date: 2026-07-28
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - spoke
  - adapter
  - baseline-ports
  - orchestration
  - surface-b
  - cas
  - occ
  - call-boundary-invariant
last_updated: 2026-10-14
applies_when:
  - Consuming the SPOKE adapter-port + injection-orchestration architecture from a product adapter boundary (Surface B adoption); deciding between pure delegates (Surface A) and port+orchestrator (Surface B); verifying CAS through orchestrators; enforcing call-boundary invariant mechanically
---

# SPOKE Adapter Port + Orchestration Adoption (Surface B)

Companion to [`spoke-adapter-conversion-seam.md`](spoke-adapter-conversion-seam.md) (Surface A). Read both: Surface A and Surface B coexist on the same adapter boundary.

## Context

The spoke crate exposes **capability-sliced adapter port traits** (`KnowledgeEntryPort`, `RelationPort`, `ScopeQueryPort`, `FindingPort`, `RuleQueryPort`, `HostManifestPort`; optional `ComputablePort` + `ForkTimelineQueryPort`) and **injection orchestration entrypoints** (`orchestrate_upsert` / `_promote` / `_relate` / `_check` / `_assemble` / `_project` / `_compute` / `_fork_check` / `_fork_assemble`). Orchestrators compose the pure helpers with port I/O — they sequence validate → load stored → apply → put-with-CAS in one call, with the adapter (product) owning persistence behind the port traits.

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
- **CAS discipline:** `put_knowledge_entry(entry, expected_base_revision)` is the OCC contract. Adapters that back this with real compare-and-put (e.g. the `kb_key_blocks` SQLite CAS pattern) get true concurrent safety; adapters that ignore `expected_base_revision` are defective.

## When to Apply

- Consuming SPOKE from a product adapter boundary
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

A common mistake: collapse both mismatches into `STORED_REVISION_STALE`. They are distinct codes — spoke's `assert_revision_match` emits them in opposite directions; tests must verify both.

## Common pitfalls

- **Reimplementing spoke invariants in the mock** — violates call-boundary §7. The mock does storage I/O only; orchestrators own lifecycle composition.
- **A static check that is too strict** — `assert extensions.rs has zero spoke_operations references` fails because `ExtensionMap` is defined in `spoke-operations` (not `spoke-schemas`) and is a wire type, not a lifecycle operation. The correct static check forbids `spoke_operations::[a-z_][a-z0-9_]*\(` (function-call pattern) while admitting type imports.
- **Pulling `nexus-local-db` into the adapter crate** to implement a "real" `KnowledgeEntryPort` — violates the dependency graph (spec §8: adapter stays `spoke-schemas` + `spoke-operations` only). Concrete product port impls live downstream (e.g. `nexus-knowledge`, which already depends on both).
- **Testing CAS by calling the mock directly** — proves the mock's CAS works, not that the orchestrator path works. Drive rejects through `orchestrate_*`.

## Production boundary

The production adapter (`NexusAdapter`) lives in `nexus-spoke-adapter/src/adapter/` and implements all 6 baseline port families against SQLite storage:

| Family | Status | Backing |
|--------|--------|---------|
| `KnowledgeEntryPort` | **Production (CAS)** | `kb_key_blocks` via the CAS path; both `STORED_REVISION_STALE` + `REVISION_CONFLICT` directions |
| `RelationPort` | **Production (OCC)** | `kb_relationships` (revision column) |
| `FindingPort` | **Production (batch tx)** | `findings` (batch wrapped in an explicit transaction) |
| `ScopeQueryPort.list_knowledge_entries` | **Production** | `kb_key_blocks` by `world_id` |
| `ScopeQueryPort.list_timeline_events` | **Production** | `narrative_timeline_events` by world/branch/event ids |
| `RuleQueryPort` | **Stub (empty)** | no persisted spoke `Rule`; rules come from works config |
| `HostManifestPort.list_peer_host_capability_manifests` | **Production** | `peer_hosts` (workspace DB); outbound-observation recording — dialer records the dialed peer at `connect()` return; `connect dial` / `connect peers list` CLI |

**HostManifestPort** follows the honesty contract: `list_peer_host_capability_manifests` returns **only observed peers** — the dialer records the dialed peer into the workspace-DB `peer_hosts` table at `connect()` return — never fabricated entries. Inbound-only peers are **not recorded**: spoke-connect exposes no hook to observe inbound peers (documented gap, out of scope).

### Production adapter patterns

1. **Wire conversion reuse:** the production `KnowledgeEntryPort` reuses the `WorldKbEntry ↔ SpokeKnowledgeEntry` `From` impls (single seam). `Relation` / `Finding` map inline at the boundary (no parallel types) with vocabulary mapping (spoke ↔ nexus severity/status) + `INVALID_INPUT` reject on unknown values.
2. **CAS mapping per spec §7.4 (6 outcomes):** `stored > expected → STORED_REVISION_STALE`; `stored < expected → REVISION_CONFLICT`; `absent + Some(_) → RevisionConflict` — the spec production table is authority, not the mock.
3. **Async→sync bridge:** `tokio::task::block_in_place` wraps async sqlx behind the sync port trait methods. Requires a multi-threaded tokio runtime; `debug_assert!` on `Handle::runtime_flavor() == MultiThread` at adapter construction catches wrong-flavor early in dev/CI. The *Connect invoke* path (a different surface — sync handler on the spoke-connect event loop) uses a bounded executor (Semaphore + spawn_blocking + deadline + response cap) instead — see [bounded-sync-async-bridge-event-loop.md](bounded-sync-async-bridge-event-loop.md); the port-trait bridge above remains for the daemon/CLI paths.
4. **Batch transaction:** `FindingPort.put_findings` wraps the batch loop in an explicit SQLite transaction (`pool.begin() → loop → tx.commit()`) so mid-batch failure rolls back the whole batch.
5. **Per-request construction:** `NexusAdapter::new(pool.clone())` is cheap (pool handle clone); construct per request, not per application.

### Orchestrator cutover on write paths

The production orchestrator consumers are `promote_adopt` (`crates/nexus-core/src/world_kb.rs`, routed through `orchestrate_promote(&NexusAdapter, PromoteRequest)`), `patch_entity` (`orchestrate_upsert`), and `patch_relationship` add/update (`orchestrate_relate`; `remove` stays Surface A). Key patterns:

1. **`orchestrate_promote` handles `stored = None`:** the orchestrator can promote a NEW candidate (not just an existing provisional entry). It skips stored-entry terminal-status checks when `stored = None`, validates `candidate.status == "provisional"`, applies acceptance (`→ confirmed`, revision bump), and calls `put_knowledge_entry(None)` (create path). This means the nexus adopt flow (create + confirm in one step) maps cleanly: build candidate with `status = "provisional"`, let the orchestrator flip it.
2. **Single-transaction adopt:** `promote_adopt` binds a handler-owned `sqlx::Transaction` into `NexusAdapter` via a shared `Arc<std::sync::Mutex<Option<Transaction>>>` for the synchronous `orchestrate_promote` bridge, then flips the extract job in the same transaction before one `COMMIT`. Rollback on any failure — no soft-delete compensation.
3. **Retry-safe idempotency:** when a prior attempt committed but returned an error (commit-ack ambiguity), retry hits `KnowledgeEntryAlreadyExists` → handler checks job status → returns success if already confirmed and attributed.
4. **SpokeRejectCode → `CoreError` mapping:** OCC codes → 409 with re-read version; validation codes → 422; remaining → 500. Preserve existing error semantics.

### Cross-domain conversion seams

Each product domain type that has a spoke counterpart gets a dedicated conversion seam. When the orphan rule (E0117) blocks a `From`/`Into` impl in the adapter crate (both types foreign), use **free functions** (`world_kb_to_spoke` / `spoke_to_world_kb`, the compiler's own `rustc --explain E0117` suggestion) plus a local **trait** (`WorldKbEntrySpokeExt` for lifecycle methods `confirm`/`deprecate`/`merge_into`/`delete`). The adapter crate re-exports the spoke wire type; product crates convert at the boundary. The conversion seam is the **sole point of evolution** when the spoke schema changes — product internals are insulated.

**Dual-path design for timeline ordering:** both `InMemoryNarrativeGateway` and `SqliteNarrativeGateway` expose an inherent `get_timeline_ordered()` method. The `sequence_no` default sort is retained as the data-collection fallback — the spoke ordering is applied when timeline IDs are available, so both gateways serve ordered timelines without coupling to the spoke ordering API at the call site.

### Structural mismatch: spoke wire types are not uniform

Before cutting over a port family to its orchestrator, check whether the spoke wire type carries a `revision` (or other OCC-relevant field).

- **`KnowledgeEntry` carries `revision: Option<u64>`** — this enables `orchestrate_upsert`'s create-or-update OCC fork: the orchestrator calls `get_knowledge_entry`, reads the stored revision, feeds it as `expected_base_revision` to `put_knowledge_entry`, and the port implementation rejects stale/conflicting writes.
- A wire type **without** an OCC field (e.g. `Relation` before spoke 0.5.0) cannot be CAS-cut-over as-is; the cutover needs a first-class adapter extension (e.g. a nexus-side `revision` column surfaced through the port impl, not the spoke type itself).

**Rule:** the OCC-mirror pattern does **not** transfer blindly across port families. Verify each port family's orchestrability against the spoke wire type's actual fields — never assume.

### Accepted behavior diffs

When a cutover adopts spoke lifecycle validation that is stricter than the legacy nexus behavior (e.g. spoke's `upsert` marks merged-terminal KnowledgeEntries `Rejected`, so patching a `merged` entry now errors instead of silently updating), decide deliberately whether to accept the diff or add a nexus override. Accepting is preferred when the spoke lifecycle is the normative authority. Document the diff in the review bundle and update the regression test either way. Do **not** silently change behavior.

### Relation OCC (spoke 0.5.0+)

Spoke 0.5.0 closed the `Relation` OCC gap at the protocol level: `Relation` carries `revision: Option<u64>`; `RelationPort` gains `get_relation(id) → Option<Relation>` + `put_relation(relation, expected_base_revision: Option<u64>)`; `orchestrate_relate` uses `expected_base_revision` internally with `RelationAlreadyExists`/`RelationNotFound` reject codes. The nexus `kb_relationships` table already had a `revision` column — no migration was needed.

The upgrade from an insert-only port to OCC-aware follows a repeatable pattern:

1. **Reuse the existing column** (or add one) on the `kb_*` table.
2. **Seed revision on insert** to match spoke's convention (relations start at revision 1).
3. **CAS guard on write:** `UPDATE … WHERE revision = ?` — a mismatch leaves the SQLite `changes()` count at 0 → `StoredRevisionStale`.
4. **Map spoke reject codes:** `RelationAlreadyExists` → 409; `StoredRevisionStale` → 409 with the current version; `RelationNotFound` → 404.
5. **Keep the insert-only path** for callers that don't supply `expected_base_revision` (legacy Surface A consumers).

### Known gaps

- **`extensions.nexus` keys don't round-trip on `Relation`:** unknown `extensions.nexus` keys are silently dropped when a Relation is stored and re-read, because there is no `extras` JSON column for `kb_relationships` (unlike `kb_key_blocks`, which stores full body fidelity). The `extras` column pattern from `kb_key_blocks` is the model for a future fix.

### Error classification

Spoke exposes a server-error reject code (`InternalError`, HTTP 500). Genuine storage/internal failures map to it — corrupt or unreadable stored rows and SQLite failures are `InternalError`, never skipped and never reclassified as client input. Validation, not-found and revision/conflict conditions keep their own distinct reject codes (`INVALID_INPUT`, `*NotFound`, `STORED_REVISION_STALE` / `REVISION_CONFLICT` / `*AlreadyExists`). The per-port CAS/reject matrix is owned by [`spoke-adapter-architecture.md`](../../specs/active/architecture/spoke-adapter-architecture.md).

## Adapter topology

`nexus-spoke-adapter` is the **capability aggregation** layer: it hosts the conversion seam and the production adapter, and depends on `nexus-local-db` storage primitives (`SqliteKbStore`, `open_pool`, `run_migrations`), `sqlx`, and `tokio`. `nexus-local-db` stays **pure storage** — no `nexus-spoke-adapter` dep, no spoke types in its public API (it does depend on `nexus-narrative` / `nexus-knowledge` for domain types). Timeline-ordering helpers that live in the pure-storage layer take a direct `spoke-operations` dep (a leaf crate with zero nexus transitive edges — legitimate standard-library usage, not a cycle-break workaround).

**Verify spoke source before claiming a field exists.** Spec prose that asserts a spoke field, function or enum variant can be wrong — check the actual published spoke crate at the `Cargo.lock`-resolved version before assuming it exists. This is a mandatory pre-implement step for any spec that asserts a spoke behavior: architect-level errors that survive QC only surface at implement time when the implementer reads the spoke source.
