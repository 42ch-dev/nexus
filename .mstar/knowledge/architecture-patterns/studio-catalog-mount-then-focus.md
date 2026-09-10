---
module: apps/design-studio
date: 2026-09-10
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
applies_when:
  - "Adding a filterable index, anchor navigation, or deep-link focus behavior to a gallery/catalog surface"
  - "Navigating across lazily-loaded routes where the target heading may not exist yet"
  - "Wiring sticky chrome that focus/scroll targets must clear"
tags:
  - design-studio
  - catalog
  - section-index
  - focus-management
  - lazy-routes
  - hash-navigation
  - accessibility
---

# Studio catalog + mount-then-focus navigation contract

**Track**: Knowledge (distilled from the v1.187 Studio workspace; normative detail in [.mstar/specs/design-studio.md](../../specs/design-studio.md) §5.4).

## Context

The contributor job "find the thing" failed on scroll-only galleries: long pages, no filter, and hash links whose target headings were often buried inside sections without ids. The v1.187 workspace added one Studio-local catalog, a filterable section index, and stable hash navigation — across a route tree that is **code-split per route and per nested Surfaces leaf**.

That last fact is what makes the contract non-obvious: a catalog entry can point at a heading on a route that is still being lazily fetched. Navigation is therefore a three-step protocol — *update route/hash, wait for mount, focus the heading below sticky chrome* — and a naive "focus after navigate" call silently no-ops exactly when the chunk has never been loaded.

## Guidance

**One metadata-only catalog per gallery.** `GalleryEntry = { path, id, label, keywords, importPaths }` holds route/anchor/label/keyword/source metadata — never token values, fixture JSX, or behavior. The catalog is keyed by pathname and drives the index, the deep-link target and the pair-frame labels.

- **Freeze every existing explicit id**; new headings get deterministic ids (the `*-heading` convention for sections whose `<section>` already carries the anchor id).
- **Filter semantics**: trimmed, case-insensitive substring over label, id and keywords; an empty query lists the whole group; filtering never hides gallery content.
- **The index is a labeled search input plus ordinary links** — not a command palette and not a listbox. ArrowDown from the input focuses the first result, ArrowUp the last, arrows within results move without wrapping, Enter activates (first result when the input has focus), Escape clears and returns focus to the input, Tab stays native.
- **Recovery is announced, not silent**: a polite status reports the match count or a no-results message, and a visible Clear control exists. Keep the results container mounted (hidden when empty) so `aria-controls` never dangles.

**Navigation protocol (mount-then-focus).**

1. Activating an entry updates the route **and** the stable hash, and announces the selection.
2. Focus is scheduled only after the target route has actually mounted. `focusGalleryHeading(id)` retries on animation frames up to a bounded deadline (2 s), resolving the real heading each attempt, then sets `tabIndex = -1` and focuses it. It returns a cancel function.
3. The caller cancels the pending attempt before a newer navigation, on effect cleanup, and while compare mode is active — one owner, no stale focus after a superseding click.
4. Focus targets resolve through `resolveGalleryFocusTarget`: a heading element is used directly; a `section`/`article` resolves to its named heading, else its first heading; a `<id>-heading` fallback covers legacy anchors; the element itself is the last resort. This keeps deep links landing on the **heading** even where the catalog id sits on a wrapper.

**Sticky chrome is measured, not hardcoded.** The sticky header publishes its own `offsetHeight` as `--studio-sticky-header-offset` on `:root` (initial sync on mount + resize observation), and every focus target uses a `.scroll-mt-sticky-header` utility consuming that variable. A hardcoded `scroll-mt-16` breaks as soon as the header wraps at narrow widths.

**Compare mode changes the target, not the protocol.** When pair view is active, the parent must not search its own document for fixture ids that exist only inside frames: catalog activation updates both frame URLs and the announcement, and the parent skips heading focus.

## Why This Matters

- **Lazy routes make naive focus wrong.** One animation frame is not a mount wait; without a bounded retry, sibling-route jumps (e.g. Shell → Canvas) race the chunk and leave keyboard users at the top of the page.
- **Stable ids are a compatibility surface.** Deep links, QA scripts and the pair-frame URLs all depend on ids surviving a redesign; ids are frozen, new ones are additive and deterministic.
- **Cancellation is what makes retries safe.** A bounded retry that is not cancelled re-focuses a target the user already navigated away from, which reads as a focus jump.
- **An index without recovery is still scroll-only.** No-results text, a match count and Clear are part of the acceptance, not polish.

## When to Apply

- Adding a new gallery route or fixture family → add its catalog entries and deterministic heading ids in the same change.
- Adding an anchor target → put the id on the heading itself when possible; otherwise rely on the wrapper-resolution rule and keep the `<id>-heading` convention.
- Changing sticky chrome height or wrapping behavior → keep the measured offset variable as the single source for both scroll margins and any other clearance.
- Reviewing a discovery feature → verify: catalog metadata-only, frozen ids, filter semantics, full keyboard path, no-results recovery, cancel-on-supersede, and the compare-mode exception.

## Examples

```ts
// bounded, cancellable mount wait — the heading may not exist on the first frame
const FOCUS_RETRY_DEADLINE_MS = 2000;
export function focusGalleryHeading(id: string): () => void {
  /* requestAnimationFrame loop until document.getElementById(id) exists or the deadline passes */
}
```

```tsx
// one owner: cancel the previous attempt before scheduling a new one
const scheduleHeadingFocus = useCallback((id: string) => {
  cancelFocusRef.current?.();
  cancelFocusRef.current = focusGalleryHeading(id);
}, []);
```

```css
.scroll-mt-sticky-header {
  scroll-margin-top: var(--studio-sticky-header-offset, 3rem);
}
```

## Pitfalls

- **A single `requestAnimationFrame` as "wait for mount"** — it fires before a suspended lazy leaf commits; use the bounded retry.
- **Focusing the wrapper section** — deep links then scroll correctly but do not satisfy "focus the heading"; resolve to the heading.
- **Unmounting the results list on no-results** — leaves `aria-controls` pointing at a missing element; keep the container and hide it.
- **Hardcoded scroll margins** — the measured header offset is the only value that stays correct when the nav wraps.
- **Focusing in compare mode** — the fixture ids are not in the parent document; announce and update the frame URLs instead.
