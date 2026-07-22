---
iteration_id: V1.132
start_date: 2026-07-22
status: completed
end_date: 2026-07-22
iteration_base_branch: main
spec_integration_branch: iteration/v1.132
target_branch: main
scale: L
direction_lock_mode: interactive
plans:
  - 2026-07-22-v1.132-p0-orch-load-404
  - 2026-07-22-v1.132-p1-titlebar-window-drag
  - 2026-07-22-v1.132-p2-vi-aesthetic-retune
  - 2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia
---

# V1.132 Delivery Compass

## Scope

**Locked direction (feedback-driven):** Control Room dogfood + load-blocker iteration — not Chat depth, not Timeline remainder, not DF-70.

1. **编排 load** — Strategy / Sessions / Compute Modules no longer fail with `Request failed with status 404` on a healthy daemon (V1.130 P3a false-Done; classification-only).
2. **Titlebar window chrome** — Chronos titlebar stays; dragging moves the window (not logo image / text selection). Supersedes V1.131 empty-paint-only drag AC.
3. **VI aesthetic retune** — logo plain vs `*-square` + icon border; Setup agent selection; theme-aware primary Button (light shell ≠ neon cyan + deep ink); compact timeline mark (−30%–50%); open VI ledger.
4. **功能区 IA** — 创作 left = Create-only (grill A: 创建 World / 延续 Work); Worlds/Works on right lists only; 工作区 always bottom-left; 创作/编排 = modes under workspace (supersede V1.131 orchestrator-only 工作区).

## Terminology (author-facing)

| Concept | Label |
|---------|-------|
| Shell layout | 左功能区 + 右内容区 |
| Mode switch | 创作 \| 编排 (功能区 footer) |
| Profile/workspace | 工作区 (always bottom-left; parent of modes) |
| 创作 hub left | Create-only — 创建 World / 延续 Work (no Menu mode) |
| 编排 surfaces | Strategy, Sessions, Compute Modules (Orchestration) |
| Healthy daemon | Engine running; daemon HTTP on expected host/port; routes registered |
| VI chrome | Chronos titlebar + theme-aware primary Button |

## Architecture Locks

**Lock date:** 2026-07-22  
**Branch path:** `main` → `iteration/v1.132` → `main`

### P0 — RCA ownership and failure boundaries

| Boundary | Owns | Failure signature / required action |
|---|---|---|
| Web client | Request construction, route mapping, response classification, and user-visible state | Capture exact URL, method, status, and body in Network evidence; a 404 remains a route-miss signal, while engine absence is rendered as UnavailableState |
| Daemon runtime | HTTP route registration, host/port serving, orchestration proxying, and healthy-vs-absent backend semantics | A missing route or stale runtime is a daemon defect; a healthy route must not return generic 404 for the locked endpoints |
| Desktop sidecar / Overlay | Starting or selecting the daemon process, injecting endpoint configuration, and desktop-host smoke context | A stale process, wrong host/port, or sidecar wiring mismatch is a desktop boundary defect; it must be distinguished from a daemon route defect |

P0 is RCA-first: client, daemon, and desktop evidence must identify the proven boundary before implementation. Classification-only tests or a 503-only result cannot close the plan. `wire_contracts_changed: false` for P0 unless RCA proves a contract shape gap and the plan is amended.

### P1 — Window chrome ownership

The desktop Overlay remains the shell owner; the web titlebar owns only the visual chrome and drag-region attributes. `data-tauri-drag-region` covers logo/title paint, while gear/theme/health controls explicitly opt out. Native image dragging and text selection are disabled without replacing the Overlay or changing wire contracts.

### P2 — VI ownership

`@42ch/nexus-ui` is the Button and token presentation SSOT; Studio is the visual proving ground; App and desktop consume the accepted primitives/assets. Plain marks, `*-square` plates, compact timeline marks, and inset icon composition remain presentation assets. Theme behavior is centralized in Button variants, not error-block-specific overrides.

### P3 — Shell ownership

`Sidebar` owns left navigation and mode intent; `ShellSidebarChrome` owns the persistent shell framing and left-slot composition; `CreatorShellContent` owns the Create-only creator hub and right-side Worlds/Works lists; `FooterProfiles` owns the always-visible workspace footer/profile anchor. Workspace identity is parent-shell state, while 创作/编排 are modes beneath it. No component may reintroduce creator Menu navigation on the left or gate the footer to 编排.

All four plans are presentation/runtime boundary work and retain `wire_contracts_changed: false` unless a proven exception is recorded in the affected plan and spec.

## Plans

| Wave | plan_id | Name | Status | blocked_by |
|------|---------|------|--------|------------|
| 1 | [`2026-07-22-v1.132-p0-orch-load-404`](../../plans/2026-07-22-v1.132-p0-orch-load-404.md) | Orch Strategy/Sessions/Modules 404 | **Done** | — |
| 1∥ | [`2026-07-22-v1.132-p1-titlebar-window-drag`](../../plans/2026-07-22-v1.132-p1-titlebar-window-drag.md) | Titlebar window-drag restore | **Done** | — |
| 2 | [`2026-07-22-v1.132-p2-vi-aesthetic-retune`](../../plans/2026-07-22-v1.132-p2-vi-aesthetic-retune.md) | VI aesthetic retune | **Done** | — |
| 3 | [`2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia`](../../plans/2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia.md) | 创作 Create hub + 工作区 persistent | **Done** | — |

**Scale budget:** L → **4** business plans (cap).

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit lock) | 2026-07-22 | **done** |
| Wave 1 (P0 ∥ P1) | 2026-07-22 | **done** |
| Wave 2 (P2 VI) | 2026-07-22 | **done** |
| Wave 3 (P3 功能区) | 2026-07-22 | **done** |
| Iteration close | 2026-07-22 | **done** |

## Acceptance Criteria

### Orch load (P0)

- **AC-0.** On a healthy daemon (engine running, daemon responding on expected host/port): Strategy list+detail, Sessions, Modules list+detail load with no happy-path 404. Engine-absent → UnavailableState (503), not 404.
- **AC-0b.** RCA matrix + regression for proven 404 mode (curl/Network evidence attached to plan gate summary).

### Titlebar (P1)

- **AC-1.** Logo/title area + empty paint drag the window; no img native-drag; title select-none; controls clickable.
- **AC-2.** Full-width Chronos ink titlebar remains.

### VI (P2)

- **AC-3.** Logo plain vs `*-square` split; Studio Brand + tokens.
- **AC-4.** Timeline mark compact vs wordmark (−30%–50% SSOT scale); Brand hero + titlebar + app match.
- **AC-5.** App icon inset compose; no light rectangular halo.
- **AC-5b.** Agent card selected state: one clear affordance.
- **AC-5c.** Primary Button theme-aware (light ≠ neon+deep ink; dark keeps strong cyan CTA).
- **AC-5d.** VI ledger: further notes Must/Should triage; no silent drop.

### 功能区 (P3)

- **AC-6.** 工作区 footer visible on both 创作 and 编排 modes.
- **AC-7.** Workspace is the parent shell: 创作 and 编排 behave as modes under 工作区 — switching mode does not change the active workspace identity, and 工作区 footer persists across both. **Supersedes V1.131 AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH`** (orchestrator-only 工作区).
- **AC-8.** 创作 hub: left = Create-only (创建 World / 延续 Work); Worlds/Works = right content lists only; no left Menu mode (grill A locked).
- **AC-9.** Studio fixtures: 创作 hub light+dark showing Create (not Menu-only).

## Supersessions (explicit)

| Prior AC / DF | Superseded by | Reason |
|----------------|---------------|--------|
| V1.131 chronos-titlebar AC (empty-paint-only drag) | P1 AC-1, AC-2 | Drag expands to logo/title chrome, not empty paint only |
| V1.131 shell-ia-finish AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH` (工作区 footer only under 编排) | P3 AC-6, AC-7 | 工作区 is parent shell; 创作/编排 = modes under it; footer always visible |
| V1.130 P2 创作-left-Menu false-Done | P3 AC-8 | 创作 hub left = Create-only; no Menu mode on left |

## Non-Goals

- Deep Creator Chat / orchestration product depth
- DF-70 execution-mode matrix, DF-71 menu-bar daemon
- Timeline / Fork / Computable deferred features (DF-V1122-*, DF-V1123-*)
- Full residual burn-down (64 open)
- Engine auto-start
- Runtime multi-theme switcher (Umbra/Aurora)
- Multi-root workspace rewrite
- Platform / cloud work

## Roadmap Position

- **Current (V1.132):** **delivered** — orch load 404 fix, titlebar window-drag, VI aesthetic retune, Create-only hub + persistent 工作区.
- **Next:** Deeper Creator entity Chat; Orchestrator 功能区 beyond interim menu; opportunistic DF-70/71; open human smokes (`R-VI-003` Dock, titlebar Overlay guide).
- **Prior:** V1.131 Chronos titlebar + DF-V1130 IA + desktop icons (#169).

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.132` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| P0 is env-only (stale daemon) | Med | Med | Curl matrix first; still ship smoke/docs + regression if code healthy |
| VI scope creep via ledger | Med | Med | Must/Should triage at PM lock; no 5th plan |
| P3 layout churn vs V1.131 AC-4 | High | Med | Explicit supersession in specs; Studio fixtures first |
| False Done repeat (classification-only) | Med | High | Dogfood Network evidence required for P0 Done |

## Iteration package

| Path | Purpose |
|------|---------|
| [README.md](README.md) | Package index, terminology, plan/spec links |
| [guides/](guides/) | Exploration / process notes (non-normative) |
| [specs/orch-load-404.md](specs/orch-load-404.md) | P0 — 编排 load 404 repair |
| [specs/titlebar-window-drag.md](specs/titlebar-window-drag.md) | P1 — Chronos titlebar window-drag |
| [specs/vi-aesthetic-retune.md](specs/vi-aesthetic-retune.md) | P2 — VI aesthetic retune |
| [specs/creator-orch-gongnengqu-ia.md](specs/creator-orch-gongnengqu-ia.md) | P3 — 功能区 IA |

## Direction lock evidence

- Feedback-driven Plan mode + deferred grill Q1 → **A** (创作 left Create-only; Worlds/Works right lists; no Menu mode).
- Branch policy: `main` → `iteration/v1.132` → `main`.


## Quality Gate Summary

| Plan | QC | QA | Merge |
|------|----|----|-------|
| P0 orch-load-404 | Pass (tri) | Pass | Done |
| P1 titlebar-window-drag | Pass (tri) | Pass | Done |
| P2 vi-aesthetic-retune | Pass (tri; F-001 fix + QC2 reval) | Pass | Done |
| P3 creator-orch-gongnengqu-ia | Pass (tri) | Pass | Done |

Open carry (accepted defer): `R-VI-003` Dock human smoke; `R-V1131P0-QC2-W-001` Overlay guide human steps; brace-param sweep residual from P0 QC.


## Compound Round Summary

**Package inventory (`v1.132/`):**

| Artifact | Disposition |
|----------|-------------|
| `guides/p0-orch-load-404-rca.md` + dogfood | **Promoted** → `knowledge/architecture-patterns/daemon-matchit-colon-capture.md` |
| `guides/titlebar-window-drag-overlay-smoke.md` | **Retain** in package (human Overlay smoke SSOT) |
| `specs/*` (4) | **Retain** (normative iteration specs; supersession already in compass) |
| P2 brand/token learnings | **Updated** `nexus-brand-token-hierarchy.md` (ink hover tokens) |
| P3 shell IA | **Promoted** → `workspace-parent-shell-ia.md` |

**Q1–Q8 (P0 matchit):** Yes≥4 → crystallize. **P3 IA:** Yes≥4 → crystallize. **P1 drag:** covered by guide + residual; skip new doc. **P2:** update existing brand doc (Q5 overlap).


## Iteration Retrospective (minimal)

- **What worked:** RCA-first P0 (empty-body 404 → matchit); Studio-first P2/P3; explicit supersession tables for false-Done priors.
- **Friction:** Feature/control `status.json` merge conflicts during lease/InReview sync; light primary Button briefly locked cyan hover until QC2.
- **Carry:** Human Dock/Overlay smokes; remaining brace-param daemon routes; optional Studio legacy footer fixture cleanup.
