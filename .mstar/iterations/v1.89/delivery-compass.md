---
iteration_id: V1.89
start_date: 2026-07-04
status: completed
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-04-v1.89-prepare-dual-track
  - 2026-07-04-v1.89-reading-depth-backend
  - 2026-07-04-v1.89-reading-depth-frontend
  - 2026-07-04-v1.89-closure
---

# V1.89 — Deeper Manuscript Reading — Delivery Compass v1

**Status**: locked (Phase 1 Review & Edit chain complete: product-manager → architect → writing-specialist → PM lock).
**Author**: `@project-manager` (compass + grill-me locked scope).

## 0. Context

V1.88 paid down the V1.85–V1.87 residual slate and cleaned up the deferred-features tracker. The product now has a clean base. V1.89 returns to author-facing value by completing the most natural deferred follow-up from V1.79: **BL-11 — deeper manuscript reading**.

V1.79 shipped a read-only reading surface with session-only progress and lightweight maturation indicators. V1.89 makes that surface durable and interactive: persisted reading progress and annotations/highlights on the frontend, plus the backend persistence and Local API needed to support them. Profile-specific reading chrome remains out of scope (deferred back to the tracker).

## 1. Locked Decisions (grill-me output)

| Decision | Resolution |
|---|---|
| Iteration direction | **B — Deeper manuscript reading (BL-11 MVP slice)**. Persisted reading progress + annotations/highlights. Non-goals: standalone maturation dashboard, profile-specific reading chrome, new write surfaces on chapter body. |
| Scope slice | **MVP**: persisted progress + annotations/highlights. Profile chrome deferred. |
| Persistence layer | **Daemon SQLite** + Local API. Cross web/Tauri unified; survives browser changes. New tables `reading_progress` and `reading_annotations`. |
| Annotation anchoring | **Character offsets** into current body plain text. Body edits after highlighting may drift; UI shows a drift notice. No fingerprint/CRDT reconciliation in MVP. |
| Plan structure | **P-1** Prepare (spec + schema + DB migration + DESIGN.md tokens) → **P0** backend (`@fullstack-dev`) ‖ **P1** frontend (`@frontend-dev`) parallel worktrees → **P-last** closure. |
| Branch policy | `iteration_base_branch=main` (current HEAD after V1.88 merge); `spec_integration_branch=iteration/v1.89`; `target_branch=main`. |
| Contract impact | `wire_contracts_changed: TRUE` — additive schemas for reading progress + annotations; `@42ch/nexus-contracts` 0.17.0 → 0.18.0. |
| Body-ownership invariant | Reading surface remains **read-only for the body**. Annotations and progress are separate tables that reference the chapter; they do not mutate `body_path` or outline files. Canvas remains the sole authoring surface per V1.75 pivot. |

## 2. Scope

This iteration locks three delivery items (SP-1 to SP-3) plus closure (SP-4):

- **SP-1: P-1 Prepare**. Lock spec amendments, additive JSON Schema contracts, SQLite migration design, and DESIGN.md token stubs so P0 and P1 can run in parallel.
- **SP-2: Reading progress persistence (P0 backend + P1 frontend)**.
  - Backend: `reading_progress(creator_id, work_id, chapter, scroll_progress, updated_at)` with upsert semantics.
  - Local API: `GET /v1/local/reading/progress` + `PUT /v1/local/reading/progress`.
  - Frontend: reading surface restores saved scroll position on mount and saves progress on scroll stop / page leave.
- **SP-3: Annotations / highlights (P0 backend + P1 frontend)**.
  - Backend: `reading_annotations` table + CRUD Local API (`POST`, `GET list`, `PATCH note/color`, `DELETE`).
  - Frontend: text selection → highlight, annotation list inspector, color + note editing, overlay rendering on `ReadingProse`.
- **SP-4: Closure (P-last)**. QC tri-review + QA + tracker hygiene (remove BL-11 open row if fully shipped; keep profile-chrome deferred) + compound + Profile B compaction + PR to `main`.

### 2.1 Architecture Hierarchy and Ownership

- **P-1 lives in specs + schemas + DESIGN.md**: `specs/web-ui.md` new section, `schemas/local-api/reading/`, `apps/web/DESIGN.md` token stubs, `crates/nexus-local-db/migrations/` migration file (schema-only; no runtime code). No codegen run in P-1.
- **P0 lives in `crates/nexus-local-db/` + `crates/nexus-daemon-runtime/src/api/`**: migration, DAO, Local API handlers, tests. P0 owns generated contract output + npm package version bump + `validate-schemas`.
- **P1 lives in `apps/web/src/components/reading/` + `apps/web/src/pages/chapter-page.tsx` + `apps/web/src/api/queries.ts`**: progress restore/save, highlight selection/rendering, annotation inspector, tests. P1 consumes existing chapter body endpoint and new reading endpoints; P1 does not edit daemon code or schemas.
- **Shared client surface**: `apps/web/src/lib/nexus/types.ts` (`NexusClient` method additions) and `apps/web/src/lib/nexus/browser-client.ts` are additive-only and must be coordinated at integration merge.

### 2.2 Product Success Criteria (measurable, product-meaningful)

**Primary outcome (author value)**:
- An author reopening a chapter sees their last scroll position restored automatically across browser tabs/sessions and in the Tauri desktop shell.
- An author can select text in the reading surface and create a highlight + optional note; the highlight survives page navigation and reappears on return.
- Annotations are listed in a side inspector and can be edited or deleted.

**Technical parity (no regression bar)**:
- Reading surface remains read-only for the body; no POST/PATCH/DELETE touches `body_path` or outline files from the reading UI.
- Existing V1.79 reading UX (typography, chapter nav, maturation indicators, canvas redirect) is preserved.
- `cargo test -p nexus-daemon-runtime` + `cargo clippy --all -- -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check` pass.
- `pnpm --filter web run typecheck/test` and `pnpm --filter @42ch/nexus-contracts run build` pass.
- `validate-schemas` clean.

**Governance**:
- QC tri-review consolidated Approve; QA verifies progress restore, highlight create/list/edit/delete, and body-ownership invariant.
- Compass `status: completed` at Phase 3.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-04-v1.89-prepare-dual-track` | P-1 — Prepare (spec + schema + migration + DESIGN tokens) | Todo | Blocks P0/P1. No codegen run yet. |
| `2026-07-04-v1.89-reading-depth-backend` | P0 — Track A: Reading depth backend (progress + annotations persistence + Local API) | Todo | `@fullstack-dev`; from `iteration/v1.89`. |
| `2026-07-04-v1.89-reading-depth-frontend` | P1 — Track B: Reading depth frontend (progress restore + highlight/annotation UI) | Todo | `@frontend-dev`; from `iteration/v1.89`; parallel after P-1 lock. |
| `2026-07-04-v1.89-closure` | P-last — Closure (QC/QA + tracker + compound + compaction + PR) | Todo | PM. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## 4. Milestones (agent-oriented wall-clock estimates)

| Milestone | Target date (agent wall-clock) | Status |
|-----------|--------------------------------|--------|
| Compass + plans locked (Phase 1 Review & Edit chain done) | 2026-07-04 | completed |
| P-1 Prepare complete (specify/clarify/plan locked) | 2026-07-04 | completed |
| P0 + P1 dev complete (parallel worktrees) | 2026-07-05 | completed |
| QC tri-review Approve | 2026-07-05 | completed |
| QA Pass | 2026-07-05 | completed |
| Iteration close + PR to `main` | 2026-07-06 | completed |

## 5. Acceptance Criteria (measurable, product-meaningful)

**Author-visible outcomes (primary):**
- An author reopening a chapter in the reading surface sees their last scroll position restored automatically (across reloads, tabs, and Tauri desktop shell).
- An author can select any contiguous text in the reading surface, invoke "Highlight", and see an immediate visual overlay. The highlight persists after navigation away and back.
- Highlights appear in a side annotation inspector showing color swatch + optional note snippet + timestamp. The author can edit the note or color, or delete the highlight; the overlay updates or disappears instantly.
- When a highlight's stored offsets no longer fit the current body text (after a body edit in canvas), the UI surfaces a clear, non-blocking drift notice ("标注可能因正文编辑而偏移" / "This highlight may have shifted after body edits") instead of mis-rendering or silently failing.
- Scroll progress and highlights are personal (per-creator) and survive daemon restart and app relaunch.

**Technical & invariant outcomes:**
- `reading_progress` and `reading_annotations` tables exist with correct constraints and indexes.
- Local API endpoints exist, are documented in the spec, and pass handler-level tests:
  - `GET /v1/local/reading/progress?work_id=&chapter=` returns the saved progress (or default 0).
  - `PUT /v1/local/reading/progress` upserts progress.
  - `GET /v1/local/reading/annotations?work_id=&chapter=` lists annotations.
  - `POST /v1/local/reading/annotations` creates a highlight.
  - `PATCH /v1/local/reading/annotations/{annotation_id}` edits note/color.
  - `DELETE /v1/local/reading/annotations/{annotation_id}` deletes.
- No write route touches chapter body or outline files from the reading surface (body-ownership invariant).
- `wire_contracts_changed: TRUE` — additive schemas committed; `@42ch/nexus-contracts` 0.17.0 → 0.18.0.
- QC tri-review consolidated Approve; QA Pass; all CI gates green.

## 6. Non-Goals (explicit to prevent scope creep)

- **Profile-specific reading chrome** (essay section breaks, game-bible cross-reference overlays, novel-specific typography presets) — deferred back to the tracker.
- **Standalone maturation dashboard** (BL-09) — not in scope; remains in deferred tracker.
- **Body/Outline editing from the reading surface** — canvas remains the sole authoring surface per V1.75 pivot.
- **Annotation range reconciliation across body edits** — MVP uses character offsets with a drift warning only.
- **Rich-text annotations / inline comments threaded by selection** — MVP is single note per highlight.
- **Real-time sync or cloud features** — platform remains paused (PD-05).
- **Desktop signing / auto-update** — blocked on external cert; not an agent deliverable.

## 7. Roadmap Position / Next Iteration Transition

- **Current iteration (V1.89)** — active: completes BL-11 MVP (persisted reading progress + annotations/highlights). The reading surface becomes a durable, interactive review tool without breaking the V1.75 body-ownership invariant.
- **Next iteration (V1.90) transition criteria**:
  - Trigger: V1.89 merged to `main`, BL-11 row moved to shipped archive, profile-chrome row remains in deferred tracker.
  - Selection input: PM reviews remaining backlog (BL-09 standalone maturation dashboard, BL-11 profile-specific reading chrome, or another strategic surface) against the now-shipped reading depth.
  - Output of V1.89 for V1.90: a reading surface with persisted state and annotations, ready for profile-aware chrome or cross-Work aggregation.
- **Long-term goal**: Nexus as a local-first creative-writing tool where authors can read, review, and annotate their manuscript as naturally as they write it — all on their own machine.

## 8. Delivery Branch Policy

> Mirror of frontmatter; kept in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.89` |
| `target_branch` | `main` |

Per-plan topic branches:

| Plan | Working branch | Merge target |
|---|---|---|
| P-1 | `feature/v1.89-prepare` | `iteration/v1.89` |
| P0 | `feature/v1.89-reading-depth-backend` | `iteration/v1.89` |
| P1 | `feature/v1.89-reading-depth-frontend` | `iteration/v1.89` |
| P-last | `feature/v1.89-closure` | `iteration/v1.89` |

**Worktree isolation**: required for P0 ‖ P1 (same-repo parallel writers). Shared web client files (`apps/web/src/lib/nexus/types.ts`, query keys) are additive-only and resolved on `iteration/v1.89` after topic-branch merges.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation | Verification |
|------|-----------|--------|------------|--------------|
| Character-offset annotations drift confusingly after body edits | Med | Med | MVP explicitly acknowledges drift; UI warns rather than silently mis-rendering. | Manual QA + unit test for clamp behavior on shortened body. |
| P0/P1 file collision on shared client surface | Med | Med | P-1 locks `NexusClient` method names and DTO shapes; changes are additive-only; integration merge resolves conflicts on `iteration/v1.89`. | `pnpm --filter web run typecheck` passes after integration merge. |
| Scroll-progress save causes excessive API writes | Low | Med | Debounce scroll events (e.g., 500 ms) and save on `beforeunload`/`visibilitychange`; avoid per-pixel writes. Server-side `CHECK (scroll_progress >= 0 AND scroll_progress <= 10000)` constraint as defense-in-depth. | Network mock test asserts ≤N calls per scroll burst. |
| Highlight overlay rendering breaks on dark mode / long prose | Low | Med | Reuse `ReadingProse` rendering pipeline; DESIGN.md token contract for highlight backgrounds in light+dark. | Visual regression via component tests + QA. |
| Scope expands to include profile chrome | Low | High | Explicitly list profile chrome as non-goal and deferred tracker row; PM guards dispatch. | Compass §6 + P-last tracker check. |

## 10. Compound Round Summary

- 结晶文档数：0（本迭代以 spec/tracker 更新为主，无新增独立知识文档）
- 新增 CONCEPTS.md 条目：0
- 触发 compound-refresh：否

## 11. Iteration Retrospective (minimal)

- 做得好的：P-1 锁定 spec/schema/DESIGN 后 P0/P1 并行工作树高效推进；QC 三审 + QA 一次性通过。
- 可改进的：product-manager subagent 在 P-1 中递归 dispatch 了 architect，违反 harness 委派规则；后续 Assignment 需强化反递归门禁。
- 下迭代建议：V1.90 评估 BL-13 profile-specific reading chrome 或 BL-09 standalone maturation dashboard。
