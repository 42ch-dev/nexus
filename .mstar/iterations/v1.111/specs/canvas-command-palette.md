# P0 — Canvas Command Palette (⌘K) Spec

> Iteration: V1.111. Status: **PM draft — architect refines at §5.2**. Primary
> consumer: plan `2026-07-12-v1.111-canvas-command-palette.md`.

## Problem

There is no command palette in the Control Room. Grep confirms zero
`CommandPalette` / `cmdk` / "command palette" usage in `apps/web/src` or
`apps/design-studio/src` (only unrelated test string matches). Authors navigate
canvas surfaces and fire actions purely by pointer, which is slow for a
keyboard-first authoring tool. The V1.108, V1.109, and V1.110 compasses all name
a "shared command palette" as the next-iteration discoverability feature.

## User value

- **FB-CP-000** — ⌘K / Ctrl+K opens a palette overlay from anywhere in the
  Control Room.
- **FB-CP-001** — Palette lists canvas navigation actions (go to Strategy /
  Outline / World KB), node-creation actions, and surface switches;
  fuzzy-filtered by typing.
- **FB-CP-002** — Arrow + Enter keyboard navigation; Escape closes; focus
  returns to the calling element.
- **FB-CP-003** — Palette consumes an **action registry** so P1 (sidebar),
  P2 (design studio), and future surfaces add commands without editing the
  palette component.
- **FB-CP-004** — Light + dark DESIGN token consumption; a11y (role=dialog,
  aria-combobox) verified.

## Open questions for architect — RESOLVED (2026-07-12)

> See plan `## Architecture locks` for full evidence. Verdicts below.

1. **Action registry contract** — **Resolved.** Shape:
   `{ id, label, group, keywords?, icon?, handler, available?() }`. Location:
   `apps/web/src/lib/canvas/command-registry.ts`. Registration: **module-level
   store + `useSyncExternalStore`-backed `useCommands()` + `useRegisterCommand(action)`
   hook** — surfaces register in a `useEffect` without editing the palette
   (satisfies FB-CP-003). Provider/context rejected (whole-tree re-render).
2. **Filter strategy** — **Resolved: case-insensitive substring + cheap rank**
   (exact-label → startswith → contains → keyword-contains). No fuzzy lib in the
   workspace; substring+rank satisfies FB-CP-001. fzf-style subsequence ranking
   deferred to next iteration.
3. **Keyboard binding layer** — **Resolved: new `useHotkey` hook**
   (`apps/web/src/lib/use-hotkey.ts`). No shared hotkey layer exists today (ad-hoc
   `addEventListener` in 5 places). ⌘K/Ctrl+K registered in `root-layout.tsx`.
   Conflict-avoidance: ignore when `activeElement` is input/textarea/
   contenteditable/within RF editor pane; ⌘K still fires from buttons/links/canvas
   background.
4. **Scope of "actions"** — **Confirmed.** Must = canvas navigation + node
   create + surface switch. Editor/format/export commands are non-goals.
5. **A11y pattern** — **Confirmed.** role=dialog + combobox + listbox/option;
   focus trap; restore focus to caller on close. Palette at
   `apps/web/src/components/command-palette.tsx` (top-level `components/`, not
   `canvas/` — it is a global shell overlay).

## Non-goals

- Editor commands (bold/italic/format) — out of scope.
- Command history / recently-used ranking (defer unless trivial).
- Customizable/rebindable shortcuts.
- Backend/wire changes — `wire_contracts_changed: false`.

## DoD

All FB-CP-000..004 accepted in App; action registry is extensible (P1/P2 add a
command without editing the palette component); light + dark token consumption
verified; a11y verified; QC tri Approve; QA gate passed.
