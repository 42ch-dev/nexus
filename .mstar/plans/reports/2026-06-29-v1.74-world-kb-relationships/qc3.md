---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-29-v1.74-world-kb-relationships"
verdict: "Approve"
generated_at: "2026-06-29"
---
# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-29T09:34:43Z

## Scope
- plan_id: `2026-06-29-v1.74-world-kb-relationships` (lead; consolidated review covers P0 world-kb-relationships + P1 hygiene-slate-clear + integration codegen)
- Review range / Diff basis: `0fed23f8..38cacda2` (origin/main merge-base..iteration/v1.74 HEAD; 26 commits). Equivalent to `git diff 0fed23f8..38cacda2`.
- Working branch (verified): `iteration/v1.74`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 112 changed files overall; focused review on the assigned migration/store/handler/virtualization files plus related tests, package manifest/lockfile, schema/codegen output, and P1 stability touchpoints.
- Commit range: `0fed23f8..38cacda2`
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD && git status --short`
  - `git diff --stat 0fed23f8..38cacda2`
  - `git diff --name-only 0fed23f8..38cacda2`
  - `git diff 0fed23f8..38cacda2 -- crates/nexus-local-db/migrations/202606290001_kb_relationships.sql crates/nexus-local-db/src/kb_relationships.rs crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs apps/web/src/components/canvas/outline-canvas/inspectors/structure-inspector.tsx`
  - `pnpm run codegen && git diff --exit-code -- crates/nexus-contracts/src/generated/ packages/nexus-contracts/src/generated/`
  - `pnpm --filter web build`
  - `pnpm --filter web test -- --run` (run twice)
  - `pnpm run validate-schemas`

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- S-QC3-001 — Consider a future graph-relationship cap/pagination path before large-world datasets become common. The current implementation is acceptable for V1.74 beta: relationship population is a single world-scoped query, not N+1; SQLite has the expected relationship indexes; symmetric projection is linear and emits at most one derived reverse edge per stored row. Still, unlike entities, relationships are not capped in `GET graph`; the compass already flags large-world graph-rendering pressure. If author worlds grow substantially, add a `GRAPH_RELATIONSHIP_CAP` or cursor/filtering contract in a later plan.

## Source Trace
- Finding ID: S-QC3-001
- Source Type: manual-reasoning + git-diff
- Source Reference:
  - `crates/nexus-local-db/migrations/202606290001_kb_relationships.sql:23-32` — indexes present: `(world_id)`, `(source_entity_id)`, `(target_entity_id)`, `(world_id, relation_type)`, plus `(world_id, source_entity_id, target_entity_id)`.
  - `crates/nexus-local-db/src/kb_relationships.rs:266-295` — `list_relationships_for_world` performs one `WHERE world_id = ? ORDER BY updated_at DESC` query.
  - `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:827-879` — `get_graph` fetches entities then calls `project_relationships_for_world` once for the graph response.
  - `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:884-904` — symmetric reverse projection loops over fetched rows and emits one optional reverse projection, preserving `relationship_id`.
  - `.mstar/iterations/v1.74-world-kb-relationships-and-hygiene-compass-v1.md:398-400` — known large-graph rendering pressure risk.
- Confidence: High

## Review Notes
- **Graph query performance**: `GET graph` does not N+1 over entities; relationship population is a single `list_relationships_for_world(pool, world_id)` call. The migration includes all four assignment-required indexes and the compass-requested `(world_id, source_entity_id, target_entity_id)` composite index.
- **Symmetric projection cost**: Projection is O(r) over stored relationship rows and bounded to at most 2 projections per row (`stored` + optional `symmetric_reverse`). The reverse projection keeps the same `relationship_id`, so edit/delete can target the single stored row.
- **B6 virtualization**: `react-window` `FixedSizeList` is used with fixed `CHAPTER_ROW_HEIGHT = 48`, bounded `MAX_LIST_HEIGHT = 384`, typed `ListChildComponentProps`, and no unbounded fallback render. The 120-row test confirms not all rows are mounted. No `ResizeObserver` or layout-thrash-prone measurement loop was introduced.
- **Codegen determinism**: Re-running `pnpm run codegen` produced no diff in committed generated TS/Rust output.
- **Dependency hygiene**: `react-window` is a runtime dependency of `apps/web` and `@types/react-window` is a dev dependency; `pnpm-lock.yaml` contains matching importer and package entries. No evidence of duplicate large runtime bloat from this addition.
- **Reliability of integrated HEAD**: Web build passed. Web tests passed twice with 223/223 tests both runs; observed stderr is limited to existing React Router future-flag and React `act(...)` test warnings, not test failures. Schema validation passed 171/171.
- **P1 B-stability spot checks**: The chapter page volume query default remains conservative (`undefined` unless a positive `?volume=N` exists), and the inspector save-trigger tests exercise the ref/replay guard path. No runtime regression surfaced in build/tests.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve
