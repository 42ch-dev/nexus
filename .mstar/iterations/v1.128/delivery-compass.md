---
iteration_id: V1.128
start_date: 2026-07-20
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.128
plans:
  - 2026-07-20-v1.128-p0-surfaces-chrome-ia
  - 2026-07-20-v1.128-p1-nle-timeline-canvas
  - 2026-07-20-v1.128-p2-creator-create-controller-shell
  - 2026-07-20-v1.128-p3-web-alias-clarity
---

# V1.128 Delivery Compass — Design Studio + UI dogfood feedback

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **L** — 4 business plans).
> Caller direction: design-studio / UI component dogfood feedback (Surfaces sidebar, Selection overlay, Banner retire, NLE Timeline, `@web-*` clarity, Creator Create vs Controller shell).
>
> **Phase 1 Review & Edit chain: COMPLETE — compass LOCKED.**
> - Seat 1 (product-manager): User value + author-observable ACs strengthened on compass and all four specs; P0 Banner retire + Selection reopen locked **Must**; P1 NLE scope capped for dogfood verify (Studio-first, no editor parity); P2 Create CTAs vs Controller **Back** author semantics clarified (Controller business TBD); P3 labeling/docs prioritized over mass promotion.
> - Seat 2 (architect): Architecture locked on all four specs + plans. **P2 Back** → clear `CreatorEntitySelectionContext` → Create page (reject list-highlight alternate). **P2 selection SSOT** → new `CreatorEntitySelectionContext` in layout layer (not route params, not `submenuItem`). **P1 RF boundary** → pull-off is Studio presentational state only; no new `@xyflow/react` in P1 extracts; App adopt swaps `@web-canvas/*` chrome into existing RF hosts only. Dependency order **P0 → P1 → P2 → P3** confirmed; extract ownership: P1 `@web-canvas/*`, P2 `@web-layout/creator-shell-content`, P3 labels/docs only. All plans: `wire_contracts_changed: false`.
> - Seat 3 (writing-specialist): Spec status lines normalized (`product-reviewed, architect-locked, writing-hygiene done`); terminology aligned (Create page, Controller Panel stub, `@web-*`, `@42ch/nexus-ui`); cross-links between P0–P3 specs; compass + package README polished. Deferred-features tracker unchanged — no new DF id for V1.128 scope.
>
> Direction is **locked** — do not re-question the Studio-first UI dogfood bundle.

## Autonomous direction lock record

**Scale budget:** L = 4 business plans (caller `L ~ L+`; overflow → Roadmap, not a 5th plan). Harness process not counted.

**Caller direction mapping:**

| Caller phrase | Plan |
|---------------|------|
| Surfaces Submenus → sidebar | P0 |
| Selection Submenu modal blocks preview | P0 |
| Banner should already be removed | P0 |
| Canvas Timeline NLE / multi-track / pull-off / Studio then App | P1 |
| `@web-*` naming confusion vs `@42ch/nexus-ui` | P3 |
| Shell Creator left: Create page vs Controller Panel stub | P2 |

**Branch policy (autonomous resolve):**

- `iteration_base_branch: main` — from `status.json` root `metadata.iteration_base_branch` (V1.122→V1.127).
- `target_branch: main` — from `status.json` root `metadata.target_branch`.
- `spec_integration_branch: iteration/v1.128` — cut from `main`.

### Locked direction (single sentence)

Ship a Studio-first UI dogfood pass that fixes Surfaces chrome (sidebar nav, Banner retire, Selection overlay), delivers an NLE-style multi-track Timeline gallery (then thin App adoption), introduces Creator **Create page** vs **Controller Panel stub** shell modes, and clarifies the `@web-*` vs `@42ch/nexus-ui` two-tier import story.

### Dependency graph (locked)

```
P0 (Surfaces chrome) ──┐
P1 (NLE Timeline)     ─┼──► P3 (alias clarity / gallery labels)
P2 (Create/Controller)─┘
```

Serial Phase 2 order: **P0 → P1 → P2 → P3** (architect-locked — no parallel implement tracks).

**Cross-plan extract ownership (architect-locked):**

| Plan | Owns | P3 labels only |
|------|------|----------------|
| P0 | Studio routing/fixtures — no new `@web-*` extracts | — |
| P1 | `@web-canvas/*` NLE timeline chrome (presentational; RF-free) | P3 badges on Canvas surfaces |
| P2 | `@web-layout/creator-shell-content` (Create page + Controller stub) | P3 badges on Shell surfaces |
| P3 | Docs + gallery badges — **no** extract ownership | — |

## User value

**Before V1.128**, a maintainer dogfooding Design Studio Surfaces fights horizontal pill nav, a ghost Banner section that no longer exists in App, a Selection Agent dialog that blocks the whole gallery page, Timeline fixtures that do not read as an NLE canvas, Creator shell fixtures that do not show Create vs Controller modes, and `@web-*` import paths that look like package names.

**After V1.128**, the same maintainer:

- Switches Surfaces sections from a persistent left sidebar in one click; Banner is gone from Studio; Selection Agent dialog opens inside a scoped frame and can be closed and reopened without hiding sibling variants.
- Reviews an NLE-style multi-track Timeline in Studio (centered band, horizontal scrub feel, pull-off demo) and sees the same chrome thin-wired in App — without expecting trim/ripple/audio-editor parity.
- Sees Creator content region flip between a **Create** page (World/Work CTAs per V1.125 honesty) and a **Controller Panel stub** (placeholder + **Back** that returns to Create); Controller business widgets remain TBD.
- Reads gallery badges and AGENTS/spec docs that distinguish `@web-*` App extracts from `@42ch/nexus-ui` promoted primitives — no mass migration required.

Net: Studio becomes a trustworthy visual proving ground for the next round of author-facing dogfood; import and shell IA confusion drops before deeper Controller content lands.

## Scope

- Surfaces section nav → persistent left sidebar; **retire Banner from Studio (Must)**; **fix Selection Agent-dialog fixture blocking + reopen (Must)**
- NLE-style Timeline in Design Studio (centered band, horizontal scrub, multi-track layers, pull-off affordance) + thin App adoption — **dogfood-verify slice only**
- Creator shell: no selection → Create page with honest CTAs; selection → Controller Panel stub (**Back** returns to Create) — business content TBD
- Gallery/docs clarify `@web-*` transitional extracts vs `@42ch/nexus-ui` promoted primitives — **labeling over promotion**

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-20-v1.128-p0-surfaces-chrome-ia | P0 — Surfaces chrome IA | Todo | **Must** — Banner retire + Selection reopen non-negotiable |
| 2026-07-20-v1.128-p1-nle-timeline-canvas | P1 — NLE Timeline canvas | Todo | **Must** — Studio-first; scope capped |
| 2026-07-20-v1.128-p2-creator-create-controller-shell | P2 — Creator Create vs Controller | Todo | **Must** — Create CTAs + Back semantics |
| 2026-07-20-v1.128-p3-web-alias-clarity | P3 — `@web-*` / `@42ch/nexus-ui` clarity | Todo | **Must** — docs/labels; promotion optional |

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review chain) | 2026-07-20 | complete |
| Dev complete | 2026-07-21 | pending |
| QC complete | 2026-07-21 | pending |
| Iteration close | 2026-07-21 | pending |

## Acceptance Criteria

Author-observable — each AC is verifiable in Design Studio and/or App without reading source.

- **AC-V1128-1 (P0 — Surfaces chrome):** In Design Studio `/surfaces/*`, the author switches sections via a **persistent left sidebar** (active route highlighted). **No** Banner nav entry, route, or fixture remains in Studio. On Selection Submenu, the Agent/dialog variant renders inside a **scoped frame** (not a full-viewport blocker); the author can **close and reopen** it while other variants stay visible and clickable.
- **AC-V1128-2 (P1 — NLE Timeline):** In Studio Canvas fixtures, the author sees a **vertically centered** multi-track Timeline band with **horizontal scrub/pan** along time and at least one **pull-off** interaction (item detaches from a track onto the canvas area). In App, the World/Work Timeline host adopts the new chrome where planned — loads without wire-contract changes; author is **not** expected to trim, ripple-edit, or use audio lanes.
- **AC-V1128-3 (P2 — Create vs Controller):** With **no Work/World selected** in Creator, the content region shows a **Create page** with card-sized CTAs: Worlds path honors V1.125 `createWorld` feature-detect (honest fallback to Work create when absent); Works path offers Create Work. With a **Work or World selected**, the same region shows a **Controller Panel stub** (placeholder copy + **Back**). **Back** clears the entity selection and returns the author to the Create page. Observable in Studio Shell fixtures **and** live App.
- **AC-V1128-4 (P3 — import clarity):** On Surfaces pages that import `@web-*` or `@42ch/nexus-ui`, the author sees **source badges or equivalent copy** distinguishing App presentational extract vs promoted primitive. `apps/design-studio/AGENTS.md`, `packages/nexus-ui/AGENTS.md`, and `.mstar/specs/design-studio.md` (or pointer) state the two-tier model unambiguously. **Zero** mass promotion is required for Must.

## Non-Goals

- Wire-contract-heavy Delete / Create World API expansion
- Fork UI / Computable pillar / multi-timeline merge
- Mass promotion of every `@web-*` extract into `@42ch/nexus-ui`
- Reintroducing MainBanner into live App chrome
- Full Controller Panel business content (stub + Back only; widgets TBD in a later iteration)
- Full video-editor feature parity (trimming, ripple edits, audio lanes, snap-to-frame, keyboard shortcuts)

## Roadmap Position

- **Current iteration (V1.128):** Studio-first UI dogfood from maintainer feedback after V1.127.
- **Next iteration:** Full Controller Panel content; deeper NLE interactions; larger promotion waves; remaining V1.126 nit residuals. Trigger: V1.128 shipped + dogfood on Create/Controller + Timeline.
- **Long-term:** Studio-first promotion workflow + Control Room shell IA that matches author mental model (Create → select → control → canvas).

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.128` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| NLE Timeline scope balloon | Medium | High | **Locked (Seat 2):** pull-off = Studio fixture local state only; no RF DnD expansion; App T3 swaps presentational chrome into existing hosts (`timeline-canvas.tsx`, `canvas-shell.tsx`) |
| Create vs Controller IA ambiguity | Medium | Medium | **Locked (Seat 2):** `CreatorEntitySelectionContext` SSOT; Back clears selection → Create page; orthogonal to work-shell routes (`/works/:workId/*`) |
| Alias “fix” misread as mass promote | Low | Medium | P3 docs/labels only; optional tiny promotion with plan list; default zero |

## Iteration package

> Sibling paths under `.mstar/iterations/v1.128/` — not in `specs/` or `knowledge/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Kind | Status |
|------|------|--------|
| `README.md` | index | active (Phase 1 §1.6 Review chain complete) |
| `specs/surfaces-chrome-ia.md` | spec (P0) | product-reviewed, architect-locked, writing-hygiene done |
| `specs/nle-timeline-canvas.md` | spec (P1) | product-reviewed, architect-locked, writing-hygiene done |
| `specs/creator-create-controller-shell.md` | spec (P2) | product-reviewed, architect-locked, writing-hygiene done |
| `specs/web-alias-clarity.md` | spec (P3) | product-reviewed, architect-locked, writing-hygiene done |

Plans: `.mstar/plans/2026-07-20-v1.128-p0-surfaces-chrome-ia.md` · `.mstar/plans/2026-07-20-v1.128-p1-nle-timeline-canvas.md` · `.mstar/plans/2026-07-20-v1.128-p2-creator-create-controller-shell.md` · `.mstar/plans/2026-07-20-v1.128-p3-web-alias-clarity.md`.

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| | | | | |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
