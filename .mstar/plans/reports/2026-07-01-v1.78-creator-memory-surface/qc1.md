---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-01-v1.78-creator-memory-surface"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (primary focus per role parameters)
- Report Timestamp: 2026-07-01T01:55:00Z

## Scope
- **plan_id**: `2026-07-01-v1.78-creator-memory-surface` (primary; this consolidated review covers the full V1.78 Wave 1 = P0 creator-memory surface + P1 slate-clear)
- **Review range / Diff basis**: `merge-base: 116296d0 (origin/main)` + `tip: 04a411c2 (iteration/v1.78 HEAD)` — equivalent to `git diff 116296d0...04a411c2`
- **Working branch (verified)**: `iteration/v1.78`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (matches `git rev-parse --show-toplevel`)
- **HEAD (verified)**: `04a411c22252d6f95de398fcf9a0162db6f8e688` (matches Assignment tip; later commits `3060fa87` (qc2) + `3e12ebc3` (qc3) only add this report's siblings)
- **Files reviewed**: 83 changed files in the assigned diff (heavy surface: 14 JSON Schemas + codegen barrel + `crates/nexus-contracts/src/generated/local_api/memory/*` + `crates/nexus-daemon-runtime/src/api/handlers/memory.rs` + `apps/web/src/pages/memory-page.tsx` + `apps/web/src/components/memory/*` + 5 `NexusClient` memory methods + DESIGN.md/DESIGN.dark.md tokens + slate-clear 11 residuals + tests)
- **Commit range**: 116296d0...04a411c2 (HEAD today is 3e12ebc3 but tips past the assigned range only contain qc2.md / qc3.md reports — out of implementation scope)
- **Deep review**: triggered (`{signals: [new contract surface, multi-module change: Rust handler + TS package + codegen, handler DTO normalization, frontend new page + 5 NexusClient methods + DESIGN tokens, slate-clear across queries.ts + finding-detail-panel.tsx + finding-inline-edit-form.tsx + world_kb.rs + suggested-relationships-pane.tsx + world-kb-canvas.tsx]}`) — ≥4 of 5 deep-review signals fire; applied lenses listed under "Deep Review Lenses Applied"
- **Tools run**:
  - `git rev-parse --show-toplevel`; `git branch --show-current`; `git rev-parse HEAD` (verified cwd/branch/HEAD)
  - `git merge-base 116296d0 HEAD`; `git log --oneline 116296d0..04a411c2` (commit history)
  - `git diff 116296d0...04a411c2 --stat` (83 files / 3667 insertions / 371 deletions)
  - `cargo clippy -p nexus-contracts -p nexus-daemon-runtime -- -D warnings` (clean — 0 errors; matches CI scope)
  - `pnpm --filter web typecheck` (clean after `pnpm --filter @42ch/nexus-contracts run build`; prebuild dependency noted in qc3 W-QC3-001 — independently confirmed)
  - `pnpm run validate-schemas` (all 184 schemas valid, including the 14 new memory ones)
  - `cargo test -p nexus-daemon-runtime --test memory_dto_roundtrip` (7/7 passed)
  - `cargo test -p nexus-contracts --test schema_drift_detection` (4/4 passed; verifies the 14 new strict-mode schemas are wired)
  - `cargo test -p nexus-daemon-runtime --test world_kb_relationships -- get_graph_truncates_relationships_at_cap` (1/1 passed; slate-clear regression)
  - `pnpm --filter web test -- --run memory` (8/8 passed; memory-page.test.tsx + memory-mutation.test.tsx)
  - `pnpm --filter web test -- --run finding` (25/25 passed; finding-detail-panel + findings-lifecycle + findings-mutation)
  - `pnpm --filter web test -- --run adapter` (23/23 passed; adapter-contract + adapters)
  - Deep reads: `handlers/memory.rs` (auth + limit + delete + summarizer + the new `fetch_pending_reviews_by_creator` helper), 14 schemas, `browser-client.ts` + `tauri-client.ts` (delete-with-query transport + memory method surface), `queries.ts` (useActiveCreatorId + memory hooks), `memory-page.tsx` (3 affordances + inspector), `memory-detail-panel.tsx`, `task-kind-badge.tsx`, `findings-lifecycle.test.ts` golden, `finding-detail-panel.tsx` ↔ `finding-inline-edit-form.tsx` split, `apps/web/src/lib/nexus/query-keys.ts` (memory.* keys + `useActiveCreatorId` reuse), `world_kb.rs` (rationale on caps) + `suggested-relationships-pane.tsx` (PromoteAllResult state machine) + `world-kb-canvas.tsx` (bulk-promote result counts), `crates/nexus-contracts/src/generated/mod.rs` (root-level allow) + `crates/nexus-contracts/src/generated/local_api/{memory,mod}.rs` (collision check), `crates/nexus-contracts/tests/schema_drift_detection.rs` (`entry!` macro additions)

## Verification of Review Context Gate
- Cwd: `/Users/bibi/workspace/organizations/42ch/nexus` (matches `git rev-parse --show-toplevel`)
- Branch: `iteration/v1.78` (matches Assignment)
- HEAD: `04a411c22252d6f95de398fcf9a0162db6f8e688` (matches Assignment tip)
- Diff basis reproducible via `git diff 116296d0...04a411c2`

## Findings

### 🔴 Critical
None.

### 🟡 Warning

**[W-QC1-001] Status.json SSOT drift: slate-clear code is closed (11 V1.77-QC residuals) but their `residual_findings[*]` rows remain `lifecycle: "open"` at the review tip.**

- **Evidence**: `git show 519ec04d -- .mstar/status.json` returns no diff. At `04a411c2`, `.mstar/status.json` `residual_findings["2026-06-30-v1.77-findings-remediation-ui"]` still lists R-V177P0-QC1-S001/002/003/004 + R-V177P0-QC2-S001/002 + R-V177P0-QC3-S001 as `lifecycle: "open"` with `target: "V1.78 QC followup (tbd-v1.78-qc-followup)"`; `residual_findings["2026-06-30-v1.77-slate-clear"]` still lists R-V177P1-QC1-S002/003 + R-V177P1-QC2-S001 + R-V177P1-QC3-S001 as `lifecycle: "open"`. All 11 code-side closures from the slate-clear plan ARE landed in commit 519ec04d (split, golden test, asymmetry note, disabled-button guard, panel-rejected test, GRAPH_RELATIONSHIP_CAP exhaustion regression, candidate limit + slider rationale comments, PromoteAllResult status machine). Only the SSOT lifecycle flip is missing.
- **Impact**: Architecture/maintainability risk — `residual_findings` is the documented SSOT (`mstar-plan-artifacts` references `references/status-and-residuals.md`; the harness's own rule). The slate-clear P1 plan DoD #1 explicitly says: *"All 12 residuals `lifecycle: resolved` in `status.json` `residual_findings` (with `resolution.commit` + `resolution.plan_id`)."* The 11 V1.77-QC rows that the slate-clear claims closed are still listed as V1.78 followups at the review tip. Wave 1's plans are `InReview`; closure tracking is unfinished, which means the slate-clear plan cannot move to `Done` until PM (or designee) flips these rows. (1 row, R-V177P1-QC1-S001, is explicitly deferred to PM per the Assignment text — but the other 11 should also be flipped at wave-1 close.)
- **Fix**: PM-owned closure pass that flips the 11 V1.77-QC rows from `lifecycle: "open"` to `lifecycle: "resolved"` with `closure_note` + `resolution.commit = 519ec04d` + `resolution.plan_id = 2026-07-01-v1.78-slate-clear`. The R-V177P1-QC1-S001 (status.json missing-row registration) is independently PM-owned per the slate-clear plan scope. This is mechanical data hygiene, not a code change, but it is the SSOT loop.
- **Source Type**: git-diff + manual-reasoning + doc-rule
- **Source Reference**: `.mstar/status.json` lines 5834-5969 (open rows); `.mstar/plans/2026-07-01-v1.78-slate-clear.md` DoD #1; `git show 519ec04d -- .mstar/status.json` (empty diff); Assignment text in `IDENTITY` block ("`R-V177P1-QC1-S001` status.json row deferred to PM")
- **Confidence**: High

**[W-QC1-002] Centralized unbounded fetch: the new `fetch_pending_reviews_by_creator` helper unifies the list + review callsites around the pre-existing unbounded `SELECT … ORDER BY created_at DESC, LIMIT absent` pattern (qc3 flagged same code as W-QC3-002 from the perf/reliability lens; this is the architecture/maintainability read).**

- **Evidence**: `crates/nexus-daemon-runtime/src/api/handlers/memory.rs:212-230` calls `fetch_pending_reviews_by_creator(state.pool(), ...)` from the list handler; `memory.rs:491-513` calls the same helper from the review handler. The helper performs `SELECT … FROM memory_pending_review WHERE creator_id = ? ORDER BY created_at DESC` with `fetch_all(pool)` — no SQL `LIMIT` and no keyset cursor. Pagination (`?cursor=`, `?limit=`) is then applied **in memory** by searching the returned `Vec` for the cursor and `split_off`-ing to the page size. The pre-V1.78 code had two separate `query_as!` blocks with the same anti-pattern; the new helper centralizes it across both callsites.
- **Impact**: Architecture/maintainability — the refactor preserved the existing behavior **verbatim** (no functional regression), but it consolidated an unbounded-fetch pattern that should logically live alongside its bounded callers. Until pagination is pushed into SQL, this means any future patch to "mem-fetch by creator" either preserves the unbounded behavior (because the helper is shaped this way) or introduces a parallel helper (drift). The qc3 review independently raised this as W-QC3-002 with the same evidence; the maintainability lens adds that the centralized helper makes the next fix-easier-to-land *and* the easier-to-replicate in dev branches.
- **Fix**: Push pagination into the helper — `LIMIT ? + 1` with a deterministic keyset cursor `(created_at DESC, pending_id)` (or document the current `pending_id` cursor semantics more visibly). Add a regression test that seeds > `limit` rows and asserts the SQL-bound queue size. Until that lands, the helper's doc-comment should call out that the SELECT is intentionally unbounded and that pagination is in-memory — so future readers don't replicate the pattern by accident.
- **Source Type**: deep-lens: Architecture Lens / Module Boundary Lens / Code Quality Lens + git-diff + manual-reasoning
- **Source Reference**: `crates/nexus-daemon-runtime/src/api/handlers/memory.rs:212-230` (list), `:491-513` (review), `:259-292` (helper); pre-V1.78 commits at the same file with two `query_as!` callsites; qc3 report `W-QC3-002`
- **Confidence**: High

### 🟢 Suggestion

**[S-QC1-001] Doc drift: stale method-count comments in the V1.78 surface.**

- **Evidence**: `apps/web/src/lib/nexus/types.ts:20` reads "21 → 24 methods" (V1.67 G2); `apps/web/src/lib/nexus/tauri-client.ts:6` reads "the 24 `NexusClient` data methods reuse the identical HTTP transport"; `apps/web/src/lib/nexus/tauri-client.ts:67` reads "Inherits all 24 data methods from `BrowserClient`". With the V1.78 P0 Batch 2 promotion of 5 memory methods (`listPendingReviews`/`countPendingReviews`/`deletePendingReview`/`reviewMemory`/`listMemoryFragments`, see `apps/web/src/lib/nexus/adapter-contract.test.ts:29-44`), the count is now **29**.
- **Suggested follow-up**: Bump the comments to "21 → 29 (V1.67 G2 + V1.78 P0 Batch 2)". Doc-only; no behavior change.
- **Source Type**: manual-reasoning + doc-rule
- **Source Reference**: `apps/web/src/lib/nexus/types.ts:20`; `apps/web/src/lib/nexus/tauri-client.ts:6`, `:67`; `apps/web/src/lib/nexus/adapter-contract.test.ts:29-44` (5 new methods verified by the test that expects `seen.size === 34` methods, which is `29 NexusClient data methods + 5 other adapter-only endpoints`).
- **Confidence**: High

**[S-QC1-002] Golden test for findings adjacency reads a hand-transcribed copy of the DAO table — a 2-step drift hazard.**

- **Evidence**: `apps/web/src/lib/findings-lifecycle.test.ts:88-109` defines `daoTransition(from, to)` as a switch-case hand-copy of the `crates/nexus-local-db/src/findings.rs:172-189` source. The test asserts the client `isValidTransition(from, to)` against this golden copy, not against the live DAO function. If the DAO ever changes adjacency, the test won't fail (because the golden copy drifts alongside), defeating the point of the gate. The slate-clear commit message itself labels this "client adjacency golden test vs DAO" — accurate, but the 2-step drift risk is worth naming.
- **Suggested follow-up**: A stronger design would either (a) export the DAO adjacency as JSON via a build script and import it into the test, or (b) add a Rust-side cross-language probe test (the `crates/nexus-daemon-runtime` tests have a parallel `tests/memory_dto_roundtrip.rs` pattern — same pattern could prove client == DAO at CI). Not blocking; the current golden test does its job for now.
- **Source Type**: deep-lens: Architecture Lens / Testing Lens
- **Source Reference**: `apps/web/src/lib/findings-lifecycle.test.ts:88-109`; `crates/nexus-local-db/src/findings.rs:172-189`
- **Confidence**: Medium

**[S-QC1-003] `useActiveCreatorId` derivation diverges from the canvas `useDerivedCreatorId` shape (no schedule fallback; only `listSessions({limit:1})`).**

- **Evidence**: `apps/web/src/api/queries.ts:436-448` derives the active creator id from `listSessions({limit:1})[0]?.creator_id`. The canvas equivalent — `useDerivedCreatorId` at `apps/web/src/lib/canvas/use-strategy-data.ts:77-84` — falls back to `schedules.data?.[0]?.creator_id` when sessions is empty. The Memory page code in `memory-page.tsx:64-69` gates on a defined creator id with an "No active creator" empty state, so the practical impact is contained (a session-less creator hits the empty state). The comments at both sites call out the pattern dependency on `listSessions` for now. Compass Phase 2b open item #1 (`creator_id` UI source) explicitly names this as a future surface (a first-class active-creator endpoint/context).
- **Suggested follow-up**: Either add the schedule fallback to `useActiveCreatorId` for symmetry, or — per compass — defer until the dedicated active-creator accessor ships. Either way, the comment in `queries.ts:432-435` already explains the derivation and the future surface; worth keeping honest if sessions/schedules fall-out shapes diverge.
- **Source Type**: manual-reasoning + doc-rule
- **Source Reference**: `apps/web/src/api/queries.ts:436-448`; `apps/web/src/lib/canvas/use-strategy-data.ts:77-84`; `apps/web/src/pages/memory-page.tsx:64-69` (empty-state gate); plan §"UseActiveCreatorId derivation"; compass Phase 2b open item #1
- **Confidence**: Medium

**[S-QC1-004] Codegen root-level `#![allow(ambiguous_glob_reexports)]` accepts current and future module-name collisions silently.**

- **Evidence**: `crates/nexus-contracts/src/generated/mod.rs:10` adds `#![allow(ambiguous_glob_reexports)]` with an explanatory module-level comment. The collision is between `domain::memory` (existing `Memory` type — generated V1.62) and `local_api::memory` (new — 14 V1.78 P0 types). The flat TYPE re-exports are uniquely named (no name collisions on the type names), so the lint fires on the **module** name (`pub mod memory` appears under both `pub use domain::*` and `pub use local_api::*` from the crate root).
- **Suggested follow-up**: The chosen approach is pragmatic (comment explains scope), but a more surgical approach is to use `#[allow(ambiguous_glob_reexports)]` only at the module-level sites where the conflict actually fires (e.g., `local_api::mod.rs` for `pub use memory::*` when both `domain` and `local_api` are globbed at the parent). That way, a future scope that adds another colliding module would surface a real lint instead of being silently absorbed. Alternatively, rename `local_api::memory` → `local_api::review` (or `…/memory_review`) and avoid the collision in the first place — but the rename has a wider cross-language blast radius (schema IDs, file paths, npm package export tree).
- **Source Type**: deep-lens: Architecture Lens / Contract Fidelity Lens
- **Source Reference**: `crates/nexus-contracts/src/generated/mod.rs:7-10` (allow); `crates/nexus-contracts/src/generated/local_api/mod.rs:6-25` (memory module re-export); `crates/nexus-contracts/src/generated/domain/mod.rs:4-15` (memory collision); `tooling/codegen/src/rust-generator.ts:278-285` (generator now emits the allow at generated crate root); historical context: commit `71aaaaee` ("V1.62 P0 T9-T14" introduced `domain::memory`, latent until V1.78 added `local_api::memory`)
- **Confidence**: Medium

## Positive / Non-blocking Observations

The architecture/maintainability lens surfaced mostly clean cohesion:

- **Schema↔runtime fidelity**: All 14 `schemas/local-api/memory/` schemas faithfully mirror `handlers/memory.rs` runtime DTOs — `CreatePendingReviewRequest.{pending_id,session_id,creator_id,raw_digest}` required; `world_id|task_kind|created_at` optional with handler default semantics documented; `PendingReviewInfo.{task_kind,created_at}` always present (item table) but absent from create-request, exactly as transcribed in the Phase 2b architect LOCK block (`plan §"Batch 1 contracts LOCK"`). Field names, optionality, types, and integer widths (`usize` → `i64` for counters) match. The `SchemaDriftDetection` strict-mode `entry!` macros (14 new lines in `crates/nexus-contracts/tests/schema_drift_detection.rs:367-430`) catch any future drift between schema files and generated Rust structs.
- **DTO normalization quality**: The `query_as!` → `query!` + explicit row-mapping bridging for `PendingReviewInfo` is a textbook orphan-rule fix — both `sqlx::FromRow` and `nexus_contracts::PendingReviewInfo` are foreign to `nexus-daemon-runtime`, and `nexus-contracts` intentionally does not depend on sqlx (so the impl can't live there either). The `fetch_pending_reviews_by_creator` helper is now shared by both `list_pending_reviews` (line ~212) and `review` (line ~491), so the SQL string + projection map live in **one place** — no second hand-written SQL struct snuck in. The round-trip test (`memory_dto_roundtrip.rs`, 7 cases) adds compile-time proof that the handler module re-exports the **exact** contract types (a re-introduced hand-written DTO would break the assignment `let _: nexus_contracts::T = handler::T = ...`).
- **Frontend architecture**: Memory page mirrors the V1.77 findings-page pattern (detail-panel + row-action hybrid, side inspector); `useActiveCreatorId` derivation shape is documented in line and consistent with the established canvas pattern (even with the [S-QC1-003] divergence noted); the `NexusClient` interface boundary is respected — screens depend on the interface, not `fetch`/`invoke` directly; `TauriClient` continues to extend `BrowserClient` (zero method duplication; the 5 new memory methods are inherited cleanly via subclass). DESIGN.md token-name verbatim discipline is held across all 12 new memory tokens (with light + dark values in `DESIGN.dark.md`); the component code references the DESIGN token names verbatim, no undocumented tokens.
- **Slate-clear surgical-ness**: All 11 slate-clear code changes are minimal & scoped:
  - `finding-detail-panel.tsx` (305 → ~125 lines, with `finding-inline-edit-form.tsx` 166 lines) preserves every public prop and behavior; the `<FindingInlineEditForm>` boundary is a thin prop-drilled presentational slice (parent retains mutation orchestration + form state). The "Not reachable" `Badge` affordances were removed in favor of disabled buttons that carry the existing defense-in-depth message. Golden test asserts the hand-transcribed DAO table parity; rationale comments at the GRAPH cap + candidate limits + confidence slider call out the bounds; the bulk Promote-all action now returns `{ succeeded, failed }` rather than just logging.
  - The 11 slate-clear changes are file-disjoint from P0 except for `apps/web/src/api/queries.ts` (forward-staging closure comment + asymmetry note, both additive) — a clean merge.
- **Module boundaries + naming**: The new `memory` module under `crates/nexus-contracts/src/generated/local_api/` follows the same nested-module convention V1.62 P0 T9-T14 established. The barrel re-export chain (`pub use local_api::*` → `pub use memory::*` in `mod.rs:20` → individual types in `crates/nexus-contracts/src/generated/local_api/memory/*.rs`) is consistent with the existing 6 sibling scopes (`canvas`, `common`, `compute`, `creators`, `findings`, `kb`, `orchestration`, `preset_management`, `schedule`, `works`, `workspace`).
- **@42ch/nexus-contracts 0.12.0 → 0.13.0**: additive bump; no existing type changed; `SCHEMA_VERSIONS` entries added in `packages/nexus-contracts/src/generated/index.ts:262-275` for all 14 new types — wire-version alignment holds.
- **contracts-built artifact**: `pnpm --filter @42ch/nexus-contracts run build` succeeds; barrel types are present; downstream `apps/web` can resolve them. (Independently hit + resolved the same prebuild-dependency that qc3 raised as W-QC3-001; bundling this into a workspace-level `pretypecheck` hook is the qc3 fix path.)

## Shared Checklist (architecture/maintainability lens)

- **Code quality** [✅] — names clear, responsibilities clean, codegen-generated types vs hand-written now consistent. The V1.62 latent `domain::memory` + new `local_api::memory` collision is a known design area (see [S-QC1-004]).
- **Security/correctness** [✅, n/a here] — no new injection surface; auth checks preserved post-normalization. (qc2 cover.)
- **Performance/reliability** [⚠, deferred to qc3] — see qc3 W-QC3-002 / W-QC3-003 / W-QC3-004. I cross-flagged W-QC3-002 as [W-QC1-002] from the architecture/maintainability angle.
- **Maintainability** [✅ with caveats] — schemas are SSOT, handler DTOs are normalized, page/components mirror the existing findings pattern, slate-clear is surgical; 2 Warnings (SSOT drift + centralized unbounded fetch) and 4 Suggestions remain open.

## Source Trace
- F (W-QC1-001) — git-diff — `519ec04d` slate-clear commit diff; `git show 519ec04d -- .mstar/status.json` is empty; slate-clear P1 plan DoD #1 — Confidence: High.
- F (W-QC1-002) — deep-lens: Architecture Lens / Module Boundary Lens — `memory.rs:259-292` centralized helper + list/review callsites — Confidence: High.
- F (S-QC1-001) — manual-reasoning + doc-rule — `types.ts:20`, `tauri-client.ts:6`, `:67`; `adapter-contract.test.ts:29-44` (`seen.size === 34` = 29 data methods + 5 other adapter-only endpoints) — Confidence: High.
- F (S-QC1-002) — deep-lens: Architecture Lens / Testing Lens — `findings-lifecycle.test.ts:88-109` hand-transcribed golden vs `findings.rs:172-189` — Confidence: Medium.
- F (S-QC1-003) — manual-reasoning + doc-rule — `queries.ts:436-448` vs `use-strategy-data.ts:77-84` pattern — Confidence: Medium.
- F (S-QC1-004) — deep-lens: Architecture Lens / Contract Fidelity Lens — `generated/mod.rs:10` allow; `local_api/mod.rs:6-25` vs `domain/mod.rs:4-15`; `rust-generator.ts:278-285` codegen change — Confidence: Medium.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

Unresolved Warnings (W-QC1-001 status.json SSOT lifecycle flip for the 11 slate-closed V1.77-QC residuals; W-QC1-002 `fetch_pending_reviews_by_creator` centralized unbounded fetch) hold back the gate per `mstar-review-qc` Verdict gate (Critical=0 AND Warning=0 → Approve; else Request Changes). The V1.78 P0 Batch 1 + Batch 2 implementation is otherwise sound (schema↔runtime fidelity held; DTO normalization clean; codegen generated types; adapter-contract parity green; round-trip regression green; memory/finding/web tests green; clippy clean; DESIGN.md token-name discipline respected; slate-clear P1 surgical) — the implementation quality is high; the open items are governance + an architecture-flagged shared-helper carryover from a pre-existing unbounded fetch.

---

## Revalidation (2026-07-01 — targeted re-review after fix-wave)

### Context
- **Re-review window**: `004ad9c5..cf167a0e` (fix-wave delta: status update `004ad9c5` + dev fix `d5ddfff8` + merge `cf167a0e`)
- **HEAD (verified)**: `cf167a0eb751a8f619a32e05a07a7be5b0add4af` (matches Assignment tip)
- **Working branch (verified)**: `iteration/v1.78`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (matches `git rev-parse --show-toplevel`)
- **Working tree**: clean (`git status --porcelain` returns no rows)
- **Tools run (this wave)**:
  - `git rev-parse --show-toplevel`; `git branch --show-current`; `git rev-parse HEAD`; `git status --porcelain` (alignment + tree)
  - `git show 004ad9c5 -- .mstar/status.json` (status flip diff — 159 insertions, 18 deletions)
  - `git diff 004ad9c5..cf167a0e --stat` (fix-wave scope: 11 files / 739 insertions / 34 deletions)
  - `git diff 004ad9c5..cf167a0e -- crates/nexus-daemon-runtime/src/api/handlers/memory.rs` (keyset helper + bounded list path)
  - `git diff 004ad9c5..cf167a0e -- crates/nexus-local-db/src/memory_fragment.rs crates/nexus-local-db/src/lib.rs` (bounded DAO + re-export)
  - `git diff 004ad9c5..cf167a0e -- apps/web/src/lib/nexus/{types.ts,tauri-client.ts}` (S-QC1-001 count-agnostic doc fix)
  - `git diff 004ad9c5..cf167a0e -- apps/web/src/api/queries.ts` (qc3 S-QC3-002 background-polling note landed)
  - `python3 -c "import json; …"` (verify V1.77 residuals + tech_debt_summary)
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test memory_pagination_bounded` — **5 passed, 0 failed**
  - `SQLX_OFFLINE=true cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` — **clean, 0 warnings**
  - Read: `crates/nexus-daemon-runtime/src/api/handlers/memory.rs:209-237` (new list handler pagination), `:273-306` (kept unbounded helper), `:308-430` (new `fetch_pending_reviews_page`), `:623` (sole remaining caller of unbounded helper), `:865-885` (bounded fragments path); `crates/nexus-daemon-runtime/tests/memory_pagination_bounded.rs` (321-line regression suite)

### Per-finding disposition

#### [W-QC1-001] status.json SSOT drift — **RESOLVED** ✅
- **Evidence — 12 V1.77-QC residuals flipped to `resolved`**: `python3` script against current `.mstar/status.json` (line 5841+) confirms all 7 V1.77-P0 rows (`R-V177P0-QC1-S001/S002/S003/S004`, `R-V177P0-QC2-S001/S002`, `R-V177P0-QC3-S001`) and all 5 V1.77-P1 rows (`R-V177P1-QC1-S001/S002/S003`, `R-V177P1-QC2-S001`, `R-V177P1-QC3-S001`) now have `lifecycle: resolved` with `closed_at: 2026-07-01` + `resolution.commit: 519ec04d` (11 rows) or `n/a (PM data closure)` for the explicitly PM-owned R-V177P1-QC1-S001 row. The `git show 004ad9c5 -- .mstar/status.json` diff (+159/-18 lines) shows the exact lifecycle flip.
- **Evidence — `tech_debt_summary` now self-consistent for the open set**: `total_open: 5` matches the 5 V1.78-QC residuals now open (cross-verified via `python3` script counting all `lifecycle: open` rows in `residual_findings`); `by_severity_active: {low: 3, nit: 2}` matches the actual open-residual severities; `by_target_active: {V1.79+ (or CI wrapper): 1, V1.78 fix-wave: 3, V1.79+ reliability roadmap: 1}` sums to 5 and matches the per-row `target` fields. `refreshed_at: 2026-07-01T00:00:00Z`; `refreshed_reason` names the fix.
- **Evidence — 5 new V1.78-QC residuals registered with proper tracking links**: `R-V178P0-QC1-001` (nit, S-QC1-001 doc drift, target V1.78 fix-wave), `R-V178P0-QC3-001` (low, typecheck prebuild order, defer V1.79+ or CI wrapper), `R-V178P0-QC3-002` (low, **W-QC1-002 / W-QC3-002 / W-QC3-003**, fix-in-wave), `R-V178P0-QC3-003` (low, review pipeline bound/timeout, defer V1.79+ reliability roadmap), `R-V178P0-QC3-004` (nit, usePendingReviewCount polling comment, fix-in-wave). Each row has `id` + `title` + `severity` + `source` (cites qc1/qc3 finding ID) + `scope` + `decision` + `owner` + `target` + `tracking_link` — full per-`mstar-plan-artifacts` discipline. The `R-V178P0-QC3-002` registration explicitly cross-references `qc1 W-QC1-002` as the architecture/maintainability source, and the W-QC1-002 fix below is the closure evidence for it.
- **Minor observation (not a new finding)**: `total_resolved: 289` is unchanged in the `004ad9c5` diff; the live count of `lifecycle: resolved` rows in `residual_findings` is 310 (12 V1.77-QC newly-closed + ~298 prior). This is a pre-existing data-hygiene drift in the rollup (existed before the fix-wave; my qc1 report did not call it out because the `total_open` staleness was the load-bearing issue), not a regression introduced by this fix-wave. The PM-fix landed at the right level (correct lifecycle flips + correct `total_open` + correct severity/target rollup). The 21-residual drift in `total_resolved` is best addressed by adding it to the existing `mstar-plan-conventions` `references/status-and-residuals.md` maintenance practice rather than by blocking V1.78 sign-off.
- **Verdict**: My qc1 W-QC1-001 was specifically about (a) the 12 V1.77-QC residuals still being `open` and (b) the `tech_debt_summary` rollup being stale. Both are now fixed. The residual observation is below the W threshold. **RESOLVED.**

#### [W-QC1-002] centralized unbounded fetch — **RESOLVED** ✅
- **Evidence — keyset helper introduced**: `git diff 004ad9c5..cf167a0e -- crates/nexus-daemon-runtime/src/api/handlers/memory.rs` introduces `fetch_pending_reviews_page` (line 348+) with the exact architecture I recommended: keyset on `(created_at DESC, pending_id DESC)` + `LIMIT ?` (called with `limit + 1` from the list handler at line 217) + cursor = `pending_id`. The doc-comment (line 308-347) explicitly cites `R-V178P0-QC3-002 (qc1 W-QC1-002 + qc3 W-QC3-002/W-QC3-003)` and analyzes the behavior-preservation cases (distinct `created_at` → identical to prior; equal-tie tiebreaker is strictly more correct; deleted cursor → first-page fallback matches prior `position()==None`).
- **Evidence — list path no longer materializes the full set**: `list_pending_reviews` (line 209-237) now calls `fetch_pending_reviews_page(... fetch_limit)` where `fetch_limit = i64::try_from(limit + 1).unwrap_or(i64::MAX)`. The prior 8-line `fetch_all → position → split_off → truncate(limit)` block is gone; replaced by a single bounded query. The list path applies `limit + 1` over-fetch + truncate + `next_cursor = last.pending_id`, matching the prior `items.len() > limit → truncate(limit) → items.last()` semantics exactly. The wire contract is unchanged.
- **Evidence — unbounded helper kept ONLY for the `review` handler**: `grep fetch_pending_reviews_by_creator crates/nexus-daemon-runtime` returns 3 matches: a doc-comment cross-ref (line 12), the helper definition (line 283), and one callsite (line 623) inside the `review` handler. The `review` handler intentionally processes the whole queue (per the residual plan + the doc-comment at line 273-282 explaining the `query!`-vs-`query_as!` orphan-rule workaround), so keeping an unbounded helper for that one callsite is the correct trade-off.
- **Evidence — fragments no-keyword path also bounded**: `git diff 004ad9c5..cf167a0e -- crates/nexus-local-db/src/memory_fragment.rs` introduces `list_fragments_limited` (LIMIT ? in SQL, same projection + ordering as the existing `list_fragments`); re-exported in `lib.rs:76`. The fragments handler's no-keyword branch (line 865-885) replaces the prior `list_fragments` + in-Rust `truncate(limit)` with the bounded DAO.
- **Evidence — regression test suite passes**: `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test memory_pagination_bounded` → **5 passed, 0 failed, 0 ignored** (321-line suite added in this fix-wave). The suite is the behavioral proof I asked for: seeds 60 rows for both pending-reviews and fragments, walks the full keyset cursor loop asserting zero duplicates + zero gaps, exercises the deleted-cursor fallback, and asserts `limit > dataset` returns all rows. The doc-comment at line 7-12 of the test file explicitly notes "a naive in-Rust truncate would also pass a single-page size check, but only a correct keyset walks all pages without overlap or gaps" — which is exactly the discrimination the test must achieve.
- **Evidence — clippy clean**: `SQLX_OFFLINE=true cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` → 0 errors, 0 warnings (matches CI scope per my qc1 tools list).
- **Minor observation (not a new finding)**: The doc-comment at `memory.rs:281-282` of the kept unbounded helper still says "the list and review handlers share it" — the list handler no longer uses this helper. The new helper's own doc-comment is the more accurate reference now. The mapping logic is no longer truly shared (the row→`PendingReviewInfo` field assignment now appears 3 times: in `fetch_pending_reviews_by_creator` and in both branches of `fetch_pending_reviews_page`). This is a minor doc-drift + minor in-helper duplication; it does not affect correctness or the bounded-fetch contract. Worth a follow-up doc cleanup in V1.78+ (low-priority; defer-able).
- **Verdict**: My qc1 W-QC1-002 was specifically about (a) the unbounded `fetch_all` being kept after the helper centralization and (b) the need for a regression test that seeds > `limit` rows. Both are now addressed — pagination is pushed into SQL with keyset, and the regression suite is comprehensive (5 tests covering size-cap, full cursor walk, deleted-cursor fallback, fragments size-cap, limit-above-dataset). The unbounded helper is correctly scoped to the one callsite that needs it. **RESOLVED.**

#### [S-QC1-001] method-count doc drift — **RESOLVED** ✅ (bonus doc fix in fix-wave)
- **Evidence**: `git diff 004ad9c5..cf167a0e -- apps/web/src/lib/nexus/types.ts apps/web/src/lib/nexus/tauri-client.ts` replaces the stale "21 → 24 methods" / "24 `NexusClient` data methods" / "all 24 data methods" literals with count-agnostic prose that points readers at the interface for the canonical count. Both files add a "Method count:" paragraph explaining the prior drift and noting the V1.78 promotion as historical context. Per the assignment note ("S-QC1-001 was fixed — comments now count-agnostic"), this is the expected fix.

#### [S-QC1-002 / S-QC1-003 / S-QC1-004] — **deferred** (per Assignment instruction; no new state)
- The Assignment explicitly says S-QC1-002/003/004 are non-blocking (defer or note). I keep them in the historical Findings list but do not re-raise them. S-QC1-001 is the only Suggestion that landed in the fix-wave (resolved above).

### Bonus fix-wave change (not in my original findings, but worth noting)
- `apps/web/src/api/queries.ts` (+5 lines, in `usePendingReviewCount`): adds a `refetchIntervalInBackground` comment explaining the intentional battery-sensitivity choice (TanStack pauses `refetchInterval` on hidden tabs by default; do not flip the flag without a reason). This is the closure for qc3 S-QC3-002 and the V1.78-QC `R-V178P0-QC3-004` registration; it lives in the same fix-wave diff but is not a qc1 finding.

### New issues introduced by the fix-wave
- **None** at Critical or Warning severity.
- Two minor observations noted above (S-QC1-001 style doc drift on the kept unbounded helper; 8-line row-mapping duplication in the two branches of `fetch_pending_reviews_page`). Both are below the S threshold and do not warrant a new finding row. They are defer-able for V1.78+ cleanup.

### Updated Shared Checklist (architecture/maintainability lens)
- **Code quality** [✅] — names clear, responsibilities clean, keyset helper is well-isolated and well-documented; row-mapping duplication is local + explicit.
- **Security/correctness** [✅, n/a here] — no new injection surface; deleted-cursor fallback is explicit and tested.
- **Performance/reliability** [✅] — list path now bounded at SQL layer; unbounded helper is correctly scoped to the only callsite that needs it. R-V178P0-QC3-003 (review pipeline bound/timeout) is registered for V1.79+ reliability roadmap per qc3.
- **Maintainability** [✅] — schemas are SSOT, handler DTOs are normalized, the bounded fetch contract is regression-tested, count-agnostic docs in `types.ts` / `tauri-client.ts` prevent future drift, status.json SSOT is now self-consistent for the open set.

### Updated Summary

| Severity | Count (initial wave) | Disposition (re-review) |
|----------|----------------------|--------------------------|
| 🔴 Critical | 0 | 0 |
| 🟡 Warning | 2 | **0** (both RESOLVED) |
| 🟢 Suggestion | 4 | 3 deferred + **1 RESOLVED** (S-QC1-001 doc-nits) |

**New Verdict**: **Approve** — both blocking Warnings are RESOLVED, the doc-nits Suggestion landed, no new Critical/Warning introduced, 5/5 regression tests pass, clippy clean. Per `mstar-review-qc` Verdict gate (Critical=0 AND Warning=0 → Approve), the V1.78 P0 + P1 implementation is now ready for sign-off. The 3 deferred Suggestions + the registered V1.78-QC residuals (R-V178P0-QC3-001/002/003/004 + R-V178P0-QC1-001) are tracked in `residual_findings[2026-07-01-v1.78-creator-memory-surface]` and will be addressed in V1.78+ / V1.79+ as planned.
