---
iteration_id: V1.79
start_date: 2026-07-01
status: completed
end_date: 2026-07-01
plans:
  - 2026-07-01-v1.79-prepare-dual-track-spec-amendments
  - 2026-07-01-v1.79-manuscript-reading-surface
  - 2026-07-01-v1.79-soul-personality-visualization
  - 2026-07-01-v1.79-closure
---

# V1.79 — Author Reflection (Manuscript Reading Surface + SOUL Personality Visualization) — Delivery Compass v1

**Status**: active (draft — pending §1.6 Review & Edit chain → PM lock).
**Author**: `@project-manager` (compass + grill-me locked scope).

## 0. Context — post-inflection framing

V1.78 (PR #105, 2026-07-01) closed the third and final author-in-command loop: writing (canvas V1.76) + work quality (findings V1.77) + creator self (memory V1.78). The product now ships a complete author-in-command experience. V1.79 is the **first post-loop-closure iteration** and takes the natural next step: rather than opening a new loop, it deepens the author's ability to **reflect on** what the closed loops produce — *read* what they wrote (Track A) and *see who they are becoming* via their internalized SOUL (Track B).

This iteration also records one **strategic cancellation**: DF-49 (Standalone MCP server) is cancelled (not deferred) because it structurally conflicts with the ACP-client product direction (`STRATEGY.md`: "CLI is an ACP client, not a server") and introduces a circular-invocation risk (Nexus drives an agent via ACP → that agent calls back into Nexus via MCP → loop). See §Non-Goals + P-last tracker hygiene.

## 1. grill-me locked decisions

| Decision | Resolution |
|---|---|
| Main direction | **C + D dual-track** under the shared theme **"Author Reflection"**. (DF-49 rejected — conflicts with ACP-client product direction + circular-invocation risk; cancelled in P-last.) |
| C scope | **Reading surface + in-context lightweight maturation indicators** (chapter completion-state badge + World KB density count + open-findings count). No standalone maturation dashboard. C = M. |
| D MVP | **Keyword clusters + temporal drift** (growth-count folded into the timeline). Independent growth-curve view → deferred tracker. D = M. |
| B companion | **Light** — close only `R-V178P0-QC3-001` (web typecheck build-order CI/prebuild wrapper). `R-V178P0-QC3-003` (synchronous-review reliability) → reliability roadmap / deferred tracker. |
| Wire contracts | C: no change. D: additive (`memory-fragment-info` gains `keywords` + `created_at`) → `@42ch/nexus-contracts` 0.13.0 → 0.14.0. |
| Plan structure | P-1 (prepare) + P0 (C) ‖ P1 (D, parallel worktree) + P-last (closure; B folded in). 4 plans. |
| Branch policy | Integration `iteration/v1.79` from `main` (`0015694f`); per-plan topic `feature/v1.79-<slug>`; C+D parallel → worktree isolation required (per `.mstar/AGENTS.md` two-tier branch model). |
| DF-49 disposal | **CANCEL** (not defer) — rationale: conflicts with ACP-client product direction + circular-invocation risk. Move to cancelled archive in P-last. |

**Naming convention**: This compass uses multiple labels for two workstreams; the table below maps them for cross-reference. Prose below uses **Track A / Track B** for workstream discussions and **P0 / P1** for plan-level references.

| Concept | Spec label | Track | Plan ID |
|---|---|---|---|
| Manuscript reading surface + maturation indicators | C / SP-1 | Track A | P0 (`2026-07-01-v1.79-manuscript-reading-surface`) |
| SOUL personality visualization | D / SP-2 | Track B | P1 (`2026-07-01-v1.79-soul-personality-visualization`) |

## 2. Scope

This iteration locks two spec points (read-only consumption surfaces on already-shipped data):

- **SP-1 (Track A / C)**: A real manuscript **reading surface** — promote the post-V1.75-pivot residual `chapter-page.tsx` (currently a bare read-only body render + frontmatter strip + Copy Path + canvas redirect) into a designed reading experience (typography, chapter/volume navigation, reading progress) with **in-context lightweight maturation indicators** (chapter completion-state badge, World KB density count, open-findings count). Read-only; consumes existing data (`work_chapters.status`, `kb_key_blocks` count, `findings` 6-state lifecycle). No new write routes.
- **SP-2 (Track B / D)**: **SOUL personality visualization** — a visualization layer over the creator's internalized memory fragments: keyword clusters (what themes the creator has internalized) + temporal drift (how keyword-theme composition shifts over time, with fragment-accumulation count folded into the timeline). Requires a small additive wire DTO extension (`memory-fragment-info` gains `keywords` + `created_at`, already present in the `memory_fragments` table but not exposed over the wire).

### 2.1 User stories (author persona)

These stories define the product success criteria for V1.79. Implementation decisions serve these stories; the stories are not negotiable but the implementation path is.

**Track A — Reading surface**

- *As an author* who has just completed a writing session, I can open any chapter in a comfortable reading view with legible typography and navigate freely between chapters and volumes, so I can review my prose as a reader would — not as an editor staring at raw markdown or file paths.
- *As an author* reading through my manuscript, I can see at a glance — without leaving the reading surface or opening a dashboard — whether a chapter is drafted or finalized, how many World KB entities are connected to it, and how many open findings remain, so I know what needs attention before I move on to the next chapter or a review pass.
- *As an author* picking up where I left off mid-session, my reading position is preserved so I can resume without hunting for the right scroll point.

**Track B — SOUL visualization**

- *As an author*, I can see what themes and concepts my creative work has internalized into my SOUL memory — displayed as keyword clusters surfacing the top accumulated themes — so I understand what my AI assistants and I have been focusing on, even if I haven't consciously tracked it.
- *As an author*, I can see how my creative themes have shifted over time — which keywords rose, which faded, with fragment-accumulation count folded into the timeline — so I can reflect on "who I am becoming" as a writer who works with AI, and notice when my focus drifts in an unintended direction.
- *As a new author* with little accumulated SOUL data, I see a graceful, encouraging empty state that communicates "this is what your SOUL will show once you've written and reviewed more" rather than a broken or blank dashboard — so I understand the feature's value proposition even before I've built up enough usage, and I know what actions (writing, reviewing) will populate it.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-01-v1.79-prepare-dual-track-spec-amendments` | P-1 — Prepare (dual-track spec + DESIGN.md amendments) | Done | Specs (web-ui §25, creator-workflow, manuscript-audit) + wire DTO contract (memory-fragment-info schema) landed in iteration-start; DESIGN.md token stubs added. QC skipped (docs/token-stubs only). |
| `2026-07-01-v1.79-manuscript-reading-surface` | P0 — Track A: Manuscript Reading Surface + lightweight maturation indicators | Done | QC tri-review 3/3 Approve (qc1/qc3 after fix-wave: honest "N+" findings label via `has_more` + chapter-nav cursor-walk across daemon's 100-cap; qc2 Approve clean — no-write invariant). QA Pass. Wire: no change. 2 low residuals → V1.80+. |
| `2026-07-01-v1.79-soul-personality-visualization` | P1 — Track B: SOUL Personality Visualization (keyword clusters + temporal drift) | Done | QC tri-review 3/3 Approve (clean first pass). QA Pass. Wire: additive `memory-fragment-info` + `keywords`/`created_at`; `@42ch/nexus-contracts` 0.13.0 → 0.14.0. 2 low residuals → V1.80+. |
| `2026-07-01-v1.79-closure` | P-last — Closure (B build-order + DF-49 cancel + compound + compaction + PR) | Done | T1 build-order (`R-V178P0-QC3-001` closed via pretypecheck) + T2 DF-49 cancelled (ACP-client conflict) + T3 `R-V178P0-QC3-003` → reliability roadmap + T4 BL-09..12 deferred + T5 compound (pagination-cursor-without-total pattern) + T6 Profile B compaction (4 plans archived) + T7 trackers/STRATEGY/README + T8 PR. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

**Parallelism**: P0 and P1 are independent by default and must remain file-disjoint: P0 = `apps/web` reading route + `chapter-page.tsx` and existing read-only client calls; P1 = `apps/web` memory route + new viz components + `schemas/local-api/memory/` + generated `@42ch/nexus-contracts` / Rust contracts output. Shared-client collision risk (`apps/web/src/lib/nexus/types.ts`, query keys, or API query helpers) is additive-only and must be integration-sequenced: P0 lands first if it only consumes existing APIs; P1 lands after codegen if generated memory DTO imports change. Both dispatch from P-1 lock; both run in separate worktrees; both merge into `iteration/v1.79`.

## 4. Branch policy

Per `.mstar/AGENTS.md` two-tier branch model:

| Tier | Branch | Purpose |
|---|---|---|
| Integration | `iteration/v1.79` (from `main` @ `0015694f`) | Single line where all plan work lands before QC/QA; QC/QA `Working branch`. |
| Final landing | `main` | `iteration/v1.79` merges here via PR after iteration sign-off. |
| P-1 topic | `feature/v1.79-prepare` | Spec/DESIGN.md amendments only. |
| P0 topic | `feature/v1.79-reading-surface` | Track A commits only. |
| P1 topic | `feature/v1.79-soul-viz` | Track B commits only. |
| P-last topic | `feature/v1.79-closure` | Closure commits only. |

**Worktree isolation**: required for P0 ‖ P1 (same-repo parallel writers). P-1 → P0/P1 sequential (P-1 must lock before implement dispatch). P0 must not touch `schemas/local-api/memory/`, generated contract output, or memory-viz route/components; P1 must not touch the reading route / `chapter-page.tsx`. Shared web client files, if unavoidable, are additive-only and resolved on `iteration/v1.79` after topic-branch merge, not by broad refactors in either track. P-last sequential after P0 + P1 merge.

## 5. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Compass locked (§1.6 Review & Edit chain complete) | 2026-07-01 | pending |
| P-1 Prepare complete (specify/clarify/plan both tracks) | 2026-07-01 | pending |
| P0 + P1 dev complete (parallel) | 2026-07-02 | pending |
| QC tri-review complete (both tracks) | 2026-07-02 | pending |
| QA Pass | 2026-07-02 | pending |
| Iteration close (P-last + PR) | 2026-07-02 | pending |

## 6. Acceptance Criteria

### Product verification (author-facing — what a user experiences)

- **SP-1 (Reading surface)**: A first-time user opening a chapter sees legible, comfortable reading typography (light + dark) that does not require configuration. The chapter/volume navigation is immediately discoverable; keyboard shortcuts (←/→) work on the first attempt. The three maturation indicators (completion badge, KB density, open-findings count) are visible without scrolling and interpretable without tooltips — an author can look at a chapter and immediately know "this chapter is finalized, has a rich World KB, and still needs 3 findings addressed." The V1.75 residuals (body prose render, frontmatter metadata strip, Copy Path, "Edit outline → Canvas" redirect CTA) are all preserved and functional.
- **SP-2 (SOUL viz)**: A user with internalized memory sees meaningful keyword clusters — not a raw tag dump, but the top themes the creator has accumulated — and a temporal-drift timeline that communicates "how my focus shifted" at a glance. A new creator with zero fragments sees a graceful, encouraging empty state that explains what the visualization will show and what actions populate it — no broken charts, no error-state styling, no implication that the feature is unavailable.
- **Reading surface value**: The reading surface is a net improvement over the pre-V1.79 bare chapter body render — it adds typography, navigation, progress, and maturation context without removing anything. The body-ownership invariant holds (no write route implied or offered).
- **SOUL viz value**: The viz answers a real author question ("what am I becoming?") using real data the system already stores. The temporal dimension is the differentiating insight; keyword clusters without drift would be a tag cloud, not a reflection tool.

### Technical verification

- Wire additive: `memory-fragment-info` exposes `keywords` + `created_at`; round-trip regression test green; `@42ch/nexus-contracts` 0.13.0 → 0.14.0; `pnpm run codegen` + `cargo build` + `validate-schemas` clean.
- No new write routes on the reading surface body — QC2 verifies body-ownership invariant intact.
- B: `R-V178P0-QC3-001` resolved (web typecheck build-order self-contained via prebuild/CI wrapper).
- DF-49 cancelled in the deferred-features tracker with the ACP-client-conflict + circular-invocation rationale; moved to cancelled archive.
- `R-V178P0-QC3-003` (synchronous-review reliability) explicitly recorded in deferred tracker under a "reliability roadmap" target (not silently dropped).
- Independent growth-curve view (D stretch) recorded in deferred tracker (Durable Roadmap Gate).
- QC tri-review 3/3 Approve (both tracks); QA Pass; `cargo clippy --all -- -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check` + `validate-schemas` clean; web typecheck/test green.
- `status.json` coherent (Profile B: only non-Done plans in `plans[]`; P-last compaction done).

## 7. Non-Goals

- **DF-49 Standalone MCP server** — cancelled (conflicts with ACP-client product direction per `STRATEGY.md`; circular-invocation risk). Not deferred.
- Standalone maturation dashboard (multi-chart cross-Work/World aggregate view) — C is reading surface + in-context indicators only.
- Independent growth-curve view as a separate D visualization — folded into the temporal-drift timeline; standalone deferred.
- `R-V178P0-QC3-003` synchronous-review reliability rewrite (async/cancelable/concurrency-bounded) — reliability roadmap, future iteration.
- Desktop signing rollout — blocked on Apple Developer ID cert (non-agent-driven).
- Any new write route on the reading surface — read-only consumption only.
- Cloud sync / platform unpause — remains paused (PD-05).

## 8. Roadmap Position

- **Current iteration (V1.79, delivered 2026-07-01)**: First post-loop-closure "Author Reflection" iteration. Track A shipped the manuscript reading surface (typography + nav + session-only progress + lightweight maturation indicators: completion-state badge + World KB density + open-findings "N+" via `has_more`; read-only, body-ownership invariant preserved). Track B shipped SOUL personality visualization (keyword clusters + temporal drift) over internalized memory fragments; additive wire DTO (`memory-fragment-info` + `keywords`/`created_at`; `@42ch/nexus-contracts` 0.13.0 → 0.14.0). QC 3/3 Approve both tracks (P0 after a fix-wave for pagination correctness). **DF-49 (Standalone MCP server) cancelled** — conflicts with the ACP-client product direction + circular-invocation risk. P-last closed `R-V178P0-QC3-001` (pretypecheck), routed `R-V178P0-QC3-003` to a reliability roadmap, registered 4 V1.80+ deferred items, and ran compound (`pagination-cursor-without-total-count-labels` pattern). 4 low QC residuals → V1.80+.
- **Next iteration (V1.80)**: Candidate evaluation at next `/iteration-start`. Backlog candidates after V1.79: standalone maturation dashboard (if Track A's in-context indicators prove insufficient), independent SOUL growth-curve view, deeper manuscript reading (annotations/highlights), reliability hardening iteration (pick up `R-V178P0-QC3-003` + any accumulated reliability residuals). Owner: `@project-manager`. Trigger: user initiates next `/iteration-start`.
- **Final goal**: Nexus as a local-first, AI-agent-driven creative writing tool where the author commands three closed loops (steer writing, triage quality, curate memory) **and** can reflect on the accumulated craft and evolving creative self.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| P0 reading surface and P1 viz both UI-heavy → unfocused dual-track | Med | Med | Strict file-disjoint boundary (reading route vs memory route); P-1 locks both specs before parallel dispatch; PM enforces scope discipline. |
| D temporal-drift viz weak when fragment data is sparse (new creators) | Med | Low | Design empty-state + low-data graceful fallback in P-1; viz must degrade to "not enough memory yet" rather than render broken charts. |
| `memory-fragment-info` DTO extension breaks a consumer | Low | Med | Contract impact is limited to `schemas/local-api/memory/memory-fragment-info.schema.json`: optional `keywords: string[]` + optional `created_at: string` (RFC 3339 by description, plain-string family convention). Required keys stay `fragment_id` + `summary`; additive-only codegen must preserve old consumers while regenerating Rust + TS DTOs and extending `memory_dto_roundtrip.rs`. |
| Contracts package version not bumped with additive schema | Low | Med | P1 owns the codegen/version bump: `packages/nexus-contracts/package.json` 0.13.0 → 0.14.0 because the public npm contract gains fields, even though pre-1.0 breaking-change policy is permissive. Rust crate remains workspace-versioned and generated only. |
| Reading surface accidentally reintroduces a write path (violates V1.75 pivot) | Low | High | P-1/P0 specs explicitly mark read-only; QC2 (security/correctness) checks no new POST/PATCH/DELETE route, no body-mutation client call, and no alternative body editor is reachable from reading. Canvas remains the sole authoring surface per V1.75. |
| DF-49 cancellation debate reopens | Low | Low | Rationale recorded in compass §0 + §7 + P-last tracker note; STRATEGY.md ACP-client decision cited verbatim. |
| Reading surface underused if authors prefer their own reading tools (external editors, e-readers, dedicated reading apps) | Med | Med | The reading surface is positioned as **in-context review** (not a replacement reading app); maturation indicators deliver value even during a quick skim — the badge + counts are visible without deep reading. Chapter/volume navigation + session progress add utility beyond the bare chapter body render the user already has today. If post-V1.79 engagement data shows low reading-surface usage, the V1.80 standalone maturation dashboard candidate (cross-Work/World aggregate) becomes the higher-leverage investment without wasting the reading-surface work — the maturation indicator components are reusable. |
| SOUL viz displays thin or uninteresting data for authors with little accumulated memory (new creators, infrequent users, early in a Work) | Med | Med | Empty-state and low-data design with empathetic, encouraging copy (see P1 plan §F product copy guidance). Viz degrades gracefully: rich clusters → simple frequency view → "not enough data yet" (no broken charts at any state). Per-creator scope means the viz shows content proportional to actual usage — empty SOUL after zero review sessions is the correct UX, not a defect. The feature's value proposition is prospective ("here is what you will see") not retrospective — the empty state must sell the vision. |

## 10. Wire contracts note

`wire_contracts_changed: TRUE` for Track B only. The exact schema file is `schemas/local-api/memory/memory-fragment-info.schema.json`; the additive change is two optional properties: `keywords` (`array` of `string`) and `created_at` (`string`, RFC 3339 timestamp by description; sibling memory schemas use plain strings rather than JSON Schema `format`). `required` remains `fragment_id` + `summary`, so old consumers can continue deserializing. P-1 edits the schema only; P1 must run `pnpm run codegen`, commit generated Rust + TypeScript output, bump `packages/nexus-contracts/package.json` from 0.13.0 to 0.14.0, wire `crates/nexus-daemon-runtime/src/api/handlers/memory.rs::fragments`, and extend `crates/nexus-daemon-runtime/tests/memory_dto_roundtrip.rs`. Track A (C) changes no schema and may only consume existing read-only endpoints.

## Compound Round Summary

- Compound docs created: **1** — [`architecture-patterns/pagination-cursor-without-total-count-labels.md`](../knowledge/architecture-patterns/pagination-cursor-without-total-count-labels.md) (Knowledge track; surfaced from the P0 fix-wave — Nexus cursor-paginated endpoints have no `total`, so count affordances render honest "N+" lower-bound labels via `has_more`).
- New CONCEPTS.md entries: **0** ("PaginationInfo" is a wire-contract term, not a domain noun needing CONCEPTS).
- Triggered compound-refresh: **no** (the new doc is distinct from the V1.78 `contracts-gap-on-shipped-backend` doc; no stale older doc to consolidate).

## Iteration Retrospective (minimal)

- **做得好的**:
  - grill-me 锁定方向时用户精准否决了 DF-49（ACP-client 冲突 + 循环调用）—— 避免了一个结构性方向错误；本迭代把 cancel 落档，是正确的战略卫生。
  - C+D 双轨 file-disjoint 并行 worktree 顺畅；唯一合并冲突（DESIGN.md YAML 两轨都填 token）机械可解。
  - P0 QC fix-wave 中 dev 发现 `PaginationInfo` 无 `total` 字段，选择了 contract-faithful 的 "N+" 方案而非臆造字段 —— 守住了 wire-contract 不变量，并结晶为可复用知识。
- **可改进的**:
  - PM 初始 fix-wave brief 假设 `pagination.total` 存在（基于记忆而非查契约）—— 应在写 brief 前先核对生成类型；这正是 compound 文档要防止的下次踩坑。
  - DESIGN.md 是两轨共享编辑点 —— P-1 本可让 architect 在 prepare 阶段就把两轨 token 写入（而非各自 implement 时填），避免合并冲突。
- **下迭代建议**:
  - V1.80 候选评估：独立 maturation 仪表盘、SOUL 增长曲线视图、深度阅读（标注/highlight）、或可靠性加固迭代（收 `R-V178P0-QC3-003`）。owner `@project-manager`。
  - 收 4 个 low V1.79-QC residuals（P0 键盘 nav 测试 + 组件单测；P1 memory-page 拆分 + token 提升）—— 可作为下迭代伴随轻轨。
