---
version: 0.1.0
name: "Nexus Local Web UI"
description: "Nexus Local Web UI dark theme. The Light theme uses the same token names with different values and lives at DESIGN.md."

colors:
  background-100: "#0a0a0a"
  background-200: "#111111"
  background-300: "#1a1a1a"
  gray-100: "#1f1f1f"
  gray-200: "#2a2a2a"
  gray-300: "#3a3a3a"
  gray-400: "#525252"
  gray-500: "#737373"
  gray-600: "#8a8a8a"
  gray-700: "#a3a3a3"
  gray-800: "#c7c7c7"
  gray-900: "#e0e0e0"
  gray-1000: "#f5f5f5"
  gray-alpha-100: "rgba(255,255,255,0.06)"
  gray-alpha-200: "rgba(255,255,255,0.08)"
  gray-alpha-300: "rgba(255,255,255,0.10)"
  gray-alpha-400: "rgba(255,255,255,0.16)"
  gray-alpha-500: "rgba(255,255,255,0.22)"
  gray-alpha-600: "rgba(255,255,255,0.30)"
  blue-700: "#52a8ff"
  blue-800: "#7bbdff"
  blue-900: "#a8d3ff"
  blue-1000: "#d6ebff"
  red-700: "#ff6b6b"
  red-800: "#ff8585"
  red-900: "#ffb3b3"
  red-1000: "#ffd6d6"
  amber-700: "#ffc043"
  amber-800: "#ffd06a"
  amber-900: "#ffe0a3"
  amber-1000: "#fff0d0"
  green-700: "#54d58a"
  green-800: "#7ae0a3"
  green-900: "#a6ebc0"
  green-1000: "#d4f7df"
  teal-700: "#4cd8c8"
  teal-800: "#75e4d7"
  teal-900: "#a2eee6"
  teal-1000: "#d2f8f4"
  purple-700: "#b794ff"
  purple-800: "#c5a8ff"
  purple-900: "#d8c6ff"
  purple-1000: "#eee5ff"
  pink-700: "#ff8ac2"
  pink-800: "#ffa6d0"
  pink-900: "#ffc4df"
  pink-1000: "#ffe3f0"

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
    primary: { backgroundColor: "{colors.blue-700}", textColor: "#ffffff", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.blue-800}", activeBackgroundColor: "{colors.blue-900}" }
    secondary: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.background-200}", hoverBorderColor: "{colors.gray-alpha-500}" }
    tertiary: { backgroundColor: "transparent", textColor: "{colors.gray-1000}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.gray-alpha-100}" }
    destructive: { backgroundColor: "{colors.red-800}", textColor: "#ffffff", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.red-700}", activeBackgroundColor: "{colors.red-900}" }
    sizes:
      small: { height: "32px", typography: "{typography.button-12}" }
      default: { height: "40px", typography: "{typography.button-14}" }
      large: { height: "48px", typography: "{typography.button-14}" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", cursor: "not-allowed" }
  input-select-textarea:
    default: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px" }
    error: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.red-700}", rounded: "{rounded.control}", height: "40px" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", borderColor: "{colors.gray-alpha-300}", rounded: "{rounded.control}", height: "40px" }
    textarea: { minHeight: "96px" }
    placeholder: { textColor: "{colors.gray-700}" }
    helperText: { typography: "{typography.copy-13}" }
    errorHelperText: { textColor: "{colors.red-700}", typography: "{typography.copy-13}" }
  card:
    default: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.card}", padding: "{spacing.space-6}", shadow: "shadow-card" }
    compact: { padding: "{spacing.space-4}" }
    hero: { padding: "{spacing.space-8}" }
  table:
    header: { backgroundColor: "{colors.background-200}", typography: "{typography.label-12}", textColor: "{colors.gray-900}", borderBottomColor: "{colors.gray-alpha-400}" }
    row: { typography: "{typography.copy-14}", textColor: "{colors.gray-1000}", secondaryTextColor: "{colors.gray-900}", hoverBackgroundColor: "{colors.background-200}", selectedBackgroundColor: "{colors.background-300}" }
    idText: { typography: "{typography.label-12-mono}" }
  badge-status-pill:
    neutral: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-300}" }
    running: { backgroundColor: "rgba(84,213,138,0.14)", textColor: "{colors.green-1000}", borderColor: "rgba(84,213,138,0.30)" }
    queued: { backgroundColor: "rgba(76,216,200,0.14)", textColor: "{colors.teal-1000}", borderColor: "rgba(76,216,200,0.30)" }
    warning: { backgroundColor: "rgba(255,192,67,0.16)", textColor: "{colors.amber-1000}", borderColor: "rgba(255,192,67,0.30)" }
    error: { backgroundColor: "rgba(255,107,107,0.16)", textColor: "{colors.red-1000}", borderColor: "rgba(255,107,107,0.30)" }
    preset: { backgroundColor: "rgba(183,148,255,0.12)", textColor: "{colors.purple-1000}", borderColor: "rgba(183,148,255,0.30)" }
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  toast: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", maxWidth: "360px", titleTypography: "{typography.label-14}", bodyTypography: "{typography.copy-13}" }
  sidebar-nav: { width: "248px", backgroundColor: "{colors.background-100}", dividerColor: "{colors.gray-alpha-400}", itemHeight: "36px", itemRounded: "{rounded.control}", itemTypography: "{typography.label-14}", activeBackgroundColor: "{colors.gray-alpha-100}", activeTextColor: "{colors.gray-1000}", activeBarColor: "{colors.blue-700}" }
  dialog: { backgroundColor: "{colors.background-100}", rounded: "{rounded.popover}", shadow: "shadow-modal", maxWidth: "560px", padding: "{spacing.space-6}" }
  popover: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", itemHeight: "36px" }
  editor:
    surface: "{colors.background-100}"
    surface-muted: "{colors.background-200}"
    border: "{colors.gray-alpha-400}"
    border-active: "{colors.blue-700}"
    toolbar-control-bg: "transparent"
    toolbar-control-hover: "{colors.gray-alpha-100}"
    toolbar-control-active: "{colors.gray-alpha-200}"
    save-clean: "{colors.green-700}"
    save-dirty: "{colors.amber-700}"
    save-error: "{colors.red-700}"
    selection: "rgba(82,168,255,0.24)"
  data-table:
    row-hover: "{colors.background-200}"
    row-selected: "{colors.background-300}"
    row-edited: "rgba(255,192,67,0.14)"
    row-protected: "rgba(183,148,255,0.12)"
    cell-edit-bg: "{colors.background-100}"
    cell-edit-border: "{colors.blue-700}"
    column-divider: "{colors.gray-alpha-200}"
  context-menu:
    bg: "{colors.background-100}"
    border: "{colors.gray-alpha-400}"
    item-hover: "{colors.gray-alpha-100}"
    item-active: "{colors.gray-alpha-200}"
    item-disabled: "{colors.gray-700}"
    shortcut: "{colors.gray-700}"
    native-action: "{colors.gray-1000}"
    native-icon: "{colors.gray-900}"
    native-disabled: "{colors.gray-700}"
    native-danger: "{colors.red-700}"
  desktop-window-chrome:
    window-bg: "{colors.background-100}"
    window-border: "{colors.gray-alpha-400}"
    titlebar-safe-area: "28px"
    window-radius: "{rounded.card}"
    window-drag-region-height: "0px"
  app-menu:
    label: "{colors.gray-1000}"
    secondary: "{colors.gray-700}"
    disabled: "{colors.gray-700}"
    danger: "{colors.red-700}"
  native-dialogs:
    title: "{typography.heading-20}"
    body: "{typography.copy-14}"
    secondary: "{colors.gray-900}"
    danger: "{colors.red-700}"
    warning: "{colors.amber-700}"
  daemon-status-indicator:
    healthy-bg: "rgba(84,213,138,0.14)"
    healthy-text: "{colors.green-1000}"
    starting-bg: "rgba(76,216,200,0.14)"
    starting-text: "{colors.teal-1000}"
    degraded-bg: "rgba(255,192,67,0.16)"
    degraded-text: "{colors.amber-1000}"
    stopped-bg: "rgba(255,107,107,0.16)"
    stopped-text: "{colors.red-1000}"

  # V1.70 canvas implement — concrete dark values (same token names as DESIGN.md)
  canvas:
    canvas-surface: "#141414"
    canvas-grid: "rgba(255,255,255,0.05)"
    canvas-node-fill: "#1a1a1a"
    canvas-node-fill-hover: "#2a2a2a"
    canvas-node-border: "rgba(255,255,255,0.18)"
    canvas-node-border-selected: "{colors.blue-700}"
    canvas-edge: "{colors.gray-400}"
    canvas-edge-hover: "{colors.gray-800}"
    canvas-port: "{colors.gray-700}"
    canvas-minimap: "{colors.gray-alpha-500}"
    canvas-strategy-accent: "{colors.purple-700}"
    canvas-write-dirty: "{colors.amber-700}"
    canvas-write-conflict: "{colors.red-700}"
    canvas-write-success: "{colors.green-700}"
    canvas-write-stale-bg: "rgba(183,110,0,0.12)"
---

# Nexus Local Web UI Design System — Dark Theme

This file is the dark-theme token companion to [`DESIGN.md`](DESIGN.md). It intentionally preserves the same token names and frontmatter structure with dark values. Rule-type documentation, component behavior, voice/content guidance, and implementation mapping live in `DESIGN.md` and apply to both themes.

Dark values were split from the former inline `Dark` columns in `DESIGN.md` during the V1.69 Production migration. Consumers should resolve dark values from this file conceptually while continuing to reference the same token names (`--color-<token>`, Tailwind `cv('<token>')`).
