---
iteration_id: V1.102
start_date: 2026-07-09
status: completed
end_date: 2026-07-09
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.102
plans:
  - 2026-07-09-v1.102-badge-soft-solid
  - 2026-07-09-v1.102-settings-shell
  - 2026-07-09-v1.102-ui-hygiene
---

# V1.102 — Badge Tone, Thin Settings & Surfaces Polish

## Scope

V1.102 closes design-system Badge soft/solid gaps, ships a **thin Settings host** for the existing app-shared `AgentPicker` (DF-70 slice A), and optionally polishes Studio Surfaces / Control Room chrome as Stretch.

### Must / Stretch integrity (locked)

| Tier | Plans | May defer? | Iteration incomplete if missing? |
|------|-------|------------|----------------------------------|
| **Must** | **P0** Badge soft/solid; **P1** thin Settings host | No | **Yes** |
| **Stretch** | **P2** UI hygiene + Surfaces polish (wizard/shell/picker/daemon + Surfaces menu) | Yes → V1.103+ | **No** — deferring P2 does not leave Must incomplete |

Order: **P0 → P1 → P2**. Do not treat P2 as Must, and do not start P2 until P0+P1 automated paths are Done (unless PM documents an explicit capacity exception). Inside each UI-visual plan: **Studio visual → App wiring**.

- **SP-1 (Must / P0): Badge soft / solid.** Strengthen soft borders; add `tone?: 'soft' | 'solid'` (default soft); Studio Soft/Solid matrix; no forced `StatusBadge` cutover.
- **SP-2 (Must / P1): Thin Settings host (DF-70 slice A).** Route **`/settings`** + footer-utility **Settings** nav + one page mounting app-shared `AgentPicker`; persist via **`setAgentProfile`**. No full multi-section Settings IA, BYOK, or AgentPicker package promotion.
- **SP-3 (Stretch / P2): Surfaces polish + hygiene.** Guardrails `PROMOTED_PRIMITIVES` consolidation; wizard Steps/Back/error chrome; sidebar aesthetic; AgentPicker card chrome; daemon strip single-line + Restart; Surfaces section menu for review (**Studio-only**).

## Product Story

Authors need clearer status pills (soft vs solid) and a place to change the local agent after first-run without re-entering setup. Contributors need a Design Studio Surfaces gallery that is reviewable by section instead of one endless page.

## Grill-Me Decisions

| Decision | Resolution |
|---|---|
| Iteration ID | `V1.102`. |
| Branch policy | `iteration_base_branch=main`; `spec_integration_branch=iteration/v1.102`; `target_branch=main`. |
| Must | P0 Badge soft/solid; P1 DF-70 **thin Settings host**. |
| Stretch | P2 UI hygiene + Surfaces polish — whole-plan defer allowed. |
| Settings slice | **A — thin host**: route/shell + nav + one AgentPicker page. |
| Settings out | Full multi-section IA; BYOK; in-app installers; AgentPicker → `@42ch/nexus-ui`. |
| Badge API | `tone?: 'soft' \| 'solid'` (default soft); stronger soft borders; solid semantic fills + white text. |
| Badge app cutover | No forced `StatusBadge` → solid this iteration. |
| Back icon | `lucide-react` `ChevronLeft` + `aria-label="Back"` — **no Iconify**. |
| Wire | Prefer `wire_contracts_changed: false`. |
| Studio-first | UI-visual work: Studio fixtures → visual acceptance → App wiring. |
| Human smoke | Separate human gate; **not** an automated Done / CI blocker. Automated Done ≠ smoke Done. |
| Surfaces menu | Stretch P2 item; **Design Studio only** (not an App Settings IA). |

## Architecture Locks (§5.2 — do not reopen in implement)

| Topic | Lock |
|-------|------|
| Badge package API | `@42ch/nexus-ui` Badge: `tone?: 'soft' \| 'solid'` via **cva + compoundVariants**; default soft; DESIGN soft+solid light/dark; app `badge` thin re-export; **no** `StatusBadge` cutover; **no** schemas |
| Settings route | **`/settings`** — child of `SetupGate`→`RootLayout` in `apps/web/src/App.tsx`; page `apps/web/src/pages/settings-page.tsx`; **no** `/settings/*` |
| Settings nav | Label **`Settings`**; lucide **`Settings`**; sidebar **footer utility** above `FooterProfiles` (outside Creator/Orchestrator tabs) + `MOBILE_NAV` entry; `ROUTE_TITLES['/settings']='Settings'` |
| Settings persistence | **`DesktopCapabilities.setAgentProfile(name, launchCommand?)`** → Tauri `set_agent_profile` (same as `SetupWizardPage.finish()`); scan via `useScanAgents` / setup mapping helpers; browser: mount OK, persist desktop-only |
| AgentPicker | Stays `apps/web/src/components/setup/agent-picker.tsx` — **not** `@42ch/nexus-ui` |
| P2 Surfaces routes | Studio-only: `/surfaces`, `/surfaces/setup`, `/surfaces/shell`, `/surfaces/agent-picker`, `/surfaces/daemon` |
| P2 chrome boundaries | StepIndicator / DaemonStatusRegion = app+fixture local; AgentPicker chrome on shared app component; sidebar within `sidebar-nav` tokens; DaemonStatusBar ↔ Studio strip |
| Wire / icons | `wire_contracts_changed: false`; lucide only — no Iconify |
| DF-70 | Thin slice closes accepted scope; fuller Settings IA deferred |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-09-v1.102-badge-soft-solid` | P0 — Badge soft / solid tone | Done | Must. Merged to integration. |
| `2026-07-09-v1.102-settings-shell` | P1 — Thin Settings host (DF-70 A) | Done | Must. Host existing AgentPicker. |
| `2026-07-09-v1.102-ui-hygiene` | P2 — UI hygiene + Surfaces polish | Done | Stretch. Whole-plan deferrable. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze and plan lock | 2026-07-09 | done (§5.1–§5.3 + PM lock) |
| P0 Badge automated gates | 2026-07-09 | done |
| P1 Thin Settings host automated gates | 2026-07-11 | pending |
| P2 Stretch (optional) | 2026-07-09 | done |
| Human desktop smoke (separate gate) | — | scheduled separately (not blocking automated Done) |
| Iteration close | 2026-07-09 | done |

## Acceptance Criteria

### Must (P0 + P1) — required for iteration Must completeness

**P0 — Badge**

- Soft Badge borders are visibly distinct on light backgrounds (neutral uses stronger alpha; semantic borders ≈ 50% alpha per contract).
- Solid `tone` exists for all six semantic variants with high-contrast (white) text and no visible border.
- Studio `/components` Badge section shows **Soft** and **Solid** matrices (6 variants each); `VariantLabel` remains label-only under columns.
- Package API: `tone?: 'soft' | 'solid'` defaults to **soft**; existing callers (`StatusBadge`, etc.) unchanged without forced cutover.
- DESIGN SSOT (`DESIGN.md` / `DESIGN.dark.md`) documents soft + solid maps.

**P1 — Thin Settings host**

- Control Room exposes a **Settings** nav entry (sidebar footer utility + mobile nav); activating it opens **`/settings`** (one host page under `RootLayout`; no nested Settings IA).
- That Settings page’s primary product content is the existing app-shared `AgentPicker` (`apps/web/src/components/setup/agent-picker.tsx`) — **not** a re-run of the full setup wizard.
- Selecting an installed agent (or custom launch) persists via **`setAgentProfile`** (same as setup `finish()`); Vitest covers route/mount; desktop reload survival confirmed by human smoke when scheduled.
- Studio fixtures cover Settings chrome / Agent page visual states **before** App wiring (studio-first).
- Prefer `wire_contracts_changed: false`. Any `schemas/` proposal = hard stop to architect/PM.

**Gates**

- Design Studio fixtures exist and pass visual acceptance **before** App wiring for each UI-bearing Must plan.
- Automated QA/CI does **not** require interactive macOS desktop smoke; human smoke is a **separate gate** after automated paths land.

### Stretch (P2) — optional; deferral allowed

- If P2 runs: Surfaces section menu (**Studio-only** deep links); wizard Steps/Back/error; sidebar aesthetic; AgentPicker chrome; daemon strip single-line + Restart; guardrails list consolidation as capacity allows — each with Studio → App where App is in scope.
- If deferred: compass + `status.json` retarget V1.103+ with reason; Must completeness unaffected.

## Non-Goals

- Full Settings IA (≥2 product sections), settings sidebar taxonomy, or nested Settings sub-routes beyond the thin host.
- BYOK / API-key execution modes; in-app agent installers.
- Promoting AgentPicker to `@42ch/nexus-ui`.
- Forced product cutover of all `StatusBadge` (or other callers) to solid.
- Introducing Iconify (use existing `lucide-react` only).
- Treating Stretch P2 as Must / iteration-incomplete.
- Shipping Surfaces section menu into the App product shell (Studio-only).
- Platform sync, billing, entitlements, signing, notarization, auto-update.
- Wire/schema changes (`schemas/`) unless architect hard-stop waived.

## Roadmap Position

- **Current iteration (V1.102):** **delivered** — Badge soft/solid + thin Settings host + Surfaces polish Stretch on `iteration/v1.102`.
- **Relation to V1.101:** Consumes shipped app-shared AgentPicker; closes DF-70 for the **accepted thin slice**; fuller Settings IA remains deferred.
- **Next iteration:** Fuller Settings IA / execution-mode matrix (DF-70 remainder); optional residual polish (R-V1102*); trigger: product priority after V1.102 PR merge. Owner: @project-manager.
- **Final target:** Authors can change local agents post-setup; contributors review Surfaces by section in Studio; status pills read clearly in soft and solid.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.102` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Thin Settings host grows into full IA | Medium | High | Slice A lock; Non-Goals; QC checks single `/settings` + single page content |
| Missing agent-profile read IPC | Low | Low | Must does not require `getAgentProfile`; optional Tauri-only read; smoke validates write |
| Badge solid dark contrast fails | Medium | Medium | DESIGN.dark solid maps + Studio dark matrix |
| Stretch crowds Must | Medium | Medium | P2 starts after P0+P1; whole-plan defer |
| AgentPicker chrome regresses setup | Low | High | Studio-first; shared component; Vitest |
| Human smoke treated as Done blocker | Low | Medium | Explicit separate-gate language in compass + plans |

## Iteration Workspace

| Path | Purpose |
|------|---------|
| `v1.102/README.md` | Workspace index |
| `v1.102/specs/badge-soft-solid-contract.md` | Badge tone + border contract |
| `v1.102/specs/settings-thin-host.md` | DF-70 slice A thin Settings host |
| `v1.102/specs/surfaces-polish-contract.md` | Stretch Surfaces / chrome polish |
| `v1.102/guides/studio-first-visual-then-app.md` | Process note (reuse V1.101 discipline) |

## Quality Gate Summary

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-09-v1.102-badge-soft-solid` | Approve with residuals | Pass with residuals | R-V1102P0QC-S001..S003 | plan Durable QC/QA Summary |
| `2026-07-09-v1.102-settings-shell` | Approve with residuals | Pass with residuals | R-V1102P1QC-S001..S005 | plan Durable QC/QA Summary |
| `2026-07-09-v1.102-ui-hygiene` | Approve with residuals | Pass with residuals | R-V1102P2QC-S001..S003 | plan Durable QC/QA Summary |


## Compound Round Summary

- Crystallized documents: `.mstar/knowledge/architecture-patterns/badge-soft-solid-tone.md` (Badge tone axis); DF-70 tracker updated (thin host shipped).
- Workspace promotion: iteration contracts retained under `v1.102/specs/` (authoritative for this ship); studio-first guide kept in workspace.
- New CONCEPTS.md entries: —
- Compound-refresh triggered: no
- Skipped crystallization: Settings thin-host details remain in iteration spec (already architect-locked; tracker updated instead of duplicate knowledge doc).


## Iteration Retrospective (minimal)

- What went well: SDD sticky implementer + composer-2.5 L2 + grok QC tri kept Must+Stretch shipping in one drive; Studio-first prevented App-only chrome drift.
- What to improve: Task-1 no-op contract tasks can be skipped when architect locks already `[x]`; Surfaces long-page split earlier would have helped P1 fixture placement.
- Next iteration suggestion: Fuller Settings IA (DF-70 remainder) + residual Suggestion polish if product prioritizes.

