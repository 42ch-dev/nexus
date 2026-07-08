# tooling/design-tokens — AGENTS.md

**`@nexus/design-tokens`** — shared Tailwind preset + CSS variable layers for all Nexus product surfaces.

Parent rules: root [`AGENTS.md`](../../AGENTS.md) ("New crate policy").

## Purpose

- `tokens.css` (`./src/tokens.css`): CSS custom property projection of repo-root `DESIGN.md` (light) and `DESIGN.dark.md` (dark). All token values are generated/derived from the DESIGN pair; do not invent tokens here.
- `tailwind.preset.ts` (`./tailwind.preset.ts`): Tailwind v3 preset that maps the CSS variables into `theme.extend` (colors, fonts, shadows, durations, radii, etc.). Shared by all Nexus product surfaces so there is no per-app `theme.extend` duplication.

## Key Rules

- **DESIGN.md / DESIGN.dark.md are the SSOT.** This package's `tokens.css` and preset are GENERATED/derived from them. If a token is missing, report it — do not handcraft a value inline.
- **Do not invent tokens here.** New token needs must originate in the root DESIGN pair, then flow through this package as CSS variables.
- **Both consumer apps** (`apps/web`, `apps/design-studio`) import `@nexus/design-tokens/tokens.css` and use the Tailwind preset — no per-app duplicate `theme.extend`.
- **No per-app token overrides.** Theme variants (light/dark) are expressed via the `.dark` class block in `tokens.css` only.

## Dependencies & Relationships

| Dependency | Role |
|-----------|------|
| `@42ch/nexus-ui` (`theme.css`) | Brand VI primitives (`--nexus-brand-*`) — imported by `tokens.css` via `@import` |

| Consumer | How |
|---------|-----|
| `apps/web` | Imports `@nexus/design-tokens/tokens.css`; uses `@nexus/design-tokens/tailwind.preset` in `tailwind.config.ts` |
| `apps/design-studio` | Same as above — no separate token layer |

## Tooling Placement

`tooling/` holds code generation, CI scripts, and shared build infrastructure. `design-tokens/` fits here because it produces derived build artifacts (CSS, Tailwind preset) from the DESIGN SSOT — it is not a product surface (`apps/`) or a reusable Rust library (`crates/`).

## Dev Commands

No build step — `tokens.css` is a plain CSS file and `tailwind.preset.ts` is consumed at Tailwind config load time. Changes to `tokens.css` or the preset are picked up on the next `pnpm dev` or `pnpm build` in any consumer app.
