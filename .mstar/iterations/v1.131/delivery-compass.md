---
iteration_id: V1.131
start_date: 2026-07-22
end_date: 2026-07-22
status: completed
iteration_base_branch: main
spec_integration_branch: iteration/v1.131
target_branch: main
scale: L
direction_lock_mode: autonomous
plans:
  - 2026-07-22-v1.131-p0-chronos-titlebar
  - 2026-07-22-v1.131-p1-logo-gallery-lockup
  - 2026-07-22-v1.131-p2-shell-ia-finish
  - 2026-07-22-v1.131-p3-desktop-icons-residuals
---

# V1.131 Delivery Compass

## Scope

**Locked direction (autonomous):** Ship Chronos **titlebar-first chrome**, **BG-matched logo placement** / Studio gallery fixes, **all V1.130 dogfood IA gaps** (`DF-V1130-*`), desktop icon verify, and residual slate-clear — **in this iteration**, not as further deferrals.

**Rationale:** User dogfood after PR #167 requires VI chrome/logo fixes **and** insists related incomplete V1.130 work (tracker `DF-V1130-*`) must be **resolved here**. Scale **auto → L** (4 business plans, at cap).

### Priority of pain (user-value rank)

1. **P0 — Light-shell Chronos titlebar** (highest). Full-width ink bar; logo on ink; Settings gear entry into the Settings modal.
2. **P1 — Studio logo gallery + lockup.** Fill previews; BG-match plates; larger wordmark.
3. **P2 — V1.130 shell IA finish (binding).** Close `DF-V1130-MODE-SWITCH-FOOTER`, `SETTINGS-MODAL`, `WORKSPACE-UNDER-ORCH`, `COMPUTE-IN-SETTINGS`; hold `PROFILE-SSOT`. **Must not re-defer these to V1.132.**
4. **P3 — Desktop icons + residual archive.** Compose/generate + slate-clear after P0–P2 land codes.

### Tracker IDs resolved this iteration

| Tracker ID | Plan owner | Outcome |
|------------|------------|---------|
| `DF-V1130-SETTINGS-MODAL` | P0 + P2 | **Shipped** — archived tracker |
| `DF-V1130-MODE-SWITCH-FOOTER` | P2 | **Shipped** |
| `DF-V1130-WORKSPACE-UNDER-ORCH` | P2 | **Shipped** |
| `DF-V1130-COMPUTE-IN-SETTINGS` | P2 | **Shipped** |
| `DF-V1130-PROFILE-SSOT` | P2 | **Held** (regression green) |
| `DF-V1131-CHRONOS-TITLEBAR` | P0 | **Shipped** (human Overlay smoke open) |
| `DF-V1131-LOGO-GALLERY-LOCKUP` | P1 | **Shipped** (human wordmark measure open) |
| `DF-V1131-DESKTOP-ICON` | P3 | **Shipped** (human Dock live open) |

## Plans

| Wave | plan_id | Name | Status | blocked_by |
|------|---------|------|--------|------------|
| 1∥ | `2026-07-22-v1.131-p0-chronos-titlebar` | Chronos titlebar chrome | **Done** | — |
| 1∥ | `2026-07-22-v1.131-p1-logo-gallery-lockup` | Logo gallery + lockup scale | **Done** | — |
| 2 | `2026-07-22-v1.131-p2-shell-ia-finish` | V1.130 shell IA finish | **Done** | P0 |
| 3 | `2026-07-22-v1.131-p3-desktop-icons-residuals` | Desktop icons + residual slate | **Done** | P0, P1, P2 |

**Scale budget:** L → **4** business plans (cap). Harness process not counted.

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit lock) | 2026-07-22 | **done** |
| Wave 1 (titlebar ∥ gallery) | 2026-07-22 | **done** |
| Wave 2 (shell IA finish) | 2026-07-22 | **done** |
| Wave 3 (desktop + residuals) | 2026-07-22 | **done** |
| Iteration close | 2026-07-22 | **done** |

## Acceptance Criteria

### VI / Chronos chrome (P0, P1, P3)

- **AC-1 (P0).** Light shell: full-width deep-blue titlebar (`#0D2B3E`); brand mark on ink — **Pass**
- **AC-2 (P0).** Titlebar labels white (light) / cyan (dark) — **Pass** (automation)
- **AC-3 (P0).** Desktop Overlay + native traffic lights — **Pass with notes** (product code; H2–H4 human → `R-V1131P0-QC2-W-001`)
- **AC-4 (P1).** Studio `primary` / `whiteBg` cards fill on matching BG — **Pass**
- **AC-5 (P1).** Dark hero wordmark cap-height ≥ 60% — **Exempt human** (`R-VI-004`; `h-[26px]` automated)
- **AC-6 (P0+P1).** Plate logos on matching surfaces — **Pass**
- **AC-7 (P3).** Desktop icons compose/generate + Dock README — **Pass with notes** (hashes match; Dock live → `R-VI-003`)

### V1.130 IA (P2 — must ship)

- **AC-8.** Footer-only 创作\|编排 — **Pass**
- **AC-9.** Settings modal primary ≥80vw×80vh + dirty/route-leave — **Pass**
- **AC-10.** 工作区 under 编排 only — **Pass**
- **AC-11.** Modules in Settings; `/modules` compat; no Compute nav — **Pass**
- **AC-12.** Profile SSOT regression — **Pass**

### Residuals (P3)

- **AC-13.** Targeted residuals archived or owner+trigger — **Pass** (DF-V1130-* not re-deferred)

## Non-Goals

- Runtime multi-theme switcher (Umbra/Aurora)
- Full Agent Chat product depth
- `DF-70` execution-mode matrix
- `DF-71` menu-bar daemon control
- Marketing/store icon packages beyond app icon pipeline
- Re-deferring `DF-V1130-*` UI gaps to V1.132 (**forbidden** — shipped)

## Roadmap Position

- **Current (V1.131):** **delivered** — Chronos titlebar + logo gallery + DF-V1130 shell/Settings IA + desktop icons + residual slate (PR pending to `main`).
- **Next:** Deeper creator Chat / orchestration polish; DF-70/71 when capacity; human smokes for Dock (`R-VI-003`) and Overlay (`R-V1131P0-QC2-W-001`) before merge-ready if required by reviewers.
- **Prior:** V1.130 partial ship (#165) + VI logo upgrade (#167).

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.131` |
| `target_branch` | `main` |

## Architecture locks (architect Seat 2)

1. **Tauri v2 titlebar:** Overlay + native traffic lights; web ink bar; non-interactive drag only.
2. **Shell boundary:** full-width titlebar above sidebar/content; presentational `@web-layout/*`.
3. **Settings boundary:** one `SettingsModalHost` + section registry SSOT + dirty/URL resolver.
4. **IA finish:** footer mode switch; 编排-only 工作区; Modules Settings section without nested dialog.
5. **Desktop icon pipeline:** `logo-primary.svg` → compose → hash-stable source/preview; Dock cache README.

## Quality Gate Summary

| Plan | QC | QA | Notes |
|------|----|----|-------|
| P0 | Approve with residuals | Pass with notes | Overlay H2–H4 human open |
| P1 | Approve | Pass with notes | Wordmark human measure open |
| P2 | Approve (after R1–R4 fix) | Pass | — |
| P3 | Approve (after F-001–F-004) | Pass with notes | Dock live human open |

## Compound Round Summary

| Item | Action |
|------|--------|
| `architecture-patterns/settings-modal-primary-host.md` | **New** — Settings modal primary host (BrowserRouter dirty leave) |
| `architecture-patterns/chronos-titlebar-overlay.md` | **New** — Tauri Overlay Chronos titlebar |
| `conventions/profile-b-residual-archival-procedure.md` | **Updated** — no archive before live-smoke QA |
| Package `specs/*` | **Retained** in iteration package (iteration-scoped; not promoted wholesale) |
| Package `guides/` | **None** |

Q1–Q8: crystallize Settings host + Overlay titlebar (Yes ≥4); residual live-smoke timing (update existing convention).

## Iteration Retrospective (minimal)

- **Worked:** L/4 must-ship DF-V1130 pack without re-deferral; Studio-first chrome; QC2 catch on premature Dock archive.
- **Friction:** Agent TCC blocks Overlay + Dock screenshots — keep human residuals honest, do not invent Archive.
- **Carry:** Human smokes + optional wordmark measure / chronos-note fixtures.

## Iteration package

| Path | Purpose |
|------|---------|
| `specs/chronos-titlebar-chrome.md` | Titlebar + logo on ink |
| `specs/logo-gallery-lockup.md` | Studio gallery + wordmark |
| `specs/shell-ia-finish.md` | DF-V1130 IA completion |
| `specs/desktop-icons-residuals.md` | Desktop icons + residual archive |
