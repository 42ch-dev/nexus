---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-30-v1.77-slate-clear"
verdict: "Request Changes"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-30

## Scope
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Working branch (verified)**: `iteration/v1.77`
- **Review range / Diff basis**: `git diff ba71d9167f6269cd0175b86f202baa3e19b517a6...a2571381b2a9865c6a98ffec461d4a99051a39f0` (10 implementation commits; merge-base `ba71d916` = origin/main, tip `a2571381` = the integration HEAD before QC report commits. NOTE: qc1/qc2 report commits may now sit atop `a2571381` — review the **implementation** diff `ba71d916..a2571381`, not QC report commits.)
- **plan_ids covered this round**: TWO —
  - P0: `2026-06-30-v1.77-findings-remediation-ui` (Track A lead, M)
  - P1: `2026-06-30-v1.77-slate-clear` (Track B companion, S-M)
- plan_id: 2026-06-30-v1.77-slate-clear
- Files reviewed: 25 implementation files in the assigned range; P1 focus on `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs`, `crates/nexus-local-db/src/kb_relationships.rs`, `crates/nexus-local-db/migrations/202606300001_kb_relationships_needs_review.sql`, `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx`, `apps/web/src/components/canvas/world-kb/suggested-relationships-pane.tsx`, and related data hooks/tests.
- Commit range: `ba71d916...a2571381` (implementation HEAD before QC report commits)
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git diff --stat ba71d916...a2571381`; `git diff --numstat ba71d916...a2571381`; targeted `read`/`grep`; `cargo test -p nexus-daemon-runtime --test world_kb_relationships`; `cargo test -p nexus-orchestration`; `pnpm --filter web build`.
- Deep review: triggered (S1: assigned implementation range is 29 files / 2018 insertions; S6: Rust daemon/local-db + web canvas mutation path; assignment-specific signals: truncation behavior and batched mutation reliability).
- Lenses applied: Performance Lens, Reliability Lens, Testing Lens.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning

- **W-QC3-P1-001 (medium, Warning)** — `GRAPH_RELATIONSHIP_CAP` is enforced after `list_relationships_for_world` has already fetched all matching relationship rows into memory. Both SQL branches end in `fetch_all(pool)` without `LIMIT`, and `project_relationships_for_world` then truncates in Rust with `.take(GRAPH_RELATIONSHIP_CAP)`. This means a large world still pays the full DB read, row decoding, allocation, and transfer cost before the safety cap applies, so the cap bounds only the final wire projection, not the handler hot path. The default branch does push `needs_review = 0` into SQL and has `(world_id, needs_review)`, but the cap itself is not pushed down.
  - Source Type: deep-lens: Performance Lens
  - Source Reference: `crates/nexus-local-db/src/kb_relationships.rs:346-410` (`fetch_all`, no `LIMIT`); `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:936-947` (`rows.into_iter().take(GRAPH_RELATIONSHIP_CAP)`); `crates/nexus-local-db/migrations/202606300001_kb_relationships_needs_review.sql:19-21` (filter index)
  - Confidence: High
  - Suggested fix: add a DAO-level limit (ideally `GRAPH_RELATIONSHIP_CAP + 1` to detect truncation) or a dedicated graph-query function that applies `WHERE ... ORDER BY ... LIMIT ?` in SQL. If `ORDER BY updated_at DESC` must stay efficient at scale, consider a composite index matching the filter and order (for example `(world_id, needs_review, updated_at DESC)` for the default path, plus an include-suggested strategy).

- **W-QC3-P1-002 (medium, Warning)** — Relationship graph truncation is silent. The code comments acknowledge a future `truncated` / `next_cursor` contract, but the current handler emits neither a response flag nor a log/metric when rows exceed `GRAPH_RELATIONSHIP_CAP`. Authors with large worlds can silently miss older confirmed/suggested relationships in the graph and operators have no observable signal that the cap is being hit. This is a reliability risk because the safety cap changes visible data without telemetry.
  - Source Type: deep-lens: Reliability Lens
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:891-900` (TODO notes future `truncated` flag); `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:942-955` (cap applied with no warning/metric); grep found no truncation-specific `warn!` in `world_kb.rs`
  - Confidence: High
  - Suggested fix: fetch `CAP + 1` rows, project only `CAP`, and emit at least a structured `tracing::warn!`/metric with `world_id`, `include_suggested`, `cap`, and observed row count when truncation occurs. A wire `truncated` flag can remain a future contract change, but local observability should land with the cap.

### 🟢 Suggestion

- **S-QC3-P1-001 (low, Suggestion)** — Bulk “Promote all” now bounds concurrency at five PATCHes per batch and continues after partial failures, which is good for reliability, but the UI has no progress indicator during long runs. `suggestionPending={patchRelationship.isPending}` only disables actions while a mutation is active; it does not show batch progress or “3/10 batches” feedback, and failures are only logged via `console.warn`. Consider adding lightweight progress/failure copy to the Suggested pane if large suggestion sets become common.
  - Source Type: deep-lens: Reliability Lens
  - Source Reference: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:223-263`, `apps/web/src/components/canvas/world-kb/suggested-relationships-pane.tsx:157-165`
  - Confidence: Medium

## Source Trace
- Finding ID: W-QC3-P1-001
- Source Type: deep-lens: Performance Lens
- Source Reference: `kb_relationships.rs:346-410`; `world_kb.rs:936-947`; migration `202606300001_kb_relationships_needs_review.sql:19-21`
- Confidence: High

- Finding ID: W-QC3-P1-002
- Source Type: deep-lens: Reliability Lens
- Source Reference: `world_kb.rs:891-900`, `world_kb.rs:942-955`, grep for `warn!`/truncation observability
- Confidence: High

- Finding ID: S-QC3-P1-001
- Source Type: deep-lens: Reliability Lens
- Source Reference: `world-kb-canvas.tsx:223-263`; `suggested-relationships-pane.tsx:157-165`
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Detailed Review Notes (qc3 lens)

### B2 graph cap performance
- The `needs_review` default filter is correctly pushed into SQL (`WHERE world_id = ? AND needs_review = 0`), and migration `202606300001_kb_relationships_needs_review.sql` adds `idx_kb_relationships_world_id_needs_review`.
- However, neither the include-suggested nor default query has `LIMIT`; both use `fetch_all(pool)`. The cap is applied only after all rows are materialized in Rust.
- The current index supports filtering by world + needs_review, but `ORDER BY updated_at DESC` is not covered by that two-column index. Large confirmed graphs may still sort many rows before the Rust cap applies.

### B2 truncation observability
- The implementation documents that a future response envelope should gain `truncated`/`next_cursor`, but current response shape remains unchanged.
- There is no structured log when truncation happens. The only `warn!` in `world_kb.rs` is for unknown relation type coercion, not cap truncation.
- Silent truncation should be fixed at least at the telemetry layer before this plan is approved.

### B3 batch performance and partial failure behavior
- `PROMOTE_BATCH_SIZE = 5` avoids the previous unbounded `Promise.allSettled(rels.map(...))` burst.
- The loop continues after a failed batch because each batch uses `Promise.allSettled` and the outer loop does not throw on rejected items. This preserves partial-success collection and makes batches 4-10 proceed even if batch 3 has failures.
- Recoverability is acceptable at the data layer: successful promotions clear `needs_review`; failed rows remain suggested and can be retried after the final `bumpReseed()`.
- UX observability is weaker: only `console.warn` reports failures, and there is no long-run progress feedback. This is recorded as a Suggestion rather than a blocker because the core partial-success semantics are recoverable.

### Verification evidence
- `cargo test -p nexus-daemon-runtime --test world_kb_relationships` — pass (16 tests).
- `cargo test -p nexus-orchestration` — pass (964 unit tests, integration suites pass; 3 doc tests ignored as existing).
- `pnpm --filter web build` — pass.

**Conclusion (qc3)**: B3's bounded batching is a reliability improvement, but B2 still fetches all graph relationship rows before truncating and gives no signal when truncation occurs. Those two cap-related issues should be fixed before approval from the performance/reliability lens.
