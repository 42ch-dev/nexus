---
module: apps/design-studio
date: 2026-09-10
problem_type: ui_bug
category: ui-bugs
severity: medium
symptoms:
  - "At 390px the sticky Studio header's wrapped navigation extends below the header's fixed 48px box and overlays the content that follows it"
  - "elementFromPoint at the Compare button's center returns a navigation anchor instead of the button; a real pointer click does not activate it, while DOM .click() does"
  - "Hash/deep-link focus targets land under the wrapped header because scroll offsets assume a 48px header"
root_cause: "A sticky header was given a fixed height (h-12) while its navigation wraps at narrow widths, so the rendered header was taller than its own box and overlaid the first content rows; scroll-margin offsets were hardcoded to the same assumed 48px."
resolution_type: code_fix
tags:
  - design-studio
  - sticky-header
  - responsive
  - hit-testing
  - focus-offset
  - narrow-viewport
---

# Studio sticky header: wrapped nav must be allowed to grow

## Problem

The v1.187 Studio shell moved to a sticky header with a wrapping top nav and added new discovery controls (Compare, section filter) as the first content row. At 390×844 the header visually grew but its layout box did not, so the wrapped navigation covered the Compare control: pointer input landed on a nav anchor, and focus targets scrolled underneath the header.

## Symptoms

- `document.elementFromPoint()` at the Compare button's center resolved to a `Surfaces` nav anchor, and a real pointer click did not activate Compare (a programmatic DOM `.click()` did — the classic false-green).
- The header's visual bottom (145px at 390×844) exceeded its box height (48px), overlapping the first content rows because the header is sticky with a raised z-index.
- Deep-link focus landed below the fold, hidden behind the header, because the focus compensation used a hardcoded `scroll-mt-16`.

## What Didn't Work

- **Treating it as a click-handler bug.** DOM `.click()` worked, so the handler was fine; the defect was hit-testable overlay geometry, only visible to a real pointer.
- **Assuming a single-row header.** The design assumed the nav stays on one line; at 390px it wraps to three, so any fixed height is wrong by construction.
- **Fixing only the header.** The same 48px assumption lived in the per-page `scroll-mt-16` classes; leaving them behind keeps focus targets under a taller header.

## Solution

Let the header grow, measure what it actually is, and let consumers read the measurement:

```tsx
// header: a minimum height plus wrapping rows — never a fixed height
<header ref={headerRef} className="sticky top-0 z-20 min-h-12 … flex-wrap …">

// publish the measured height; keep the initial sync even without observers
const syncStickyHeaderOffset = () => {
  document.documentElement.style.setProperty(
    '--studio-sticky-header-offset',
    `${header.offsetHeight}px`,
  );
};
syncStickyHeaderOffset();
if (typeof ResizeObserver === 'undefined') return;   // jsdom
const observer = new ResizeObserver(syncStickyHeaderOffset);
observer.observe(header);
return () => observer.disconnect();
```

```css
.scroll-mt-sticky-header { scroll-margin-top: var(--studio-sticky-header-offset, 3rem); }
```

All hardcoded `scroll-mt-16` usages across the gallery pages/utilities were replaced by `.scroll-mt-sticky-header`.

## Why This Works

The overlap had two independent halves, both caused by an assumed 48px height: the header's own box (fixed) and the focus offset (hardcoded). Growing the box removes the overlay; measuring the real height and consuming it from one CSS variable keeps focus targets correct at every wrap state. The ResizeObserver keeps the variable honest when the nav re-wraps after a resize.

## Prevention

- Never give sticky chrome a fixed height when its content can wrap — use a minimum height and let it grow.
- Hit-test overlay geometry with a real pointer (`elementFromPoint`) at narrow viewports; DOM `.click()` cannot detect an overlay.
- Drive scroll compensation from a measured CSS variable, not from class literals duplicated across pages.
- Add the narrow viewport (390×844) hit-test and focus-below-header checks to the acceptance sweep for any change to sticky chrome.
- Related: [studio-catalog-mount-then-focus.md](../architecture-patterns/studio-catalog-mount-then-focus.md) (consumer of the measured offset), [jsdom-resizeobserver-unconditional-construction.md](../test-failures/jsdom-resizeobserver-unconditional-construction.md) (the guard for the observer above).
