---
module: schemas + crates/nexus-contracts + packages/nexus-contracts + crates/nexus-daemon-runtime
date: 2026-07-18
problem_type: convention
category: conventions
severity: medium
plan_id: "2026-07-18-v1.122-timeline-first-canvas (V1.122 P1; compound of timeline-canvas-architecture.md §9)"
tags: [wire-contracts, verification, codegen, schemas, additive-frontend, regression]
applies_when:
  - "Verifying that an additive-frontend iteration produced no wire-contract drift"
  - "Running a `wire_contracts_changed: false` gate before marking a plan Done"
  - "Reviewing a PR that claims no schema/codegen/daemon changes"
---

# Wire Contracts Frozen Verification (8-Point Gate)

**Track**: Knowledge (durable guidance, distilled from V1.122 P1 verification gate).

## Context

Additive-frontend iterations (those that add a new `CanvasSurfaceKind` value, new adapter, or new UI components without changing the backend) must be verified as `wire_contracts_changed: false`. Without a systematic verification gate, it is easy to:

- Accidentally edit a schema file while adding a new derived type.
- Regenerate codegen output that introduces drift.
- Bump the `@42ch/nexus-contracts` npm version unintentionally.
- Add a daemon HTTP route registration or handler change.

The V1.122 P1 Timeline-first Canvas plan established an 8-point verification gate that covers all dimensions of wire-contract drift. This doc makes it a reusable convention.

## Guidance

### The 8-Point Verification Gate

Run these commands against the plan branch (compared to the iteration base branch, e.g., `iteration/v1.122`):

| # | Command | Expected | What it catches |
|---|---------|----------|-----------------|
| 1 | `git diff --stat schemas/` | **empty** | New schema files or edits to existing schemas |
| 2 | `git diff --stat crates/nexus-contracts/` | **empty** (or only codegen-regenerated changes from unchanged schemas) | Rust generated type changes, hand-written DTO changes |
| 3 | `git diff --stat packages/nexus-contracts/` | **empty** (or only codegen-regenerated changes from unchanged schemas) | npm TypeScript generated type changes |
| 4 | `git diff --stat crates/nexus-daemon-runtime/src/api/` | **empty** | New HTTP route registrations, handler additions, router wiring changes |
| 5 | `pnpm run codegen` on a clean checkout -> `git status` under `**/generated/` | **no untracked, no modified** | Codegen drift — regenerated output that differs from committed state |
| 6 | `jq '.version' packages/nexus-contracts/package.json` | matches pre-iteration version | Accidental npm version bump |
| 7 | Search `schemas/` for the new surface kind name (e.g., `rg -n '"timeline"' schemas/`) | only existing pre-shipped entries present | New schema entries for the frontend-only kind |
| 8 | Search `schemas/` for the frontend-only enum (e.g., `rg -n 'CanvasSurfaceKind' schemas/`) | **empty** | Schema drift — enum promoted to schemas/ when it should stay frontend-only |

### When to Run

- **Before marking plan Done:** run the full 8-point gate as the final step.
- **On every additive-frontend iteration:** even if the scope seems small, run all 8 checks.
- **After rebasing:** if the plan branch was rebased onto a new base, re-run the gate.

### Failure Protocol

If any sub-step fails:

1. **STOP** — do not mark the plan Done.
2. **Escalate to architect** — the drift may be intentional (e.g., a necessary schema fix) or accidental (e.g., a stray `git add`).
3. **Document the failure** — record the failing command output in the plan's `## Review Gate Summary`.
4. **Fix or justify** — either revert the drift (if accidental) or add a new schema entry (if intentional) with a plan amendment.

### Recording Results

Record the pass/fail status of all 8 checks in the plan's `## Review Gate Summary`:

```markdown
**wire_contracts_changed: false verification:**
| # | Check | Result |
|---|-------|--------|
| 1 | schemas/ diff empty | PASS |
| 2 | crates/nexus-contracts/ diff empty | PASS |
| 3 | packages/nexus-contracts/ diff empty | PASS |
| 4 | daemon api/ diff empty | PASS |
| 5 | codegen clean | PASS |
| 6 | npm version unchanged | PASS |
| 7 | no new timeline schema entries | PASS |
| 8 | CanvasSurfaceKind not in schemas | PASS |
```

## Why This Matters

- **Prevents silent wire-contract drift** — a single accidental schema edit can break the `@42ch/nexus-contracts` npm package for Platform consumers.
- **Systematic, not ad-hoc** — the 8-point gate is a complete checklist. Without it, developers might check only `git diff --stat schemas/` and miss a codegen drift or version bump.
- **Auditable** — the pass/fail table in the plan provides a permanent record that the `wire_contracts_changed: false` claim was verified.

## When to Apply

- Closing any additive-frontend iteration (no new schemas, no daemon Rust changes, frontend-only).
- Verifying a PR that claims `wire_contracts_changed: false` before merging.
- Training new contributors on the verification protocol for additive-frontend work.

## Relationship to Schema Boundary Policy

This verification gate is complementary to the `schemas-external-consumer-boundary.md` policy. The boundary policy defines **what belongs in `schemas/`** (the rule); this gate defines **how to verify nothing leaked into `schemas/`** (the verification). The two documents should be used together:

- **Boundary policy** -> read before deciding whether a new type belongs in `schemas/`.
- **Verification gate** -> run before marking an additive-frontend plan Done.