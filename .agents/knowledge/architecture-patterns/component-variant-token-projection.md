---
module: tooling/design-tokens + packages/nexus-ui + apps/design-studio
date: 2026-09-04
problem_type: architecture_pattern
category: architecture-patterns
severity: low
tags:
  - design-tokens
  - token-projection
  - component-library
  - color-mix
  - tailwind-preset
  - compound-variants
  - check-tokens
  - design-studio
applies_when:
  - "migrating a component library's raw color-mix/opacity utilities into projected semantic tokens"
  - "auditing whether a token family landed through the full pipeline"
last_updated: 2026-09-10
---

# Component-variant token projection with a four-piece pipeline

**Refreshed 2026-09-10** — v1.187: `tokens.css` is compiler-generated; the third piece is now regeneration + byte-compare instead of a handwritten needle list.

## Context

`@42ch/nexus-ui` Badge shipped soft variants as raw `color-mix(in srgb, var(--color-{hue}-700) …)` arbitrary classes, with the values entrenched by component tests pinning the emitted class strings. The entrenchment made a later token migration a two-surface change (component + test pins), and the raw arbitraries were invisible to the design-tokens gate — the tokens existed nowhere, so nothing could assert them.

v1.183 P0 (R-V1121P1QC1-S001) migrated the five soft variants to projected `nexus-ui-badge-soft-{variant}-{bg,text,border}` tokens and closed the companion disabled-chrome gap (R-V1182P0-002) by promoting a landed alpha scalar (`--color-states-disabled-opacity`) into the shared surface. **v1.187 moved the whole pipeline under one compiler**: `tokens.css` is now a generated artifact, the projection is declared as DESIGN source paths, and the gate is regeneration + byte comparison. The four-piece shape survives; the way each piece is produced and checked changed.

## Guidance

**A component-library variant colour is a token, not an inline recipe.** Project it and land it through the four pieces; anything less is a partial landing the gate cannot see:

1. **DESIGN pair → projection entry.** The value lives in the DESIGN pair (`components.badge-status-pill.soft.<variant>.{backgroundColor,textColor,borderColor}`), and the compiler's mapping declares `--color-nexus-ui-badge-soft-<variant>-<bg|text|border>` → that dot-path. `tokens.css` is then **generated** (`pnpm --filter @nexus/design-tokens generate`); never hand-edit it. Namespace: `--color-` only for colour-valued vars; alpha scalars are allowed in the colour namespace when they express a colour-mix operand (precedent: `--color-states-disabled-opacity`, projected from `components.states.disabled.opacity`).
2. **Tailwind preset mapping** — expose via `cv()` (colour) in the shared preset consumed by every app; delete app-local duplicates (promotion rule: when ≥2 surfaces consume a token, it moves to the shared preset).
3. **Projection gate** — every member of the family (bg/text/border per variant, not samples) must be declared in the mapping and survive `pnpm --filter @nexus/design-tokens check`, which regenerates in memory and compares the three derived artifacts **byte-for-byte**. A missing or wrong entry now fails the gate by producing different bytes — there is no needle list to keep in sync and no handwritten CSS to drift.
4. **Consumer + test pins** — migrate the component classes and move the test pins to the semantic names in the same change; a pin left on the old arbitrary class is a stale contract that will "pass" while the token dies.

**Design Studio surfaces follow the value type**: colour-valued tokens → colour swatches; alpha scalars / non-colour washes → standalone demo chips bound to the live CSS variable (style binding with theme-flip re-resolution), never a colour swatch (which assumes a colour value).

## Pitfalls

- **Cross-package pin drift**: when a shared component's class changes (e.g. solid fill `-700` → `-800` for AA), every *other* package's fixture/test that pins the old class fails at its own suite run — not in the changing package's CI leg. v1.183 P1 had to follow through `apps/design-studio/src/App.test.tsx` after the badge AA fix. Audit `grep -rn "<old-class>"` across all packages before merging a shared-component visual change.
- **Promotion without a consumers check**: verify existing consumers still resolve after moving a mapping from app-local config to the shared preset — the same Tailwind key must emit the same class.
- **Twin recipes**: when a fix targets one member of a recipe family (badge solids), grep for recipe-identical siblings (teal twin) and fix them together — a half-family fix is the same SSOT drift this pattern exists to prevent.
- **Hand-editing generated CSS**: the v1.187 compiler overwrites `tokens.css`, `theme.css` and the brand snapshot; a manual "quick fix" there is reverted on the next generate and fails the byte-compare gate in the meantime.
- **Pinning the arbitrary instead of the token**: assert the semantic token class (or the observable result), not the raw `color-mix(...)` string — see [behavior-first-assertions-shared-ui.md](../testing-patterns/behavior-first-assertions-shared-ui.md).

## References

- [design-pair-token-compiler.md](design-pair-token-compiler.md) — the compiler contract, fail-closed rules and generate/check commands
- [nexus-brand-token-hierarchy.md](nexus-brand-token-hierarchy.md) — token SSOT chain + namespace guards
- [tailwind-theme-key-routing-for-sizing-tokens.md](tailwind-theme-key-routing-for-sizing-tokens.md) — theme-key → utility-family routing (the `cv()` vs sizing-key table)
- [embedded-pinned-wasm-sha256-alignment.md](../best-practices/embedded-pinned-wasm-sha256-alignment.md) — sibling pattern: pinned artifacts vs generated sources
