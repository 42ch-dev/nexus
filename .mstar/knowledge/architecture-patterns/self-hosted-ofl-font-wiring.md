---
module: packages/nexus-ui + apps/web + apps/design-studio + tooling/design-tokens
date: 2026-07-18
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - fonts
  - self-hosted
  - ofl
  - font-face
  - preload
  - bundle-gate
  - bundler-agnostic
  - offline-typography
applies_when:
  - "reintroducing a self-hosted OFL font into the Nexus design system (display tier, mono companion, CJK companion)"
  - "reviewing why the generated token pipeline currently ships no font files"
last_updated: 2026-09-10
---

# Self-Hosted OFL Font Wiring Pattern (currently inactive)

**Refreshed 2026-09-10** — v1.187 v0.5: no self-hosted font is active; the V1.121 Source Serif 4 instance is retired from the pipeline, and the generated `tokens.css` must never be hand-edited to re-add one.

**Track**: Knowledge (durable procedure distilled from the V1.121 v0.4 display-tier implementation; amended for the v1.187 generated-token pipeline).

## Status in the current language (v0.5)

**No self-hosted font is active.** The v1.187 "Precision Creative Tool" language resolves interface *and* content display to the offline OS sans stack (`system-ui` + Apple/Segoe UI + CJK system fallbacks). Consequences to keep straight:

- The generated `tooling/design-tokens/src/tokens.css` carries **no `@font-face`** declarations and no font URLs; the projection gate rejects stale `Source Serif 4` / `source-serif-4-latin-*` pins if they reappear in generated output.
- Unused packaged font files and app `public/fonts/**` copies may still exist on disk from V1.121 — they are inert remains, not an active typography authority. Do not treat their presence as evidence that a font is wired.
- DESIGN states the policy: do not introduce a font dependency solely for visual novelty. Reintroducing one is a design-language decision, not a styling tweak.

**The pipeline change that matters most if you do reintroduce one:** `tokens.css` is now a **generated artifact**. The V1.121 procedure below placed `@font-face` blocks in that file by hand; doing that today would be overwritten by the next `pnpm --filter @nexus/design-tokens generate` (and would fail the byte-compare gate). A future font must be wired either by teching the compiler to emit it from a DESIGN source field, or by adding a separate handwritten shared CSS module imported alongside the generated one by each app. Never hand-edit generated output.

## Context (the original problem, still the pattern's rationale)

V1.121 needed a literary serif (Source Serif 4) as the content-voice typeface. Nexus is a local-first product — no webfont CDN dependency. The font also had to respect the `@42ch/nexus-ui` bundler-agnostic asset boundary (see [bundler-agnostic-component-library-assets.md](bundler-agnostic-component-library-assets.md)): a tsup-built React component library cannot import `.woff2` binaries in source.

The pattern spans five layers — canonical provenance → package boundary → app vendoring → shared CSS declarations → preload — plus a measurable bundle gate so the pattern cannot silently bloat the shipped bundle.

## Guidance (the pattern, for a future reintroduction)

### Step 1 — Justify and acquire

- Justify the font against the offline-system-default policy (DESIGN §Typography); prefer an OS stack unless the typeface carries a product decision.
- Acquire the OFL font via `@fontsource/<family>` (or the foundry's release), and verify the OFL-1.1 license permits self-hosting, modification and sub-licensing.

### Step 2 — Canonical provenance + license

- Place the full binary (or canonical subset source) in `packages/nexus-ui/assets/fonts/<family>/` with the license file (`OFL.txt`).
- Track binaries via Git LFS if they exceed the repo's threshold (`.gitattributes` — matching the `assets/logos/` precedent). This is the canonical provenance.

### Step 3 — App vendoring (bundler boundary)

Each consuming app **must not** import font binaries from package source (`packages/nexus-ui/AGENTS.md` forbids binary asset imports from component source; deep imports of `@42ch/nexus-ui/src/*|assets/*` are forbidden). Instead:

- Generate subset `.woff2` files (Latin + punctuation + figures; static weights) from the canonical file.
- Vendor each subset into each app's `public/fonts/<family>/` with a `PROVENANCE.md` pointing back to the package canonical path and the regeneration command.
- Regenerate by a documented copy step — no runtime cross-package binary import.

### Step 4 — `@font-face` declarations + token variable

- **v0.5 amendment:** declarations cannot live in the generated `tokens.css`. Emit them through the compiler (preferred, keeps one pipeline) or place them in a handwritten shared CSS import that every app loads next to the generated file.
- Each weight gets its own block with `font-display: swap`; the `--font-<role>` variable must be projected from the DESIGN pair like every other token (do not hand-define a parallel variable).
- URLs are served-root-relative (e.g. `url('/fonts/<family>/…-latin-400-normal.woff2')`).

### Step 5 — Preload

- Add `<link rel="preload" as="font" type="font/woff2" crossorigin>` for the most-used weight (typically 400) in each app's `index.html`; other weights load on demand via `font-display: swap`.

### Step 6 — Bundle gate (measurable cost)

- Gate the gzipped `.woff2` size per weight (V1.121 used ≤ 80 KB gz/weight and measured 19.7 KB gz (400) / 21.1 KB gz (600) for a Latin subset).
- CJK subsets are far larger — expect a per-script tuning decision and a explicit threshold; if a subset exceeds its gate, fall back to the OS stack and record the follow-up instead of shipping it.

### Step 7 — Preset + twMerge wiring

- Point the preset's font-family entry at the new `var(--font-<role>)` and keep the `font-size` metric projections from the DESIGN pair.
- Register the family utility in the `tailwind-merge` `font-family` group in `packages/nexus-ui/src/lib/cn.ts` so it is not silently stripped (see [nexus-brand-token-hierarchy.md](nexus-brand-token-hierarchy.md) §twMerge registry hardening).

## Why This Matters

- **Local-first principle**: no webfont CDN dependency — the app works offline with no third-party DNS, network or tracking.
- **Bundler-agnostic package boundary**: fonts never break the `@42ch/nexus-ui` build; each app vendors its own copy (the standard Vite static-asset pattern).
- **Measurable cost gate**: the per-weight bundle delta is visible and gated, preventing silent bloat.
- **One pipeline**: because declarations and variables must come from (or layer beside) the compiler output, a font can never become a second, hand-maintained token authority.

## When to Apply

- Reintroducing a display/identity typeface (product decision backed by DESIGN).
- Adding a mono companion for code/terminal surfaces.
- Adding a CJK companion (expect the larger-subset tuning above).
- Any future self-hosted font in a local-first, bundler-agnostic workspace.

## Examples

### V1.121 (historical — retired by v1.187)

| Layer | Path | Note |
|-------|------|------|
| Canonical + OFL | `packages/nexus-ui/assets/fonts/source-serif-4/` | LFS-tracked binary + `OFL.txt` |
| App vendored subset | `apps/web/public/fonts/source-serif-4/`, `apps/design-studio/public/fonts/source-serif-4/` | `*.woff2` + `PROVENANCE.md` (files may remain on disk, unused) |
| `@font-face` + `--font-display` | `tooling/design-tokens/src/tokens.css` | Removed from the generated pipeline; must not be re-added by hand |
| Preload | app `index.html` | Removed from the active pipeline |
| Bundle gate | V1.121 plan AC-P0-5 | 19.7 / 21.1 KiB gz — PASS (≤ 80 KB) |

## Do NOT

- Import `.woff2`/`.ttf` binaries in `packages/nexus-ui/src/` — the bundler-agnostic contract forbids it.
- Hand-edit generated `tokens.css` (or `theme.css`, or the brand snapshot) to add font wiring — the next `generate` reverts it and `check` fails.
- Point `@font-face` `src` at a CDN — defeats the local-first principle.
- Skip the preload for the primary weight, or skip the bundle gate.
- Ship a giant variable-font file without subsetting — gate and tune per script.
- Hardcode font URLs in app-local CSS; declarations belong to the shared pipeline so all surfaces consume the same wiring.
