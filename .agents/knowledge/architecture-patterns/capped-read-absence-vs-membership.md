---
module: nexus-knowledge + nexus-core + apps/nexus-service
date: 2026-09-29
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
applies_when:
  - "A membership/existence check is answered by scanning a capped or paginated list read instead of a targeted single-key lookup"
  - "A validator treats 'not in the returned page' as proof of non-existence"
  - "Reviewing a referent check that shares its read path with a UI projection (graph/list endpoint)"
tags:
  - "membership"
  - "existence-check"
  - "capped-read"
  - "truncation"
  - "graph-read"
  - "referent-validation"
  - "world-kb"
  - "false-refusal"
---

# Capped Read Is Not Absence Proof: Membership Needs a Targeted Lookup

## Context

Plan 002 (v1.200, cross-surface Timeline event binding) validates a `bind_world_event`
referent by checking the event exists in the Work's stored bound World. The first
implementation reused `nexus-knowledge`'s `get_graph` — the same read the Canvas
graph projection uses — and treated "entity not in the returned set" as
non-existence. But `get_graph` is a **projection read capped at
`GRAPH_ENTITY_CAP = 500` non-deleted entities**: it answers "what does this World
look like on a canvas", not "does this one id exist".

## What Didn't Work

The L2 reviewer caught the shape, not a live failure: any World with more than
500 active graph rows silently refuses binds for events outside the capped
window, with a misleading `OutlineValidation: referent does not exist` message.
The implementer had disclosed the cap honestly (the plan's declared interface
named `get_graph`), but a disclosed truncation is still a correctness defect when
truncation is interpreted as absence.

## Solution

A cap-free, indexed single-row read that answers existence for exactly one id
over the same row set the projection walks:

```rust
// world_kb.rs — find_entity: no ceiling, same visibility rules as get_graph
pub async fn find_entity(pool, world_id, key_block_id)
    -> CoreResult<Option<WorldKbEntityProjection>>
// indexed get_knowledge_entry; owner_kind='world' + world_id match +
// status NOT IN (deleted, merged, deprecated)
```

`resolve_validated_binding` in `nexus-core/src/outline.rs` now calls it after
`require_world_owner` and before any mutation. The regression test creates 500
earlier rows to exhaust the capped window, then binds a referent the capped read
could never see — red before the fix, green after (`content_services.rs::timeline_world_event_binding_accepts_a_referent_past_the_graph_cap`).

## Why This Works

The projection and the membership question have **different completeness
contracts**. Reusing the projection's read for validation couples the validator's
correctness to an unrelated UX budget (the cap exists to bound canvas rendering).
A keyed lookup has no truncation to interpret, so "absent" regains its meaning.

## Prevention

- When a validator needs existence, ask for the **key**, not the **list** — even
  when a suitable-looking list read is already in scope.
- In review, flag any `contains`-style check whose source collection is capped,
  paginated, or lazily loaded: the burden of proof is on showing the check
  cannot observe truncation.
- If a plan's declared interface names the wrong read, treat the deviation as a
  required fix (as the L2 review did here), not a disclosed limitation — and
  register the residual so the decision is visible (`residual R1 → fix-now`).

Source: iteration v1.200 plan 002 (compound 2026-09-29).
