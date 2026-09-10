---
module: apps/design-studio + packages/nexus-ui
date: 2026-09-10
problem_type: convention
category: conventions
severity: medium
applies_when:
  - "Adding a fixture, gallery section, or shared component that can render intrinsically wide content (long ids, tables, canvas graphs, code chips)"
  - "Making a desktop-first surface usable at 390×844 / 1280×800 without dropping or scaling content"
  - "Reviewing responsive acceptance where a document-wide horizontal scrollbar appears"
tags:
  - responsive
  - overflow
  - min-w-0
  - local-scroll
  - fixtures
  - design-studio
  - viewport-acceptance
---

# Bounded local scrolling as the responsive containment pattern

**Track**: Knowledge (distilled from the v1.187 Studio workspace acceptance; rule stated in [DESIGN.md](../../../DESIGN.md) §Spacing & Layout and [.mstar/specs/design-studio.md](../../specs/design-studio.md) §6).

## Context

Studio fixtures intentionally contain intrinsically wide content: long run-correlation ids in tables, canvas node graphs, code chips, badge rows. At narrow viewports two failure modes appear:

1. **Document-wide horizontal overflow** — one wide child widens the page, so every other section shifts and the mobile layout breaks (observed as `/components` document `scrollWidth` 872 vs `clientWidth` 390).
2. **Content deletion or transform scaling** — the tempting "fix" that silently drops information or shrinks type below readability.

The accepted pattern is neither: keep the document within its viewport and let the wide content live in a **bounded local scroll region** that still exposes the full content by scrolling.

## Guidance

**The invariant.** For every validated route and viewport (1440×900, 1280×800, 390×844) the document must satisfy `document.documentElement.scrollWidth === clientWidth`. Never "fix" a violation by hiding document overflow (`overflow-x: hidden`) — that conceals the broken layout instead of containing it.

**Contain it where it is wide.**

- A grid/flex item that holds wide content needs `min-w-0` (or an equivalent constrained track). The default `min-width: auto` lets a max-content child force the track: the wide `RunsTable` specimen stretched its card to 856px and the document to 872px; adding `min-w-0` returned the card to 356px and the document to 390px **while the table kept its own `overflow-x-auto`** (local `clientWidth` 306 / `scrollWidth` 804).
- Wide fixture bodies use the shared utility:

  ```css
  .studio-fixture-boundary { max-width: 100%; min-width: 0; overflow-x: auto; }
  ```

- Keep-web tables keep their existing overflow wrapper (`clientWidth` 356 / `scrollWidth` 922 locally at 390px) — containment is local, not global.
- **Multi-column shells degrade by stacking, not squeezing**: the Surfaces rail is `w-full` below `md` and `md:w-44` above it; the pair view stacks below 1024px.
- **Chrome that can wrap must be allowed to grow**: the sticky header uses a minimum height plus wrapping rows instead of a fixed height that clips wrapped content (and publishes its measured height for scroll offsets — see [studio-catalog-mount-then-focus.md](../architecture-patterns/studio-catalog-mount-then-focus.md)).
- **Inline pills/badges wrap inside themselves**: `max-w-full flex-wrap whitespace-normal` on the pill plus `break-all` on the mono path keeps the full label visible without forcing a wider column.

**Prove it with measurements, not with one screenshot.** The v1.187 acceptance captured 13 routes × 3 viewports and asserted `scrollWidth === clientWidth` on every capture, then additionally verified that wide content is still reachable by scrolling inside its region (and counted the local scroll regions per route: shell 17, canvas 4, components 3 at 390px).

## Why This Matters

- **Mobile usability is a document property.** A single wide fixture changes the layout of every section below it; local containment keeps the failure from spreading.
- **No information is lost.** Scroll regions keep the full id/graph visible; hiding or scaling trades correctness for a passing screenshot.
- **It keeps desktop density.** The desktop layout can stay information-dense because narrow behavior is handled per region rather than by globally reducing density.
- **It is cheap to verify.** `scrollWidth === clientWidth` is a one-line measurement per route/viewport, and it fails loudly.

## When to Apply

- Any new fixture/component that can render unbounded-width content (ids, paths, tables, graphs, code).
- Any acceptance run that includes 390×844 or 1280×800 viewports.
- Any PR that introduces `overflow-x-hidden` on a document/root element, or removes a local overflow wrapper.

## Examples

```tsx
// grid item containing a wide table: constrain the track, keep local scroll
<div className="min-w-0 rounded-card border border-gray-alpha-200 p-4">
  <div className="overflow-x-auto">
    <RunsTable /* long correlation ids */ />
  </div>
</div>
```

```js
// measurement used by the acceptance sweep (per route, per viewport)
const doc = document.documentElement;
return { scrollWidth: doc.scrollWidth, clientWidth: doc.clientWidth };
```

## Pitfalls

- **`min-width: auto` on the container** — the most common cause; the symptom appears on a *different* element than the wide child.
- **Fixing the child instead of the container** — clipping the table or truncating ids removes information; constrain the wrapper.
- **Assuming one screenshot proves the layout** — a wide viewport can pass while 390px overflows; measure each viewport.
- **Treating a local scroll region as a failure** — intrinsically wide specimens (canvas graphs) are expected to scroll locally; the failure is the *document* scrolling.
