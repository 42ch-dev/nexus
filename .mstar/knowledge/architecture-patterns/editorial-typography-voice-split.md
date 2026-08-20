---
module: apps/web + packages/nexus-ui
date: 2026-07-18
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
tags: [typography, voice-split, font-display, content-voice, interface-voice, design-system, literary-engine]
applies_when: adding a new surface that presents a creative entity (work, world, chapter, manuscript) or an authoring empty state; also when reviewing a PR for serif usage on chrome or sans on entity titles
last_updated: 2026-07-18 (V1.121 v0.4 Literary Engine)
---

# Editorial Typography Voice Split — Content Voice vs Interface Voice

**Track**: Knowledge (durable guidance distilled from V1.121 design language v0.4 "The Literary Engine").

## Context

Before V1.121, Nexus was 100% system sans (Inter) — functionally complete but expressively flat for a creative-writing product. The only serif was a hardcoded `Georgia` in the reading-chrome CSS block, fragile and theme-agnostic. V1.121 introduced a display serif tier (Source Serif 4, self-hosted) as part of the "Literary Engine" design language v0.4, reconciling two registers authors already live in: **content** (the author's material — literary, editorial) and **interface** (the engine's chrome — precise, computational).

The risk was clear: serif everywhere looks like a themed blog or a vanity publishing tool; serif nowhere wastes the literary identity that distinguishes Nexus from generic dark SaaS. The voice-split discipline was the product decision that made the identity work without sacrificing professionalism.

## Guidance (the pattern)

Two registers, strictly separated:

### Content voice (serif — `font-display` + `display-*` tokens)

- **Font stack**: `"Source Serif 4", Georgia, "Times New Roman", ui-serif, serif` (via `--font-display` CSS var).
- **Typographic scale**: `display-32` / `display-24` / `display-20` (semibold, serif metrics: line-height 1.2–1.3, tracking `-0.01em…0`).
- **Where it appears**: creative-entity titles (work, world, chapter, manuscript headings), empty-state headlines on authoring surfaces, brand page. Also the novel-profile reading-chrome chapter title (replaces hardcoded Georgia).
- **Card.Title opt-in**: `CardTitle` gains an additive `voice?: 'interface' | 'content'` prop (default `'interface'`). When `voice="content"`, switches to `font-display text-display-20`. Used for cards presenting a creative entity (work card, world card, brand-page card). Greppable — see `voice="content"` in working code.

### Interface voice (sans — `Inter` + existing `heading-*` / `label-*` / `copy-*` tokens)

- **Font stack**: `Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` (via `--font-sans` CSS var).
- **Where it stays**: all chrome — sidebar nav, tabs, tables, buttons, badges, labels, section headers, helper text, settings, command palette, status indicators, page chrome (page titles that are not entity names), dialog/sheet headers, tooltips, progress indicators.
- **Card.Title default**: `interface` — preserves the existing `text-heading-16 font-heading` treatment exactly. No breaking change.

### Enforcement rules

1. **Greppable both directions**: `font-display` or `Source Serif` in a component → must be a content voice position. `font-sans` or `Inter` on an entity title → must be fixed. Both directions are tested in the bidrectional voice-split tests added in V1.121 P2.
2. **Test-pinned**: a vitest suite asserts that content voice elements render with serif computed style and interface elements render with sans computed style — in both themes.
3. **Studio gallery documents the rule**: the design-studio Typography gallery shows the display tier with a "Content voice" label and the sans tiers with an "Interface voice" label, making the split explicit and inspectable.
4. **No exception for "just this one button"**: serif on buttons, badges, table cells, or tabs is a lint failure. The product must feel like a serious writing atelier, not a themed blog.

## Why This Matters

- **Keeps the product premium**: serif on buttons and tables would hurt legibility at small sizes and density. The split preserves the literary identity where it matters (the author's material) while keeping the engine chrome precise and scannable.
- **Makes the design system teachable**: new contributors know exactly which font to use on which surface without guessing. The rule is documented in DESIGN.md body, greppable, and test-pinned.
- **Prevents drift**: without the split, one developer's "this page looks better in serif" would erode the identity. The content-voice opt-in is additive and backward-compatible, so no existing surface breaks.

## When to Apply

- **New surface**: ask "is this the author's material or the engine's chrome?" — entity titles, reading surfaces, and authoring empty states get content voice; everything else gets interface voice.
- **PR review**: grep for `font-display` or `Source Serif 4` — verify it appears only on content voice positions. Grep for entity titles rendered in sans — verify they are fixed.
- **Card.Title usage**: cards presenting a work, world, or brand-level entity may set `voice="content"`. Interface cards (settings, dialog content, table cells) must not.
- **Future typography additions**: any new display tier weight or size must follow the same voice-split discipline. CJK serif companion (e.g. Noto Serif SC for zh-CN content titles, V1.122 candidate) would extend the content voice only.

## Examples

| Surface | Voice | Token | Rationale |
|---------|-------|-------|-----------|
| Works list page title | Content | `display-24` | Creative entity (the author's works) |
| Worlds page h1 | Content | `display-24` | Creative entity |
| Chapter page title | Content | `display-24` | Creative entity |
| Manuscript novel chapter heading | Content | `display-32` | Reading surface — the author's prose |
| Empty-state headline on authoring surface | Content | `display-24` | Content voice per DESIGN.md §Design Concept |
| Brand page headline | Content | `display-32` | Brand moment |
| Sidebar nav item | Interface | `label-14` | Chrome — navigation |
| Settings tabs | Interface | `button-12` | Chrome — controls |
| Findings table headers | Interface | `label-12` | Chrome — data |
| Button label | Interface | `button-14` | Chrome — action |
| Card.Title (default) | Interface | `heading-16` | No voice prop → sans |
| Card.Title (work card) | Content | `display-20` | `voice="content"` → serif |
| Page description helper text | Interface | `copy-14` | Chrome — instruction |

## Do NOT

- Use `font-display` on buttons, badges, table cells, tabs, section headers, labels, helper text, or any chrome element.
- Override the voice rule with a "small serif" workaround — small serif on interface elements hurts legibility and blurs the identity.
- Add a third voice register without a design-language iteration (V1.122+).
- Create a `CardTitle` variant component (e.g. `CardTitleContent`) — the additive prop avoids API proliferation (see P0 spec T8 rationale).
- Use raw CSS `font-family` overrides that bypass the `--font-display` / `--font-sans` token system — the CSS var is the SSOT for both voices.