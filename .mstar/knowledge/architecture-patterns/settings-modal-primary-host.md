---
module: apps/web + apps/design-studio
date: 2026-07-22
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-22-v1.131-p2-shell-ia-finish
tags: [settings-modal, shell-ia, react-router, dirty-guard, studio-first]
applies_when: Settings is the primary configuration surface (modal, not full-page), with deep links, section registry, and dirty-aware close
---

# Settings Modal Primary Host Pattern

**Track**: Knowledge (durable guidance, distilled from V1.131 P0 gear wire + P2 shell IA finish).

## Context

V1.130 left Settings as a full-page shell with a half-wired `SettingsModalHost`. Authors expected a large modal (≥80vw×80vh), deep links (`/settings/*`), and a single dirty close path. V1.131 ships **one** app-level host as the primary Settings surface: titlebar gear (P0) opens it; P2 owns section registry, URL resolver, last safe background route, and dirty registrations.

## Guidance

### 1. One host owns chrome — section bodies are content-only

| Concern | Owner |
| --- | --- |
| Radix dialog open/size, focus restore | `SettingsModalHost` |
| Section registry (`id`, `labelKey`, icon, `Content`) | Shared descriptors SSOT (Frame / Host / ShellLayout consume the same list) |
| URL resolve (`/settings`, aliases, unknown section, `/modules`) | Host controller |
| Last safe non-settings route | Host (direct load → `/works`) |
| Dirty registrations + discard confirm | Host `requestClose` / `registerDirtySource` |
| Section forms | Content components only — **never** own a second Dialog root for the primary chrome |

**Reject:** full-page Settings as primary; per-section nested Settings dialogs; duplicate descriptor lists per layout.

### 2. Deep links over a safe background

- Open Settings **over** the last non-settings location.
- Direct `/settings/*` loads use `/works` (or another product-safe default) behind the modal.
- Close restores the saved safe route.

### 3. Dirty leave on BrowserRouter

`apps/web` uses **BrowserRouter**, not a data router. `useBlocker` is unavailable (and MSW `AbortSignal` interactions made data-router migration non-trivial in-iteration).

**Pattern that ships:**

1. Detect dirty leave (in-modal close **and** in-app navigation away from `/settings/*`).
2. Restore the Settings URL so the modal stays mounted for the confirm UI.
3. Host discard confirm → commit leave or stay.

**Upgrade path:** when forms register dirty via `registerDirtySource` and the app moves to a data router, prefer `useBlocker` for pre-navigation block. Do not archive “route-leave dirty” as Done until the chosen strategy is tested (see V1.131 QC F-001 regression).

### 4. Studio-first chrome fixtures

Settings chrome (five-section nav, modal size) must appear in Design Studio fixtures (light + dark) before claiming App wiring. Section product bodies may stay app-local; chrome is shared.

## What did not work

- Premature reliance on data-router `useBlocker` while the app remained on BrowserRouter.
- Archiving route-leave dirty as “green” without a regression that leaves settings and asserts the confirm + URL restore.

## See also

- Spec: `shell-ia-finish.md`
- Plans: `2026-07-22-v1.131-p0-chronos-titlebar` (gear), `…-p2-shell-ia-finish` (modal primary)
- Related: [ui-component-promotion-workflow.md](ui-component-promotion-workflow.md), [creator-shell-content-mode-pattern.md](creator-shell-content-mode-pattern.md)
