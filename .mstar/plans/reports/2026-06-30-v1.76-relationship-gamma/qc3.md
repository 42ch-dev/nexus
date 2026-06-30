---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-30-v1.76-relationship-gamma"
verdict: "Request Changes"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk (extraction flooding, GET graph filter/index behavior, codegen determinism, confidence-edge render cost, Vite chunking reliability)
- Report Timestamp: 2026-06-30T12:20:00Z

## Scope
- plan_id: `2026-06-30-v1.76-relationship-gamma` (lead; covers P0 + P1)
- Review range / Diff basis: `aadefa0e41..bb35a8fedf` (origin/main merge-base..iteration/v1.76 HEAD; 21 commits). Equivalent to `git diff aadefa0e41..bb35a8fedf`.
- Working branch (verified): `iteration/v1.76`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- HEAD note: checkout HEAD at review time was `cbd93c02` because `qc2.md` was already committed after assigned tip `bb35a8fe`; the reviewed implementation range remained exactly `aadefa0e41..bb35a8fedf`.
- Files reviewed: 50 changed files in the assigned range; focused on the migration/indexes, GET graph handler/storage query, extraction relationship persistence, confidence-weighted rendering, Suggested pane, and `apps/web/vite.config.ts` manual chunks.
- Commit range: exact assigned diff `aadefa0e41..bb35a8fedf`.
- Tools run: `git diff aadefa0e41..bb35a8fedf`, `pnpm run codegen`, `git diff --exit-code -- crates/nexus-contracts/src/generated packages/nexus-contracts/src/generated packages/nexus-contracts/dist tooling/codegen/dist`, `pnpm --filter web build`, `pnpm --filter web test -- --run`, `pnpm run validate-schemas`, `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test world_kb_relationships`, `SQLX_OFFLINE=true cargo test -p nexus-orchestration relationship --lib`.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **F-QC3-001 — Web canvas bypasses the `needs_review` default-exclude gate and renders suggested edges by default.** The server default correctly hides `needs_review=1` rows when `include_suggested` is absent, but the web hook always calls `getWorldKbGraph(worldId, { includeSuggested: true })`. `WorldKbCanvas` then derives React Flow edges from all returned relationships, and its threshold filter explicitly keeps suggested edges visible. There is no graph-mode Suggested toggle gating whether suggested rows are fetched/rendered. This violates the compass §6.1/§7 flooding mitigation: extraction suggestions are supposed to be hidden by default with only a Suggested count/triage affordance visible until opt-in. With many suggestions, the local web surface fetches and renders the flood path on every canvas load. -> **Fix**: make the default graph query omit `include_suggested`; fetch suggestions only for an explicit Suggested pane/toggle or keep them out of React Flow until opted in. If the toolbar needs a count, use a lightweight count/list path rather than rendering all suggested edges by default. Evidence: `apps/web/src/lib/canvas/use-world-kb-data.ts:53-64`, `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:81-100`, `apps/web/src/components/canvas/world-kb/relationship-projection.ts:51-87`.
- **F-QC3-002 — Backend GET graph filter is applied after fetching all relationship rows, so the new `(world_id, needs_review)` index does not protect the default graph path from suggestion floods.** `project_relationships_for_world` calls `list_relationships_for_world(pool, world_id)`, which runs `WHERE world_id = ? ORDER BY updated_at DESC`; only after all rows are allocated does Rust skip `row.needs_review != 0`. The migration creates `idx_kb_relationships_world_id_needs_review`, but the default GET query does not push `needs_review = 0` into SQL, so SQLite cannot use that index for the assigned default-exclude performance requirement. Existing data remains behaviorally unaffected, but a world with many extraction suggestions still pays DB IO + deserialization/allocation cost for hidden rows. -> **Fix**: add a filtered storage query (or parameterize `list_relationships_for_world`) so default GET uses `WHERE world_id = ? AND needs_review = 0`, while `include_suggested=true` intentionally fetches both confirmed and suggested rows. Evidence: migration `crates/nexus-local-db/migrations/202606300001_kb_relationships_needs_review.sql:19-21`; storage query `crates/nexus-local-db/src/kb_relationships.rs:340-365`; post-fetch filter `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:915-939`.
- **F-QC3-003 — Confidence threshold slider displays 0.00–1.00 but compares 0–100 against confidence values in 0–1.** `confidenceThreshold` is stored from the range input as `0, 5, 10, ... 100`; the label divides by 100, but filtering uses `data.confidence >= threshold` directly. At the first non-zero UI step (label `0.05`, internal value `5`), every confirmed edge with confidence in `[0,1]` is hidden. This makes the confidence filter unreliable and can make users believe high-confidence confirmed edges disappeared. -> **Fix**: compare against `confidenceThreshold / 100` (or store threshold as 0.0–1.0 and render the input accordingly) and add a unit/component regression for threshold filtering. Evidence: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:76-96` and slider label/value wiring at `274-293`.

### 🟢 Suggestion
- **S-QC3-001 — Bulk promote sends one mutation per visible suggestion without throttling or batching.** `onPromoteAllSuggestions` loops over all visible rows and calls the same mutation for each. This is acceptable for the current per-pass extraction cap, but accumulated suggestions across rescans can still create a burst of concurrent requests and repeated graph invalidations. Consider sequential throttling or a future bulk-promote API if suggestion volumes grow. Evidence: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:211-215`.

## Key Checks (per assignment)

- **Flooding mitigation / needs_review default-exclude**: Partially failed. Backend semantics are correct for callers that omit `include_suggested` (existing relationships default to `needs_review=0`, and `include_suggested=true` opts in), but the web canvas always opts in and renders suggested edges by default (F-QC3-001). Backend also filters after fetch, so the new index does not protect the default SQL path (F-QC3-002).
- **Migration/indexes**: Existing data behavior is safe: migration adds `needs_review INTEGER NOT NULL DEFAULT 0` and `source TEXT NOT NULL DEFAULT 'manual' CHECK (...)`, so existing rows remain visible in the default graph. The `(world_id, needs_review)` index is present but currently underused by the default graph query because the filter is post-fetch.
- **GET graph default-filter behavior change**: Existing relationships are unaffected by migration defaults. New extraction suggestions are excluded only for callers that do not set `include_suggested=true`; the web client currently always sets it.
- **Codegen determinism (0.12.0)**: Passed. Re-running `pnpm run codegen` produced zero diff in generated TypeScript/Rust outputs and package/codegen dist artifacts.
- **B9 manualChunks build reliability**: Passed. `pnpm --filter web build` completed successfully; largest emitted chunk was `tiptap-CXIgA64u.js` at 437.82 kB, below Vite's 500 kB warning threshold; no chunk-size warning was emitted.
- **Confidence-weighting render performance**: The band calculation is pure and memoized via `useMemo` over `relationships` and threshold, so there is no per-frame recompute. However, the threshold comparison bug makes the filter unreliable (F-QC3-003), and the web fetch/render path currently includes suggested edges by default (F-QC3-001).
- **Extraction relationship persistence cap/dedup**: `MAX_RELATIONSHIPS_PER_PASS = 20` and idempotent upsert limit per-pass flooding/re-scan duplication. Accumulated suggestions remain possible over time; the default graph path should therefore avoid fetching/rendering suggestions until requested.

## Source Trace
- Finding ID: F-QC3-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `apps/web/src/lib/canvas/use-world-kb-data.ts:53-64`; `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:81-100`; compass §6.1 / §7 DoD #2
  - Confidence: High
- Finding ID: F-QC3-002
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus-local-db/src/kb_relationships.rs:340-365`; `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:915-939`; migration index `202606300001_kb_relationships_needs_review.sql:19-21`
  - Confidence: High
- Finding ID: F-QC3-003
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:76-96` and `274-293`
  - Confidence: High
- Finding ID: S-QC3-001
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:211-215`
  - Confidence: Medium
- Validation Evidence:
  - `pnpm run codegen` passed; generated outputs/dists had zero diff by targeted `git diff --exit-code`.
  - `pnpm --filter web build` passed with no Vite chunk-size warning; largest chunk 437.82 kB.
  - `pnpm --filter web test -- --run` passed: 37 files / 254 tests.
  - `pnpm run validate-schemas` passed: 170 valid / 0 invalid.
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test world_kb_relationships` passed: 16/16.
  - `SQLX_OFFLINE=true cargo test -p nexus-orchestration relationship --lib` passed: 5/5.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

The verification suite is green and B9/codegen are reliable, but the flooding/performance gate is not met: the web canvas opts into and renders suggested relationships by default, the backend default filter does not use the new index because it filters after fetching all rows, and the confidence threshold slider compares mismatched units. These should be fixed before approval.
