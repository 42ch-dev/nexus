---
module: apps/design-studio
date: 2026-09-10
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
applies_when:
  - "Building a side-by-side comparison of the same UI in two themes (or two forced environments) on one page"
  - "Embedding the app's own routes as isolated previews without forking components or the theme pipeline"
  - "Reviewing a preview surface that must not read or write the host session's storage, media queries, or IDs"
tags:
  - design-studio
  - pair-view
  - iframe
  - theme-isolation
  - postmessage
  - remount
  - forced-theme
---

# Same-origin iframe pair view with forced frame-local theme

**Track**: Knowledge (distilled from the v1.187 Studio pair view; normative detail in [.mstar/specs/design-studio.md](../../specs/design-studio.md) §5.3).

## Context

Studio needed a real "see this specimen in light **and** dark at the same time" workflow — the contributor job the old single-theme gallery could not satisfy. The obvious alternative, rendering two React subtrees in the parent and scoping one with a nested `.dark` (or a light reset), fails in ways that are easy to miss:

- an ancestor-scoped theme leaks into `dark:` variant resolution, computed-style reads, portals and any component that reads the document theme;
- two copies of the same fixture produce duplicate DOM ids (labels, SVG defs, tab/panel associations);
- body portals (Dialog, Toaster) escape the scoped subtree entirely;
- the parent document has exactly one `color-scheme`, so native controls cannot show both.

The pair view therefore uses **two same-origin iframe documents running the same Studio entrypoint**, each with a forced theme. The normative contract is §5.3 of the Studio spec; this doc records the mechanics that make it honest.

## Guidance

**Two documents, one entrypoint, one allowlisted parameter.**

- The frame URL is the current allowlisted pathname + `?studio-embed=light|dark` + the current hash (`buildStudioEmbedSrc`). This is a display parameter, not a route.
- `resolveEmbeddedTheme(search, isFramed)` returns a theme only when `window.self !== window.top` **and** the value is exactly `light` or `dark`; top-level Studio ignores the parameter entirely, so a shared/stale URL never forces the app's own theme.
- The frame applies its theme to its own `document.documentElement` **before React renders** (class + `color-scheme`), so there is no flash of the wrong theme.
- A `forcedTheme` prop on the frame's `ThemeProvider` bypasses everything that makes the session theme sticky: no `localStorage` read, no `localStorage` write, no `prefers-color-scheme` listener, and `setTheme`/`toggleTheme` become no-ops. Session preference survives mount/unmount/refresh of frames untouched.

**Each frame owns its own world.** DOM ids, SVG `<defs>`, native focus, body portals (Dialog/Toaster), computed-style reads and reduced-motion queries all resolve inside that frame's document. Do not clone fixture markup into the parent, do not monkey-patch `createPortal`, and do not transport tokens or user events across frames — "compare" means the same specimen in two themes, not synchronized interaction.

**Readiness is a handshake, not a load event.** A successful iframe `load` proves neither app mount nor route commit.

- The embedded app posts `{ type: 'nexus-studio-embed-ready', theme, path }` to its same-origin parent **after the matched route commits** — the notifier lives inside the Suspense-wrapped lazy leaf, and the parent layout route deliberately omits it.
- The parent accepts a message only when **all** hold: `event.origin === window.location.origin`, the payload matches the expected shape/theme, the payload path equals the requested path, and `event.source` is that frame's own `contentWindow`.
- Readiness resets on pathname/theme/remount changes (not on fragment-only navigation inside an already-ready document), and a 10-second no-ready deadline plus the native `error` event raise an explicit failure state.
- Failure offers two honest exits: **Retry** (remount the frames) and **Open current gallery** (leave compare mode rather than link to a path that keeps the failed pair mounted).

**Reset means remount, and remount needs a key.** Reset comparison / Retry must recreate the frame documents; merely clearing ready/failed state and recomputing `src` does not, because the computed URL is identical for the same path/hash/theme. Bind each `<iframe>` to a React `key` derived from the effective reset counter (see [iframe-reset-remount-by-key.md](../ui-bugs/iframe-reset-remount-by-key.md)).

**Layout thresholds are part of the contract.** Two equal `minmax(0, 1fr)` columns with a 16px gap at `lg` (1024px) and above; light-then-dark stacked below that. Each frame is full width × 640px with its own vertical scroll — never transform-scaled, clipped, or replaced by a screenshot. The parent hash stays the canonical shareable target; parent navigation/filter/hash changes update both frame URLs; compare is default off and turning it off restores the ordinary gallery at that hash.

## Why This Matters

- **No second theme pipeline.** The frames run the same entrypoint and CSS as the main app, so a variant that looks right in a frame is the real component under a real `.dark`/`:root` cascade — not a scoped approximation.
- **No state or ID leakage.** Document scope gives id uniqueness, portal containment and theme-local computed reads for free, which subtree scoping cannot.
- **Honest failure.** Because readiness is explicit and failures produce recovery actions, a broken frame can never present as a "successful" loading comparison.

## When to Apply

- Any UI that must show two themes (or light/dark parity in general) side by side on one page.
- Any embedded preview of the app's own routes that must not touch host session state.
- Reviewing a comparison feature: check the parameter allowlist, the pre-render theme application, the forced-mode storage bypass, the handshake validation quartet, and the remount key before looking at styling.

## Examples

```ts
// allowlist: framed + exact value, else the host behaves normally
export function resolveEmbeddedTheme(search: string, isFramed: boolean): EmbeddedTheme | null {
  if (!isFramed) return null;
  const value = new URLSearchParams(...).get('studio-embed');
  return value === 'light' || value === 'dark' ? value : null;
}
```

```tsx
// remount key: reset/retry really replaces the frame documents
<iframe key={effectiveResetKey} src={src} title={`Light — ${galleryLabel}`} />
```

```ts
// parent-side readiness validation (all four must hold)
if (event.origin !== window.location.origin) return;
if (!isStudioEmbedReadyMessage(event.data)) return;
if (readyPath !== path || readyTheme !== requestedTheme) return;
if (frameWindow !== event.source) return;
```

## Pitfalls

- **Ready before mount.** An unconditional notifier that sits above the lazy `<Routes>` tree posts ready while the route is still a Suspense fallback — the parent then cancels its only failure deadline. Tie the handshake to the mounted leaf.
- **Reset without a key.** See [iframe-reset-remount-by-key.md](../ui-bugs/iframe-reset-remount-by-key.md).
- **Deriving "embedded" too loosely.** Embedded mode is defined by the presence of a forced theme (not by framing alone), so a framed document without a valid parameter keeps ordinary Studio behavior.
- **Searching the parent document for frame content.** In compare mode the parent must not query for fixture ids/headings that exist only inside the frames; selection announcements and both frame hashes carry the state instead.
