# Spec — Shell & form polish (V1.120 P1)

**Status:** writing-specialist reviewed (Phase 1 §5.3) — pending PM §5.4 lock  
**Iteration:** V1.120  
**Plan:** `2026-07-17-v1.120-shell-form-polish`  
**Feedback:** F1, F2, F6

## Problem

After Agent profile Save, the desktop footer `DaemonStatusBar` agent badge stays stale until the 10s poll. Save stays enabled when nothing changed. Create-Work Select shows a duplicate chevron artifact. Settings → Advanced “re-run wizard” CTA is nearly invisible (not danger). Dark-theme disabled primary buttons have poor contrast. Footer Restart lacks an explicit hint that it restarts the daemon.

## User value

Authors trust shell chrome: Save means something changed, the footer reflects reality immediately, and destructive or daemon actions are clearly labeled.

## Goals

1. **Dirty Save (iteration rule):** Persist CTAs on V1.120-touched surfaces are disabled until local state ≠ last-saved baseline.
2. After successful Agent Save, footer agent badge refreshes immediately from the saved profile (query invalidation and/or explicit refetch — not poll-only).
3. Select trigger shows a single chevron (no stray icon outside the control).
4. Advanced re-run wizard uses danger `Button` variant (readable in light and dark).
5. Disabled primary/destructive buttons remain legible in dark theme per DESIGN tokens.
6. `DaemonStatusBar` Restart (and agent badge where helpful) expose tooltip + accessible name stating daemon restart.

## Dirty Save — touched surfaces (locked)

| Surface | Persist CTA | Baseline |
| --- | --- | --- |
| Settings → Agent | **Save** (`settings-save-agent`) | Last saved agent profile from `getAgentProfile` / post-save snapshot |
| Other P1 edits | Any primary persist control modified in this plan | Last successful save for that form |

**Out of scope for dirty-gate:** Create Work dialog (create-on-submit, not edit-save); read-only pages; workspace path Apply (not in P1 scope unless incidentally touched).

**Behavior:** On load/hydrate, persist CTA **disabled**. After user edit, **enabled**. After successful save, baseline updates and CTA **disabled** again.

## Non-goals

- Redesigning Settings IA
- Changing daemon restart semantics or sidecar lifecycle
- Global audit of every form in the app
- Workspace path dirty-gate (unless trivially bundled — not required for V1.120 Done)

## Product decisions (locked)

| Decision | Rule |
| --- | --- |
| Footer refresh | After Agent Save success, `DaemonStatusBar` must show new agent name **before** next poll tick |
| Advanced CTA | Re-run setup wizard button = `danger` variant |
| Restart copy | Tooltip + `aria-label` must name **daemon** restart (not app/window); i18n keys in `shell` or `settings` namespace |
| Tokens | Align danger/disabled with `@nexus/design-tokens` / `Button` variants — no one-off hex |

## Copy & i18n requirements

Per DESIGN.md §Voice & Content: Title Case for buttons and tooltips; sentence case for helpers; verb-only persist CTAs.

| Surface | en intent | i18n |
| --- | --- | --- |
| **Restart** | Tooltip and `aria-label` must name **daemon** restart — not app/window quit | Reuse `shell.daemon.restart` (“Restart daemon”) for `title` / `aria-label` on the status-bar control; `shell.daemon.restartButton` (“Restart”) may remain the visible label if tooltip carries the object |
| **Save Agent** | Verb-only **Save**; disabled when clean | Reuse `common.action.save` |
| **Advanced re-run** | Verb-led label; danger variant carries affordance — do not rely on color alone | Reuse existing Settings Advanced copy; optional tooltip if label is ambiguous |

## Acceptance criteria

| ID | Criterion |
|----|-----------|
| AC-P1-1 | Agent Save disabled when selection matches last-saved profile; enabled when any field differs (dirty) |
| AC-P1-2 | After Save success, `DaemonStatusBar` agent badge shows new agent name without waiting for 10s poll |
| AC-P1-3 | Native Select / shadcn Select in Create Work has no duplicate chevron outside the control boundary |
| AC-P1-4 | Advanced re-run wizard button uses `danger` variant and meets contrast in dark mode |
| AC-P1-5 | Disabled primary button text/background contrast is acceptable in dark (token- or variant-level fix) |
| AC-P1-6 | Restart control has `title` or tooltip and `aria-label` clarifying **daemon** restart |
| AC-P1-7 | Component/unit tests cover dirty gate (clean → dirty → save → clean) and post-save footer refresh signal |

## Architecture decisions (locked)

| ID | Decision |
| --- | --- |
| **AD-P1-1** | After Agent Save success, call `queryClient.invalidateQueries` for keys read by `DaemonStatusBar` (agent profile label + daemon health/scan — match existing `queryKeys` in `settings-agent-section.tsx` / `daemon-status-bar.tsx`). Do **not** shorten global poll interval as the primary fix. |
| **AD-P1-2** | Dirty gate: compare hydrated form state to last-saved snapshot from `getAgentProfile` response. Extract a small `useDirtyForm<T>` helper **only if** it stays scoped to Settings Agent; otherwise inline dirty flag in `settings-agent-section.tsx`. |
| **AD-P1-3** | Select chevron: fix in the shared Select primitive (`apps/web/src/components/ui/select.tsx` or whichever Create Work uses) — remove duplicate lucide chevron when native `appearance` already renders one. |
| **AD-P1-4** | Danger/disabled contrast: prefer `Button` variant CSS against `@nexus/design-tokens` — no one-off hex. Scope to `danger` + `disabled` primary variants touched by P1. |
| **AD-P1-5** | Restart tooltip/`aria-label`: use existing `shell.daemon.restart` (“Restart daemon”) for accessible name; ensure control exposes `title` and/or `aria-label` — not bare “Restart” without object. |

## Notes for implementers

- `DaemonStatusBar` must subscribe to the same query keys invalidated on save (no prop-drilling event bus unless invalidation insufficient in tests).
- Create Work dialog submit remains create-on-submit — **out of** dirty-gate scope.
