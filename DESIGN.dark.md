---
version: 0.2.0
name: "Nexus Brand Design System"
description: "Cross-application Nexus brand contract — dark theme. Same token names as DESIGN.md with dark-tuned values."

colors:
  # ── Brand core (VI values unchanged; usage shifts on dark surfaces) ──
  brand-deep-blue: "#1E3A5F"
  brand-cyan: "#25D1E0"
  brand-white: "#FFFFFF"

  # ── Brand extended (dark-tuned interaction steps) ──
  brand-deep-blue-800: "#254A75"
  brand-deep-blue-900: "#2E5A8F"
  brand-deep-blue-1000: "#3D6A94"
  brand-cyan-800: "#3DD9E6"
  brand-cyan-900: "#5DE0EB"
  brand-cyan-1000: "#7FE8F0"
  brand-deep-blue-alpha-100: "rgba(37,209,224,0.10)"
  brand-deep-blue-alpha-200: "rgba(37,209,224,0.18)"
  brand-cyan-alpha-100: "rgba(37,209,224,0.14)"
  brand-cyan-alpha-200: "rgba(37,209,224,0.22)"

  # ── Neutral surfaces (dark) ──
  background-100: "#0A0A0A"
  background-200: "#111111"
  background-300: "#1A1A1A"
  gray-100: "#1F1F1F"
  gray-200: "#2A2A2A"
  gray-300: "#3A3A3A"
  gray-400: "#525252"
  gray-500: "#737373"
  gray-600: "#8A8A8A"
  gray-700: "#A3A3A3"
  gray-800: "#C7C7C7"
  gray-900: "#E0E0E0"
  gray-1000: "#F5F5F5"
  gray-alpha-100: "rgba(255,255,255,0.06)"
  gray-alpha-200: "rgba(255,255,255,0.08)"
  gray-alpha-300: "rgba(255,255,255,0.10)"
  gray-alpha-400: "rgba(255,255,255,0.16)"
  gray-alpha-500: "rgba(255,255,255,0.22)"
  gray-alpha-600: "rgba(255,255,255,0.30)"

  # ── Semantic accents (dark-tuned) ──
  red-700: "#FF6B6B"
  red-800: "#FF8585"
  amber-700: "#FFC043"
  amber-800: "#FFD06A"
  green-700: "#54D58A"
  green-800: "#7AE0A3"
  teal-700: "#4CD8C8"
  teal-800: "#75E4D7"
  purple-700: "#B794FF"
  purple-800: "#C5A8FF"

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
    primary: { backgroundColor: "{colors.brand-cyan}", textColor: "{colors.brand-deep-blue}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.brand-cyan-800}", activeBackgroundColor: "{colors.brand-cyan-900}" }
    secondary: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.background-200}", hoverBorderColor: "{colors.gray-alpha-500}" }
    accent: { backgroundColor: "{colors.brand-cyan-alpha-100}", textColor: "{colors.brand-cyan-900}", borderColor: "{colors.brand-cyan-alpha-200}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.brand-cyan-alpha-200}" }
  focus-ring:
    outer: "{colors.brand-cyan}"
    inner: "{colors.background-100}"
    width-outer: "2px"
    width-inner: "4px"
  shell-nav:
    logo-variant: "logo-color.svg"
    logo-min-height: "24px"
    logo-clear-space-ratio: "0.25"
    active-bar-color: "{colors.brand-cyan}"
    brand-mark-on-dark: "{colors.brand-cyan}"
    brand-accent-on-dark: "{colors.brand-cyan-800}"
  logo:
    min-size-px: "24"
    clear-space-ratio: "0.25"
    alt-text: "Nexus"
  # V1.92 P-1 T4 — connection setup screen surface treatments (dark).
  connection-setup:
    security-note:
      backgroundColor: "{colors.brand-cyan-alpha-100}"
      borderColor: "{colors.brand-cyan-alpha-200}"
      textColor: "{colors.gray-1000}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.brand-cyan}"
    warning:
      backgroundColor: "rgba(255,192,67,0.10)"
      borderColor: "rgba(255,192,67,0.22)"
      textColor: "{colors.gray-1000}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.amber-700}"
    info-note:
      backgroundColor: "{colors.gray-alpha-200}"
      borderColor: "{colors.gray-alpha-400}"
      textColor: "{colors.gray-900}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.gray-500}"
    fingerprint-block:
      backgroundColor: "{colors.background-200}"
      borderColor: "{colors.gray-alpha-400}"
      textColor: "{colors.gray-1000}"
      borderWidth: "1px"
      rounded: "{rounded.control}"
      padding: "{spacing.space-3}"
      fontFamily: "\"SFMono-Regular\", \"Cascadia Code\", \"Roboto Mono\", Consolas, monospace"
      fontSize: "13px"
      fontWeight: 400
      lineHeight: 1.5
    form-gap: "{spacing.space-6}"
    form-section-gap: "{spacing.space-8}"
    label-gap: "{spacing.space-1}"
    helper-text-size: "13px"
    helper-text-color: "{colors.gray-600}"
---

# Nexus Brand Design System — Dark Theme

Dark-theme companion to [`DESIGN.md`](DESIGN.md). Same token names; values tuned for dark surfaces. Rule-type documentation, package exposure, and voice guidance live in `DESIGN.md` and apply to both themes.

### Contrast review (WCAG 2.1 AA — dark theme)

| Pairing | Ratio | Intended usage | Verdict |
| --- | --- | --- | --- |
| `gray-1000` on `background-100` | 18.2:1 | Primary UI text | **Pass** |
| `brand-cyan` on `background-100` | 10.6:1 | Focus ring, nav accent, active bar | **Pass** |
| `brand-deep-blue` on `brand-cyan` | 6.2:1 | Primary button label on cyan fill | **Pass** |
| `brand-cyan` on `background-200` | 10.1:1 | Icons, inline accents | **Pass** |
| `brand-deep-blue` on `background-100` | 1.7:1 | — | **Fail** — do not use deep blue fills on dark chrome; use cyan accent instead |
| `gray-700` on `background-100` | 7.0:1 | Secondary/helper text | **Pass** |

**Dark primary button:** cyan fill (`brand-cyan`) + deep blue label (`brand-deep-blue`) — passes AA for button text. Deep blue filled buttons on dark chrome fail surface contrast; reserve deep blue for text on cyan or white-on-brand panels only.

**Background-driven contrast invariant:** Text color on any filled element is decided by the **perceived lightness of that element's background**, not by the active light/dark mode. In the dark theme, bright accent fills that are dark in light mode (e.g. `brand-cyan`, `red-800`, `green-700`) become light/bright surfaces and must use dark text (`brand-deep-blue`) instead of white. Dark surfaces (deep gray, low-alpha tints over dark backgrounds) continue to use light text.

**Logo:** use `logo-color.svg` (cyan) in dark nav/sidebar; `logo-white.svg` on photography or deepest panels.
