---
module: tooling/design-tokens + packages/nexus-ui
date: 2026-09-04
problem_type: architecture-pattern
category: architecture-patterns
severity: low
tags: [design-tokens, token-projection, component-library, color-mix, tailwind-preset, compound-variants, check-tokens, design-studio, semantic-tokens]
applies_when: migrating a component library's raw color-mix/opacity utilities into projected semantic tokens, or auditing whether a token family landed through the full pipeline
---

# Component-variant token projection with a four-piece pipeline

## Context

`@42ch/nexus-ui` Badge shipped soft variants as raw `color-mix(in srgb, var(--color-{hue}-700) …)` arbitrary classes, with the values entrenched by component tests pinning the emitted class strings. The entrenchment made a later token migration a two-surface change (component + test pins), and the raw arbitraries were invisible to the design-tokens build gate — the tokens existed nowhere, so nothing could assert them. v1.183 P0 (R-V1121P1QC1-S001) migrated the five soft variants to projected `nexus-ui-badge-soft-{variant}-{bg,text,border}` tokens (15 vars, single `:root` def with themed `color-mix` values) and closed the companion disabled-chrome gap (R-V1182P0-002) by promoting an already-landed alpha scalar (`--color-states-disabled-opacity`, V1.113) into the shared preset.

## Guidance

**A component-library variant color is a token, not an inline recipe.** Project it and land it through the four-piece pipeline; anything less is a partial landing that the gate cannot see:

1. **tokens.css** — define the family in both themes (light + dark), single `:root` def per value when the dark pair shares a formula via `color-mix` on themed hues. Namespace: `--color-` only for color-valued vars; alpha scalars are allowed in the color namespace when they express a color-mix operand (precedent: `--color-states-disabled-opacity`).
2. **Tailwind preset mapping** — expose via `cv()` (color) in the preset consumed by every app; delete any app-local duplicates (promotion rule: when ≥2 surfaces consume a token, it moves to the shared preset).
3. **check-tokens needles** — every token in the family gets a required needle (bg/text/border per variant, not samples). A family that can be half-deleted without failing the gate is not landed. Guard rails (forbidden `--color-` on structural, etc.) keep the namespace boundary.
4. **Consumer + test pins** — migrate the component classes and move the test pins to the semantic names in the same change; a pin left on the old arbitrary class is a stale contract that will "pass" while the token dies.

**Design Studio surfaces follow the value type**: color-valued tokens → COLOR_GROUPS swatches; alpha scalars / non-color washes → standalone demo chips bound to the live CSS var (style binding with theme-flip re-resolution), never a ColorSwatch (which assumes a color value).

## Pitfalls

- **Cross-package pin drift**: when a shared component's class changes (e.g. solid fill `-700` → `-800` for AA), every *other* package's fixture/test that pins the old class fails at its own suite run — not in the changing package's CI leg. v1.183 P1 had to follow through `apps/design-studio/src/App.test.tsx:1605` after the badge AA fix. Audit `grep -rn "<old-class>"` across all packages before merging a shared-component visual change.
- **Promotion without consumers check**: verify existing `opacity-disabled` (or equivalent) consumers still resolve after moving the mapping from app-local config to the shared preset — same Tailwind key must emit the same class.
- **Twin recipes**: when a fix targets one member of a recipe family (badge solids), grep for recipe-identical siblings (teal twin) and fix them together — a half-family fix is the same SSOT drift the residual warned about.

## References

- `.mstar/knowledge/architecture-patterns/nexus-brand-token-hierarchy.md` — token SSOT chain + namespace guards
- `.mstar/knowledge/architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md` — theme-key → utility-family routing (the `cv()` vs sizing-key table)
- v1.183 P0 disposition (`.mstar/sdd/2026-09-04-v1.183-p0-desktop-nit-tokens/disposition.md`) — the migration evidence