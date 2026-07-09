---
version: 0.3.0
name: "Nexus Design System"
description: "Unified design contract — dark theme. Same token names as DESIGN.md with dark-tuned values."

colors:
  # ── Brand core (VI palette — unchanged; usage shifts on dark surfaces) ──
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

  # ── Neutral surfaces (dark — apps/web parity) ──
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

  # ── Primary interactive scale (dark; maps to brand-cyan steps) ──
  blue-700: "#25D1E0"
  blue-800: "#3DD9E6"
  blue-900: "#5DE0EB"
  blue-1000: "#7FE8F0"

  # ── Semantic accent scales (dark — apps/web parity) ──
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
  # ── button: apps/web dark superset ──
  button:
    primary: { backgroundColor: "{colors.brand-cyan}", textColor: "{colors.brand-deep-blue}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.blue-800}", hoverTextColor: "{colors.brand-deep-blue}", activeBackgroundColor: "{colors.blue-900}", activeTextColor: "{colors.brand-deep-blue}" }
    secondary: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.background-200}", hoverBorderColor: "{colors.gray-alpha-500}" }
    tertiary: { backgroundColor: "transparent", textColor: "{colors.gray-1000}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.gray-alpha-100}" }
    destructive: { backgroundColor: "{colors.red-800}", textColor: "{colors.brand-deep-blue}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.red-700}", activeBackgroundColor: "{colors.red-900}" }
    sizes:
      small: { height: "32px", typography: "{typography.button-12}" }
      default: { height: "40px", typography: "{typography.button-14}" }
      large: { height: "48px", typography: "{typography.button-14}" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", cursor: "not-allowed" }

  # ── focus-ring: root dark ──
  focus-ring:
    outer: "{colors.brand-cyan}"
    inner: "{colors.background-100}"
    width-outer: "2px"
    width-inner: "4px"

  # ── input-select-textarea: apps/web dark ──
  input-select-textarea:
    default: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px" }
    error: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.red-700}", rounded: "{rounded.control}", height: "40px" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", borderColor: "{colors.gray-alpha-300}", rounded: "{rounded.control}", height: "40px" }
    textarea: { minHeight: "96px" }
    placeholder: { textColor: "{colors.gray-700}" }
    helperText: { typography: "{typography.copy-13}" }
    errorHelperText: { textColor: "{colors.red-700}", typography: "{typography.copy-13}" }

  # ── card: apps/web dark ──
  card:
    default: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.card}", padding: "{spacing.space-6}", shadow: "shadow-card" }
    compact: { padding: "{spacing.space-4}" }
    hero: { padding: "{spacing.space-8}" }

  # ── table: apps/web dark ──
  table:
    header: { backgroundColor: "{colors.background-200}", typography: "{typography.label-12}", textColor: "{colors.gray-900}", borderBottomColor: "{colors.gray-alpha-400}" }
    row: { typography: "{typography.copy-14}", textColor: "{colors.gray-1000}", secondaryTextColor: "{colors.gray-900}", hoverBackgroundColor: "{colors.background-200}", selectedBackgroundColor: "{colors.background-300}" }
    idText: { typography: "{typography.label-12-mono}" }

  # ── badge-status-pill: apps/web dark (tone=soft|solid; default soft) ──
  badge-status-pill:
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}", fontWeight: 600 }
    # soft (default): tinted fill + semantic text; strengthened borders (neutral gray-alpha-400; semantic ~50% alpha)
    soft:
      neutral: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-400}" }
      running: { backgroundColor: "rgba(84,213,138,0.14)", textColor: "{colors.green-1000}", borderColor: "rgba(84,213,138,0.50)" }
      queued: { backgroundColor: "rgba(76,216,200,0.14)", textColor: "{colors.teal-1000}", borderColor: "rgba(76,216,200,0.50)" }
      warning: { backgroundColor: "rgba(255,192,67,0.16)", textColor: "{colors.amber-1000}", borderColor: "rgba(255,192,67,0.50)" }
      error: { backgroundColor: "rgba(255,107,107,0.16)", textColor: "{colors.red-1000}", borderColor: "rgba(255,107,107,0.50)" }
      preset: { backgroundColor: "rgba(183,148,255,0.12)", textColor: "{colors.purple-1000}", borderColor: "rgba(183,148,255,0.50)" }
    # solid (opt-in): semantic fills + AA text per Button Contrast Invariant.
    # Bright dark *-700/*-800 fills use brand-deep-blue (white fails AA ~1.6–2.4:1).
    # Neutral uses dark gray-200 + white (~14:1). No visible border.
    solid:
      neutral: { backgroundColor: "{colors.gray-200}", textColor: "#ffffff", borderColor: "transparent" }
      running: { backgroundColor: "{colors.green-700}", textColor: "{colors.brand-deep-blue}", borderColor: "transparent" }
      queued: { backgroundColor: "{colors.teal-700}", textColor: "{colors.brand-deep-blue}", borderColor: "transparent" }
      warning: { backgroundColor: "{colors.amber-700}", textColor: "{colors.brand-deep-blue}", borderColor: "transparent" }
      error: { backgroundColor: "{colors.red-800}", textColor: "{colors.brand-deep-blue}", borderColor: "transparent" }
      preset: { backgroundColor: "{colors.purple-700}", textColor: "{colors.brand-deep-blue}", borderColor: "transparent" }

  # ── toast: apps/web dark ──
  toast: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", maxWidth: "360px", titleTypography: "{typography.label-14}", bodyTypography: "{typography.copy-13}" }

  # ── sidebar-nav: apps/web dark ──
  sidebar-nav: { width: "248px", backgroundColor: "{colors.background-100}", dividerColor: "{colors.gray-alpha-400}", itemHeight: "36px", itemRounded: "{rounded.control}", itemTypography: "{typography.label-14}", activeBackgroundColor: "{colors.gray-alpha-100}", activeTextColor: "{colors.gray-1000}", activeBarColor: "{colors.blue-700}" }

  # ── dialog / popover: apps/web dark ──
  dialog: { backgroundColor: "{colors.background-100}", rounded: "{rounded.popover}", shadow: "shadow-modal", maxWidth: "560px", padding: "{spacing.space-6}" }
  popover: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", itemHeight: "36px" }

  # ── editor: apps/web dark ──
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

  # ── data-table: apps/web dark ──
  data-table:
    row-hover: "{colors.background-200}"
    row-selected: "{colors.background-300}"
    row-edited: "rgba(255,192,67,0.14)"
    row-protected: "rgba(183,148,255,0.12)"
    cell-edit-bg: "{colors.background-100}"
    cell-edit-border: "{colors.blue-700}"
    column-divider: "{colors.gray-alpha-200}"

  # ── context-menu: apps/web dark ──
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

  # ── desktop-window-chrome / app-menu / native-dialogs / daemon-status: apps/web dark ──
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

  # ── shell-nav / logo: root dark ──
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

  # ── connection-setup: root dark ──
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

  # ── finding-status-pill: apps/web dark ──
  finding-status-pill:
    open: { backgroundColor: "rgba(255,192,67,0.16)", textColor: "{colors.amber-1000}", borderColor: "rgba(255,192,67,0.30)" }
    triaged: { backgroundColor: "rgba(76,216,200,0.14)", textColor: "{colors.teal-1000}", borderColor: "rgba(76,216,200,0.30)" }
    in_review: { backgroundColor: "rgba(82,168,255,0.14)", textColor: "{colors.blue-1000}", borderColor: "rgba(82,168,255,0.30)" }
    resolved: { backgroundColor: "rgba(84,213,138,0.14)", textColor: "{colors.green-1000}", borderColor: "rgba(84,213,138,0.30)" }
    wont_fix: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-300}" }
    duplicate: { backgroundColor: "rgba(183,148,255,0.14)", textColor: "{colors.purple-1000}", borderColor: "rgba(183,148,255,0.30)" }
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }

  # ── finding-triage: apps/web dark ──
  finding-triage:
    panel-bg: "{colors.background-100}"
    panel-border: "{colors.gray-alpha-400}"
    row-active: "{colors.background-300}"
    action-button: "secondary"
    executor-select: "input-select-textarea.default"

  # ── memory: apps/web dark ──
  memory-pending-count:
    backgroundColor: "rgba(248,113,113,0.16)"
    textColor: "{colors.red-1000}"
    borderColor: "rgba(248,113,113,0.30)"
    base: { height: "20px", minInlineSize: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  memory-review-button:
    basis: "primary"
  memory-task-kind-brainstorm:
    backgroundColor: "rgba(255,192,67,0.16)"
    textColor: "{colors.amber-1000}"
    borderColor: "rgba(255,192,67,0.30)"
  memory-task-kind-outline:
    backgroundColor: "rgba(82,168,255,0.14)"
    textColor: "{colors.blue-1000}"
    borderColor: "rgba(82,168,255,0.30)"
  memory-task-kind-chapter:
    backgroundColor: "rgba(76,216,200,0.14)"
    textColor: "{colors.teal-1000}"
    borderColor: "rgba(76,216,200,0.30)"
  memory-task-kind-research:
    backgroundColor: "rgba(183,148,255,0.14)"
    textColor: "{colors.purple-1000}"
    borderColor: "rgba(183,148,255,0.30)"
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

  # ── reading-chapter-nav / reading-progress / maturation-badge: apps/web dark ──
  reading-chapter-nav:
    chrome-bg: "{colors.background-200}"
    chrome-border: "{colors.gray-alpha-400}"
    control-prev: "button.secondary basis"
    control-next: "button.secondary basis"
    volume-group-bg: "{colors.background-300}"
    volume-group-border: "{colors.gray-alpha-300}"
  reading-progress-indicator:
    track: "{colors.gray-alpha-200}"
    fill: "{colors.blue-700}"
    label: "{colors.gray-700}"
  reading-maturation-badge:
    chapter-completion-state: "ChapterStatusBadge basis"
    world-kb-density-count: { backgroundColor: "rgba(76,216,200,0.14)", textColor: "{colors.teal-1000}", borderColor: "rgba(76,216,200,0.30)" }
    open-findings-count: { backgroundColor: "rgba(255,192,67,0.16)", textColor: "{colors.amber-1000}", borderColor: "rgba(255,192,67,0.30)" }
    base: { height: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }

  # ── SOUL visualization: apps/web dark ──
  soul-viz-keyword-cluster-node:
    shape: "circle"
    size: "min-max 10px-44px by frequency"
    fill: "rgba(183,148,255,0.20)"
    stroke: "{colors.purple-700}"
    label: "{colors.gray-1000}"
  soul-viz-timeline-axis:
    line: "{colors.gray-alpha-400}"
    tick: "{colors.gray-500}"
    label: "{typography.label-12} @ {colors.gray-700}"
  soul-viz-drift-band:
    fill: "rgba(82,168,255,0.22)"
    fill-2: "rgba(183,148,255,0.22)"
    fill-3: "rgba(76,216,200,0.22)"
    fill-4: "rgba(255,192,67,0.22)"
    fill-5: "rgba(255,108,162,0.22)"
    fill-6: "rgba(134,204,108,0.22)"
    step-stroke: "{colors.gray-alpha-200}"
    label: "{typography.label-12} @ {colors.gray-900}"
  soul-narrative-prose: "{typography.copy-16} @ {colors.gray-900}"
  soul-growth-curve-stroke: "{colors.purple-700}"

  # ── canvas: apps/web dark ──
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
    canvas-write-stale-bg: "color-mix(in srgb, {colors.amber-700} 12%, transparent)"
    canvas-outline-volume-fill: "#1F1F1E"
    canvas-outline-chapter-card-status-pending: "#64748B"
    canvas-outline-chapter-card-status-drafted: "#60A5FA"
    canvas-outline-chapter-card-status-completed: "#34D399"
    canvas-outline-timeline-event-pin: "#FBBF24"
    canvas-outline-foreshadow-edge: "#C4B5FD"
    canvas-outline-timeline-marker: "#38BDF8"
    canvas-outline-conflict-marker: "#F87171"
    canvas-worldkb-entity-card-fill-default: "#1A1A1A"
    canvas-worldkb-entity-card-fill-hover: "#2A2A2A"
    canvas-worldkb-entity-card-fill-selected: "rgba(82,168,255,0.14)"
    canvas-worldkb-entity-card-stroke-default: "rgba(255,255,255,0.18)"
    canvas-worldkb-entity-card-stroke-selected: "{colors.blue-700}"
    canvas-worldkb-promotion-pending: "#FBBF24"
    canvas-worldkb-promotion-confirmed: "#34D399"
    canvas-worldkb-promotion-rejected: "#F87171"
    canvas-worldkb-promotion-merged: "#A78BFA"
    canvas-worldkb-source-anchor-edge: "#C4B5FD"
    canvas-worldkb-source-anchor-node: "#2A2440"
    canvas-worldkb-computable-badge: "#38BDF8"
    canvas-worldkb-conflict-marker: "#F87171"
    canvas-worldkb-conflict-marker-fill: "rgba(248,113,113,0.12)"
    canvas-worldkb-nonspatial-row-highlight: "#1F1F1E"
    canvas-worldkb-focus-ring: "{colors.blue-700}"
    canvas-worldkb-relationship-edge: "#64748B"
    canvas-worldkb-relationship-edge-default: "#64748B"
    canvas-worldkb-relationship-edge-symmetric: "#A78BFA"
    canvas-worldkb-relationship-edge-custom: "#F472B6"
    canvas-worldkb-relationship-confidence-low: "#FF6B6B"
    canvas-worldkb-relationship-confidence-mid: "#FFC043"
    canvas-worldkb-relationship-confidence-high: "#54D58A"
    canvas-worldkb-relationship-grounded-badge: "rgba(82,168,255,0.14)"
    canvas-worldkb-relationship-asserted-badge: "rgba(183,148,255,0.14)"
    canvas-worldkb-relationship-inspector-fill: "#1A1A1A"

  # ── annotation highlights / inspector / selection-toolbar: apps/web dark ──
  reading-annotation-highlight-yellow:
    backgroundColor: "color-mix(in srgb, {colors.amber-700} 22%, transparent)"
    textColor: "{colors.amber-1000}"
  reading-annotation-highlight-blue:
    backgroundColor: "color-mix(in srgb, {colors.blue-700} 20%, transparent)"
    textColor: "{colors.blue-1000}"
  reading-annotation-highlight-green:
    backgroundColor: "color-mix(in srgb, {colors.green-700} 20%, transparent)"
    textColor: "{colors.green-1000}"
  reading-annotation-highlight-pink:
    backgroundColor: "color-mix(in srgb, {colors.pink-700} 20%, transparent)"
    textColor: "{colors.pink-1000}"
  reading-annotation-inspector:
    width: "320px"
    backgroundColor: "{colors.background-100}"
    borderColor: "{colors.gray-alpha-400}"
    textColor: "{colors.gray-1000}"
    elevation: "shadow-elevation-3"
  reading-selection-toolbar:
    backgroundColor: "{colors.background-200}"
    borderColor: "{colors.gray-alpha-500}"
    textColor: "{colors.gray-1000}"
    shadow: "0px 4px 12px rgba(0,0,0,0.40)"

  # ── reading chrome (V1.91): apps/web dark ──
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
      backgroundColor: "rgba(0,133,119,0.08)"
      borderLeft: "3px solid {colors.teal-700}"
      padding: "12px 16px"
      labelColor: "{colors.teal-900}"
      labelFontWeight: 600
    category-badge:
      backgroundColor: "rgba(183,110,0,0.14)"
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

  # ── footer-profile (V1.94): apps/web dark ──
  footer-profile:
    avatar-size: "32px"
    avatar-rounded: "{rounded.pill}"
    avatar-bg: "{colors.gray-alpha-100}"
    avatar-bg-hover: "{colors.gray-alpha-200}"
    avatar-bg-active: "{colors.blue-700}"
    avatar-text: "{colors.gray-1000}"
    avatar-text-active: "{colors.brand-deep-blue}"
    avatar-fallback-bg: "{colors.gray-alpha-200}"
    avatar-fallback-text: "{colors.gray-700}"
    add-button-bg: "transparent"
    add-button-border: "{colors.gray-alpha-400}"
    add-button-text: "{colors.gray-700}"
    add-button-hover-bg: "{colors.gray-alpha-100}"
    add-button-hover-border: "{colors.gray-alpha-500}"
    add-button-hover-text: "{colors.gray-1000}"
    gap: "{spacing.space-2}"

  # ── setup-wizard-step (V1.94): apps/web dark ──
  setup-wizard-step:
    step-row-height: "40px"
    step-circle-size: "32px"
    step-circle-active-bg: "{colors.blue-700}"
    step-circle-active-text: "{colors.brand-deep-blue}"
    step-circle-complete-bg: "{colors.green-700}"
    step-circle-complete-text: "{colors.brand-deep-blue}"
    step-circle-pending-bg: "{colors.gray-alpha-100}"
    step-circle-pending-text: "{colors.gray-700}"
    step-connector: "{colors.gray-alpha-400}"
    step-label-typography: "{typography.label-14}"
    step-label-active-color: "{colors.gray-1000}"
    step-label-pending-color: "{colors.gray-700}"
    wizard-max-width: "480px"
    wizard-max-height: "720px"
    wizard-padding: "{spacing.space-8}"

  # ── setup-wizard-surface (V1.96): apps/web dark ──
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

# Nexus Design System — Dark Theme

Dark-theme companion to [`DESIGN.md`](DESIGN.md). Same token names; values tuned for dark surfaces. Rule-type documentation, component behavior, voice/content guidance, and implementation mapping live in `DESIGN.md` and apply to both themes.

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

**Background-driven contrast invariant:** Text color on any filled element is decided by the **perceived lightness of that element's background**, not by the active light/dark mode. In the dark theme, bright accent fills (e.g. `brand-cyan`, `red-800`, `green-700`) become light/bright surfaces and must use dark text (`brand-deep-blue`) instead of white.

**Logo:** use `logo-color.svg` (cyan) in dark nav/sidebar; `logo-white.svg` on photography or deepest panels.

### Brand → Web alias map (dark)

| Root token | Web frontmatter key | Notes |
| --- | --- | --- |
| `brand-cyan` | `brand-cyan`, `blue-700` | Primary interactive, focus ring, nav accent |
| `brand-cyan-800` | `blue-800` | Hover |
| `brand-deep-blue` | `brand-deep-blue` | Logo text; primary button text on cyan fill |
| `brand-white` | `brand-white` | Logo on deepest panels |

Dark primary button uses **cyan fill + deep-blue text** (V1.94 contrast correction; was white-on-cyan). `blue-*` names preserved for existing `{colors.blue-700}` consumers.

Canvas/SOUL/World-KB brand-blue tokens resolve through `var(--color-blue-700)` and `color-mix(in srgb, var(--color-blue-700) N%, transparent)`. In the dark theme `--color-blue-700: #25d1e0` (brand-cyan). See `DESIGN.md` § Implementation Mapping.

This file intentionally preserves the same token names and frontmatter structure with dark values. Rule-type documentation, component behavior, voice/content guidance, and implementation mapping live in `DESIGN.md` and apply to both themes.
