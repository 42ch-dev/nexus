---
iteration_id: V1.80
start_date: 2026-07-01
status: completed
end_date: 2026-07-01
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-01-v1.80-memory-review-reliability
  - 2026-07-01-v1.80-frontend-hygiene-residuals
  - 2026-07-01-v1.80-closure
---

# V1.80 — Memory Review Reliability + Frontend Hygiene — Delivery Compass v1

**Status**: active (draft — pending §1.6 Review & Edit chain, then PM lock).
**Author**: `@project-manager` (compass + grill-me locked scope).

## 0. Context — post-V1.79 reliability investment

V1.79 (Author Reflection, PR #106, 2026-07-01) was the fourth consecutive feature iteration (V1.76–V1.79) after the three author-in-command loops closed: writing (V1.76), quality (V1.77), and memory (V1.78). Each of those feature iterations registered low-severity reliability and hygiene residuals that have accumulated. The highest-leverage accumulated item is **REL-01** (`R-V178P0-QC3-003`): the `POST /v1/local/memory/review` synchronous whole-queue pipeline has no bound, timeout, cancellation, or in-flight serialization guard.

V1.80 is a **reliability investment iteration** rather than a feature iteration. It pays down accumulated debt before building more on top, and folds four low `V1.79-QC` frontend hygiene residuals in as a parallel companion track. No new user-facing product surface ships; the goal is a more robust foundation for the next feature iteration (V1.81 candidate evaluation).

This is the natural rhythm after four feature iterations: stabilize before extending. The reliability stories use operator/maintainer personas, but the author experiences the benefit concretely — no more frozen desktop shell when the memory review pipeline hits a large queue, no more fragile component tests that make future reading-surface changes risky.

## 1. grill-me locked decisions

| Decision | Resolution |
|---|---|
| Main direction | **A — Reliability hardening iteration** (pick up REL-01 + accumulated reliability/hygiene residuals). |
| REL-01 rewrite depth | **(a) Minimal bounded synchronous** — per-creator in-flight serialization lock + bounded fetch (N rows per call) + request timeout + client `has_more` drain loop. Reuses the V1.78 `has_more`/bounded pattern (compound doc `pagination-cursor-without-total-count-labels.md`). Async background-job infrastructure rejected as over-engineering for the local-only / single-creator / small-queue threat model. |
| Companion track | **4 low V1.79-QC frontend hygiene residuals** (R-V179P0-QC1-001/002 + R-V179P1-QC1-001/002) fold in as a parallel companion plan (owner `@frontend-dev`). |
| Wire contracts | **Additive** — `ReviewResponse` gains `has_more` (bool) + `processed` (i64, rows inspected this call) so the client can drain via repeated calls. `@42ch/nexus-contracts` 0.14.0 → 0.15.0. |
| Plan structure | P0 (REL-01, `@fullstack-dev`) ‖ P1 (frontend hygiene, `@frontend-dev`) + P-last (closure). 3 plans. |
| Branch policy | Integration `iteration/v1.80` from `main` (`ed5c6074`); per-plan topic `feature/v1.80-<slug>`; P0‖P1 parallel → worktree isolation required (per `.mstar/AGENTS.md` two-tier branch model). |
| No new product surface | This iteration ships no new author-facing feature. REL-01 is a backend reliability improvement (existing endpoint behaves better under stress); P1 is frontend test/token hygiene. |

## 2. Scope

This iteration locks two workstreams, both paying down debt rather than adding product surface:

- **SP-1 (Track A / P0 — REL-01)**: Rewrite `POST /v1/local/memory/review` (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs`) from an unbounded synchronous whole-queue loop into a **bounded, serialized, timeout-guarded** pipeline. The endpoint keeps its synchronous request/response shape (no async job infrastructure), but each call processes at most N pending rows, holds a per-creator in-flight guard so two concurrent reviews cannot double-process the same rows, and is wrapped in a request timeout. The response gains `has_more` + `processed` so the web client can drain the queue via repeated calls (mirroring the established `has_more` pattern). Closes `R-V178P0-QC3-003` / REL-01.
- **SP-2 (Track B / P1 — Frontend hygiene)**: Close the four low `V1.79-QC` frontend residuals: reading-surface keyboard-nav test coverage + overlay-guard robustness (R-V179P0-QC1-001), reading-surface component unit-test coverage + defensive-coding nits (R-V179P0-QC1-002), `memory-page.tsx` module split into `components/memory/` siblings (R-V179P1-QC1-001), and SOUL `temporal-drift.tsx` BAND_PALETTE token promotion + dead-alias cleanup (R-V179P1-QC1-002).

### 2.1 User stories (reliability / maintenance persona)

These are not author-facing feature stories; they describe the reliability and maintainability outcomes.

**Track A — Memory review reliability**

- *As the local daemon operator*, when I trigger `POST /memory/review` on a large pending queue (e.g. 1000+ rows), each daemon call returns within the locked per-call budget (default 5 seconds) rather than blocking indefinitely, so the desktop shell / browser client does not hang and I can observe steady drain progress via repeated calls.
- *As the local daemon operator*, if I (or a stale client) trigger two concurrent `POST /memory/review` calls for the same creator, the second is serialized (or rejected with a clear in-flight signal) rather than double-processing the same pending rows, so I do not get duplicate promoted/fragmented memory.
- *As the local daemon operator*, when the review pipeline is draining a large queue, the client receives `has_more` + `processed` so the UI can decide whether to re-request, rather than assuming a single call always drains the queue.

**Track B — Frontend hygiene**

- *As a frontend maintainer*, the reading surface and SOUL viz components carry the test coverage and token discipline expected of production code, so future changes can be made with regression confidence and without hardcoded palette drift.
- *As a frontend maintainer*, `memory-page.tsx` is decomposed into focused modules under the size discipline, so the V1.79 SOUL additions do not leave a 360-line god-module behind.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-01-v1.80-memory-review-reliability` | P0 — Track A: Memory Review Reliability (REL-01 bounded/serialized/timeout + client drain) | Done | QC 3/3 Approve (after fix-wave for qc3 W-QC3-001 drain-completion accounting). QA Pass. Closes `R-V178P0-QC3-003`. Wire additive `ReviewResponse` + `has_more`/`processed` → `@42ch/nexus-contracts` 0.14.0 → 0.15.0. 2 low accepted residuals (mutex-map growth + best-effort delete). |
| `2026-07-01-v1.80-frontend-hygiene-residuals` | P1 — Track B: Frontend Hygiene (4 V1.79-QC residuals) | Done | QC 3/3 Approve (clean). QA Pass. Closes `R-V179P0-QC1-001/002` + `R-V179P1-QC1-001/002`. No wire change. |
| `2026-07-01-v1.80-closure` | P-last — Closure (compound + compaction + PR + tracker/STRATEGY + residual close-out) | Done | Compound: bounded-drain-completion-contract. Profile B compaction (3 plans archived). Deferred tracker (REL-01 shipped) + STRATEGY decision-log updated. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

**Parallelism**: P0 and P1 are independent and file-disjoint.

- **P0** touches `crates/nexus-daemon-runtime/src/api/handlers/memory.rs`, `crates/nexus-daemon-runtime/src/workspace/mod.rs`, `schemas/local-api/memory/review-response.schema.json`, generated contract output, and the `apps/web` memory client mutation (`apps/web/src/api/queries.ts::useReviewMemory`; `apps/web/src/api/memory-mutation.test.tsx` for API-level tests if needed). It does not touch `crates/nexus-local-db` on the selected reuse path.
- **P1** touches `apps/web/src/components/reading/*`, `apps/web/src/components/soul/temporal-drift.tsx`, and `apps/web/src/pages/memory-page.tsx`.

The single collision risk is `apps/web/src/pages/memory-page.tsx`: P1's R-V179P1-QC1-001 extracts sections **from** it, while the memory client mutation (P0) lives in `apps/web/src/api/queries.ts`, not in `memory-page.tsx` — so the files are distinct. P0 must not move UI sections out of `memory-page.tsx`; P1 must not change the `useReviewMemory` drain-loop semantics.

Both tracks dispatch after compass lock, run in separate worktrees, and merge into `iteration/v1.80`. P-last runs sequentially after P0 + P1 merge.

## 4. Branch policy

Per `.mstar/AGENTS.md` two-tier branch model:

| Tier | Branch | Purpose |
|---|---|---|
| Integration | `iteration/v1.80` (from `main` @ `ed5c6074`) | Single line where all plan work lands before QC/QA; QC/QA `Working branch`. |
| Final landing | `main` | `iteration/v1.80` merges here via PR after iteration sign-off. |
| P0 topic | `feature/v1.80-memory-review-reliability` | Track A commits only (crates/* + schemas + generated output + web memory client). |
| P1 topic | `feature/v1.80-frontend-hygiene` | Track B commits only (apps/web reading/soul/memory-page). |
| P-last topic | `feature/v1.80-closure` | Closure commits only. |

**Worktree isolation**: required for P0 ‖ P1 (same-repo parallel writers). P0 must not touch `apps/web/src/components/reading/`, `apps/web/src/components/soul/`, or `apps/web/src/pages/memory-page.tsx`; P1 must not touch `crates/`, `schemas/`, generated contract output, or the web memory client mutation. Shared web client files, if unavoidable, are additive-only and resolved on `iteration/v1.80` after topic-branch merge. P-last sequential after P0 + P1 merge.

## 5. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Compass locked (§1.6 Review & Edit chain complete) | 2026-07-01 | pending |
| P0 + P1 dev complete (parallel) | 2026-07-02 | pending |
| QC tri-review complete (both tracks) | 2026-07-02 | pending |
| QA Pass | 2026-07-02 | pending |
| Iteration close (P-last + PR) | 2026-07-02 | pending |

## 6. Acceptance Criteria

### Reliability verification (Track A — what the operator experiences)

- `POST /memory/review` returns within the bounded per-call server budget (locked plan default: 5s) even on a 1000+ pending row queue; the author does not see an indefinite hang or a frozen desktop shell.
- A per-creator in-flight guard prevents two concurrent reviews from double-processing the same pending rows. The second concurrent call is serialized by waiting on the per-creator guard — not a silent double-promote and not a new 409/429 wire branch.
- The response carries `has_more` + `processed`; the web client drains via repeated calls until `has_more = false`. The drain loop has a defined max-iterations cap, and the UI surfaces a "still draining" state if the cap is hit.
- No regression in the existing promote/fragment/drop classification correctness; the bounded path produces identical per-row decisions to the current whole-queue path.
- `R-V178P0-QC3-003` / REL-01 closed in `status.json` (`lifecycle: resolved`) and removed from the deferred tracker reliability-roadmap grouping.

### Hygiene verification (Track B — what the maintainer experiences)

- Reading surface keyboard-nav interaction tests exist and pass (`apps/web` test suite: `pnpm --filter web run test -- --testPathPattern reading` or equivalent). The overlay-guard short-circuit path (`hasOpenOverlay`) is covered by a test asserting that keyboard navigation is suppressed when a menu or dialog DOM node is detected.
- Reading surface components carry unit-test coverage at the granularity the QC1 review requested; the documented defensive-coding nits (`defensive ?? 1` on volume, debounce design comment) are addressed.
- `memory-page.tsx` is decomposed into focused modules ≤ the size discipline, with extracted sections under `apps/web/src/components/memory/`. The original page file is reduced to a layout/data-fetching shell; section components own their rendering.
- SOUL `temporal-drift.tsx` band palette slots reference DESIGN.md tokens via CSS variables (not hardcoded RGBA except slot 0 which already uses `var()`); the unused `driftDateHelper` alias is removed with no remaining callers.
- All four V1.79-QC residuals closed in `status.json` (`lifecycle: resolved`).

### Technical verification

- Wire additive: `ReviewResponse` exposes `has_more` + `processed`; round-trip regression test green; `@42ch/nexus-contracts` 0.14.0 → 0.15.0; `pnpm run codegen` + `cargo build` + `validate-schemas` clean.
- QC tri-review 3/3 Approve (both tracks); QA Pass; `cargo clippy --all -- -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check` + `validate-schemas` clean; web typecheck/test green.
- `status.json` coherent (Profile B: only non-Done plans in `plans[]`; P-last compaction done).

## 7. Non-Goals

- **Async background-job infrastructure** for the review pipeline (job table, status polling, cancellation tokens) — rejected as over-engineering for the current threat model; REL-01 (a) keeps the synchronous request/response shape.
- Any new author-facing product surface — this is a reliability + hygiene iteration, not a feature iteration.
- BL-09/10/11/12 (standalone maturation dashboard, SOUL growth-curve, deeper reading, SOUL viz refinements) — remain in backlog pending V1.81+ candidate evaluation.
- Cloud sync / platform unpause — remains paused (PD-05).
- Desktop signing rollout — blocked on Apple Developer ID cert (non-agent-driven).

## 8. Roadmap Position

- **Current iteration (V1.80, delivered 2026-07-01)**: First "stabilize before extending" reliability investment iteration after four consecutive feature iterations (V1.76–V1.79). Track A (P0) closed REL-01 (`R-V178P0-QC3-003`): rewrote `POST /v1/local/memory/review` from an unbounded synchronous whole-queue loop into a bounded (`REVIEW_BATCH_LIMIT=50`) / per-creator serialized (in-process mutex map on `WorkspaceState`) / deadline-aware (5s partial-progress) synchronous pipeline — no async job infrastructure, proportional to the local-only / single-active-creator / small-queue threat model. Additive wire DTO (`ReviewResponse` + `has_more`/`processed`; `@42ch/nexus-contracts` 0.14.0 → 0.15.0); client drains via repeated calls. QC tri-review 3/3 Approve (P0 after a fix-wave for the drain-completion `has_more` accounting bug qc3 W-QC3-001 — `has_more` must reflect queue advancement, not rows attempted). Track B (P1) closed four low V1.79-QC frontend hygiene residuals (reading keyboard-nav tests + component tests + `memory-page.tsx` 360→71-line split + SOUL `temporal-drift` BAND_PALETTE token promotion); QC 3/3 Approve clean. QA Pass (762+ Rust tests, 354 web tests, clippy/fmt/codegen/validate-schemas clean). Compound captured the bounded-drain-completion-contract pattern. 2 low P0 residuals accepted (mutex-map unbounded growth + best-effort delete at-least-once) under the documented threat model.
- **Next iteration (V1.81)**: Candidate evaluation at next `/iteration-start`. Backlog candidates after V1.80: BL-09 standalone maturation dashboard, BL-10 independent SOUL growth-curve view, BL-11 deeper manuscript reading (annotations/highlights), BL-12 SOUL viz refinements — any of which becomes a higher-leverage investment once usage data accumulates. Owner: `@project-manager`. Trigger: user initiates next `/iteration-start`.
- **Final goal**: Nexus as a local-first, AI-agent-driven creative writing tool where the author commands three closed loops (steer writing, triage quality, curate memory) **and** can reflect on the accumulated craft and evolving creative self — on a reliable, well-tested foundation.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| In-flight guard implementation races (guard released before side effects + best-effort deletes complete) | Med | Med | Use the selected in-process per-creator `tokio::sync::Mutex` guard from `WorkspaceState`; hold it across fetch, classification, promote/fragment/drop side effects, and `delete_pending_by_id`; use an explicit inner scope so release occurs only after the batch finishes; add a concurrency regression test that asserts no double-promotion under overlapping calls. |
| Bounded fetch changes per-row decision ordering vs whole-queue | Low | Low | Reuse `fetch_pending_reviews_page` with `cursor=None` and `LIMIT REVIEW_BATCH_LIMIT+1`, preserving the V1.78 deterministic `ORDER BY created_at DESC, pending_id DESC`; per-row classification is stateless, and successful rows are deleted after their side effect as today. |
| `ReviewResponse` additive DTO breaks a consumer | Low | Med | Additive-only: `has_more` + `processed` are optional in the schema; old consumers deserialize unchanged and new consumers treat absent values from older daemons as `false` / `0`. Required keys stay `promoted`/`fragmented`/`dropped`; round-trip regression test extended. |
| P0 web memory-client change and P1 memory-page split touch adjacent `apps/web` memory surface → merge conflict | Med | Low | Strict file boundary: P0 client mutation lives in `apps/web/src/api/queries.ts` (plus `apps/web/src/api/memory-mutation.test.tsx` if needed), P1 extracts sections from `apps/web/src/pages/memory-page.tsx`; resolve any generated-import churn on `iteration/v1.80` after topic merge. |
| Timeout threshold mis-tuned (too short leaves work for extra drain calls; too long loses the bound) | Med | Low | Use a named 5s server budget and a 50-row batch; return partial progress with `has_more=true` rather than 503 on ordinary budget exhaustion; client drain caps at 20 calls / 1,000 rows per user action and surfaces "still draining" if more remains. |
| Reliability iteration perceived as "no visible progress" by stakeholders | Low | Low | Compass §0 + STRATEGY decision-log entry frame this as deliberate stabilize-before-extend; the four closed residuals + REL-01 closeout are tangible debt reduction. |

## 10. Wire contracts note

`wire_contracts_changed: TRUE` for Track A (P0) only. The exact schema file is `schemas/local-api/memory/review-response.schema.json`; its current required keys are `promoted`, `fragmented`, and `dropped`, with `additionalProperties: false`. The additive change is two optional properties: `has_more` (`boolean`; consumers treat absent as `false`) and `processed` (`integer`, rows inspected this call; consumers treat absent as `0`). `required` remains unchanged so old consumers continue to deserialize; the V1.80 daemon should always emit concrete values. P0 owns the schema edit + `pnpm run codegen` + commit generated Rust + TypeScript output + `packages/nexus-contracts/package.json` 0.14.0 → 0.15.0 + wiring the handler to populate the new fields + extending the round-trip regression test. Track B (P1) changes no schema.

## Compound Round Summary

- Compound docs created: **1** — [`architecture-patterns/bounded-drain-completion-contract.md`](../knowledge/architecture-patterns/bounded-drain-completion-contract.md) (Knowledge track; surfaced from the P0 fix-wave — `has_more` drain-completion must reflect queue advancement, not rows attempted; the W-QC3-001 anti-pattern + fix + regression-test recipe).
- New CONCEPTS.md entries: **0** (the drain-completion contract is an architecture pattern, not a domain noun).
- Triggered compound-refresh: **no** — the new doc is related to but distinct from the V1.79 `pagination-cursor-without-total-count-labels.md` (read-side count labels vs write-side drain-completion); flagged for a future consolidation review but no stale older doc to merge now.

## Iteration Retrospective (minimal)

- **做得好的**:
  - grill-me 在 V1.76–V1.79 四连 feature 迭代后精准选择了「stabilize before extending」方向 —— 避免了在未验证特性上过早加码；REL-01 是当时唯一带 medium 级遗留的方向。
  - P0‖P1 双轨 file-disjoint 并行 worktree 顺畅，零合并冲突；rel101 与 frontend hygiene 各归其位。
  - qc3 在初轮三审中根因定位了 drain-completion 的 attempt-vs-advancement 记账陷阱（W-QC3-001），fix-wave 后 targeted re-review 原位翻转 Approve —— 守住了 REL-01 的「client uncertain-completion」第三轴，并结晶为可复用知识。
- **可改进的**:
  - P0 初版实现把 `processed`（rows attempted）直接用于 `has_more` 推导，漏掉了 failure/timeout 路径的 completion 语义 —— 该类「bounded drain」的完成契约应在 plan 阶段就显式写出「has_more 必须反映 queue advancement」，而非依赖 QC 发现。这正是 compound 文档要防止的下次踩坑。
  - deadline `Err(_elapsed)` 路径无确定性集成测试（row actions 在 µs 级完成，top-of-loop 检查先触发）—— 依赖与 `Ok(0,0,0)` 路径的结构同构论证。未来若引入可注入的慢操作 testability 会更稳。
- **下迭代建议**:
  - V1.81 候选评估：BL-09 独立 maturation 仪表盘、BL-10 SOUL 增长曲线、BL-11 深度阅读（标注/highlight）、BL-12 SOUL viz 精炼 —— 任一在 V1.79/V1.80 地基上加码。owner `@project-manager`。
  - 收 2 个 low P0 accepted residuals（mutex-map 生命周期注释 + best-effort delete 文档化）—— 可作为下迭代伴随轻轨。
