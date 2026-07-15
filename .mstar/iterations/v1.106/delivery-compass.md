---
iteration_id: V1.106
start_date: 2026-07-10
status: completed
end_date: 2026-07-10
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.106
plans:
  - 2026-07-10-v1.106-studio-first-pipeline
  - 2026-07-10-v1.106-first-launch-polish
  - 2026-07-10-v1.106-ui-continuity
---

# V1.106 — UI Continuity & Studio-First Pipeline

## Scope

Continue UI product completeness after V1.105, and **finish** the Design Studio ↔ DESIGN.md authoring pipeline for author-facing chrome this iteration.

**Locked invariant (all future UI work):**

> **需求 →【design-studio ↔ DESIGN.md】组件打磨 → Real App 采用**

No App-first visual invention for author-facing chrome. Studio fixtures + DESIGN contract (or explicit keep-web classification) precede App wiring claims.

### Must / Stretch integrity (locked)

| Tier | Plans | May defer? | Iteration incomplete if missing? |
|------|-------|------------|----------------------------------|
| **Must** | **P0** Studio-first pipeline complete; **P1** First-launch polish (FB-003/004 + V1.105 residual seeds) | No | **Yes** |
| **Stretch** | **P2** UI continuity polish (Badge/Select/Settings IA/AgentPicker chrome) | Yes | **No** |

- **SP-1 (Must / P0): Studio-first pipeline.** DESIGN.md gaps (States, Tabs, Form Field composition, Launch & daemon status) + Studio Surfaces fixtures (launch splash, MainBanner, Toast matrix) + keep-web lock for Dialog/Tabs/Table/States + promotion-boundary hygiene.
- **SP-2 (Must / P1): First-launch polish.** Wizard Agent uses shared `AgentPicker` (Studio stub → same component as Settings); Workspace/Done centering + Done celebratory emoji + shorter copy; V1.105 residual seeds.
- **SP-3 (Stretch / P2): UI continuity.** Badge Soft/Solid contrast; Select chevron inset; Settings nav Agent→Workspace→Advanced; AgentPicker chrome polish.

## Product Story

**Who:** Authors using desktop first-launch and Settings, plus contributors iterating UI in Design Studio.

**Problem:** V1.105 reshaped first-launch, but Studio chrome fixtures still stub Agent lists; portrait Workspace/Done feel sparse; Design Studio ↔ DESIGN.md coverage is incomplete for States/Toast/launch chrome — so App-first drift remains possible.

**Narrative:** This iteration locks the studio-first pipeline as Must, polishes first-launch chrome for consistency with Settings, and parks additional component polish as Stretch until capacity allows.

**Iteration complete when:** P0 + P1 automated gates pass (pipeline classified/fixtured/DESIGN-backed; first-launch FB-003/004 landed). P2 Stretch may remain open without blocking close.

### User-visible outcomes by backlog ID

| ID | Plan | What the author sees |
|----|------|----------------------|
| SP-V1106-001 (SP-001) | P0 | One studio-first rule in compass + guide; promotion doc matches promoted Input/Select |
| SP-V1106-002 (SP-002) | P0 | Tabs, States, Form Field, and Launch/daemon chrome documented in DESIGN.md |
| SP-V1106-003 (SP-003) | P0 | Launch splash, degraded banner, and toast variants previewable in Design Studio |
| FB-V1106-003 (FB-003) | P1 | Wizard Agent step uses the same AgentPicker as Settings (not stub rows) |
| FB-V1106-004 (FB-004) | P1 | Workspace/Done centered in portrait shell; Done shows **You're ready 🎉** + short helper |
| FB-V1106-001 (FB-001) | P2 | Badge Soft/Solid hues distinguish status at a glance |
| FB-V1106-002 (FB-002) | P2 | Select chevron has clear inset from right border |
| FB-V1106-005 (FB-005) | P2 | Settings nav: Agent · Workspace · Advanced (Connection + Setup inside Advanced) |
| FB-V1106-006 (FB-006) | P2 | AgentPicker divider, badges, icons, dots, and muted uninstalled titles polished |

## Grill-Me Decisions

| Decision | Resolution |
|---|---|
| Iteration direction | Scaffold-first UI continuity + **complete Studio↔DESIGN.md pipeline this iteration** |
| Iteration ID | `V1.106` |
| Branch policy | `iteration_base_branch=main`; `spec_integration_branch=iteration/v1.106`; `target_branch=main` |
| Theme | A+B umbrella + first-launch Must default; pipeline elevated to Must by user |
| Plan split | **Triple**: P0 pipeline Must + P1 first-launch Must + P2 continuity Stretch |
| Further UI opinions | Auto-route (no per-item Must/Stretch questions): pipeline→P0; first-launch→P1; components/Settings polish→P2 |
| Studio-first invariant | Locked for all subsequent UI Assignments |
| §5 Review & Edit / lock / integration branch | §5.1–§5.3 **done**; §5.4 PM lock **done** — compass `status: locked`; proceed to §6 `iteration/v1.106` |
| Phase 1 review models | `composer-2.5` only (no `composer-2.5-fast` / MAX Fast) when §5 runs |
| Wire | Prefer `wire_contracts_changed: false` |
| Human smoke | Separate human gate; not automated Done / CI blocker |
| Promote Dialog/Tabs/Table/States to package | **Out** this iteration — classified + DESIGN-backed + fixtured is enough |
| DF-70 execution-mode / BYOK | Out unless later promoted |

## Architecture Locks (§5.2 architect — locked)

| Topic | Lock |
|-------|------|
| Studio-first gate | Author-facing visual changes: Studio fixture → visual accept → App; DESIGN.md updated when tokens/prose become normative |
| Pipeline Done | Every setup → settings → control-room daemon chrome path has DESIGN contract + Studio fixture + explicit `promote` \| `keep-web` decision — **not** package promotion |
| keep-web (V1.106) | Dialog, Tabs, Table, States remain `apps/web/src/components/ui/*`; DESIGN.md §Tabs + §States; Studio references via transitional `@web-ui/*` only |
| Surfaces fixtures (SP-003) | `apps/design-studio/src/fixtures/launch-daemon-fixtures.tsx` → `/surfaces/launch`; `main-banner-fixtures.tsx` → `/surfaces/banner`; Toast matrix on `/components` (extend existing page or `toast-fixtures.tsx`) |
| MainBanner fixture | **Composition-only** props-driven chrome in Studio — do **not** extract a presentational layer from `main-banner.tsx` in V1.106 (`useDesktopCapabilities` / daemon IPC stay App-owned) |
| DaemonReadySplash fixture | Presentational module import: `@web-setup/daemon-ready-splash` → `apps/web/src/components/setup/daemon-ready-splash.tsx` |
| AgentPicker | App-shared `apps/web/src/components/setup/agent-picker.tsx`; Settings + wizard same module; Studio `@web-setup/agent-picker`; `density?: 'default' \| 'compact'` (default `'default'`; wizard may pass `'compact'` only) |
| TopStepIndicator SSOT | **Single module** `apps/web/src/components/setup/top-step-indicator.tsx` (exports `WizardStep` + `TopStepIndicator`); Studio `@web-setup/top-step-indicator`; delete inline duplicate in `setup-wizard-chrome-fixtures.tsx` — closes `R-V1105P2-001` |
| Portrait shell | Keep V1.105 H1 height tokens; FB-004 centers scroll body only; CTAs stay bottom-anchored |
| Settings IA (P2 Stretch) | Single route `/settings/advanced` with stacked sections (`id="connection"`, `id="setup"`); nav **Agent · Workspace · Advanced**; redirects: `/settings/connection` → `/settings/advanced#connection`, `/settings/setup` → `/settings/advanced#setup`, `/connect` → `/settings/advanced#connection` |
| Badge / Select tokens (P2) | Token owners: DESIGN.md `components.badge-status-pill` (soft/solid); Select chevron inset via `packages/nexus-ui` + DESIGN `components.select` — WCAG AA validation before merge |
| Wire | `wire_contracts_changed: false` |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-10-v1.106-studio-first-pipeline` | P0 — Studio-first pipeline complete | Done | Must |
| `2026-07-10-v1.106-first-launch-polish` | P1 — First-launch polish | Done | Must; after/with P0 as capacity allows |
| `2026-07-10-v1.106-ui-continuity` | P2 — UI continuity polish | Done | Stretch |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Phase 1 lock) | 2026-07-10 | done |
| Dev complete | TBD | pending |
| QC complete | TBD | pending |
| Iteration close | TBD | pending |

## Acceptance Criteria

### P0 — Studio-first pipeline

- [ ] **SP-001:** Promotion-boundary + AGENTS + `nexus-ui` index reflect Input/Select promoted and studio-first invariant locked.
- [ ] **SP-002:** DESIGN.md covers Tabs, States, Form Field composition (V1.100 cross-ref), and Launch & daemon status with Voice & Content examples.
- [ ] **SP-003:** Studio `/surfaces/launch`, `/surfaces/banner`, and `/components` Toast matrix demonstrate all required variants (light + dark).
- [ ] Dialog / Tabs / Table / States remain **keep-web** — classified, DESIGN-backed, fixtured; **not** package-promoted.

### P1 — First-launch polish

- [ ] **FB-003:** Studio + App wizard Agent step render shared `AgentPicker` (`data-testid="agent-picker"`); optional compact density only — no second picker.
- [ ] **FB-004:** Workspace/Done bodies centered in portrait shell; Done heading **You're ready 🎉** with one-line sentence-case helper; CTAs bottom-anchored.
- [ ] V1.105 residual seeds (`R-V1105P2-001/002`, `R-V1105P0-003..005`, `R-V1105P1-001`) closed or re-targeted with evidence.

### P2 — Stretch (optional for iteration close)

- [ ] **FB-001:** Badge Soft hues distinct; Solid fills hue-aligned (six variants).
- [ ] **FB-002:** Select chevron inset on closed/disabled/invalid (native `<select>` — no Radix).
- [ ] **FB-005:** Settings nav Agent · Workspace · Advanced; Connection + Setup as Advanced sections; legacy routes redirect.
- [ ] **FB-006:** AgentPicker chrome polish (divider, Installed badge by title, tighter outbound icon, status dots, uninstalled title muted).

## Non-Goals

- Promoting Dialog / Tabs / Table / States into `@42ch/nexus-ui` this iteration
- Promoting AgentPicker / SettingsShell into the package (remain app-shared)
- DF-70 execution-mode / BYOK matrix
- Radix Select rewrite; multi-workspace switcher
- Wire/schema changes unless a Must item proves unavoidable
- Compass `locked`, `iteration/v1.106` branch creation, and Phase 2 implement (PM owns §5.4 lock + branch)

## Roadmap Position

- **Current iteration (V1.106):** delivered — Studio-first pipeline Must + first-launch polish Must + continuity Stretch all complete.
- **Next iteration:** Remaining deferred residuals (Toast dedup consolidation, lucide-react boundary doc, scrollIntoView); further author-desk UI under studio-first invariant; DF-70 leftovers if promoted.
- **Final goal:** Trustworthy desktop UI where every author-facing chrome is designed in Studio against DESIGN.md before App adoption.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.106` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Pipeline scope balloons into package promotions | Med | Med | Done = classify + DESIGN + fixture; promote is Stretch/out |
| Studio AgentPicker density fights Settings layout | Med | Low | Shared component + optional `density` prop only |
| Portrait centering regresses CTA anchoring | Low | Med | Keep CTA bottom-anchored; center only scroll body |
| §5 deferred too long → stale FB backlog | Low | Low | §5.1 PM done; auto-route continues; lock when PM runs §5.4 |

## Iteration workspace

| Path | Purpose |
|------|---------|
| `v1.106/specs/` | Iteration contracts (Draft — §5.1 product-complete) |
| `v1.106/guides/` | Studio-first invariant guide |

## Feedback backlog index

| ID | Plan | Summary |
|----|------|---------|
| SP-V1106-001 | P0 | Process + doc lock |
| SP-V1106-002 | P0 | DESIGN.md SSOT gaps |
| SP-V1106-003 | P0 | Studio Surfaces fixtures |
| FB-V1106-003 | P1 | Wizard AgentPicker parity |
| FB-V1106-004 | P1 | Workspace/Done layout |
| FB-V1106-001 | P2 | Badge Soft/Solid |
| FB-V1106-002 | P2 | Select chevron inset |
| FB-V1106-005 | P2 | Settings Advanced nav |
| FB-V1106-006 | P2 | AgentPicker chrome |

## Quality Gate Summary

| Plan | QC verdict | QA verdict | Tests |
|------|-----------|------------|-------|
| P0 studio-first pipeline | Approve with residuals (fixed Critical) | Accept | 112 nexus-ui, 86 studio |
| P1 first-launch polish | Approve with residuals (fixed Warning) | Accept | 606 web, 87 studio |
| P2 UI continuity | Approve (no Criticals/blocking Warnings) | Accept | 121 nexus-ui, 608 web, 86 studio |

Open residuals: R-V1106P0-001..005 (defer/accept), R-V1106P2-001..003 (defer/accept). All non-blocking.

## Compound Round Summary

**Workspace inventory (`v1.106/`):**
- `guides/studio-first-invariant.md` — **Keep snapshot**. Pattern already covered by `knowledge/architecture-patterns/ui-component-promotion-workflow.md`. The guide is iteration-specific; no promotion needed.
- `specs/studio-first-pipeline.md`, `first-launch-polish.md`, `ui-continuity.md` — **Keep snapshot**. Iteration-scoped contracts; superseded by their shipped implementations.

**Knowledge update:**
- Updated `knowledge/architecture-patterns/ui-component-promotion-workflow.md` with V1.106 Toast promotion lesson: when a Studio fixture requires a package primitive that doesn't yet exist (Toast), the implementer promotes it — but must follow the V1.99 re-export pattern (thin wrapper + call-site migration) to avoid duplication hazard (R-V1106P0-001).

**No new knowledge documents created** — Q5 overlap check determined all V1.106 learnings are covered by existing architecture-patterns docs.

## Iteration Retrospective (minimal)

**What went well:**
- All 3 plans (2 Must + 1 Stretch) completed in one drive session — 10 tasks across 3 SDD per-plan cycles.
- Studio-first pipeline enforced: DESIGN.md contracts written before fixtures; fixtures before App claims.
- Toast timer race (QC3 Critical) caught by plan QC tri, not production.

**What to improve:**
- Toast promotion created a duplicate-implementation hazard (R-V1106P0-001) — the V1.99 re-export pattern should be the default for package promotions, not verbatim copy.
- `status.json` grew to ~47KB — Profile B compaction (archive resolved residuals) needed before next iteration.
