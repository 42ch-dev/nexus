---
module: apps/web
date: 2026-07-12
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
tags: [command-palette, action-registry, useSyncExternalStore, useHotkey, discoverability, a11y]
applies_when:
  - "adding a global ⌘K/Ctrl+K command palette or keyboard-driven action surface"
  - "registering surface-local actions without coupling them to the palette component"
  - "avoiding React context re-render storms for a global action list"
---

# Action Registry + Command Palette Pattern

**Track**: Knowledge (durable guidance, distilled from V1.111 P0 Shared Canvas Command Palette).

## Context

After V1.108–V1.109 deepened canvas graphs, surfaces were capable but not discoverable: authors navigated by pointer only. V1.111 landed a global command palette (⌘K/Ctrl+K) backed by an extensible **action registry** so Strategy / Outline / World KB (and future surfaces) can register commands without editing the palette component (FB-CP-003).

## Guidance

### 1. Module-level store + `useSyncExternalStore` (not React context)

```ts
// apps/web/src/lib/canvas/command-registry.ts (shape)
type Command = {
  id: string;
  label: string;
  group: string;
  keywords?: string[];
  icon?: LucideIcon;
  handler: () => void;
  available?: () => boolean; // render-time predicate — NOT applied by the store
};

// registerCommand / unregisterCommand on a module Map
// useCommands() → useSyncExternalStore(subscribe, getSnapshot)
// useRegisterCommand(action) → useEffect mount register / unmount unregister
```

**Why not context/provider:** every register would re-render the whole tree under the provider. A module store + `useSyncExternalStore` only notifies palette subscribers, keeps `filterCommands` pure/testable, and stays StrictMode-safe when registration is id-keyed (last-write-wins, idempotent).

### 2. `available?()` is render-time, not store-time

The store does **not** filter by `available()`. The palette (or any consumer) evaluates `available?.() ?? true` at render. That keeps the store honest (emits only on register/unregister) and lets predicates read live URL/params without forcing re-registration.

### 3. Hotkey layer with conflict-avoidance

```ts
// apps/web/src/lib/use-hotkey.ts
useHotkey('mod+k', openPalette); // mod = meta || ctrl
// Ignore when focus is INPUT / TEXTAREA / [contenteditable] /
// [data-command-palette-ignore] / .react-flow
```

Register once in `RootLayout`. Palette open-state can be a co-located module store (same pattern as the registry) so the layout stays free of React state for the overlay.

**Residual risk (tracked):** relying on the `.react-flow` CSS class is not part of React Flow's public API (`R-V1111P0QC2-W001`). Prefer a data-attribute escape hatch for long-term robustness.

### 4. Palette a11y contract

- `role="dialog"` + focus trap + restore focus to pre-open element
- Input: `role="combobox"` with `aria-activedescendant`
- List: `role="listbox"` / `role="option"`
- Filter: case-insensitive substring + rank (exact → startswith → contains → keyword); no fuzzy dep required for MVP

### 5. Registration sites

- **Global nav commands** (e.g. Go to Strategy / Outline / World KB): a small component mounted in `RootLayout` that reads route params via refs so mount-once handlers stay live across navigation.
- **Surface-local commands** (toggle view, create transition): register inside the surface with `useRegisterCommand` so they auto-unregister on unmount.

### 6. File placement

- Registry: `apps/web/src/lib/canvas/command-registry.ts` (canvas-action-scoped this iteration)
- Hotkey: `apps/web/src/lib/use-hotkey.ts`
- Palette: `apps/web/src/components/command-palette.tsx` (**top-level components/**, not under `canvas/` — it is a global shell overlay)

## Why This Matters

- Discoverability scales with surfaces: new canvas (or non-canvas) features add a command, not a palette fork.
- Avoids context re-render tax on a global list that mutates as routes mount/unmount.
- Keyboard + a11y first class (⌘K, Arrow/Enter/Escape, focus restore).

## When to Apply

- Any global action palette or command menu over multiple product surfaces.
- Registering route- or selection-gated actions without centralizing handlers in one mega-component.

## What Didn't Work

- **React context for the registry:** every surface registration would re-render the tree; rejected at architecture lock.
- **Fuzzy search dependency:** none in the workspace; substring + rank satisfied FB-CP-001 without a new dep.
- **Capturing workId/worldId in mount-once handlers without refs:** stale closures across same-layout route changes; fix: `idsRef.current` updated every render + functional updaters for toggles.

## Examples

- V1.111 P0: `command-registry.ts`, `use-hotkey.ts`, `command-palette.tsx`, `canvas-nav-commands.tsx`
- V1.111 P1: sidebar nests Canvas surfaces; palette already owns `go.*` — do not re-register the same ids from the sidebar
