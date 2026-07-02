---
version: 0.2.0
name: "Nexus Brand Design System"
description: "Cross-application Nexus brand contract — light/default theme. Canonical token names and values for shared visual identity; app surfaces map these tokens into local CSS/Tailwind layers. Dark theme uses the same token names with dark values in DESIGN.dark.md."

colors:
  # ── Brand core (VI palette — do not rename) ──
  brand-deep-blue: "#1E3A5F"
  brand-cyan: "#25D1E0"
  brand-white: "#FFFFFF"

  # ── Brand extended (hover/active/surface tints derived from VI) ──
  brand-deep-blue-800: "#182F4D"
  brand-deep-blue-900: "#12243B"
  brand-deep-blue-1000: "#0C1A2B"
  brand-cyan-800: "#1FB8C6"
  brand-cyan-900: "#1896A2"
  brand-cyan-1000: "#117480"
  brand-deep-blue-alpha-100: "rgba(30,58,95,0.08)"
  brand-deep-blue-alpha-200: "rgba(30,58,95,0.14)"
  brand-cyan-alpha-100: "rgba(37,209,224,0.12)"
  brand-cyan-alpha-200: "rgba(37,209,224,0.20)"

  # ── Neutral surfaces (refined for contrast; not raw VI) ──
  background-100: "#FFFFFF"
  background-200: "#F7F8FA"
  background-300: "#EEF1F5"
  gray-100: "#F3F4F6"
  gray-200: "#E8EAEE"
  gray-300: "#D5D9E0"
  gray-400: "#B8BEC8"
  gray-500: "#949BA8"
  gray-600: "#6F7785"
  gray-700: "#4F5663"
  gray-800: "#363C47"
  gray-900: "#242933"
  gray-1000: "#111318"
  gray-alpha-100: "rgba(17,19,24,0.04)"
  gray-alpha-200: "rgba(17,19,24,0.06)"
  gray-alpha-300: "rgba(17,19,24,0.08)"
  gray-alpha-400: "rgba(17,19,24,0.12)"
  gray-alpha-500: "rgba(17,19,24,0.18)"
  gray-alpha-600: "rgba(17,19,24,0.24)"

  # ── Semantic accents (state colors; independent of brand primary) ──
  red-700: "#D92D32"
  red-800: "#B91C22"
  amber-700: "#B76E00"
  amber-800: "#935800"
  green-700: "#1F8F4D"
  green-800: "#18753E"
  teal-700: "#008577"
  teal-800: "#006B60"
  purple-700: "#7C3AED"
  purple-800: "#6D28D9"

typography:
  heading-32: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "32px", fontWeight: 650, lineHeight: 1.18, letterSpacing: "-0.025em" }
  heading-24: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "24px", fontWeight: 650, lineHeight: 1.25, letterSpacing: "-0.02em" }
  heading-20: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "20px", fontWeight: 600, lineHeight: 1.3, letterSpacing: "-0.015em" }
  heading-16: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "16px", fontWeight: 600, lineHeight: 1.4, letterSpacing: "-0.01em" }
  label-14: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "14px", fontWeight: 500, lineHeight: 1.35, letterSpacing: "0" }
  label-12: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "12px", fontWeight: 600, lineHeight: 1.35, letterSpacing: "0.02em" }
  copy-16: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "16px", fontWeight: 400, lineHeight: 1.6, letterSpacing: "0" }
  copy-14: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "14px", fontWeight: 400, lineHeight: 1.55, letterSpacing: "0" }
  copy-13: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "13px", fontWeight: 400, lineHeight: 1.5, letterSpacing: "0" }
  button-14: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "14px", fontWeight: 550, lineHeight: 1, letterSpacing: "0" }
  button-12: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "12px", fontWeight: 600, lineHeight: 1, letterSpacing: "0.01em" }
  label-12-mono: { fontFamily: "\"SFMono-Regular\", \"Cascadia Code\", \"Roboto Mono\", Consolas, monospace", fontSize: "12px", fontWeight: 500, lineHeight: 1.4, letterSpacing: "0" }
  copy-13-mono: { fontFamily: "\"SFMono-Regular\", \"Cascadia Code\", \"Roboto Mono\", Consolas, monospace", fontSize: "13px", fontWeight: 400, lineHeight: 1.5, letterSpacing: "0" }

spacing:
  base: "4px"
  space-1: "4px"
  space-2: "8px"
  space-3: "12px"
  space-4: "16px"
  space-6: "24px"
  space-8: "32px"
  space-10: "40px"
  space-16: "64px"
  space-24: "96px"

rounded:
  control: "6px"
  card: "8px"
  popover: "12px"
  fullscreen: "16px"
  pill: "9999px"

components:
  button:
    primary: { backgroundColor: "{colors.brand-deep-blue}", textColor: "{colors.brand-white}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.brand-deep-blue-800}", activeBackgroundColor: "{colors.brand-deep-blue-900}" }
    secondary: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.background-200}", hoverBorderColor: "{colors.gray-alpha-500}" }
    accent: { backgroundColor: "{colors.brand-cyan-alpha-100}", textColor: "{colors.brand-deep-blue}", borderColor: "{colors.brand-cyan-alpha-200}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.brand-cyan-alpha-200}" }
  focus-ring:
  # Two-layer focus ring — brand primary on light surfaces
    outer: "{colors.brand-deep-blue}"
    inner: "{colors.background-100}"
    width-outer: "2px"
    width-inner: "4px"
  shell-nav:
    logo-variant: "logo-dark.svg"
    logo-min-height: "24px"
    logo-clear-space-ratio: "0.25"
    active-bar-color: "{colors.brand-deep-blue}"
    brand-mark-on-light: "{colors.brand-deep-blue}"
    brand-accent-on-light: "{colors.brand-cyan}"
  logo:
    min-size-px: "24"
    clear-space-ratio: "0.25"
    alt-text: "Nexus"
---

# Nexus Brand Design System

<!-- COMPLETENESS_LEVEL: 2 — Standard+, last audited 2026-07-02 -->

This file is the **cross-application brand SSOT** for Nexus. It defines canonical brand token names, values, accessibility intent, logo rules, and voice guidance. App-specific design files (for example `apps/web/DESIGN*.md`) are **consumption mappings** — they may alias or extend these tokens for Tailwind/CSS/shadcn consumers but must not invent parallel brand values.

**Token hierarchy (V1.83):**

1. Root `DESIGN.md` / `DESIGN.dark.md` — brand contract (this file)
2. `packages/nexus-ui` (`@42ch/nexus-ui`) — package-consumable tokens, `theme.css`, logo assets
3. `apps/web/DESIGN*.md` — Web CSS variable and component-token mapping
4. `apps/web` implementation — applies mapped tokens in shell and primitives (P2)

**Author experience intent:** Nexus should feel like a calm, trustworthy creative workspace — recognizable from the shell, low-distraction, with clear action hierarchy. Brand visibility in V1.83 targets navigation identity, focus states, and base primitives — not full-page redesign.

---

## Brand Colors

VI palette (frozen names):

| Token | Hex | Role |
| --- | --- | --- |
| `brand-deep-blue` | `#1E3A5F` | Primary brand, primary actions on light surfaces, links, focus rings |
| `brand-cyan` | `#25D1E0` | Accent — icons, active indicators, dark-theme interactive emphasis |
| `brand-white` | `#FFFFFF` | Text on deep blue fills, logo on dark hero surfaces |

Extended brand steps (`brand-deep-blue-800` … `brand-cyan-1000`) support hover, active, and low-opacity washes without raw VI drift.

### Contrast review (WCAG 2.1 AA — light theme)

| Pairing | Ratio | Intended usage | Verdict |
| --- | --- | --- | --- |
| `brand-deep-blue` on `background-100` | 11.5:1 | Headings, links, primary button label context | **Pass** — body text OK |
| `brand-white` on `brand-deep-blue` | 11.5:1 | Primary button label | **Pass** |
| `brand-cyan` on `background-100` | 1.9:1 | — | **Fail** — accent/icon/active indicator only; **never body text on white** |
| `brand-cyan` on `brand-deep-blue` | 6.2:1 | Accent chips on brand panels | **Pass** |
| `gray-1000` on `background-100` | 18.9:1 | Primary UI text | **Pass** |
| `gray-700` on `background-100` | 5.7:1 | Secondary/helper text | **Pass** |

**Cyan usage rule:** `brand-cyan` is an accent token. Use it for marks, borders, icons, progress fills, and dark-theme interactive emphasis — not for paragraph text on white or light gray.

---

## Neutrals

Neutral tokens (`background-*`, `gray-*`, `gray-alpha-*`) are refined for UI contrast and are not part of the VI brief. They carry surface hierarchy, borders, and text — brand colors carry identity and primary action.

---

## Typography

System stack by default (no webfont fetch required). If a future build bundles a brand font, map it to the existing token names.

| Role | Tokens | Use |
| --- | --- | --- |
| Headings | `heading-32` … `heading-16` | Page titles, section headers |
| Labels | `label-14`, `label-12` | Nav, form labels, badges |
| Body | `copy-16`, `copy-14`, `copy-13` | Prose, helpers, dense UI |
| Actions | `button-14`, `button-12` | Buttons and compact controls |
| Mono | `label-12-mono`, `copy-13-mono` | IDs, ports, code-like values |

Numeric columns use `font-variant-numeric: tabular-nums`.

---

## Spacing & Layout

Base unit: **4px**. Prefer mechanical rhythm (`8px` label+control, `16px` groups, `24px` card padding, `32–40px` major sections).

| Breakpoint | Width | Intent |
| --- | --- | --- |
| `sm` | `401px` | Small phones |
| `md` | `601px` | Large phones / narrow tablets |
| `lg` | `961px` | Desktop shell with sidebar |
| `xl` | `1200px` | Wide content |
| `2xl` | `1400px` | Dense admin displays |

---

## Elevation

Borders and tonal surfaces first; shadows clarify layers only.

| Token | Light value | Use |
| --- | --- | --- |
| `shadow-card` | `0 1px 2px rgba(17,19,24,0.06)` | Raised cards |
| `shadow-popover` | `0 1px 1px rgba(17,19,24,0.04), 0 8px 24px -12px rgba(17,19,24,0.14)` | Menus, tooltips |
| `shadow-modal` | `0 1px 1px rgba(17,19,24,0.05), 0 24px 48px -24px rgba(17,19,24,0.22)` | Dialogs |

---

## Motion

| Token | Value | Use |
| --- | --- | --- |
| `duration-instant` | `0ms` | Data refresh |
| `duration-state` | `120ms` | Hover/focus/pressed |
| `duration-popover` | `160ms` | Menus, dropdowns |
| `duration-modal` | `220ms` | Dialog open/close |
| `ease-standard` | `cubic-bezier(0.16, 1, 0.3, 1)` | Default UI |
| `ease-emphasized` | `cubic-bezier(0.2, 0.8, 0.2, 1)` | Modal enter |

Honor `prefers-reduced-motion: reduce`.

---

## Focus

All interactive elements expose `:focus-visible` using the two-layer ring in `components.focus-ring`:

```css
box-shadow:
  0 0 0 2px var(--color-background-100),
  0 0 0 4px var(--color-brand-deep-blue);
```

On brand-filled surfaces (deep blue button), invert the inner ring to `brand-white` or use `brand-cyan` outer ring only when contrast remains ≥ 3:1.

---

## Logo Usage

Canonical SVG assets ship from `@42ch/nexus-ui/assets/logos/`. PNG sources are provenance-only (Git LFS).

| Variant | File | Surface |
| --- | --- | --- |
| Deep blue mark | `logo-dark.svg` | Light nav, sidebar, light shell header |
| Cyan mark | `logo-color.svg` | Dark nav, dark shell header |
| White mark | `logo-white.svg` | Dark hero, photography overlays, high-contrast panels |
| Monotone mark | `logo-mono.svg` | Inline UI; inherits `color` via `currentColor` |

**Rules:**

- Minimum rendered height: **24px** (`logoMinSizePx`).
- Clear space: **≥ 25%** of logo height on all sides.
- Alt text: `Nexus` on `<img>`; inline SVGs include `<title>` and `<desc>`.
- Do not recolor SVG fills outside the mono variant.
- Do not place cyan mark on white/light gray without a contrast check — prefer `logo-dark.svg` on light surfaces.

---

## Package Exposure (`@42ch/nexus-ui`)

| Root token | Package export | CSS variable |
| --- | --- | --- |
| `brand-deep-blue` | `brandColors.deepBlue` | `--nexus-brand-deep-blue` |
| `brand-cyan` | `brandColors.cyan` | `--nexus-brand-cyan` |
| `brand-white` | `brandColors.white` | `--nexus-brand-white` |
| Logo variants | `logoVariants.*` | Import SVG path from package exports |
| Logo sizing | `logoMinSizePx`, `logoClearSpaceRatio` | — |

**Import paths (stable public API):**

- `@42ch/nexus-ui` — token constants
- `@42ch/nexus-ui/tokens` — direct token module
- `@42ch/nexus-ui/theme.css` — brand CSS custom properties
- `@42ch/nexus-ui/assets/logos/*.svg` — logo assets

React component exports are **out of scope** for V1.83. Future `nexus-platform` consumption should use only documented package entries — no deep `src/` imports.

Extended brand scales (`brand-deep-blue-800`, neutrals, semantic accents) are defined here first; package `theme.css` may grow in a later plan after P2 proves stable usage. P2 Web implementation should map from root tokens via `apps/web/DESIGN*.md`, not hard-code hex values.

---

## Voice & Content (brand-level)

- Helpful, plain, local-first — a careful CLI message translated into UI copy.
- Title Case for nav, buttons, page titles; sentence case for helpers and errors.
- Actions are **Verb + Noun**: `Create Work`, `Validate Preset`.
- Prefer author-facing nouns: `Work`, `preset`, `finding`, `local daemon`.
- Avoid protocol jargon (`ACP`, `cursor token`) in product surfaces unless diagnostics explicitly require it.

---

## P2 Implementation Notes

Locked token names for `@frontend-dev`:

| Root token | Web alias (light) | Web alias (dark) | Notes |
| --- | --- | --- | --- |
| `brand-deep-blue` | `blue-700` | — | Light primary interactive |
| `brand-deep-blue-800` | `blue-800` | — | Light hover |
| `brand-deep-blue-900` | `blue-900` | — | Light active |
| `brand-cyan` | — | `blue-700` | Dark interactive accent / primary fill |
| `brand-deep-blue` | — | primary button `textColor` | Dark primary button label on cyan fill |
| `brand-deep-blue` | `brand-deep-blue` (new) | same name | Explicit brand CSS vars |
| `brand-cyan` | `brand-cyan` (new) | same name | Explicit brand CSS vars |

**Files P2 may edit:** `apps/web/src/index.css`, `apps/web/tailwind.config.ts`, shell layout components, `src/components/ui/*` primitives. **Do not** rename root tokens or package export paths without routing back through P1/P0.

**Hardcoded rgba tints** in canvas/SOUL/findings tokens that reference legacy blue `rgba(0,107,255,…)` should be re-tinted to `rgba(30,58,95,…)` (light) or `rgba(37,209,224,…)` (dark) during P2 — names stay frozen.
