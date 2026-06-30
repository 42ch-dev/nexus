---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-30-v1.76-relationship-gamma"
verdict: "Request Changes"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence + maintainability risk
- Report Timestamp: 2026-06-30

## Scope
- plan_id: `2026-06-30-v1.76-relationship-gamma` (lead; consolidated covers P0 relationship-gamma + P1 slate-clear)
- Review range / Diff basis: `aadefa0e41..bb35a8fedf` (origin/main merge-base..iteration/v1.76 HEAD; 21 commits). Equivalent to `git diff aadefa0e41..bb35a8fedf`.
- Working branch (verified): `iteration/v1.76`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 50 changed (2765 insertions / 197 deletions)
- Commit range: aadefa0e41..bb35a8fedf (HEAD `bb35a8fe` — merge: P1 slate-clear into iteration/v1.76)
- Tools run:
  - `git diff aadefa0e41..bb35a8fedf --stat`
  - `cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings` → exit 0
  - `cargo build -p nexus-daemon-runtime --tests` → **FAIL** (see F-001)
  - `cargo test -p nexus-daemon-runtime --test world_kb_relationships` → 16/16 passed
  - `cargo test -p nexus-daemon-runtime --test world_kb_patch` → **FAIL** (compilation error)
  - `cargo test -p nexus-orchestration` → 964 passed + integration targets passed
  - `cargo test -p nexus-local-db` → 285+ tests passed (incl. `kb_relationships` unit tests)
  - `pnpm --filter web typecheck` → exit 0
  - `pnpm --filter web test -- relationship-confidence` → 12/12 passed
  - `pnpm --filter web test -- chapter-outline-content-editor` → 18/18 passed (incl. B2 transition test)
  - `pnpm --filter nexus-codegen typecheck` → exit 0
  - `pnpm --filter @42ch/nexus-contracts run build` → exit 0

## Architecture / Maintainability Findings

### 🔴 Critical

#### F-001 — Test target `world_kb_patch.rs` no longer compiles after the GET graph signature change
- **Evidence**: `cargo build -p nexus-daemon-runtime --tests` exits with
  `error[E0061]: this function takes 3 arguments but 2 arguments were supplied` at
  `crates/nexus-daemon-runtime/tests/world_kb_patch.rs:508` (test
  `get_graph_returns_non_deleted_entities`):
  ```rust
  let Json(resp) = get_graph(State(state.clone()), Path("wld_test_world".to_string()))
      .await
      .expect("graph should succeed");
  ```
  The V1.76 handler signature is now `pub async fn get_graph(State, Path(world_id), Query<GraphQuery>)`
  (`crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:834`). The sibling
  test `world_kb_relationships.rs` was updated to pass `Query(GraphQuery { include_suggested: None })`
  in 3 call sites (lines 472, 842, 858, 946), but `world_kb_patch.rs` was not.
- **Impact**: `cargo test -p nexus-daemon-runtime --test world_kb_patch` is a hard
  CI gate failure. The full test gate (`cargo test --all` in the `Rust test` CI
  job) will fail at the test-target compile step before any assertions run.
  `world_kb_patch.rs` was touched (i.e., read/edited) by V1.76's
  `RelationshipInput.needs_review` addition (lines 111, 202, 424, 728 of the
  test), so the maintainer had the file in their working set and should have
  caught the `get_graph` call-site change. The omission is a missed
  file-disjointness claim — the GET-filter change (`Query<GraphQuery>`) was
  not actually disjoint from the P1 backend test path.
- **Fix**: add `Query(GraphQuery { include_suggested: None })` (or `Some(false)`)
  to the call at `world_kb_patch.rs:508`, matching the pattern already used in
  `world_kb_relationships.rs:472`. One-line change; do it as a targeted
  fix-up. Per the QC role contract this report does NOT modify code — only
  documents the finding.

### 🟡 Warning

#### F-002 — `LlmExtractOutcome::Candidates` now carries a `relationships` field, but `LlmExtractTask::evaluate` does not consume it
- **Evidence**: `crates/nexus-orchestration/src/quality_loop.rs:645-654` defines:
  ```rust
  pub(crate) enum LlmExtractOutcome {
      Candidates {
          candidates: Vec<KbCandidate>,
          /// V1.76: relationship candidates proposed by the LLM. ...
          relationships: Vec<KbRelationshipCandidate>,
      },
      ...
  }
  ```
  All four task-level call sites in `tasks/mod.rs` destructure with
  `Candidates { candidates: c, .. }` (lines 2869, 2937, 2960, 2999), silently
  dropping the new field. The pre-existing `R-V152TA-S003` allows
  `LlmExtractTask::evaluate` to be `#[cfg_attr(not(test), allow(dead_code))]`
  because production preset routing does not yet use it — but with V1.76 the
  task also no longer "exposes" the relationship output to its caller.
- **Architectural concern**: this is correct architecturally — the architect
  lock says `LlmExtractTask` is a pure parser/invoker, persistence belongs to
  the review-time hook (`extract_kb_candidates_for_review` →
  `persist_relationship_candidates`). The `relationships` field IS consumed
  there (`quality_loop.rs:499-503`). So the wiring is sound end-to-end.
  The concern is purely API surface: a future preset author wiring a custom
  state machine that calls `LlmExtractTask` directly will not see
  relationships come back, and may need to call `run_llm_extract` directly to
  access them. Document this explicitly on `LlmExtractTask` (one-line doc
  tweak) so the next reader doesn't think the task silently dropped the
  field.
- **Severity rationale**: Warning, not Critical — the production wiring is
  correct, and the `R-V152TA-S003` dead-code allow is a known limitation of
  the existing scaffolding. The risk is only discoverability for future
  preset authors.

#### F-003 — `useWorldKbGraph` always fetches both confirmed + suggested, coupling payload size to world size
- **Evidence**: `apps/web/src/lib/canvas/use-world-kb-data.ts:59-67`:
  ```ts
  queryFn: async () => client.getWorldKbGraph(worldId!, { includeSuggested: true }),
  ```
  The comment justifies this as "single fetch with client-side filtering,
  avoiding a refetch on toggle". The companion client builds the query string
  only when `includeSuggested` is truthy (`browser-client.ts:327-334`), which
  is consistent. However, the Suggested tab is **off by default**, so most
  non-administrative work sessions ship suggested rows over the wire even when
  they are not visible.
- **Architectural concern**: the compasses' §A3 explicitly mandates the
  suggested toggle surfacing all `needs_review=1` rows in the Suggested pane,
  so this is intentional. For very large worlds with thousands of extraction
  suggestions, the always-fetch pattern increases payload size and re-fetch
  cost. The threshold filter is also client-only, so server-side pagination
  is not engaged even for large worlds. The `V1.76 relationship graph
  pagination` TODO at `world_kb.rs:882-887` acknowledges this is a known
  follow-up.
- **Severity rationale**: Warning, not Critical — the V1.76 scope explicitly
  caps via `GRAPH_ENTITY_CAP = 500` for entities but not for relationships
  (the TODO above). For pre-1.0 local-first data sets this is fine; a V1.77+
  residual should add pagination. The current design is consistent with the
  locked decisions in §A3+A5.

### 🟢 Suggestion

#### F-004 — `run_llm_extract` always parses relationships even when the fallback heuristic path is taken
- **Evidence**: `quality_loop.rs:733-741` runs `parse_relationships` regardless
  of pathway (LLM vs heuristic). On the heuristic path the LLM never runs,
  so `relationships` is empty (`(extract_candidates_from_text(&ctx.prose), Vec::new())`
  at line 508), making the parse work a no-op. Per-line cost is small, but
  the parser still allocates the empty Vec. Could short-circuit on
  `LlmExtractOutcome::WorkerUnavailable` / `CapabilityError` paths.
- **Severity rationale**: Suggestion only — no observable cost, no correctness
  impact. Skip unless adjacent perf work touches this hot path.

#### F-005 — Suggested pane `onPromoteAll` issues N concurrent patch mutations without batching
- **Evidence**: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:211-215`:
  ```ts
  const onPromoteAllSuggestions = (rels: WorldKbRelationshipProjection[]) => {
    for (const rel of rels) {
      onPromoteSuggestion(rel);
    }
  };
  ```
  Each `onPromoteSuggestion` invokes `patchRelationship.mutate(...)` with
  `bumpReseed()` on success. For N suggestions, this fires N mutations, each
  invalidating the graph query, plus N `bumpReseed` signals that reset local
  node state. Under network latency this could race the local React state
  N times. The UI is protected because mutations are queued by TanStack, but
  each success invalidates `queryKeys.worldKb.graph`, so the graph may
  refetch mid-batch.
- **Severity rationale**: Suggestion — UX-level concern, no data loss. The
  TanStack mutation cache serializes writes; a future iteration could add a
  bulk-promote backend route and a single mutation. Out of scope for V1.76.

#### F-006 — B2 state-transition test covers three of the four forced-reset paths (chapter-switch + contentVersion share the same `forcedReset` boolean)
- **Evidence**: `chapter-outline-content-editor.tsx:132-147` defines
  `forcedReset = chapterSwitched || contentVersion !== prevContentVersion.current`.
  The new state-transition test (`chapter-outline-content-editor.state.test.tsx:183-194`)
  asserts the chapter-switch path but does not separately assert the
  `contentVersion` bump path. The orchestrator in `outline-canvas.tsx` only
  bumps `contentVersion` on conflict "Use current" (per inline comment at
  `chapter-outline-content-editor.tsx:65-67`), so the contentVersion path
  is harder to reach in production. Both branches share the same boolean so
  the semantics are equivalent, but the test labels suggest asymmetry.
- **Severity rationale**: Suggestion — the equivalence is documented inline
  and architecturally correct (commit 1f0c614c removed `contentVersion` bumps
  on ordinary patches). Adding one assertion (`it('resets to clean on explicit
  contentVersion bump')`) would make the asymmetry explicit. Low value.

### B3 regression check (V1.75 content-loss invariants)

The compass explicitly asks whether B3 (state-machine simplification)
regresses the V1.75 content-loss invariants (dirty/saving guard +
forcedReset + `isConflicting→dirty`). Verified against the new code at
`apps/web/src/components/canvas/outline-canvas/inspectors/chapter-outline-content-editor.tsx`:

- **Dirty/saving guard preserved**: line 138 — `if (!forcedReset && (saveState === 'dirty' || saveState === 'saving')) return;`
  guards both ordinary refetches and contentVersion bumps. ✓
- **Forced-reset sources unified**: line 135 —
  `const forcedReset = chapterSwitched || contentVersion !== prevContentVersion.current;`
  folds the two previous reset paths (chapter-switch + contentVersion) into
  a single effect. ✓
- **Settle effect preserves draft on 409**: line 157 —
  `setSaveState(isConflicting ? 'dirty' : 'clean');` keeps the draft
  intact while the conflict modal is open AND after dismissal. ✓
- **`useChapterOutline` has no `placeholderData`** (per inline comment line 120-122) — a
  chapter switch flips `isLoading` and the component renders its loading state
  until fresh data arrives; the forced reset then runs against the fresh
  content. ✓

**No regression detected.** The B2 transition test (`R-V175QC1-S003`) covers
the dirty/saving/conflict paths and now passes (4/4 tests in
`chapter-outline-content-editor.state.test.tsx`). Confidence that the B3
simplification did not silently drop a content-loss invariant: high.

### B1 chapter-inspector split check

- `chapter-inspector.tsx`: **232 lines** (was 248). Headroom under the 250
  cap. ✓
- Split into:
  - `chapter-inspector-utils.ts` (57 lines, pure `buildChapterPatchSet`
    helper — unit-testable, no React).
  - `chapter-meta-field.tsx` (23 lines, `INPUT_CLASS` token + `MetaField`
    presentational wrapper — no behavior).
- Clean separation of concerns: orchestrator in main file, pure logic in
  utils, presentational primitives in meta-field. ✓
- `BUILD_CHAPTER_PATCH_SET` is a hermetic pure function and should be unit-
  tested in isolation, but the diff does not add a `chapter-inspector-utils.test.ts`
  sibling. The transitions are exercised end-to-end by the B2 state-transition
  test, so coverage is functionally adequate for V1.76; suggest adding a
  hermetic test in V1.77+.

### Module boundary & coherence

The extraction pipeline extension coheres cleanly with the existing
entity-extraction architecture:

1. **Pure layer** — `LlmExtractTask::evaluate` (`tasks/mod.rs:570-604`) and
   `LlmExtract::run` (`capability/builtins/llm_extract.rs:123-184`) remain
   pure parse-and-invoke. Neither persists. The LLM capability grows an
   optional `relationships` array alongside the existing required
   `candidates` array (backward compatible per `llm-extract.md` §1.2).
2. **Shared pathway** — `run_llm_extract` (`quality_loop.rs:678-747`) is
   the single shared helper that returns
   `LlmExtractOutcome::Candidates { candidates, relationships }`. Both the
   review-time hook and `LlmExtractTask::evaluate` route through it.
3. **Persistence layer** — `persist_relationship_candidates`
   (`quality_loop.rs:1352-1431`) is the only caller of
   `upsert_extraction_relationship`. Entity-existence prerequisite
   (architect lock) is enforced inside the persist loop:
   `let (Some(source_id), Some(target_id)) = (source_id, target_id) else { skip + log }`.
   No second promotion state machine — promotion reuses
   `update_relationship_in_tx` with `needs_review=false`.
4. **Read-side gate** — `project_relationships_for_world`
   (`world_kb.rs:915-940`) applies the `include_suggested` filter at the
   storage query layer. Default is `needs_review = 0` (confirmed only);
   opt-in `?include_suggested=true` surfaces both. Symmetric-reverse
   derivation is unchanged — the filter runs before the symmetric
   projection, so the architect's "stored + symmetric_reverse = 3"
   test assertion is preserved.
5. **Client-side rendering** — `deriveRelationshipEdges` (`relationship-projection.ts:51-88`)
   reads `rel.needs_review ?? false` and applies dashed stroke + `· suggested`
   label marker. Confidence banding lives in a dedicated module
   (`relationship-confidence.ts`) with stepped bands at 0.4 / 0.7 — exactly
   per compass Phase 2b product lock. The Suggested pane
   (`suggested-relationships-pane.tsx`) is a self-contained triage table
   that filters `projection_direction === 'stored'` to avoid double-
   counting symmetric reverses.

**Clean boundary: no suggested relationship leaks into the confirmed graph by
default.** The `needs_review` gate at the storage query (`row.needs_review != 0`)
plus the alt-view split (`confirmedRels` vs `suggestedRels` filtering on
`!r.needs_review`) plus the canvas's `confidenceThreshold` filter (which
explicitly passes `data?.needsReview` through) all preserve the confirmed-
vs-suggested separation.

### Wire contract & codegen coherence

- Schema + generated types + `@42ch/nexus-contracts` 0.11.0 → **0.12.0** are
  consistent. `WorldKbRelationshipProjection` gains required `needs_review:
  bool` + `source: 'manual' | 'extraction'`. `WorldKbRelationshipInput`
  gains optional `needs_review: bool` (defaults to false on add, preserves
  on update when omitted). The `GET graph` endpoint gains
  `?include_suggested=true` opt-in.
- `pnpm --filter @42ch/nexus-contracts run build` produces `dist/` cleanly.
- TS typecheck (`pnpm --filter web typecheck`) is green — the new
  `WorldKbEdgeData.needsReview?: boolean` and `source?: 'manual' | 'extraction'`
  fields (`apps/web/src/components/canvas/world-kb/types.ts:60-62`) match the
  generated `WorldKbRelationshipProjection` shape.

### Spec promotions

Six specs amended:
- `entity-scope-model.md` §5.6.7 — extraction-sourced suggested relationships
  (entity-existence prerequisite, gate semantics).
- `world-kb-runtime-architecture.md` — extraction pipeline extension.
- `web-ui.md` V1.76 stage — Suggested pane + confidence-weighting UX.
- `local-api-surface-conventions.md` — `include_suggested` + `needs_review`.
- `canvas-strategy-surface.md` — V1.76 γ shipped entry.
- `llm-extract.md` §1.2 + §5.2 — output schema extension + persist
  semantics.

All amendments match the implemented behavior; no drift detected between
specs and code.

### Scope creep check

Diff is **strictly within** the locked scope:

- P0 (relationship γ): extraction extension, needs_review gate, GET filter,
  promotion, confidence UX, tests, specs — all present.
- P1 (slate-clear): B1–B9 all present (B1 inspector split, B2 transition
  test, B3 state-machine simplification, B4 `#[allow]` restructure to
  `persist_chapter_outline_content` helper, B5 stale-PUT tokens cleaned, B6
  enum coercion promoted to `tracing::warn!(metric = "world_kb_relation_type_coercion", ...)`,
  B7 CAS note added, B8 two-file write comment added, B9 Vite manualChunks).
- **No out-of-scope changes** detected: no new extraction capabilities, no
  new canvas surfaces, no platform publish, no mobile, no entity promotion
  state machine clone.

The slate-clear B4 fix in `outline.rs` is the architecturally interesting
refactor — extracting `persist_chapter_outline_content` removes the
`#[allow(clippy::too_many_arguments, clippy::too_many_lines)]` from
`apply_chapter_patch` and reduces its signature from 8 to 7 args. The body-
ownership invariant (`this block writes ONLY to outline_path`) and the two-
file write ordering invariant (`outline file first, work-level revision
second`) are now documented in the new helper and in `apply_chapter_patch`'s
inline comment. Architecturally clean.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: **Request Changes**

The architecture and maintainability story for V1.76 is **strong**:

- The extraction pipeline extension is a textbook additive change — pure
  parser-invoker layer untouched, persistence layer composed from existing
  helpers, relationship upsert co-located with the existing entity upsert,
  one shared `run_llm_extract` pathway, no duplicated state machines.
- The `needs_review` gate + GET `?include_suggested` filter is a clean
  boundary — confirmed graph is the default; suggestions are opt-in by both
  the URL param and the alt-view tab split.
- The confidence-weighting state machine is appropriately scoped (3 stepped
  bands per PM lock) and lives in a dedicated module with hermetic unit
  tests.
- B1/B2/B3/B4 refactors are all small and reversible; B3 specifically does
  not regress the V1.75 content-loss invariants.
- All static checks pass (`cargo clippy`, `pnpm typecheck`, codegen).
- All non-`world_kb_patch` test targets pass.

The single blocker is **F-001**: the `world_kb_patch` integration test
target was not updated for the `Query<GraphQuery>` signature change on
`get_graph`, producing a hard compile error that will fail the CI test
gate. This is a one-line fix in the test (`Query(GraphQuery { include_suggested: None })`)
and matches the pattern already used in `world_kb_relationships.rs`. The
implementation behavior is correct — only the test call site is stale.

PM should dispatch a **targeted fix** to update the call site; no
implementation change is needed. After the fix, this reviewer is confident
the iteration can move from `InReview` to `Done` without further tri-review
(the change is mechanical and the architecture is sound).

## Source Trace
- F-001:
  - Source Type: `cargo test` compilation error + manual diff trace.
  - Source Reference: `crates/nexus-daemon-runtime/tests/world_kb_patch.rs:508`
    calls `get_graph(State, Path)` without the new `Query<GraphQuery>`
    third argument. Handler signature at
    `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:834`.
  - Confidence: High.
- F-002:
  - Source Type: `git diff` + manual inspection.
  - Source Reference: `crates/nexus-orchestration/src/quality_loop.rs:645-654`
    vs `crates/nexus-orchestration/src/tasks/mod.rs:2869,2937,2960,2999`.
  - Confidence: High.
- F-003:
  - Source Type: `git diff` + manual inspection.
  - Source Reference: `apps/web/src/lib/canvas/use-world-kb-data.ts:59-67`
    + `apps/web/src/lib/nexus/browser-client.ts:327-334`.
  - Confidence: High.
- F-004:
  - Source Type: `git diff` + manual inspection.
  - Source Reference: `crates/nexus-orchestration/src/quality_loop.rs:733-741`.
  - Confidence: Medium (suggestion only).
- F-005:
  - Source Type: `git diff` + manual inspection.
  - Source Reference: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:211-215`.
  - Confidence: Medium (suggestion only).
- F-006:
  - Source Type: `git diff` + manual inspection.
  - Source Reference: `apps/web/src/components/canvas/outline-canvas/inspectors/chapter-outline-content-editor.tsx:132-147`
    vs the new test at
    `apps/web/src/components/canvas/outline-canvas/inspectors/chapter-outline-content-editor.state.test.tsx`.
  - Confidence: Medium.