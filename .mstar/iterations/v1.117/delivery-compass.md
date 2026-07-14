---
iteration_id: V1.117
start_date: 2026-07-14
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.117
plans:
  - 2026-07-14-v1.117-profiles-workspace
  - 2026-07-14-v1.117-setup-agents-catalog
  - 2026-07-14-v1.117-shell-layout-ia
  - 2026-07-14-v1.117-mac-dock-icon
  - 2026-07-14-v1.117-button-voice-verb-only
---

# V1.117 Delivery Compass — Desktop UX Polish

> **Phase 1:** PM reviewed (§5.1). Architect locked (§5.2). Writing complete (§5.3).
> **PM lock (§5.4):** `status: locked`. Prepare gates pass on all five plans. Spec freeze locked.

### PM lock notes (§5.4)

1. **P3 Dock icon** remains **Should** — recommended for closeout, not a Must gate for iteration Done.
2. **Filenames** (`setup-step-workspace.tsx`, `settings-workspace-section.tsx`) may stay; product copy uses Setup Profile step / Settings Profiles.
3. **`running` tag** stays lowercase in both locales (AD-P2-5) — intentional.
4. **Profiles Settings** edits the **active** Profile only this iteration.
5. **Button voice:** Setup/Settings flows require Verb-only hard zero; other surfaces may register ≤5 stragglers as residuals (AD-P4-3).

## Product story

Authors using the Nexus desktop app still hit first-impression friction after
V1.116 honesty fixes: Profiles/workspace identity is incomplete, Setup agent
cards leak ACP jargon and broken outbound links, the shell IA mislabels Canvas
as a nav group, the Dock icon looks like a white square, and button copy is
bloated with Verb+Noun.

The single coherent bet:

> **Make the desktop shell feel product-complete: Profiles own workspaces,
> Agents are honest and open in the system browser, the shell layout and menus
> match author mental models, and chrome (icon, buttons, status bar) is calm.**

| Who | What they get when V1.117 is Done |
| --- | --- |
| **Authors (first launch)** | Default Profile always exists; Setup edits Profile name; workspace path follows Profile |
| **Authors choosing agents** | "选择智能体"; cards show registry name/description/icon/website; Install/Docs open in system browser; **Claude/Codex Native Adapters** in default grid |
| **Authors navigating** | No Canvas menu group; Strategy under Orchestration; World KB under Creation; Settings+Profiles bottom-aligned; content-only scroll; work drill-in skeleton |
| **Desktop users** | Transparent Dock icon; status bar shows Daemon Running + agent badge |

### Grill-me decisions (locked)

1. **Menu IA depth**: Static regroup + work drill-in skeleton; defer full multi-level IA.
2. **Mac Dock icon**: Transparent background + keep logo (compose pipeline).
3. **Agent catalog config**: Repo-tracked whitelist/overrides only (no `~/.nexus42` user layer).
4. **Profiles depth**: UI/Default Profile + **P0 workspace follows Profile**.
5. **Plan split**: 5 plans (P0–P4); P4 Button voice added after author feedback.
6. **Branch policy**: `main` → `iteration/v1.117` → `main`.
7. **Status bar**: Daemon "Running"; tag `running`; state via left dot; clickable agent badge (name+version or placeholder) → Settings Agents.
8. **Button voice**: Buttons/CTAs = **Verb-only**; update DESIGN.md + norms + sweep locales; titles/helpers unchanged.
9. **Sidebar footer**: Settings + Profiles form one bottom-aligned section.

## Scope

- P0 Profiles + per-Profile workspace path
- P1 Setup Agents catalog polish
- P2 Shell layout + menu IA + status bar + work drill-in
- P3 Mac Dock transparent icon
- P4 Button voice Verb-only + DESIGN.md

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-14-v1.117-profiles-workspace | Profiles + per-Profile workspace | Done | Must / P0 |
| 2026-07-14-v1.117-setup-agents-catalog | Setup Agents catalog polish | Done | Must |
| 2026-07-14-v1.117-shell-layout-ia | Shell layout + menu IA + status bar | Done | Must |
| 2026-07-14-v1.117-mac-dock-icon | Mac Dock transparent icon | Todo | Should (recommended close) |
| 2026-07-14-v1.117-button-voice-verb-only | Button voice Verb-only + DESIGN.md | Todo | Must |

### Plan dependencies (implement order)

| Plan | Depends on | Notes |
| --- | --- | --- |
| P0 Profiles | — | Foundation; P2 depends on P0 T4 (footer switch) |
| P1 Agents | — | Parallel-safe with P0/P4 |
| P2 Shell IA | P0 T4 | Footer switch must activate per-Profile path |
| P3 Mac icon | — | Parallel-safe; asset pipeline only |
| P4 Button voice | — | Parallel-safe; normative docs + locale sweep |

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze | 2026-07-14 | locked (§5.3 writing) |
| Dev complete | TBD | pending |
| QC complete | TBD | pending |
| Iteration close | TBD | pending |

## Acceptance Criteria

Iteration is **incomplete** if any **Must** plan (P0, P1, P2, P4) fails its
author-observable AC. P3 (Should) is recommended for close but not a hard gate
unless promoted by PM.

### Must — P0 Profiles

- Default Profile always exists; Setup edits Profile name; Settings **Profiles**
  manages name + per-Profile workspace path
- Switching Profile activates that Profile's workspace (restart honesty when
  applicable)
- Legacy single `workspace_path` migrates to Default Profile

### Must — P1 Agents

- De-ACP picker copy; cards show registry name/description/icon/website
- Install/Docs open in system browser on desktop; whitelist-gated install URLs
- Claude/Codex native in default grid; More Agents installed-first

### Must — P2 Shell

- Sidebar/status bar fixed; **Settings + Profiles** bottom-aligned together
- Menu IA: no Canvas group; Strategy → Orchestration; World KB → Creation
- Work drill-in skeleton (`Back to all` / `返回所有`, `Outline` / `大纲`, `Body` / `正文`)
- Status bar: Daemon Running + `running` tag + state dot; agent badge →
  `/settings/agent`

### Should — P3 Dock icon

- Dock icon no longer full-bleed opaque white square

### Must — P4 Button voice

- DESIGN.md / web AGENTS / design-studio Voice: **Verb-only** for buttons
- Button locale strings swept (en + zh-CN); titles/helpers unchanged

### Author-visible scenarios

| # | Scenario | Pass when |
| --- | --- | --- |
| 1 | First launch Setup | Author names Profile on Setup Profile step; Default Profile exists in footer |
| 2 | Settings Profiles | Tab says Profiles; path edit + honesty copy; per-Profile path persists |
| 3 | Profile switch | Footer switch changes active Profile and workspace root (desktop) |
| 4 | Setup agents | Title says 选择智能体; Install opens system browser; native Claude/Codex visible |
| 5 | Shell navigation | No Canvas group; Strategy under Orchestration; content-only scroll |
| 6 | Work drill-in | Inside work context → back + Outline + Body skeleton |
| 7 | Status bar | Running + running tag; agent badge shows selection or placeholder; click → Agent settings |
| 8 | Dock (macOS) | Logo on squircle, not white square |
| 9 | Buttons | Primary CTAs are single verbs in en + zh-CN |

Per-plan AC IDs: see each spec under `specs/*.md`.

## Non-Goals

- Full multi-level menu tree beyond work drill-in skeleton
- Extra Profile fields beyond name + workspace path
- User-layer `~/.nexus42` agent-catalog overrides
- Enterprise registry CDN mirror
- Rewriting historical iteration specs that mention Verb+Noun (except active normative hygiene)

## Roadmap Position

- **Current iteration (V1.117)**: Desktop UX polish — Profiles/workspace, Agents, shell IA, Dock icon, button voice
- **Next iteration**: Full hierarchical menu depth and/or deeper Profiles fields — trigger: V1.117 shipped + author feedback
- **North star**: Local-first creative desk that feels complete without protocol jargon

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.117` |
| `target_branch` | `main` |

## Architect decisions (§5.2 — locked)

Cross-plan ADRs resolved in iteration specs; implementers treat these as normative.

| # | Topic | Decision |
| --- | --- | --- |
| AD-P0-1 | Per-Profile path storage | `[workspace_path_by_creator]` TOML table keyed by `creator_id`; mirrors `active_workspace_slug_by_creator` |
| AD-P0-2 | Legacy migration | One-shot dual-read on first access: map entry ← global `workspace_path` when missing; writes mirror active path to legacy `workspace_path` for CLI/daemon parity |
| AD-P0-3 | Tauri API | `set_workspace_path` infers `creator_id` from `active_creator_id`; profile switch updates `active_creator_id` + per-creator path + legacy mirror |
| AD-P0-4 | Settings route | Keep `/settings/workspace`; label-only rename to Profiles |
| AD-P1-1 | Overrides config | `apps/web/config/agent-catalog-overrides.json` schema v1 — `install_whitelist` + per-id `agents` overrides |
| AD-P1-2 | External URLs | New Tauri `open_external_url` (http/https only; no path guard); `DesktopCapabilities.openExternalUrl` |
| AD-P1-3 | Native default grid | Canonical ids `claude-native`, `codex-native`; hide `claude-acp` / `codex-acp` from default via overrides |
| AD-P2-1 | Drill-in trigger | `workId` present in route (`/works/:workId/*`) — skeleton replaces primary nav groups |
| AD-P2-2 | Body target | `/works/:workId/chapters` (existing chapters surface; skeleton only) |
| AD-P2-3 | Agent badge | Display name from saved profile + version from last scan cache (or providers list); placeholder when unset |
| AD-P2-4 | Layout SSOT | `shell-sidebar-chrome.tsx` bottom block = Settings + Profiles footer slot; `root-layout.tsx` owns scroll split |
| AD-P3-1 | Dock shadow | Transparent canvas; **keep** soft shadow at reduced opacity (0.12) |
| AD-P4-1 | Locale sweep | Keys with `.button` / `.submit` / `.cta` suffix or `common.actions.*` when rendered as `<Button>` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Per-Profile workspace path migration breaks existing single `workspace_path` users | Med | High | Migrate global path into Default Profile map; restart honesty banner if needed |
| System browser opener gaps in Tauri webview | Med | Med | Dedicated `open_external_url` + desktop capability; browser keeps `target=_blank` |
| Button voice sweep misses hardcoded strings | Med | Low | Locale-first sweep + grep for Verb+Noun patterns; residual for stragglers |

## Iteration package

| Path | Purpose |
|------|---------|
| `specs/` | Iteration-scoped specs (one per plan theme) |
| `README.md` | Package index |

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| — | — | — | — | — |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
