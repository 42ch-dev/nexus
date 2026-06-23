# World Delta Propose / Apply — Local Parity Overlay

**Status**: Master (V1.60 P-last promotion)
**Document class**: Feature line (promoted from Draft overlay V1.60 P0)
**Coordinates with**: [`acp-capability-set.md`](acp-capability-set.md) §4 + §8, [`capability-registry.md`](capability-registry.md), [`entity-scope-model.md`](entity-scope-model.md) §5, [`orchestration-engine.md`](orchestration-engine.md) §5

## 0. Purpose and scope

This overlay defines the local-runtime contract for the three world-mutation
`nexus.*` capabilities shipped in V1.60 P0 (Track A, DF-46 local parity):

- `nexus.world.state.query` — read KB/timeline slices for a world
- `nexus.world.delta.propose` — produce a structured proposed delta package
- `nexus.world.delta.apply` — apply staged deltas locally under policy

It also pins the admission/policy semantics referenced by the two sibling
capabilities shipped in the same batch:

- `nexus.timeline.event.append` — immutable timeline append
- `nexus.fork.create` — explicit local timeline branch

**This overlay resolves `acp-capability-set.md` §8 Open Item line 223** ("Decide
whether `world.delta.apply` is agent-side or runtime-side by default"). The
decision is recorded in §3 below.

**Scope boundary (PD-01).** All five capabilities are **local-runtime, single
creator** operations against the workspace `state.db`. They are **not** the
platform community/social fork or shared-canon promotion governed by PD-01.
`nexus.fork.create` is local timeline branching (a new `branch_id` within an
existing world owned by the caller); platform community fork remains
platform-only.

## 1. Storage baseline (no new migrations)

V1.60 P0 adds **no new database migrations**. All five capabilities operate on
existing tables:

| Table | Columns used | Owner module |
| --- | --- | --- |
| `narrative_worlds` | `world_id`, `workspace_id`, `owner_creator_id`, `status`, `canon_revision`, `current_timeline_head_id`, `root_fork_branch_id`, `metadata_json` | `nexus-local-db` |
| `narrative_timeline_events` | `timeline_event_id`, `world_id`, `branch_id`, `event_type`, `status` (`canon`/`provisional`/`rejected`), `sequence_no`, `title`, `summary`, `metadata_json` | `nexus-local-db` |
| `kb_key_blocks` | `key_block_id`, `world_id`, `block_type`, `canonical_name`, `status`, `revision`, `body_json`, `source_anchor_json` | `nexus-local-db` |

Reused DAO / gateway surfaces (already shipped, no V1.60 changes):

- `nexus_local_db::narrative_gateway::SqliteNarrativeGateway` — read paths:
  `get_world_state`, `get_timeline`, `get_event`, `list_worlds`.
- `nexus_local_db::narrative_write::{create_world, append_event}` — write paths
  with sequence-conflict detection and FK validation.
- `nexus_local_db::kb_store::SqliteKbStore` (implements `nexus_kb::KbStore`) —
  `list_by_world`, `query`, `get_key_block`, `update_key_block`,
  `insert_key_block`, `insert_key_block_in_tx`.

Because the data layer is complete, each capability handler is a thin
admission-gated wrapper — **L effort, no fallback triggered** (compass §0.1 Q4).

## 2. Delta package structure

`nexus.world.delta.propose` produces (and `nexus.world.delta.apply` consumes) a
**delta package**: a serializable JSON object describing a set of proposed
changes against a world's KB / state, plus the policy context required to
evaluate them.

```jsonc
{
  "schema_version": 1,
  "policy_context": {
    "world_id": "wld_…",
    "creator_id": "ctr_…",
    "source_work_id": "wrk_local",
    "workspace_slug": "default"
  },
  "proposed_changes": [
    {
      "entity": "kb_key_block",
      "entity_id": "kb_…",
      "field": "body_json",
      "old_value": null,
      "new_value": { "category": "character", "summary": "…" },
      "rationale": "Agent-synthesized character block from chapter 3 draft."
    }
  ],
  "atomic": true
}
```

Field semantics:

- `schema_version` — integer; `1` for V1.60.
- `policy_context.world_id` — target world; must exist and be owned by
  `creator_id` (see §4).
- `policy_context.source_work_id` — the work/workspace the delta originated
  from. Default `"wrk_local"` for local-only sessions.
- `proposed_changes[].entity` — one of `kb_key_block` (create/update) or
  `world_metadata` (title/rules update). V1.60 ships `kb_key_block` create +
  update + `world_metadata` title update; other fields are validated but
  rejected as `unsupported_field` until a later version.
- `proposed_changes[].field` — the column being changed (`body_json`,
  `canonical_name`, `title`, …).
- `old_value` / `new_value` — JSON values. `propose` fills `old_value` from the
  current row so `apply` can detect lost-update drift (compares `old_value` to
  the live row before writing).
- `rationale` — free text, mandatory; surfaced in the audit log entry emitted
  by `apply`.
- `atomic` — boolean; when `true` (the V1.60 default), `apply` commits all
  changes in a single transaction or none.

## 3. Agent-side vs runtime-side (resolves acp §8 line 223)

**Decision: `world.delta.apply` is runtime-side.**

| Concern | Agent-side (rejected) | Runtime-side (chosen) |
| --- | --- | --- |
| Atomicity | Agent cannot guarantee multi-row tx | Runtime wraps changes in one sqlx tx |
| Policy enforcement | Trusts agent to self-check ownership | Runtime enforces `owner_creator_id` match |
| Audit trail | Best-effort, agent-authored | Runtime emits structured audit row |
| Workspace lock | None (race risk) | Runtime serializes per-world applies |

The split is:

- `nexus.world.delta.propose` — **agent-facing**. The agent (or a preset graph)
  calls it with an input changeset; it returns a validated delta package with
  `old_value` populated from current state. It performs **no writes**.
- `nexus.world.delta.apply` — **runtime-facing**. Takes a delta package,
  re-checks world ownership, verifies each `old_value` still matches (lost-update
  guard), then applies all changes inside a single transaction and emits an
  audit entry. Returns a per-change result list.

This mirrors the V1.58 P1 `nexus.reference.refresh` pattern: the orchestration
handler owns policy + atomicity; the agent only supplies intent.

## 4. Policy gates

All five capabilities enforce **creator isolation** via world ownership.

**Ownership gate** (replicated inline in each handler, mirroring
`ensure_world_accessible_for_creator` from the daemon host-tool layer — the
orchestration crate cannot depend on the daemon-runtime crate, so the check is
duplicated as a one-line `sqlx::query_scalar`):

```sql
SELECT world_id FROM narrative_worlds
WHERE world_id = ? AND owner_creator_id = ?
```

- Missing row → `CapabilityError::Forbidden("world not owned by creator")`.
- DB error → `CapabilityError::Internal`.

**Workspace binding.** `narrative_worlds.workspace_id` is consulted for
diagnostics only in V1.60 (the local workspace is the single writer); it is
recorded in `policy_context.workspace_slug` for audit. Cross-workspace apply is
not possible locally (single `state.db` per workspace).

**Canon immutability (invariant from `acp-capability-set.md` §6).**

- `timeline.event.append` never rewrites an existing `event_id` and never mutates
  a `canon`-status row. New events are inserted with `status = 'provisional'`.
  Canonization is a separate, later flow (not in V1.60 scope).
- `fork.create` never edits past events; it creates a new `branch_id` and the
  first event on that branch records the fork point.

## 5. Atomicity contract (delta.apply)

1. Begin a sqlx transaction.
2. Re-verify world ownership inside the tx (TOCTOU guard).
3. For each `proposed_change`: load the live row; assert
   `live_value == proposed.old_value` (lost-update guard); on mismatch, roll
   back and return `Conflict` for that change with the live value.
4. Apply all writes (`update_key_block` / `insert_key_block_in_tx`) inside the
   same tx.
5. Commit. On commit failure, roll back and return `TransientExternal`.
6. Emit one audit entry per applied change into the delta-package result
   (V1.60 records audit in the capability output JSON; a dedicated audit table
   is deferred — pre-1.0 local-first).

`atomic: false` is accepted on input but, in V1.60, the handler still applies
the whole package transactionally and sets `atomic_applied: true` in the output.
Per-change partial commit is a post-1.0 feature.

## 6. Per-capability I/O summary

Full JSON Schemas live in the Rust handler `input_schema()` / `output_schema()`
constants (the runtime SSOT). This section is the human-readable summary.

### 6.1 `nexus.world.state.query`

- **Input**: `{ world_id, slice?: "kb" | "timeline" | "all", limit?, branch_id? }`
- **Output**: `{ world_id, world, kb_blocks: [...], timeline: [...], generated_at }`
- **Gate**: world ownership. Returns the joined KB + timeline snapshot for
  reasoning.

### 6.2 `nexus.world.delta.propose`

- **Input**: `{ world_id, changeset: [{ entity, entity_id?, field, new_value, rationale }] }`
- **Output**: `{ schema_version, policy_context, proposed_changes: [...], atomic }`
- **Gate**: world ownership. **No writes.** Populates `old_value` per change.

### 6.3 `nexus.world.delta.apply`

- **Input**: `{ schema_version, policy_context, proposed_changes: [...], atomic }`
  (the delta package from `propose`, possibly agent-edited).
- **Output**: `{ applied: [{ entity, entity_id, status: "applied"|"conflict", live_value? }], atomic_applied }`
- **Gate**: world ownership (re-checked in tx) + lost-update guard.

### 6.4 `nexus.timeline.event.append`

- **Input**: `{ world_id, branch_id, event_type, title?, summary? }`
- **Output**: `{ event_id, sequence_no, status: "provisional", created_at }`
- **Gate**: world ownership. Rejects duplicate `event_id` and any attempt to set
  `status = "canon"` (canon is append-only-via-later-flow).

### 6.5 `nexus.fork.create`

- **Input**: `{ world_id, parent_branch_id, forked_from_event_id, label? }`
- **Output**: `{ branch_id, parent_branch_id, forked_from_event_id, created_at }`
- **Gate**: world ownership. Generates a new `fbk_*` branch id; does **not** copy
  events (lazy fork — the new branch is established by its first appended event).
- **PD-01 boundary**: this is local timeline branching only; community/social
  fork remains platform-only.

## 7. Promotion path

This Draft overlay is promoted to Master at V1.60 P-last T4 **if and only if**
Track A ships all five capabilities (no Q4 fallback). If the Q4 fallback
triggered (3-of-5 path), this overlay stays Draft and the world-delta sections
are re-scoped in V1.61.
