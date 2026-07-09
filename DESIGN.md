---
version: 0.3.0
name: "Nexus Design System"
description: "Unified design contract for all Nexus product surfaces — light/default theme. Canonical token names and values for shared visual identity; consumed by apps/web, apps/design-studio, and @42ch/nexus-ui. Dark theme uses the same token names with dark values in DESIGN.dark.md."

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

  # ── Neutral surfaces (apps/web parity — shipped values) ──
  background-100: "#ffffff"
  background-200: "#fafafa"
  background-300: "#f5f5f5"
  gray-100: "#f5f5f5"
  gray-200: "#eeeeee"
  gray-300: "#e0e0e0"
  gray-400: "#c7c7c7"
  gray-500: "#a3a3a3"
  gray-600: "#8a8a8a"
  gray-700: "#666666"
  gray-800: "#4a4a4a"
  gray-900: "#333333"
  gray-1000: "#111111"
  gray-alpha-100: "rgba(0,0,0,0.04)"
  gray-alpha-200: "rgba(0,0,0,0.06)"
  gray-alpha-300: "rgba(0,0,0,0.08)"
  gray-alpha-400: "rgba(0,0,0,0.12)"
  gray-alpha-500: "rgba(0,0,0,0.18)"
  gray-alpha-600: "rgba(0,0,0,0.24)"

  # ── Primary interactive scale (light; maps to brand-deep-blue steps) ──
  # blue-* keys preserved as web aliases for backward compatibility.
  blue-700: "#1E3A5F"
  blue-800: "#182F4D"
  blue-900: "#12243B"
  blue-1000: "#0C1A2B"

  # ── Semantic accent scales (apps/web parity — four-step, shipped) ──
  red-700: "#e5484d"
  red-800: "#d11f2a"
  red-900: "#a91520"
  red-1000: "#7f1018"
  amber-700: "#b76e00"
  amber-800: "#935800"
  amber-900: "#704300"
  amber-1000: "#4d2d00"
  green-700: "#1f8f4d"
  green-800: "#18753e"
  green-900: "#125a30"
  green-1000: "#0d4023"
  teal-700: "#008577"
  teal-800: "#006b60"
  teal-900: "#00524a"
  teal-1000: "#003b35"
  purple-700: "#7c3aed"
  purple-800: "#6d28d9"
  purple-900: "#581cbd"
  purple-1000: "#3b1686"
  pink-700: "#db2777"
  pink-800: "#be185d"
  pink-900: "#9d174d"
  pink-1000: "#831843"

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
  # Author Reflection — reading-surface typography.
  # Theme-independent metrics (a reading measure is a line-length target, not a
  # color); values are identical in DESIGN.dark.md so the prose column shape does
  # not shift between themes.
  reading-prose-measure: "68ch"
  reading-prose-line-height: "1.75"
  reading-prose-paragraph-spacing: "1.25em"

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
  # ── button: apps/web superset (tertiary + destructive + sizes + disabled) wins ──
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

  # ── focus-ring: root two-layer ring spec ──
  focus-ring:
    outer: "{colors.blue-700}"
    inner: "{colors.background-100}"
    width-outer: "2px"
    width-inner: "4px"

  # ── input-select-textarea: apps/web ──
  input-select-textarea:
    default: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px" }
    error: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.red-700}", rounded: "{rounded.control}", height: "40px" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", borderColor: "{colors.gray-alpha-300}", rounded: "{rounded.control}", height: "40px" }
    textarea: { minHeight: "96px" }
    placeholder: { textColor: "{colors.gray-700}" }
    helperText: { typography: "{typography.copy-13}" }
    errorHelperText: { textColor: "{colors.red-700}", typography: "{typography.copy-13}" }

  # ── card: apps/web ──
  card:
    default: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.card}", padding: "{spacing.space-6}", shadow: "shadow-card" }
    compact: { padding: "{spacing.space-4}" }
    hero: { padding: "{spacing.space-8}" }

  # ── table: apps/web ──
  table:
    header: { backgroundColor: "{colors.background-200}", typography: "{typography.label-12}", textColor: "{colors.gray-900}", borderBottomColor: "{colors.gray-alpha-400}" }
    row: { typography: "{typography.copy-14}", textColor: "{colors.gray-1000}", secondaryTextColor: "{colors.gray-900}", hoverBackgroundColor: "{colors.background-200}", selectedBackgroundColor: "{colors.background-300}" }
    idText: { typography: "{typography.label-12-mono}" }

  # ── badge-status-pill: apps/web (tone=soft|solid; default soft) ──
  badge-status-pill:
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}", fontWeight: 600 }
    # soft (default): tinted fill + semantic text; strengthened borders (neutral gray-alpha-400; semantic ~50% alpha)
    soft:
      neutral: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-400}" }
      running: { backgroundColor: "rgba(31,143,77,0.10)", textColor: "{colors.green-1000}", borderColor: "rgba(31,143,77,0.50)" }
      queued: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.50)" }
      warning: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.50)" }
      error: { backgroundColor: "rgba(229,72,77,0.12)", textColor: "{colors.red-1000}", borderColor: "rgba(229,72,77,0.50)" }
      preset: { backgroundColor: "rgba(124,58,237,0.10)", textColor: "{colors.purple-1000}", borderColor: "rgba(124,58,237,0.50)" }
    # solid (opt-in): semantic fill + high-contrast white text; no visible border
    solid:
      neutral: { backgroundColor: "{colors.gray-1000}", textColor: "#ffffff", borderColor: "transparent" }
      running: { backgroundColor: "{colors.green-700}", textColor: "#ffffff", borderColor: "transparent" }
      queued: { backgroundColor: "{colors.teal-700}", textColor: "#ffffff", borderColor: "transparent" }
      warning: { backgroundColor: "{colors.amber-700}", textColor: "#ffffff", borderColor: "transparent" }
      error: { backgroundColor: "{colors.red-800}", textColor: "#ffffff", borderColor: "transparent" }
      preset: { backgroundColor: "{colors.purple-700}", textColor: "#ffffff", borderColor: "transparent" }

  # ── toast: apps/web ──
  toast: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", maxWidth: "360px", titleTypography: "{typography.label-14}", bodyTypography: "{typography.copy-13}" }

  # ── sidebar-nav: apps/web ──
  sidebar-nav: { width: "248px", backgroundColor: "{colors.background-100}", dividerColor: "{colors.gray-alpha-400}", itemHeight: "36px", itemRounded: "{rounded.control}", itemTypography: "{typography.label-14}", activeBackgroundColor: "{colors.gray-alpha-100}", activeTextColor: "{colors.gray-1000}", activeBarColor: "{colors.blue-700}" }

  # ── dialog / popover: apps/web ──
  dialog: { backgroundColor: "{colors.background-100}", rounded: "{rounded.popover}", shadow: "shadow-modal", maxWidth: "560px", padding: "{spacing.space-6}" }
  popover: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", itemHeight: "36px" }

  # ── editor: apps/web ──
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
    selection: "rgba(0,107,255,0.14)"

  # ── data-table: apps/web ──
  data-table:
    row-hover: "{colors.background-200}"
    row-selected: "{colors.background-300}"
    row-edited: "rgba(183,110,0,0.08)"
    row-protected: "rgba(124,58,237,0.06)"
    cell-edit-bg: "{colors.background-100}"
    cell-edit-border: "{colors.blue-700}"
    column-divider: "{colors.gray-alpha-200}"

  # ── context-menu: apps/web ──
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

  # ── desktop-window-chrome: apps/web ──
  desktop-window-chrome:
    window-bg: "{colors.background-100}"
    window-border: "{colors.gray-alpha-400}"
    titlebar-safe-area: "28px"
    window-radius: "{rounded.card}"
    window-drag-region-height: "0px"

  # ── app-menu: apps/web ──
  app-menu:
    label: "{colors.gray-1000}"
    secondary: "{colors.gray-700}"
    disabled: "{colors.gray-700}"
    danger: "{colors.red-700}"

  # ── native-dialogs: apps/web ──
  native-dialogs:
    title: "{typography.heading-20}"
    body: "{typography.copy-14}"
    secondary: "{colors.gray-900}"
    danger: "{colors.red-700}"
    warning: "{colors.amber-700}"

  # ── daemon-status-indicator: apps/web ──
  daemon-status-indicator:
    healthy-bg: "rgba(31,143,77,0.10)"
    healthy-text: "{colors.green-1000}"
    starting-bg: "rgba(0,133,119,0.10)"
    starting-text: "{colors.teal-1000}"
    degraded-bg: "rgba(183,110,0,0.12)"
    degraded-text: "{colors.amber-1000}"
    stopped-bg: "rgba(229,72,77,0.12)"
    stopped-text: "{colors.red-1000}"

  # ── shell-nav: root (brand navigation tokens) ──
  shell-nav:
    logo-variant: "logo-primary.svg"
    logo-min-height: "24px"
    logo-clear-space-ratio: "0.25"
    active-bar-color: "{colors.blue-700}"
    brand-mark-on-light: "{colors.brand-deep-blue}"
    brand-accent-on-light: "{colors.brand-cyan}"

  # ── logo: root ──
  logo:
    min-size-px: "24"
    clear-space-ratio: "0.25"
    alt-text: "Nexus"

  # ── connection-setup: root (V1.92 P-1 T4) ──
  connection-setup:
    security-note:
      backgroundColor: "{colors.brand-deep-blue-alpha-100}"
      borderColor: "{colors.brand-deep-blue-alpha-200}"
      textColor: "{colors.gray-900}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.brand-deep-blue}"
    warning:
      backgroundColor: "rgba(183,110,0,0.08)"
      borderColor: "rgba(183,110,0,0.20)"
      textColor: "{colors.gray-900}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.amber-700}"
    info-note:
      backgroundColor: "{colors.gray-alpha-100}"
      borderColor: "{colors.gray-alpha-300}"
      textColor: "{colors.gray-800}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.gray-600}"
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

  # ── finding-status-pill: apps/web ──
  finding-status-pill:
    open: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    triaged: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    in_review: { backgroundColor: "rgba(0,107,255,0.10)", textColor: "{colors.blue-1000}", borderColor: "rgba(0,107,255,0.30)" }
    resolved: { backgroundColor: "rgba(31,143,77,0.10)", textColor: "{colors.green-1000}", borderColor: "rgba(31,143,77,0.30)" }
    wont_fix: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-300}" }
    duplicate: { backgroundColor: "rgba(124,58,237,0.10)", textColor: "{colors.purple-1000}", borderColor: "rgba(124,58,237,0.30)" }
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }

  # ── finding-triage: apps/web ──
  finding-triage:
    panel-bg: "{colors.background-100}"
    panel-border: "{colors.gray-alpha-400}"
    row-active: "{colors.background-300}"
    action-button: "secondary"
    executor-select: "input-select-textarea.default"

  # ── memory: apps/web ──
  memory-pending-count:
    backgroundColor: "rgba(229,72,77,0.12)"
    textColor: "{colors.red-1000}"
    borderColor: "rgba(229,72,77,0.30)"
    base: { height: "20px", minInlineSize: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  memory-review-button:
    basis: "primary"
  memory-task-kind-brainstorm:
    backgroundColor: "rgba(183,110,0,0.12)"
    textColor: "{colors.amber-1000}"
    borderColor: "rgba(183,110,0,0.30)"
  memory-task-kind-outline:
    backgroundColor: "rgba(0,107,255,0.10)"
    textColor: "{colors.blue-1000}"
    borderColor: "rgba(0,107,255,0.30)"
  memory-task-kind-chapter:
    backgroundColor: "rgba(0,133,119,0.10)"
    textColor: "{colors.teal-1000}"
    borderColor: "rgba(0,133,119,0.30)"
  memory-task-kind-research:
    backgroundColor: "rgba(124,58,237,0.10)"
    textColor: "{colors.purple-1000}"
    borderColor: "rgba(124,58,237,0.30)"
  memory-task-kind-unknown:
    backgroundColor: "{colors.gray-alpha-100}"
    textColor: "{colors.gray-900}"
    borderColor: "{colors.gray-alpha-300}"
  memory-task-kind-base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  memory-fragment-summary:
    typography: "{typography.copy-14}"
  memory-fragment-id:
    typography: "{typography.copy-13-mono}"
    textColor: "{colors.gray-800}"
  memory-inspector-header:
    panel-bg: "{colors.background-100}"
    panel-border: "{colors.gray-alpha-400}"
    row-active: "{colors.background-300}"
  memory-inspector-field-label:
    typography: "{typography.label-14}"
    textColor: "{colors.gray-900}"
  memory-inspector-field-value:
    typography: "{typography.copy-13}"
    textColor: "{colors.gray-1000}"
  memory-fragment-filter-input:
    basis: "input-select-textarea.default"

  # ── reading-chapter-nav: apps/web ──
  reading-chapter-nav:
    chrome-bg: "{colors.background-200}"
    chrome-border: "{colors.gray-alpha-400}"
    control-prev: "button.secondary basis"
    control-next: "button.secondary basis"
    volume-group-bg: "{colors.background-300}"
    volume-group-border: "{colors.gray-alpha-300}"

  # ── reading-progress-indicator: apps/web ──
  reading-progress-indicator:
    track: "{colors.gray-alpha-200}"
    fill: "{colors.blue-700}"
    label: "{colors.gray-700}"

  # ── reading-maturation-badge: apps/web ──
  reading-maturation-badge:
    chapter-completion-state: "ChapterStatusBadge basis"
    world-kb-density-count: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    open-findings-count: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    base: { height: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }

  # ── SOUL visualization: apps/web ──
  soul-viz-keyword-cluster-node:
    shape: "circle"
    size: "min-max 10px-44px by frequency"
    fill: "rgba(124,58,237,0.18)"
    stroke: "{colors.purple-700}"
    label: "{colors.gray-1000}"
  soul-viz-timeline-axis:
    line: "{colors.gray-alpha-400}"
    tick: "{colors.gray-400}"
    label: "{typography.label-12} @ {colors.gray-700}"
  soul-viz-drift-band:
    fill: "rgba(0,107,255,0.16)"
    fill-2: "rgba(124,58,237,0.16)"
    fill-3: "rgba(0,133,119,0.16)"
    fill-4: "rgba(183,110,0,0.16)"
    fill-5: "rgba(190,24,93,0.16)"
    fill-6: "rgba(53,142,53,0.16)"
    step-stroke: "{colors.gray-alpha-200}"
    label: "{typography.label-12} @ {colors.gray-900}"
  soul-narrative-prose: "{typography.copy-16} @ {colors.gray-900}"
  soul-growth-curve-stroke: "{colors.purple-700}"

  # ── canvas: apps/web ──
  canvas:
    canvas-surface: "#ebebeb"
    canvas-grid: "rgba(0,0,0,0.05)"
    canvas-node-fill: "#ffffff"
    canvas-node-fill-hover: "#f5f5f5"
    canvas-node-border: "rgba(0,0,0,0.14)"
    canvas-node-border-selected: "{colors.blue-700}"
    canvas-edge: "{colors.gray-500}"
    canvas-edge-hover: "{colors.gray-800}"
    canvas-port: "{colors.gray-700}"
    canvas-minimap: "{colors.gray-alpha-600}"
    canvas-strategy-accent: "{colors.purple-700}"
    canvas-write-dirty: "{colors.amber-700}"
    canvas-write-conflict: "{colors.red-700}"
    canvas-write-success: "{colors.green-700}"
    canvas-write-stale-bg: "color-mix(in srgb, {colors.amber-700} 8%, transparent)"
    # Outline/timeline
    canvas-outline-volume-fill: "#F5F5F4"
    canvas-outline-chapter-card-status-pending: "#94A3B8"
    canvas-outline-chapter-card-status-drafted: "#3B82F6"
    canvas-outline-chapter-card-status-completed: "#10B981"
    canvas-outline-timeline-event-pin: "#F59E0B"
    canvas-outline-foreshadow-edge: "#A78BFA"
    canvas-outline-timeline-marker: "#0EA5E9"
    canvas-outline-conflict-marker: "#EF4444"
    # World KB
    canvas-worldkb-entity-card-fill-default: "#FFFFFF"
    canvas-worldkb-entity-card-fill-hover: "#F5F5F5"
    canvas-worldkb-entity-card-fill-selected: "#EBF2FF"
    canvas-worldkb-entity-card-stroke-default: "rgba(0,0,0,0.14)"
    canvas-worldkb-entity-card-stroke-selected: "{colors.blue-700}"
    canvas-worldkb-promotion-pending: "#F59E0B"
    canvas-worldkb-promotion-confirmed: "#10B981"
    canvas-worldkb-promotion-rejected: "#EF4444"
    canvas-worldkb-promotion-merged: "#8B5CF6"
    canvas-worldkb-source-anchor-edge: "#A78BFA"
    canvas-worldkb-source-anchor-node: "#EDE9FE"
    canvas-worldkb-computable-badge: "#0EA5E9"
    canvas-worldkb-conflict-marker: "#EF4444"
    canvas-worldkb-conflict-marker-fill: "rgba(239,68,68,0.10)"
    canvas-worldkb-nonspatial-row-highlight: "#F5F5F4"
    canvas-worldkb-focus-ring: "{colors.blue-700}"
    canvas-worldkb-relationship-edge: "#94A3B8"
    canvas-worldkb-relationship-edge-default: "#94A3B8"
    canvas-worldkb-relationship-edge-symmetric: "#8B5CF6"
    canvas-worldkb-relationship-edge-custom: "#DB2777"
    canvas-worldkb-relationship-confidence-low: "#E5484D"
    canvas-worldkb-relationship-confidence-mid: "#B76E00"
    canvas-worldkb-relationship-confidence-high: "#1F8F4D"
    canvas-worldkb-relationship-grounded-badge: "rgba(0,107,255,0.12)"
    canvas-worldkb-relationship-asserted-badge: "rgba(124,58,237,0.12)"
    canvas-worldkb-relationship-inspector-fill: "#FFFFFF"

  # ── reading annotations (V1.89): apps/web ──
  reading-annotation-highlight-yellow:
    backgroundColor: "color-mix(in srgb, {colors.amber-700} 18%, transparent)"
    textColor: "{colors.amber-1000}"
  reading-annotation-highlight-blue:
    backgroundColor: "color-mix(in srgb, {colors.blue-700} 12%, transparent)"
    textColor: "{colors.blue-1000}"
  reading-annotation-highlight-green:
    backgroundColor: "color-mix(in srgb, {colors.green-700} 16%, transparent)"
    textColor: "{colors.green-1000}"
  reading-annotation-highlight-pink:
    backgroundColor: "color-mix(in srgb, {colors.pink-700} 16%, transparent)"
    textColor: "{colors.pink-1000}"
  reading-annotation-inspector:
    width: "320px"
    backgroundColor: "{colors.background-100}"
    borderColor: "{colors.gray-alpha-400}"
    textColor: "{colors.gray-1000}"
    elevation: "shadow-elevation-3"
  reading-selection-toolbar:
    backgroundColor: "{colors.background-100}"
    borderColor: "{colors.gray-alpha-400}"
    textColor: "{colors.gray-1000}"
    shadow: "0px 4px 12px rgba(0,0,0,0.12)"

  # ── reading chrome (V1.91): apps/web ──
  reading-chrome-novel:
    chapter-title:
      fontFamily: "Georgia, 'Times New Roman', ui-serif, serif"
      fontSize: "28px"
      fontWeight: 700
      lineHeight: 1.3
      letterSpacing: "-0.01em"
      color: "{colors.gray-1000}"
    scene-separator:
      color: "{colors.gray-500}"
      fontSize: "16px"
      textAlign: "center"
      paddingBlock: "12px"
    epigraph:
      fontStyle: "italic"
      textAlign: "right"
      color: "{colors.gray-700}"
      paddingLeft: "25%"
  reading-chrome-essay:
    section-heading:
      fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, sans-serif"
      fontSize: "18px"
      fontWeight: 500
      lineHeight: 1.4
      letterSpacing: "-0.01em"
      color: "{colors.gray-1000}"
      marginTop: "28px"
    blockquote:
      borderLeft: "3px solid {colors.gray-alpha-400}"
      paddingLeft: "16px"
      fontStyle: "italic"
      color: "{colors.gray-900}"
    footnote-marker:
      verticalAlign: "super"
      fontSize: "0.75em"
      color: "{colors.teal-700}"
  reading-chrome-game-bible:
    term-link:
      color: "{colors.teal-700}"
      textDecoration: "underline dotted"
      cursor: "pointer"
    definition-callout:
      backgroundColor: "rgba(0,133,119,0.06)"
      borderLeft: "3px solid {colors.teal-700}"
      padding: "12px 16px"
      labelColor: "{colors.teal-900}"
      labelFontWeight: 600
    category-badge:
      backgroundColor: "rgba(183,110,0,0.12)"
      textColor: "{colors.amber-1000}"
  reading-chrome-script:
    character-name:
      textAlign: "center"
      textTransform: "uppercase"
      fontWeight: 700
      fontSize: "14px"
      letterSpacing: "0.08em"
      marginTop: "20px"
      color: "{colors.gray-1000}"
    parenthetical:
      fontStyle: "italic"
      color: "{colors.gray-700}"
      paddingLeft: "32px"
    scene-heading:
      fontWeight: 700
      textTransform: "uppercase"
      fontSize: "14px"
      letterSpacing: "0.05em"
      color: "{colors.gray-900}"
      marginTop: "24px"

  # ── footer-profile (V1.94): apps/web ──
  footer-profile:
    avatar-size: "32px"
    avatar-rounded: "{rounded.pill}"
    avatar-bg: "{colors.gray-alpha-100}"
    avatar-bg-hover: "{colors.gray-alpha-200}"
    avatar-bg-active: "{colors.blue-700}"
    avatar-text: "{colors.gray-1000}"
    avatar-text-active: "#ffffff"
    avatar-fallback-bg: "{colors.gray-alpha-200}"
    avatar-fallback-text: "{colors.gray-700}"
    add-button-bg: "transparent"
    add-button-border: "{colors.gray-alpha-400}"
    add-button-text: "{colors.gray-700}"
    add-button-hover-bg: "{colors.gray-alpha-100}"
    add-button-hover-border: "{colors.gray-alpha-500}"
    add-button-hover-text: "{colors.gray-1000}"
    gap: "{spacing.space-2}"

  # ── setup-wizard-step (V1.94): apps/web ──
  setup-wizard-step:
    step-row-height: "40px"
    step-circle-size: "32px"
    step-circle-active-bg: "{colors.blue-700}"
    step-circle-active-text: "#ffffff"
    step-circle-complete-bg: "{colors.green-700}"
    step-circle-complete-text: "#ffffff"
    step-circle-pending-bg: "{colors.gray-alpha-100}"
    step-circle-pending-text: "{colors.gray-700}"
    step-connector: "{colors.gray-alpha-400}"
    step-label-typography: "{typography.label-14}"
    step-label-active-color: "{colors.gray-1000}"
    step-label-pending-color: "{colors.gray-700}"
    wizard-max-width: "480px"
    wizard-max-height: "720px"
    wizard-padding: "{spacing.space-8}"

  # ── setup-wizard-surface (V1.96): apps/web ──
  setup-wizard-surface:
    card-bg: "{colors.background-100}"
    card-border: "{colors.gray-alpha-400}"
    card-shadow: "shadow-modal"
    card-rounded: "{rounded.popover}"
    step-panel-width: "208px"
    step-panel-right-divider: "{colors.gray-alpha-200}"
    step-panel-padding-x: "{spacing.space-6}"
    step-panel-padding-y: "{spacing.space-8}"
    content-panel-padding-x: "{spacing.space-10}"
    content-panel-padding-y: "{spacing.space-8}"
    input-row-gap: "{spacing.space-3}"
    input-row-min-height: "48px"
    input-row-bg: "{colors.background-200}"
    input-row-border: "{colors.gray-alpha-400}"
    input-row-rounded: "{rounded.control}"
    input-row-padding-x: "{spacing.space-4}"
    input-row-padding-y: "{spacing.space-3}"
    input-row-label-color: "{colors.gray-700}"
    input-row-path-color: "{colors.gray-1000}"
    input-row-icon-color: "{colors.blue-700}"
    cta-primary-max-width: "400px"
    cta-container-gap: "{spacing.space-4}"
    step-transition-duration: "duration-state"
---

# Nexus Design System

<!-- COMPLETENESS_LEVEL: 3 — Production, last audited 2026-07-08 -->

**This file is the sole normative design SSOT for all Nexus product surfaces.** It defines canonical token names, values, accessibility intent, logo rules, voice guidance, and component-level design tables for `apps/web`, `apps/design-studio`, and `@42ch/nexus-ui`. Dark theme uses the same token names with dark values in [`DESIGN.dark.md`](DESIGN.dark.md).

**Token hierarchy:**

1. Root `DESIGN.md` / `DESIGN.dark.md` — sole design SSOT (this file)
2. `packages/nexus-ui` (`@42ch/nexus-ui`) — package-consumable brand tokens, `theme.css`, logo assets
3. `tooling/design-tokens` (`@nexus/design-tokens`) — shared Tailwind preset + `tokens.css` for all consumers
4. `apps/web`, `apps/design-studio` — product surfaces that consume the CSS pipeline

**Author experience intent:** Nexus should feel like a calm, trustworthy creative workspace — recognizable from the shell, low-distraction, with clear action hierarchy. Brand visibility targets navigation identity, focus states, and base primitives — not full-page redesign.

Nexus Local Web UI is a restrained, author-focused design system for the local-first **Control Room + Setup + Authoring** SPA. It should feel calm and trustworthy: quiet surfaces, dense but readable data, explicit status language, and high-confidence controls for local creative runtime work without making writers feel like they are operating infrastructure.

Product inputs from `.mstar/specs/web-ui-design-requirements.md`:

- Primary persona: writers/authors, not engineers; calm and focused over dashboard anxiety.
- Control Room screens are data-dense; Setup screens are form-dense with first-class validation and destructive-action confirmation.
- Authoring screens include outline editing, chapter structure tables, and a body read-only context menu.
- WCAG 2.1 AA is the floor in both light and dark; focus rings, keyboard paths, status text, and reduced motion are non-negotiable.
- Brand voice: helpful, plain, local-first, and consistent with CLI terms (`Work`, `preset`, `stage`, `finding`, `capability`).

---

## Brand Colors

VI palette (frozen names):

| Token | Hex | Role |
| --- | --- | --- |
| `brand-deep-blue` | `#1E3A5F` | Primary brand, primary actions on light surfaces, links, focus rings |
| `brand-cyan` | `#25D1E0` | Accent — icons, active indicators, dark-theme interactive emphasis |
| `brand-white` | `#FFFFFF` | Text on deep blue fills, logo on dark hero surfaces |

Extended brand steps (`brand-deep-blue-800` … `brand-cyan-1000`, `brand-deep-blue-alpha-*`, `brand-cyan-alpha-*`) support hover, active, and low-opacity washes without raw VI drift.

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

### Background-driven contrast invariant

Text color on any filled element is decided by the **perceived lightness of that element's background**, not by the active light/dark mode. Dark backgrounds (deep blue, dark red, dark gray, saturated dark colors) use light text; light/bright backgrounds (cyan, light red, light green, pastel tints) use dark text. The same component may need dark text in both themes if its background stays light/bright in both.

---

## Colors

Color values live in frontmatter `colors:`. Color tokens follow the Geist-style intent scale: `100` background/quiet, `400` border, `700` solid fill, `900` secondary text, `1000` primary text. Use color for state and hierarchy, not decoration.

- Background values: see frontmatter `colors.background-*`. Background scale encodes surface hierarchy: `100` default, `200` subtle panel/table header, `300` hover/selected.
- Gray values: see frontmatter `colors.gray-*`. Solid gray carries text, icons, disabled fills, and opaque border fallback.
- Gray-alpha values: see frontmatter `colors.gray-alpha-*`. Alpha gray carries hover wash, separators, active wash, borders, and dividers over either theme.
- Accent values: see frontmatter `colors.blue-*`, `red-*`, `amber-*`, `green-*`, `teal-*`, `purple-*`, and `pink-*`. Accent color carries semantic state and should not be decorative.

### Semantic Mapping

| Meaning | Token |
| --- | --- |
| Primary action/focus/link | `blue-700` → maps to `brand-deep-blue` in light |
| Brand accent (icons, dark nav; not body text on white) | `brand-cyan` |
| Running/healthy/completed | `green-700` |
| Warning/stale/needs review | `amber-700` |
| Failed/error/destructive | `red-700` / `red-800` |
| Informational/queued | `teal-700` |
| Preset/capability metadata | `purple-700` |

### Brand → Web alias map (light)

| Root token | Web frontmatter key | CSS variable |
| --- | --- | --- |
| `brand-deep-blue` | `brand-deep-blue`, `blue-700` | `--color-brand-deep-blue`, `--color-blue-700` |
| `brand-deep-blue-800` | `blue-800` | `--color-blue-800` |
| `brand-deep-blue-900` | `blue-900` | `--color-blue-900` |
| `brand-cyan` | `brand-cyan` | `--color-brand-cyan` |
| `brand-white` | `brand-white` | `--color-brand-white` |
| Package mirror | — | `--nexus-brand-deep-blue` via `@42ch/nexus-ui/theme.css` |

`blue-*` keys are **preserved aliases** so existing component tokens (`{colors.blue-700}`) continue to resolve without renames.

---

## Neutrals

Neutral tokens (`background-*`, `gray-*`, `gray-alpha-*`) are refined for UI contrast and are not part of the VI brief. They carry surface hierarchy, borders, and text — brand colors carry identity and primary action.

---

## Typography

Typography values live in frontmatter `typography:`. Use a system stack by default so the UI works without webfont fetch. If a future build bundles Geist, map to the same token names. Prioritize long-session readability over visual novelty.

Font families:

- `font-sans`: `Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` for UI and prose.
- `font-mono`: `"SFMono-Regular", "Cascadia Code", "Roboto Mono", Consolas, monospace` for IDs, ports, code-like values, and tabular metrics.

Role intent:

- `heading-*`: page titles, view titles, card section titles, and dense section titles.
- `label-*`: form labels, nav labels, table headers, and badge labels.
- `copy-*`: primary body copy, default UI copy, and dense helper text.
- `button-*`: default and compact buttons.
- `*-mono`: IDs, schema versions, cursor values, and code-like inline values.
- `reading-prose-*`: Theme-independent reading-surface metrics (measure, line-height, paragraph-spacing) — identical values in light and dark.

Numeric columns use `font-variant-numeric: tabular-nums`.

---

## Spacing & Layout

Spacing values live in frontmatter `spacing:`. Base unit: **4px**. Prefer mechanical spacing over bespoke values.

### Rhythm

- `8px`: label + input, icon + text, badge + label.
- `16px`: related control groups and table toolbars.
- `24px`: card body padding and form section padding.
- `32–40px`: between major dashboard sections.

### Breakpoints

| Token | Width | Intent |
| --- | --- | --- |
| `sm` | `401px` | Small phones and up |
| `md` | `601px` | Large phones / narrow tablets |
| `lg` | `961px` | Desktop shell with sidebar |
| `xl` | `1200px` | Wide dashboard content |
| `2xl` | `1400px` | Dense admin displays |

### Layout Rules

- Use a fixed sidebar only at `lg` and above; collapse to top navigation below `lg`.
- Main content max width: `1200px`, with `24px` side padding on desktop and `16px` on mobile.
- Dashboard cards use 2 columns at `lg`, 3 columns only when the content remains readable.
- Tables must have horizontal overflow wrappers on narrow screens.

---

## Elevation

Hierarchy comes from borders and tonal surfaces first. Shadows are subtle and only clarify layers.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `shadow-card` | `0 1px 2px rgba(0,0,0,0.04)` | `0 1px 2px rgba(0,0,0,0.40)` | Raised dashboard cards |
| `shadow-popover` | `0 1px 1px rgba(0,0,0,0.03), 0 8px 24px -12px rgba(0,0,0,0.18)` | `0 1px 1px rgba(0,0,0,0.60), 0 12px 28px -12px rgba(0,0,0,0.70)` | Menus, tooltips, command panels |
| `shadow-modal` | `0 1px 1px rgba(0,0,0,0.04), 0 24px 48px -24px rgba(0,0,0,0.30)` | `0 1px 1px rgba(0,0,0,0.70), 0 28px 56px -24px rgba(0,0,0,0.85)` | Dialogs and blocking overlays |

---

## Motion

Motion clarifies state change; it is not decoration. Most dashboard interactions should feel instant.

| Token | Value | Use |
| --- | --- | --- |
| `duration-instant` | `0ms` | Table filtering, data refresh replacement |
| `duration-state` | `120ms` | Hover/focus/pressed states |
| `duration-popover` | `160ms` | Menus, dropdowns, tooltips |
| `duration-modal` | `220ms` | Dialog open/close |
| `ease-standard` | `cubic-bezier(0.16, 1, 0.3, 1)` | Default UI ease |
| `ease-emphasized` | `cubic-bezier(0.2, 0.8, 0.2, 1)` | Modal/panel enter |

Always honor `prefers-reduced-motion: reduce` by dropping nonessential transform/opacity transitions.

---

## Shapes

Radius values live in frontmatter `rounded:`. Radii stay tight and utility-oriented. Do not mix very rounded and sharp corners in a single view.

---

## Focus

All interactive elements expose `:focus-visible` using the two-layer ring defined in `components.focus-ring`:

```css
box-shadow:
  0 0 0 2px var(--color-background-100),
  0 0 0 4px var(--color-blue-700);
```

On brand-filled surfaces (deep blue button), invert the inner ring to `brand-white` or use `brand-cyan` outer ring only when contrast remains ≥ 3:1.

---

## Logo Usage

Canonical SVG assets ship from `@42ch/nexus-ui/assets/logos/`. PNG sources are provenance-only (Git LFS).

| Variant | File | Surface |
| --- | --- | --- |
| Deep blue mark | `logo-primary.svg` | Light nav, sidebar, light shell header |
| Cyan mark | `logo-color.svg` | Dark nav, dark shell header |
| White mark | `logo-white.svg` | Dark hero, photography overlays, high-contrast panels |
| Monotone mark | `logo-mono.svg` | Inline UI; inherits `color` via `currentColor` |

**Rules:**

- Minimum rendered height: **24px** (`logoMinSizePx`).
- Clear space: **≥ 25%** of logo height on all sides.
- Alt text: `Nexus` on `<img>`; inline SVGs include `<title>` and `<desc>`.
- Do not recolor SVG fills outside the mono variant.
- Do not place cyan mark on white/light gray without a contrast check — prefer `logo-primary.svg` on light surfaces.

---

## Voice & Content

- Helpful, plain, local-first — a careful CLI message translated into UI copy.
- Title Case for nav, buttons, page titles, action verbs; sentence case for helpers, errors, and toasts.
- Actions are **Verb + Noun**: `Create Work`, `Validate Preset`.
- Prefer author-facing nouns: `Work`, `preset`, `finding`, `local daemon`.
- Toasts name the changed object; no trailing period.
- Avoid protocol jargon (`ACP`, `cursor token`) in product surfaces unless diagnostics explicitly require it.

---

## Package Exposure (`@42ch/nexus-ui`)

| Root token | Package export | CSS variable |
| --- | --- | --- |
| `brand-deep-blue` | `brandColors.deepBlue` | `--nexus-brand-deep-blue` |
| `brand-cyan` | `brandColors.cyan` | `--nexus-brand-cyan` |
| `brand-white` | `brandColors.white` | `--nexus-brand-white` |
| Logo variants | `logoVariants.*` | Import SVG path from package exports |

**Import paths (stable public API):**

- `@42ch/nexus-ui` — token constants
- `@42ch/nexus-ui/tokens` — direct token module
- `@42ch/nexus-ui/theme.css` — brand CSS custom properties
- `@42ch/nexus-ui/assets/logos/*.svg` — logo assets

---

## Component Primitives

Component token values live in frontmatter `components:`. All components must expose visible `:focus-visible` styles using a two-layer ring.

### Button

Variants and sizes: see frontmatter `components.button`. The preset `Validate` action uses `primary` when it is the main form action, or `secondary` with a `blue-700` leading icon when paired with a separate save action.

#### Button Contrast Invariant (V1.94 corrected)

> **Background decides text color, independent of light/dark mode.**
>
> - **Dark background** (deep blue, red, dark gray, saturated dark colors) → **light/white text**.
> - **Light/bright background** (cyan, light gray, pastels) → **dark text**.
>
> Practical applications:
> - Light mode primary `bg-blue-700` (dark) → `text-white` (light).
> - Dark mode primary `dark:bg-brand-cyan` (light/bright) → `dark:text-brand-deep-blue` (dark).
> - Destructive `bg-red-800` (dark) → `text-white` (light), unchanged across modes.

**Dark primary token fix (V1.94):** `dark:bg-brand-cyan dark:text-brand-deep-blue` (was `dark:text-white`).

### Input / Select / Textarea

Variants: see frontmatter `components.input-select-textarea`. Textarea min height: `96px`. Placeholder uses `gray-700`. Helper text uses `copy-13`; error helper uses `red-700`.

### Card

Default, compact, and hero/status card values: see frontmatter `components.card`.

### Table

- Header: `background-200`, `label-12`, `gray-900`, bottom border `gray-alpha-400`.
- Rows: `copy-14`, primary text `gray-1000`, secondary `gray-900`; hover `background-200`; selected `background-300`.
- Use `label-12-mono` for IDs/cursors and tabular figures for numeric columns.
- Empty table row: sentence-case helper plus first action if applicable.

### Badge / Status Pill

Tone axis: `tone=soft|solid` (API on `@42ch/nexus-ui` Badge). Default is **soft** — omitting `tone` preserves soft callers (`StatusBadge` and other wrappers need no cutover).

- **Soft (default):** tinted fill + semantic text. Borders are strengthened for light-background readability: neutral uses `gray-alpha-400`; semantic variants use ~50% alpha borders (was ~30%).
- **Solid (opt-in):** solid semantic fill + high-contrast text; `border-transparent` (no visible border). Light solid fills are locked in frontmatter `components.badge-status-pill.solid` with `#ffffff` text. Dark solid follows the Button Contrast Invariant: bright semantic fills use `brand-deep-blue` text (white fails AA); dark neutral solid uses `gray-200` + white (see `DESIGN.dark.md`).

Variant × tone values: see frontmatter `components.badge-status-pill` (nested `.soft` / `.solid`). Base: height 24px, `px-2`, `rounded-pill`, `label-12`, `font-semibold`.

### Toast

Toast values: see frontmatter `components.toast`. Variants use the semantic accent on the leading icon/bar. Toasts name the changed object; no trailing period.

### Sidebar Nav

Sidebar values: see frontmatter `components.sidebar-nav`. Collapsed/mobile nav must keep labels accessible via text, not icon-only navigation.

### Dialog / Popover

Dialog/popover values: see frontmatter `components.dialog` and `components.popover`.

### Editor

The outline editor is a planning surface, not the body manuscript editor. It should feel closer to an intentional note/workbench than a document processor: compact toolbar, clear save state, and no hidden background writes.

Editor token values: see frontmatter `components.editor`.

| Element | Token use | Size / rhythm | States |
| --- | --- | --- | --- |
| Editor frame | `editor-surface`, `editor-border`, `rounded.card` | Min height `360px`; padding `space-6` | `:focus-within` swaps border to `editor-border-active` and uses global focus ring |
| Toolbar | `editor-surface-muted`, bottom border `editor-border` | Height `44px`; gap `space-1`; horizontal padding `space-2` | Sticky within editor panel if content scrolls |
| Toolbar button | `button-12`, `rounded.control` | `32px` square or min-width `32px` | hover `editor-toolbar-control-hover`; active `editor-toolbar-control-active` |
| Save-state indicator | `label-12`, semantic dot | Dot `8px`; gap `space-2` | `Saved` green, `Unsaved` amber, `Save failed` red; always include text, not color alone |
| Markdown helper | `copy-13`, `gray-900` | Footer padding `space-3` | Explain that body writing is read-only/deferred when relevant |

### Data Table

Chapter structure tables extend the base `Table` primitive with inline-edit and chapter-status semantics. Token values: see frontmatter `components.data-table`.

### Context Menu

Token values: see frontmatter `components.context-menu`. Actions: `Copy Path` only for body/outline path affordances.

---

## Setup Wizard Surface (V1.96 — Level 3 Production; V1.105 portrait amend)

The setup wizard is the user's first interaction with Nexus. V1.96 introduced a centered single-card surface with a left step rail. **V1.105 (P2)** reshapes the card to a fixed portrait geometry with top horizontal steps. All token values live in frontmatter `components.setup-wizard-surface` and `components.setup-wizard-step`. The dark theme shares the same token names with dark-tuned values in [`DESIGN.dark.md`](DESIGN.dark.md).

### Layout shell (V1.105)

- Portrait card: **480px** max width (`wizard-max-width` → `max-w-setup-wizard-step-wizard-max-width`), **min(720px, 85vh)** height (`wizard-max-height` → `h-setup-wizard-wizard-max-height` + `max-h-[85vh]`).
- **Top horizontal** step indicator (`TopStepIndicator`) above scrollable step content — labels **Agent** / **Workspace** / **Done** (`agent` / `workspace` / `done`); states `complete` / `active` / `pending` reuse `setup-wizard-step-circle-*` and `setup-wizard-step-label-*`; horizontal `flex` row with optional short connectors (`setup-wizard-step-connector` color).
- Step body: `flex-1 min-h-0 overflow-y-auto`; primary CTA stays bottom-anchored (`mt-auto` on `data-testid="wizard-cta-row"`).
- Left step rail (`step-panel-width` 208px / `w-setup-wizard-surface-step-panel-width`) **retired** for wizard chrome — do not reference `step-panel-*` or vertical connectors in P2 wizard layout.

### Layout shell (V1.96 legacy — superseded by V1.105 for wizard)

- The wizard card is centered in the viewport both horizontally and vertically.
- The step indicator list (left panel) and the current step's content area (right panel) live inside **one shared card chrome** container.

### Inline input row pattern (Workspace step)

The workspace location affordance is one tightly-coupled inline row (V1.105 step 2 — Workspace; historically step 1 Welcome):

- Folder icon (`input-row-icon-color`) + label (`input-row-label-color`) + path text (`input-row-path-color`) + Browse button.
- The entire row uses `input-row-bg` with `input-row-border` and `input-row-rounded`.
- Minimum row height: `48px` (`input-row-min-height`).

### Primary CTA

The Continue/Finish button has max-width `cta-primary-max-width`. Back button renders as a smaller secondary button adjacent, with `cta-container-gap` spacing.

---

## Author Reflection — Reading Surface (V1.79+)

Reading-surface tokens (`reading-prose-*`, `reading-chapter-nav`, `reading-progress-indicator`, `reading-maturation-badge`) define the prose column shape and chapter navigation chrome. All token values are in the frontmatter. Theme-independent metrics (`reading-prose-measure`, `reading-prose-line-height`, `reading-prose-paragraph-spacing`) keep the prose column shape stable across light/dark.

### Reading Chrome (V1.91)

Four work-profile-specific reading rendering tokens define profile-differentiated typography for read-only prose rendering. Token values in frontmatter `components.reading-chrome-novel/essay/game-bible/script`.

### Annotations (V1.89)

Four-color annotation highlight system + inspector/selection-toolbar chrome. Token values in frontmatter `components.reading-annotation-highlight-*`, `components.reading-annotation-inspector`, `components.reading-selection-toolbar`.

---

## SOUL Personality Visualization (V1.79–V1.81)

Keyword-cluster (network of nodes sized by frequency) and temporal-drift (stacked-band timeline) tokens define the Creator SOUL visualization surface. Token values in frontmatter `components.soul-viz-*` and `components.soul-narrative-prose` / `components.soul-growth-curve-stroke`.

---

## Canvas Surface (V1.70, V1.73)

Infinite-canvas graph primitives and World KB entity-card / promotion / relationship tokens define the canvas workspace. Token values in frontmatter `components.canvas.*`.

---

## Footer Profile Switcher (V1.94)

Sidebar footer avatar row tokens: see frontmatter `components.footer-profile`.

---

## Creator Memory (V1.79–V1.81)

Review-loop pending-count badge, task-kind chips, fragment browser chrome, and inspector tokens. Token values in frontmatter `components.memory-*`.

---

## Findings Remediation

6-state finding-status badges + triage chrome: see frontmatter `components.finding-status-pill` and `components.finding-triage`.

---

## Implementation Mapping

CSS variable tokens are projected from the frontmatter into `tooling/design-tokens/src/tokens.css`. Tailwind theme extensions live in `tooling/design-tokens/tailwind.preset.ts`. Both `apps/web` and `apps/design-studio` import the shared CSS pipeline via `@nexus/design-tokens/tokens.css` + the Tailwind preset.

**CSS variable naming convention:** `--color-<token-name>` (hyphenated). Example: `colors.blue-700` → `--color-blue-700`. Structural tokens (spacing, typography sizes) use `--<category>-<name>`.

**Theme toggle:** Tailwind `class` strategy — `.dark` class on `<html>` swaps color + shadow CSS variables; token names are identical in both themes.

**File references (post-V1.98 unification):**

- Design SSOT: [`DESIGN.md`](DESIGN.md) / [`DESIGN.dark.md`](DESIGN.dark.md) (repo root — this file)
- CSS vars + theme layers: `tooling/design-tokens/src/tokens.css`
- Tailwind preset: `tooling/design-tokens/tailwind.preset.ts`
- Brand package: `@42ch/nexus-ui/theme.css`

**Blue-* alias policy:** `blue-700` remains the web primary interactive token name in light theme (maps to `brand-deep-blue`). In dark theme, `blue-700` maps to `brand-cyan`. Component tokens using `{colors.blue-700}` continue to resolve correctly in both themes. Do **not** rename `blue-*` to `brand-deep-blue` in CSS vars.

---

## Intentional Drift Register (V1.98 merge)

| Token | Pre-merge (apps/web) | Post-merge (root) | Reason |
| --- | --- | --- | --- |
| *(none)* | — | — | apps/web neutrals/accent values preserved verbatim; no value changes |
