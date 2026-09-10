---
module: apps/web + packages/nexus-ui + apps/design-studio
date: 2026-07-18
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - typography
  - voice-split
  - font-display
  - content-voice
  - interface-voice
  - design-system
  - display-tier
  - title-hierarchy
applies_when:
  - "adding a new surface that presents a creative entity (work, world, chapter, manuscript) or an authoring empty state"
  - "reviewing a PR for a second typeface sneaking into chrome, or a creative-entity title rendered in the wrong tier"
last_updated: 2026-09-10
---

# Voice split — content voice vs interface voice (title hierarchy, not a typeface)

**Refreshed 2026-09-10** — v1.187 v0.5: content voice is the larger sans `display-*` tier; the V1.121 serif typeface is retired.

**Track**: Knowledge (durable guidance first distilled in V1.121 v0.4, carried into the v1.187 v0.5 "Precision Creative Tool" language).

## Context

V1.121 introduced a **content voice** (editorial serif display tier) so creative-entity titles would not read like generic dark-SaaS chrome, while interface surfaces stayed on the sans system. That split named a real product distinction — *the author's material* versus *the engine's chrome* — but attached it to a typeface.

v1.187 kept the distinction and dropped the second typeface: interface and authoring titles are **both** offline system sans, and content voice is now a **larger title hierarchy** (the `display-*` tier) rather than a different font. What survives unchanged is the discipline below: which surfaces get the display tier, which never do, and why the choice must stay greppable instead of becoming a per-page judgement call.

## Guidance (the pattern)

Two registers, same family, different tier:

### Content voice (`display-*` tokens, `font-display`)

- **Family**: the shared offline system sans stack (`--font-display` resolves to the same OS stack as `--font-sans`, with CJK fallbacks) — content voice is *not* a separate font dependency.
- **Typographic scale**: `display-32` / `display-24` / `display-20` (weight 600, line-height 1.25–1.3, tracking `-0.01em…0`).
- **Where it appears**: creative-entity titles (work, world, chapter, manuscript headings), empty-state headlines on authoring surfaces, brand moments, and the novel-profile reading-chrome chapter title.
- **Card.Title opt-in**: `CardTitle` keeps the additive `voice?: 'interface' | 'content'` prop (default `'interface'`). `voice="content"` selects `display-20`; used for cards presenting a creative entity (work card, world card, brand-page card). Greppable — see `voice="content"` in working code.

### Interface voice (`heading-*` / `label-*` / `copy-*` / `button-*`)

- **Family**: the same offline system sans stack (interface chrome never uses `font-display`).
- **Where it stays**: sidebar nav, tabs, tables, buttons, badges, labels, section headers, helper text, settings, command palette, status indicators, page chrome (titles that are not entity names), dialog/sheet headers, tooltips, progress indicators.
- **Card.Title default**: `interface` → the existing `heading-16` treatment. No breaking change for existing callers.

### Enforcement rules

1. **Greppable both directions**: `font-display` or a `display-*` utility in a component → must be a content-voice position. A creative-entity title rendered with `heading-*`/`copy-*` → must be fixed. Keep it visible in code review.
2. **Token-pinned, not font-pinned**: the contract is the *tier* (display vs heading vs copy metrics), so tests assert the token/class the component is meant to emit — not a font family or a computed serif style. Stale typeface pins are themselves defects (the projection gate rejects the retired serif markers).
3. **Studio gallery documents the rule**: the Typography gallery shows the display tier labeled "Content voice" and the sans tiers labeled "Interface voice", so the split stays inspectable.
4. **No exception for "just this one button"**: the display tier on buttons, badges, table cells, tabs, or helper text is a review blocker. Density surfaces keep their interface tier.

## Why This Matters

- **Keeps the product premium without a second face.** The hierarchy — not a serif — is what distinguishes an entity title from chrome; a display-tier title at 24/32px reads as authored content while small text stays scannable.
- **Makes the design system teachable**: contributors know which tier a surface gets without guessing, and the rule is documented, greppable and testable.
- **Prevents drift in both directions**: without the split, one page's "this looks better bigger" erodes the hierarchy, or a special-case typeface returns and re-fragments the offline system stack.
- **Offline by construction**: both voices resolve to OS fonts — no network request, no font-load flash, no bundle gate.

## When to Apply

- **New surface**: ask "is this the author's material or the engine's chrome?" — entity titles, reading surfaces and authoring empty states get the content tier; everything else gets the interface tier.
- **PR review**: grep for `font-display` / `display-*` usages — verify they appear only on content-voice positions; grep entity titles rendered in chrome tiers and fix them.
- **Card.Title usage**: cards presenting a work, world, or brand-level entity may set `voice="content"`. Interface cards (settings, dialog content, table cells) must not.
- **Future typography additions**: any new tier must follow the same split; adding a typeface back requires a design-language iteration and the full offline-font wiring (see [self-hosted-ofl-font-wiring.md](self-hosted-ofl-font-wiring.md)).

## Examples

| Surface | Voice | Token | Rationale |
|---------|-------|-------|-----------|
| Works list page title | Content | `display-24` | Creative entity (the author's works) |
| Worlds page h1 | Content | `display-24` | Creative entity |
| Chapter page title | Content | `display-24` | Creative entity |
| Manuscript novel chapter heading | Content | `display-32` | Reading surface — the author's prose |
| Empty-state headline on authoring surface | Content | `display-24` | Content voice per DESIGN §Design Concept |
| Brand page headline | Content | `display-32` | Brand moment |
| Sidebar nav item | Interface | `label-14` | Chrome — navigation |
| Settings tabs | Interface | `button-12` | Chrome — controls |
| Findings table headers | Interface | `label-12` | Chrome — data |
| Button label | Interface | `button-14` | Chrome — action |
| Card.Title (default) | Interface | `heading-16` | No voice prop → interface tier |
| Card.Title (work card) | Content | `display-20` | `voice="content"` → display tier |
| Page description helper text | Interface | `copy-14` | Chrome — instruction |

## Do NOT

- Use `font-display` or `display-*` on buttons, badges, table cells, tabs, section headers, labels, helper text, or any chrome element.
- Re-introduce a distinct content typeface without a design-language iteration and the offline-font wiring pattern — the voice split is a hierarchy, not a font slot.
- Add a third voice register without a design-language iteration.
- Create a `CardTitle` variant component (e.g. `CardTitleContent`) — the additive prop avoids API proliferation.
- Hardcode `font-family` in components; the `--font-display` / `--font-sans` variables are the SSOT for both voices.
- Freeze exact copy or computed-style strings in tests to "protect" the voice split — assert the tier contract instead (see [behavior-first-assertions-shared-ui.md](../testing-patterns/behavior-first-assertions-shared-ui.md)).
