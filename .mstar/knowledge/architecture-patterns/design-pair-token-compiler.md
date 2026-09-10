---
module: tooling/design-tokens + apps/design-studio + packages/nexus-ui
date: 2026-09-10
problem_type: architecture_pattern
category: architecture-patterns
severity: high
applies_when:
  - "Changing the DESIGN.md / DESIGN.dark.md projection pipeline (token compile, generated CSS, brand snapshot)"
  - "Adding, renaming, or retiring a token family that must reach tokens.css, the Tailwind preset, or the Studio dev server"
  - "Reviewing a dev-server integration that compiles repo-root design sources in memory"
  - "Diagnosing an edit that does not show up in the Studio gallery or in the shipped CSS"
tags:
  - design-tokens
  - design-md
  - compiler
  - fail-closed
  - parity
  - generated-artifacts
  - vite-plugin
  - ssot
---

# Design-pair token compiler — one deterministic projection with checked-in outputs

**Track**: Knowledge (distilled from the v1.187 precision-creative-tool overhaul of `@nexus/design-tokens`).

## Context

Before v1.187, `tooling/design-tokens` shipped a hand-maintained `tokens.css` plus a `check-tokens.mjs` gate that searched strings in handwritten CSS and preset files. That gate could only find literals it was told to search: a DESIGN edit that changed a value without a matching handwritten update either shipped stale CSS or was invisible to the check entirely. The design-language overhaul (full palette, type stack, radius, motion and elevation change in one revision) made the gap unacceptable — every changed token had to flow from the DESIGN pair with no second transcription.

The replacement is a single build-time compiler, `tooling/design-tokens/scripts/project-tokens.mjs`, that owns the entire projection from the repo-root DESIGN pair to every derived artifact. The normative contract is [.mstar/specs/design-studio.md](../../specs/design-studio.md) §3.5.

## Guidance

### One compiler, three checked-in outputs

| Layer | Path | Written by |
| --- | --- | --- |
| Sole token authority | `DESIGN.md` (light, `:root`) / `DESIGN.dark.md` (dark, `.dark`) | hand-edited |
| Compiler | `tooling/design-tokens/scripts/project-tokens.mjs` | hand-maintained code |
| CSS variable layers | `tooling/design-tokens/src/tokens.css` | `generate-tokens.mjs` only |
| Package brand CSS | `packages/nexus-ui/theme.css` | `generate-tokens.mjs` only |
| Numeric brand snapshot | `packages/nexus-ui/src/generated-brand.ts` | `generate-tokens.mjs` only |

- `projectDesign(pair)` returns all three strings from one resolved projection table per theme — computed once per call, not once per output.
- `pnpm --filter @nexus/design-tokens generate` is the **only writer** of the three artifacts; `pnpm --filter @nexus/design-tokens check` regenerates in memory and compares byte-for-byte, so a stale artifact fails the gate with the exact "re-run generate" instruction.
- The projection registry carries **DESIGN source paths only** (`cssVar` ↔ dot-path plus an extraction rule). It never embeds a copied palette, so a value can only disagree with the pair by being out of date — never by transcription.
- `brandColors` in `generated-brand.ts` is a light/default numeric snapshot resolved through the same validated projection index; theme-aware UI reads CSS (`--nexus-brand-*` / `--color-*`), not JavaScript constants.
- `tokens.css` forwards `--color-brand-*` onto the package's `--nexus-brand-*` layer (`@import '@42ch/nexus-ui/theme.css'`), keeping the public contract that consumers import both.

### Fail-closed invariants (a violation aborts the compile)

- duplicate YAML keys in either DESIGN frontmatter;
- light/dark leaf-path parity (a path existing on only one side);
- projected CSS-variable surface parity (both themes must emit the identical var set);
- unresolved `{path}` references and reference cycles;
- `null` / `undefined` / empty-string mapped scalars — never "keep the last good value" and never emit a variable with an empty value;
- non-scalar values where a scalar is required (`'[object Object]'` must be impossible);
- missing brand-snapshot keys in `colors`.

Malformed input surfaces as a Vite error overlay in dev and a non-zero exit from `generate`/`check`; there is no silent fallback anywhere in the pipeline.

### Reference resolution rules

- A whole-value reference (`'{elevation.elevation-1}'`) resolves recursively to its scalar.
- References embedded in longer strings resolve in place — e.g. `color-mix(in srgb, {colors.green-700} 16%, transparent)` becomes a concrete `color-mix(…)` string.
- Compound recipes of the form `'{typography.X} @ {colors.Y}'` project **the color member** to their existing `--color-*` property; neither the object nor the literal `@` text may reach CSS.
- `{fontSizeOf}`-style extraction rules read one member of a referenced role (e.g. a wizard label token projects only the referenced role's `fontSize`).

### Dev-time: the same compiler inside Vite

`apps/design-studio/plugins/design-tokens.ts` imports the same compiler and serves the shared CSS modules transformed in memory during `vite dev`:

- `resolveId` maps `@nexus/design-tokens/tokens.css` and `@42ch/nexus-ui/theme.css` to virtual ids; `load`/`transform` return freshly compiled CSS and rewrite the literal `@import` lines in `src/index.css`. No raw YAML reaches the browser and no source file is written during HMR.
- The compiled pair is memoized so one page load compiles at most once (an uncached compile per load/transform was a review finding).
- DESIGN files live at the repo root, **outside** Studio's watch root: register them explicitly with `server.watcher.add([...])` and compare normalized absolute paths on `add`/`change`/`unlink`.
- On invalidation, invalidate **every** CSS module id — both virtual modules and the Studio entry CSS whose cached transform embeds the compiled tokens. Invalidating only the virtual modules leaves the stale entry transform in place.
- On a document (HTML) request, reset the compile cache and invalidate the CSS graph **without** sending `full-reload` from that request path (that path is already a fresh navigation; sending a reload loops). This is the missed-watch safety net for manual reloads.
- Malformed DESIGN input throws inside `load`/`transform` → Vite overlay; the plugin never keeps last-good tokens.

## Why This Matters

- **The pair is the only place values live.** Generated artifacts are reproducible; no reviewer has to diff handwriting against YAML.
- **The gate has teeth.** A byte-compare gate fails on any drift — including a compiler change that silently alters output — whereas the previous literal-search gate could only catch the cases it was written for.
- **Dev and build agree by construction.** Studio dev CSS, production tokens.css and the package brand layer come from the same function, so "it looked right in Studio" cannot diverge from the shipped stylesheet.
- **Failure is loud.** A null scalar, a parity break, or an unresolved reference aborts the compile instead of shipping an empty variable or a stale value.

## When to Apply

- Adding/renaming/retiring any token family → change the DESIGN pair first, extend the mapping, then `generate` and let `check` prove the three artifacts.
- Changing the compiler or its extraction rules → keep the existing CSS variable surface (all existing names are a cutover boundary; historical structural names under `--color-*` stay).
- Wiring a new dev/transform consumer → consume the compiler API; do not add a second YAML parse, a browser-side token load, or an HMR source write.
- Diagnosing "my DESIGN edit did nothing" → check the memo/cache invalidation path first (watch registration, module-graph invalidation, document-request reset), then the projection registry.

## Examples

```bash
# regenerate the three derived artifacts from the DESIGN pair
pnpm --filter @nexus/design-tokens generate

# prove checked-in artifacts match the compiler output (byte-for-byte) and parity holds
pnpm --filter @nexus/design-tokens check
```

```js
// a projection entry is a path mapping, never a value
projections.push(scalar('--text-label-12', 'typography.label-12.fontSize'));
projections.push(scalar('--text-label-12--font-family', 'typography.label-12.fontFamily'));
```

```
# what a failure looks like (compile aborts; nothing is written)
[projectDesign] empty value at colors.brand-cyan — a mapped token must resolve to a non-empty scalar (fail closed, no last-good).
```

## Pitfalls (all observed in the v1.187 compiler work)

- **Per-request recompilation.** Compiling on every load/transform re-reads and re-resolves the whole pair; memoize and invalidate explicitly.
- **Partial invalidation.** Invalidating only the virtual shared-CSS modules leaves the entry CSS transform stale; walk `moduleGraph.idToModuleMap` for every CSS id.
- **Reload loops.** A `full-reload` sent from the document-request middleware loops on that navigation; only the watcher path sends `full-reload`.
- **Silent omission.** Returning `''` or skipping a null leaf "because last-good is nicer" ships an invalid or stale variable; reject instead.
- **Regex state leakage.** A shared `/g` regex used with `.test()` keeps `lastIndex` and poisons later matches; iterate `matchAll` or build a fresh regex per call.
- **Satisfying an incompatible external validator by renaming tokens.** A generic DESIGN validator that flags this repository's intentional scalar extensions (`font-display`, `space-*`, semantic `rounded` keys) is not a reason to flatten or rename the schema; document the repository-aware duplicate-key/parity evidence instead and keep the public token names stable.
