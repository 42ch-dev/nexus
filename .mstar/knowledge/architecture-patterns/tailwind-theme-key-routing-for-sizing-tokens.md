---
module: apps/web
date: 2026-07-07
problem_type: build-config
category: architecture-patterns
severity: low
plan_id: 2026-07-07-v1.95-implement-fixes
tags: [tailwind, design-tokens, css, setup-wizard, theme-config]
applies_when: adding sizing/spacing/maxWidth tokens that the JSX consumes as max-w-*/p-*/h-*/w-* utilities
---

# Tailwind theme-key routing for sizing tokens

## Context

V1.94's setup wizard rendered with broken layout (no max-width cap, no padding,
undersized step circles). The `setup-wizard-step-*` CSS variables existed in
`index.css` and were referenced by name in `tailwind.config.ts`, and the JSX
used the expected utility classes — yet Tailwind generated **no** CSS for the
sizing utilities. The color utilities (`bg-*`, `text-*`) worked fine, masking
the partial failure.

## Guidance

Tailwind generates each utility family from a **specific** `theme` key:

| JSX utility prefix | Generated from `theme.<key>` |
|---|---|
| `max-w-*` | `theme.maxWidth` |
| `p-*` / `px-*` / `py-*` | `theme.padding` |
| `h-*` / `w-*` | `theme.height` / `theme.width` (or `theme.spacing` for both) |
| `text-*` (size) | `theme.fontSize` |
| `bg-*` / `text-*` (color) | `theme.colors` (or `theme.extend.colors`) |

A token placed under **`theme.extend.colors`** produces **only** color utilities.
It does **not** also produce `max-w-*`/`p-*`/`h-*`/`w-*` utilities, even if the
JSX asks for them. Tailwind silently emits nothing for those classes — there is
no build error, no warning. The class simply resolves to no CSS, and the
element renders unstyled for that property.

In this repo, the `cv()` helper (`tailwind.config.ts`) is
`const cv = (token) => \`var(--color-${token})\``. The CSS variables are named
`--color-setup-wizard-step-*` in `index.css`. This is fine for **color**
utilities. But routing a *sizing* value through `colors` means the value (a
`var(--color-...)` reference) never reaches `maxWidth`/`padding`/`spacing`, so
the sizing utility is never generated.

**Fix pattern**: register each token under the theme key that generates the
utility family the JSX actually uses. The value can still be a
`var(--color-...)` reference — Tailwind does not care that a `maxWidth` value
points at a `--color-*` variable; it only cares that the value lives under
`theme.maxWidth`.

```ts
// tailwind.config.ts — CORRECT routing
extend: {
  colors: {
    'setup-wizard-step': {
      'circle-active-bg': cv('setup-wizard-step-circle-active-bg'),  // → bg-*
      // ... color tokens stay here
    },
  },
  padding:    { 'setup-wizard-step-wizard-padding':   cv('setup-wizard-wizard-padding') },    // → p-*
  maxWidth:   { 'setup-wizard-step-wizard-max-width': cv('setup-wizard-wizard-max-width') }, // → max-w-*
  spacing:    { 'setup-wizard-step-circle-size':      cv('setup-wizard-step-circle-size') },  // → h-*/w-*
  fontSize:   { 'setup-wizard-step-label-typography': cv('setup-wizard-step-label-typography') }, // → text-*
}
```

## Why This Matters

- **Silent failure**: Tailwind emits no diagnostic when a utility class is
  requested but not generated. The layout breaks with no build error. The only
  signal is visual (missing padding/max-width) or a generated-CSS inspection.
- **Easy to misdiagnose**: the tokens "exist" in both `index.css` and the
  config, so a casual read suggests the pipeline is wired. The bug is the
  *category* of theme key, not the existence of the token.
- **Dark-mode parity**: the `--color-*` variables are duplicated under `:root`
  and `.dark`; a fix that touches the variable names must update both, but the
  theme-key routing fix does not require renaming any CSS variable.

## When to Apply

- Adding any token consumed as `max-w-*`, `p-*`, `h-*`/`w-*`, or `text-*`
  (size) in JSX — register it under `maxWidth` / `padding` / `spacing` /
  `fontSize` respectively, NOT under `colors`.
- Auditing a "layout is broken / no padding / element has no max-width"
  symptom where the token appears to exist — check the theme-key category
  first, not the CSS variable.
- When a single conceptual "design token" (e.g. a wizard step circle size) is
  consumed as both a color and a size — split it across the two theme keys;
  do not assume `colors` covers both.

## Examples

V1.95 fix (`apps/web/tailwind.config.ts`): four sizing tokens
(`wizard-padding`, `wizard-max-width`, `circle-size`, `label-typography`) moved
out of `theme.extend.colors['setup-wizard-step']` into `padding`,
`maxWidth`, `spacing`, and `fontSize` respectively. Color tokens stayed under
`colors`. No CSS variable was renamed. A vitest asserts the utility classes
resolve in the rendered DOM.

Anti-pattern to avoid: dumping every `setup-wizard-step-*` token under
`colors` because they share a naming prefix. The prefix is cosmetic; the
theme-key category determines which utilities generate.
