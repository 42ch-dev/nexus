---
version: 0.1.0
name: "Nexus Local Web UI"
description: "Nexus Local Web UI is the light/default theme for the local-first Control Room, Setup, and Authoring SPA. YAML frontmatter is the light-token SSOT; the dark theme uses the same token names with dark values in DESIGN.dark.md."

colors:
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
  blue-700: "#006bff"
  blue-800: "#0057d9"
  blue-900: "#0046ad"
  blue-1000: "#003680"
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
  # V1.79 Author Reflection — reading-surface typography (P0 concrete values).
  # Theme-independent metrics (a reading measure is a line-length target, not a
  # color); values are identical in DESIGN.dark.md so the prose column shape does
  # not shift between themes. Consumed via CSS vars in index.css.
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
    running: { backgroundColor: "rgba(31,143,77,0.10)", textColor: "{colors.green-1000}", borderColor: "rgba(31,143,77,0.30)" }
    queued: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    warning: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    error: { backgroundColor: "rgba(229,72,77,0.12)", textColor: "{colors.red-1000}", borderColor: "rgba(229,72,77,0.30)" }
    preset: { backgroundColor: "rgba(124,58,237,0.10)", textColor: "{colors.purple-1000}", borderColor: "rgba(124,58,237,0.30)" }
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
    selection: "rgba(0,107,255,0.14)"
  data-table:
    row-hover: "{colors.background-200}"
    row-selected: "{colors.background-300}"
    row-edited: "rgba(183,110,0,0.08)"
    row-protected: "rgba(124,58,237,0.06)"
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
    healthy-bg: "rgba(31,143,77,0.10)"
    healthy-text: "{colors.green-1000}"
    starting-bg: "rgba(0,133,119,0.10)"
    starting-text: "{colors.teal-1000}"
    degraded-bg: "rgba(183,110,0,0.12)"
    degraded-text: "{colors.amber-1000}"
    stopped-bg: "rgba(229,72,77,0.12)"
    stopped-text: "{colors.red-1000}"

  # V1.77 findings-remediation — 6-state finding-status badges + triage chrome.
  # Severity reuses the existing `severityVariant` mapping (no new severity
  # tokens); triage chrome (inspector panel, action buttons, assignment
  # selector, inline-edit inputs) composes the existing card/button/input/
  # data-table primitives. Token names frozen verbatim (V1.69 invariant).
  finding-status-pill:
    open: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    triaged: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    in_review: { backgroundColor: "rgba(0,107,255,0.10)", textColor: "{colors.blue-1000}", borderColor: "rgba(0,107,255,0.30)" }
    resolved: { backgroundColor: "rgba(31,143,77,0.10)", textColor: "{colors.green-1000}", borderColor: "rgba(31,143,77,0.30)" }
    wont_fix: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-300}" }
    duplicate: { backgroundColor: "rgba(124,58,237,0.10)", textColor: "{colors.purple-1000}", borderColor: "rgba(124,58,237,0.30)" }
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  finding-triage:
    panel-bg: "{colors.background-100}"
    panel-border: "{colors.gray-alpha-400}"
    row-active: "{colors.background-300}"
    action-button: "secondary"
    executor-select: "input-select-textarea.default"

  # V1.78 Creator Memory review-loop — pending-count badge, task-kind chips,
  # fragment browser chrome, and inspector tokens. Concrete colors use the same
  # `color-mix` low-opacity + matching-text pattern as the V1.77 findings-status
  # badges. Composition tokens (review-button, fragment-summary, fragment-id,
  # inspector chrome, fragment-filter-input) reference existing primitives so
  # the surface stays discoverable without duplicating primitive values. Token
  # names frozen verbatim (V1.69 invariant continues).
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

  # V1.79 Author Reflection — Track A/B token stubs only (names + structure).
  # Concrete light values land in P0 (reading surface) and P1 (SOUL viz).
  # V1.79 Author Reflection — Track A reading-surface component tokens (P0
  # concrete light values). Token names frozen verbatim (V1.69 invariant
  # continues); dark values live in DESIGN.dark.md under the same names.
  # Composition tokens reference existing primitives so the surface composes
  # card/button/badge semantics without duplicating primitive values. Track B
  # (soul-viz-*) stubs remain for P1 and are NOT filled here.
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
    world-kb-density-count: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    open-findings-count: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    base: { height: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  soul-viz-keyword-cluster-node:
    shape: "TODO-V1.79-light-soul-viz-keyword-cluster-node-shape"
    size: "TODO-V1.79-light-soul-viz-keyword-cluster-node-size"
    fill: "TODO-V1.79-light-soul-viz-keyword-cluster-node-fill"
    stroke: "TODO-V1.79-light-soul-viz-keyword-cluster-node-stroke"
    label: "TODO-V1.79-light-soul-viz-keyword-cluster-node-label"
  soul-viz-timeline-axis:
    line: "TODO-V1.79-light-soul-viz-timeline-axis-line"
    tick: "TODO-V1.79-light-soul-viz-timeline-axis-tick"
    label: "TODO-V1.79-light-soul-viz-timeline-axis-label"
  soul-viz-drift-band:
    fill: "TODO-V1.79-light-soul-viz-drift-band-fill"
    step-stroke: "TODO-V1.79-light-soul-viz-drift-band-step-stroke"
    label: "TODO-V1.79-light-soul-viz-drift-band-label"

  # V1.70 canvas implement — concrete light values (canvas-strategy-surface.md Draft §3.6 / B4)
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
    # V1.72 outline/timeline canvas-write tokens — concrete light values (locked names, verbatim)
    canvas-outline-volume-fill: "#F5F5F4"
    canvas-outline-chapter-card-status-pending: "#94A3B8"
    canvas-outline-chapter-card-status-drafted: "#3B82F6"
    canvas-outline-chapter-card-status-completed: "#10B981"
    canvas-outline-timeline-event-pin: "#F59E0B"
    canvas-outline-foreshadow-edge: "#A78BFA"
    canvas-outline-timeline-marker: "#0EA5E9"
    canvas-outline-conflict-marker: "#EF4444"
    # V1.73 World KB canvas-write tokens — concrete light values (locked names, verbatim; 17 tokens)
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
---

# Nexus Local Web UI Design System

<!-- COMPLETENESS_LEVEL: 3 — Production, last audited 2026-06-27 -->

Nexus Local Web UI is a restrained, author-focused design system for the local-first **Control Room + Setup + Authoring** SPA. It should feel calm and trustworthy: quiet surfaces, dense but readable data, explicit status language, and high-confidence controls for local creative runtime work without making writers feel like they are operating infrastructure.

This file is the light/default theme and the token-value SSOT through the YAML frontmatter above. The Dark theme lives at [`DESIGN.dark.md`](DESIGN.dark.md) with the same token names and dark values. The Markdown body below is supplementary documentation: usage intent, interaction rules, content rules, and implementation mapping.

Product inputs from `.mstar/knowledge/specs/web-ui-design-requirements.md`:

- Primary persona: writers/authors, not engineers; calm and focused over dashboard anxiety.
- Control Room screens are data-dense; Setup screens are form-dense with first-class validation and destructive-action confirmation.
- V1.65 Authoring screens add outline editing, chapter structure tables, and a body read-only context menu. Browser V1.65 ships `Copy path` only, while `Open with` / `Reveal in file manager` wait for the V1.66 Tauri shell.
- WCAG 2.1 AA is the floor in both light and dark; focus rings, keyboard paths, status text, and reduced motion are non-negotiable.
- Brand voice: helpful, plain, local-first, and consistent with CLI terms (`Work`, `preset`, `stage`, `finding`, `capability`).

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
| Primary action/focus/link | `blue-700` |
| Running/healthy/completed | `green-700` |
| Warning/stale/needs review | `amber-700` |
| Failed/error/destructive | `red-700` / `red-800` |
| Informational/queued | `teal-700` |
| Preset/capability metadata | `purple-700` |

---

## Typography

Typography values live in frontmatter `typography:`. Use a system stack by default so the UI works without webfont fetch. If a future build bundles Geist, map `font-sans` to Geist Sans and `font-mono` to Geist Mono with the same token names. Prioritize long-session readability over visual novelty.

Font families:

- `font-sans`: `Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` for UI and prose.
- `font-mono`: `"SFMono-Regular", "Cascadia Code", "Roboto Mono", Consolas, monospace` for IDs, ports, code-like values, and tabular metrics.

Role intent:

- `heading-*`: page titles, view titles, card section titles, and dense section titles.
- `label-*`: form labels, nav labels, table headers, and badge labels.
- `copy-*`: primary body copy, default UI copy, and dense helper text.
- `button-*`: default and compact buttons.
- `*-mono`: IDs, schema versions, cursor values, and code-like inline values.

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

Radius values live in frontmatter `rounded:`. The previous `radius-*` semantic identity is preserved as `rounded.control/card/popover/fullscreen/pill`.

Radii stay tight and utility-oriented. Do not mix very rounded and sharp corners in a single view.

---

## Component Primitives

Component token values live in frontmatter `components:`. All components must expose visible `:focus-visible` styles using a two-layer ring: `0 0 0 2px var(--color-background-100), 0 0 0 4px var(--color-blue-700)`.

### Button

Variants and sizes: see frontmatter `components.button`. The preset `Validate` action uses `primary` when it is the main form action, or `secondary` with a `blue-700` leading icon when paired with a separate save action. It must read as reassurance (“is this safe?”), not as a debug-only tool.

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

Variant values: see frontmatter `components.badge-status-pill`.

### Toast

Toast values: see frontmatter `components.toast`. Variants use the semantic accent on the leading icon/bar. Toasts name the changed object; no trailing period.

### Sidebar Nav

Sidebar values: see frontmatter `components.sidebar-nav`. Collapsed/mobile nav must keep labels accessible via text, not icon-only navigation.

### Dialog / Popover

Dialog/popover values: see frontmatter `components.dialog` and `components.popover`.

### Editor (V1.65 Standard+)

The outline editor is a planning surface, not the body manuscript editor. It should feel closer to an intentional note/workbench than a document processor: compact toolbar, clear save state, and no hidden background writes.

Editor token values: see frontmatter `components.editor`.

| Element | Token use | Size / rhythm | States |
| --- | --- | --- | --- |
| Editor frame | `editor-surface`, `editor-border`, `rounded.card` | Min height `360px`; padding `space-6` | `:focus-within` swaps border to `editor-border-active` and uses global focus ring |
| Toolbar | `editor-surface-muted`, bottom border `editor-border` | Height `44px`; gap `space-1`; horizontal padding `space-2` | Sticky within editor panel if content scrolls |
| Toolbar button | `button-12`, `rounded.control` | `32px` square or min-width `32px` | hover `editor-toolbar-control-hover`; active `editor-toolbar-control-active` |
| Save-state indicator | `label-12`, semantic dot | Dot `8px`; gap `space-2` | `Saved` green, `Unsaved` amber, `Save failed` red; always include text, not color alone |
| Markdown helper | `copy-13`, `gray-900` | Footer padding `space-3` | Explain that body writing is read-only/deferred when relevant |

Editor content typography:

- Prose defaults to `copy-16`; headings use `heading-24` / `heading-20` / `heading-16` in descending order.
- Lists use `space-2` vertical rhythm and `space-6` indentation.
- Inline code uses `copy-13-mono`, `gray-alpha-100` background, `rounded.control`, horizontal padding `4px`.
- Unknown markdown/frontmatter preservation warnings use `amber-700` icon + `copy-13` text.

### Data Table (V1.65 Standard+)

Chapter structure tables extend the base `Table` primitive with inline-edit and chapter-status semantics. Token values: see frontmatter `components.data-table`.

Chapter status badge mapping:

| Chapter status | Badge variant | Notes |
| --- | --- | --- |
| `not_started` | `neutral` | Quiet default |
| `outlined` | `queued` | Informational planning-ready state |
| `draft` | `warning` | Body exists / may need review |
| `finalized` | `running` | Positive terminal-ish local state; pair with lock/protection copy when edited |
| `published` | `preset` | Rare protected/public state; hard-block destructive edits |

Inline edit rules:

- Editable cells keep row height at `44px` minimum; controls use `32px` height and `copy-14`.
- Numeric columns (`planned_word_count`, `actual_word_count`, chapter number, volume) use tabular figures and right alignment.
- Save/cancel controls appear at row end; use icon + accessible label. Do not rely on hover-only controls for keyboard users.
- Validation errors render under the edited cell in `copy-13` + `red-700`; row remains in `table-row-edited` until resolved or canceled.
- `finalized` edits require an explicit confirmation dialog; `published` edits surface a hard-block message.

### Context Menu (V1.65 Standard+)

The V1.65 browser context menu is intentionally narrow: **Copy path** only for body/outline path affordances. Native `Open with` and `Reveal in file manager` are V1.66 Tauri-shell capabilities. Token values: see frontmatter `components.context-menu`.

| Element | Token use | Size / rhythm | States |
| --- | --- | --- | --- |
| Menu surface | `context-menu-bg`, `context-menu-border`, `shadow-popover`, `rounded.popover` | Min width `180px`; padding `space-1` | Opens near pointer/focused row; closes on Escape |
| Menu item | `copy-14`, `gray-1000` | Height `36px`; horizontal padding `space-3`; gap `space-2` | hover/focus `context-menu-item-hover`; active `context-menu-item-active` |
| Path preview | `copy-13-mono`, `gray-900` | Max width `320px`, truncates middle when needed | Read-only; never expose absolute path unless API returns it intentionally |

Copy-path behavior:

- The action label is `Copy Path`; success toast is `Path copied`.
- If clipboard write fails, show `Path not copied. Copy it manually from the details panel.`
- Menu items must be keyboard reachable from the row/body read-only surface.

---

## Canvas Surface (V1.70 placeholder)

Canvas token names are stubbed in frontmatter `components.canvas` as commented LEVEL placeholders. They are not consumed in V1.69. The canonical Draft list lives in `.mstar/knowledge/specs/canvas-strategy-surface.md` §3.6 / B4 and gives V1.70 a reviewed target for infinite-canvas surfaces.

Minimal placeholder set: `canvas-surface`, `canvas-grid`, `canvas-node-fill`, `canvas-node-fill-hover`, `canvas-node-border`, `canvas-node-border-selected`, `canvas-edge`, `canvas-edge-hover`, `canvas-port`, `canvas-minimap`, `canvas-strategy-accent`.

### Outline & Timeline Canvas Tokens (V1.72)

The V1.72 outline/timeline canvas-write tokens extend `components.canvas` with concrete light + dark values for the volume-lane, chapter-card status, timeline, foreshadow-edge, and outline-conflict surfaces. Token names are frozen verbatim across docs and the design system (V1.69→V1.72 preservation invariant); `canvas-outline-conflict-marker` is intentionally distinct from V1.71's generic conflict marker and the V1.70 `canvas-write-conflict` write-state token.

| Token | Purpose | Light | Dark | Example |
| --- | --- | --- | --- | --- |
| `canvas-outline-volume-fill` | Volume lane fill (subtle band behind a volume's chapters) | `#F5F5F4` | `#1F1F1E` | `background: var(--color-canvas-outline-volume-fill);` |
| `canvas-outline-chapter-card-status-pending` | Chapter card `not_started` state accent | `#94A3B8` | `#64748B` | `border-left: 3px solid var(--color-canvas-outline-chapter-card-status-pending);` |
| `canvas-outline-chapter-card-status-drafted` | Chapter card `outlined` / `draft` / `finalized` state accent | `#3B82F6` | `#60A5FA` | `border-left: 3px solid var(--color-canvas-outline-chapter-card-status-drafted);` |
| `canvas-outline-chapter-card-status-completed` | Chapter card `completed` state accent | `#10B981` | `#34D399` | `border-left: 3px solid var(--color-canvas-outline-chapter-card-status-completed);` |
| `canvas-outline-timeline-event-pin` | Timeline event node pin | `#F59E0B` | `#FBBF24` | `fill: var(--color-canvas-outline-timeline-event-pin);` |
| `canvas-outline-foreshadow-edge` | Foreshadow edge color/weight | `#A78BFA` | `#C4B5FD` | `stroke: var(--color-canvas-outline-foreshadow-edge); stroke-width: 1.5px;` |
| `canvas-outline-timeline-marker` | Timeline lane marker color | `#0EA5E9` | `#38BDF8` | `background: var(--color-canvas-outline-timeline-marker);` |
| `canvas-outline-conflict-marker` | Outline-specific conflict marker (distinct from `canvas-write-conflict`) | `#EF4444` | `#F87171` | `color: var(--color-canvas-outline-conflict-marker);` |

### World KB Canvas Tokens (V1.73)

The V1.73 World KB canvas-write tokens extend `components.canvas` with concrete light + dark values for entity-card fills/strokes, promotion-state lifecycle badges (pending/confirmed/rejected/merged), source-anchor provenance, computable-state, conflict markers, the non-spatial alternate view, the focus ring, and the read-only relationship edge. Token names are frozen verbatim across the compass Phase 2b architect lock, docs, and the design system (V1.69→V1.73 preservation invariant). Promotion-state colors reuse the established semantic mapping (pending=amber, confirmed=green, rejected=red, merged=purple) so state is not color-only — badges also carry a text label and the selected card pairs `canvas-worldkb-entity-card-stroke-selected` with the global focus ring.

| Token | Purpose | Light | Dark | Example |
| --- | --- | --- | --- | --- |
| `canvas-worldkb-entity-card-fill-default` | Entity card default fill | `#FFFFFF` | `#1A1A1A` | `background: var(--color-canvas-worldkb-entity-card-fill-default);` |
| `canvas-worldkb-entity-card-fill-hover` | Entity card hover fill | `#F5F5F5` | `#2A2A2A` | `background: var(--color-canvas-worldkb-entity-card-fill-hover);` |
| `canvas-worldkb-entity-card-fill-selected` | Entity card selected fill | `#EBF2FF` | `rgba(82,168,255,0.14)` | `background: var(--color-canvas-worldkb-entity-card-fill-selected);` |
| `canvas-worldkb-entity-card-stroke-default` | Entity card default border | `rgba(0,0,0,0.14)` | `rgba(255,255,255,0.18)` | `border: 1px solid var(--color-canvas-worldkb-entity-card-stroke-default);` |
| `canvas-worldkb-entity-card-stroke-selected` | Entity card selected border | `{colors.blue-700}` | `{colors.blue-700}` | `border: 1px solid var(--color-canvas-worldkb-entity-card-stroke-selected);` |
| `canvas-worldkb-promotion-pending` | Pending candidate lifecycle badge | `#F59E0B` | `#FBBF24` | `background: var(--color-canvas-worldkb-promotion-pending);` |
| `canvas-worldkb-promotion-confirmed` | Confirmed entity lifecycle badge | `#10B981` | `#34D399` | `background: var(--color-canvas-worldkb-promotion-confirmed);` |
| `canvas-worldkb-promotion-rejected` | Rejected candidate lifecycle badge | `#EF4444` | `#F87171` | `background: var(--color-canvas-worldkb-promotion-rejected);` |
| `canvas-worldkb-promotion-merged` | Merged entity lifecycle badge | `#8B5CF6` | `#A78BFA` | `background: var(--color-canvas-worldkb-promotion-merged);` |
| `canvas-worldkb-source-anchor-edge` | Source-anchor provenance edge stroke | `#A78BFA` | `#C4B5FD` | `stroke: var(--color-canvas-worldkb-source-anchor-edge);` |
| `canvas-worldkb-source-anchor-node` | Source-anchor node fill | `#EDE9FE` | `#2A2440` | `fill: var(--color-canvas-worldkb-source-anchor-node);` |
| `canvas-worldkb-computable-badge` | Computable-state badge fill | `#0EA5E9` | `#38BDF8` | `background: var(--color-canvas-worldkb-computable-badge);` |
| `canvas-worldkb-conflict-marker` | World KB conflict marker stroke | `#EF4444` | `#F87171` | `color: var(--color-canvas-worldkb-conflict-marker);` |
| `canvas-worldkb-conflict-marker-fill` | World KB conflict marker background | `rgba(239,68,68,0.10)` | `rgba(248,113,113,0.12)` | `background: var(--color-canvas-worldkb-conflict-marker-fill);` |
| `canvas-worldkb-nonspatial-row-highlight` | Non-spatial alternate view row highlight | `#F5F5F4` | `#1F1F1E` | `background: var(--color-canvas-worldkb-nonspatial-row-highlight);` |
| `canvas-worldkb-focus-ring` | Entity card focus ring color | `{colors.blue-700}` | `{colors.blue-700}` | `box-shadow: 0 0 0 2px var(--color-canvas-worldkb-focus-ring);` |
| `canvas-worldkb-relationship-edge` | Relationship edge stroke (read-only until V1.74) | `#94A3B8` | `#64748B` | `stroke: var(--color-canvas-worldkb-relationship-edge);` |

---

## Voice & Content

Nexus UI copy should sound like a careful CLI message translated into a local dashboard: direct, specific, and recoverable.

- Use **Title Case** for page titles, nav items, tabs, buttons, and table headers.
- Use **sentence case** for helper text, empty states, error bodies, and toasts.
- Actions are **Verb + Noun**: `Create Work`, `Validate Preset`, `Archive Work`, `Refresh Sessions`.
- Avoid generic `OK`, `Submit`, `Confirm`; name the object being changed.
- Errors: `What happened. What to do next.` Example: `Preset validation failed. Fix the YAML errors and validate again.`
- Toasts: specific object + result, no `successfully`, no trailing period. Example: `Preset validated`, `Work archived`.
- Empty states point to the first action: `No works yet. Create a Work to start the local loop.`
- Loading states use present participle + ellipsis: `Loading works…`, `Validating preset…`.
- Prefer author-facing local-first nouns: `local daemon`, `workspace`, `Work`, `preset`, `stage`, `finding`, `capability`, `session`.
- Avoid protocol jargon in the UI surface: do not expose `ACP`, `orchestration graph`, `cursor token`, or schema internals unless a future advanced diagnostics screen explicitly calls for them.
- Use numerals for counts and percentages; use the ellipsis character (`…`).
- Never imply cloud/platform behavior in this UI unless the screen explicitly belongs to a future cloud line.

---

## Implementation Mapping for P1

- Map color tokens to CSS variables: `--color-background-100`, `--color-gray-1000`, etc.
- Tailwind should reference CSS variables, not hard-coded hex values inside components.
- Shadcn component defaults should read from the component primitive entries above.
- `data-theme="dark"` or a root class may swap values; token names must remain identical.
- Production-level split is now active: `DESIGN.md` holds light frontmatter values; `DESIGN.dark.md` holds dark frontmatter values.

---

## Desktop Shell Supplement (V1.66 Standard+)

The V1.66 Tauri desktop shell ([desktop-shell.md](../.mstar/knowledge/specs/desktop-shell.md)) uses the same token names and voice rules as the local Web UI. Desktop shell surfaces should feel native enough to be trustworthy on macOS, but not custom-chromed or distribution-polished yet. Production polish — custom title bars, system tray/menu bar app, global shortcuts, native notifications, signing/notarization copy, animation refinements — is deferred to V1.67+.

### Desktop Scope

| Surface | V1.66 decision |
| --- | --- |
| Window chrome | Standard OS window chrome; no custom title bar |
| App menu | Native menu structure only; minimal commands |
| Native dialogs | Use for open/reveal errors, restart confirmation, about/system info |
| Desktop context menu | Enables `Open With…` and `Reveal in Finder` only in desktop mode |
| System tray / menu bar | None in V1.66 |
| Daemon status | Visible lightweight indicator with restart affordance |

### Desktop Window Chrome

Desktop token values: see frontmatter `components.desktop-window-chrome`.

Rules:
- Do not implement a custom title bar in V1.66.
- Keep primary navigation below the native title bar / traffic-light area.
- Never place destructive or high-frequency actions where they may conflict with native window controls.

### App Menu

| Menu | Required items | Notes |
| --- | --- | --- |
| `Nexus` | About Nexus, Quit Nexus | About may show version/build/daemon status |
| `File` | Open Workspace… (disabled unless implemented), Reveal Workspace in Finder, Close Window | Disable unavailable items instead of hiding roadmap commands |
| `Edit` | Undo/Redo, Cut/Copy/Paste/Select All | Use native defaults where possible |
| `View` | Reload, Toggle Developer Tools (dev only), Reset Zoom | Dev-only items must not imply production support |
| `Window` | Minimize, Zoom, Bring All to Front | Native defaults |
| `Help` | Open Logs Folder, Copy Diagnostics | Keep author-facing copy; avoid protocol jargon |

App menu token values: see frontmatter `components.app-menu`.

### Native Dialogs

Native dialog token values: see frontmatter `components.native-dialogs`.

Copy rules:
- Sentence case in dialog bodies.
- State the object and recovery action: `Daemon did not start. Restart Nexus or run diagnostics from the Help menu.`
- Do not expose stack traces in dialogs; offer `Copy Diagnostics`.

### Desktop Context Menu

Desktop mode extends the V1.65 context-menu tokens with native file actions. Token values: see frontmatter `components.context-menu`.

Behavior rules:
- Browser mode shows `Copy Path` only.
- Desktop mode shows `Copy Path`, `Open With…`, and `Reveal in Finder`.
- `Open With…` uses an ellipsis (opens a system chooser).
- `Reveal in Finder` is macOS wording for V1.66. Future Windows/Linux builds map to `Reveal in File Explorer` / `Reveal in Files`.
- If the path guard rejects a path: `Path not opened. The file is outside the active workspace.`

### Daemon Status Indicator

Daemon status token values: see frontmatter `components.daemon-status-indicator`.

Status labels:

| State | Label | Helper copy |
| --- | --- | --- |
| Starting | `Daemon starting…` | `Nexus is starting the local daemon.` |
| Healthy | `Daemon running` | `Local API is reachable on the configured port.` |
| Degraded | `Daemon reconnecting` | `Nexus is retrying the local daemon connection.` |
| Stopped | `Daemon stopped` | `Restart the daemon to use local workspace features.` |
| Port conflict | `Port unavailable` | `Another process is using the configured Nexus port.` |

Interaction rules:
- The status indicator must include text, not color alone.
- The primary recovery action is `Restart Daemon`.
- Do not show daemon internals by default; detailed diagnostics belong behind `Copy Diagnostics` or a Help menu item.

### Findings Remediation (V1.77)

The Control-Room findings page promotes from read-only to a remediation authoring surface. Token values: see frontmatter `components.finding-status-pill` and `components.finding-triage`.

**Finding status badge mapping** (6-state lifecycle; server-enforced adjacency at `crates/nexus-local-db/src/findings.rs:172`):

| Status | Token | Color intent |
| --- | --- | --- |
| `open` | `finding-status-pill.open` | Amber — newly raised, needs triage attention |
| `triaged` | `finding-status-pill.triaged` | Teal — reviewed, ready to route |
| `in_review` | `finding-status-pill.in_review` | Blue — actively under master review |
| `resolved` | `finding-status-pill.resolved` | Green — addressed, positive terminal |
| `wont_fix` | `finding-status-pill.wont_fix` | Gray — explicitly waived, quiet terminal |
| `duplicate` | `finding-status-pill.duplicate` | Purple — superseded by another finding |

Each status gets an intentional, distinct color (the generic `statusVariant` keyword matcher cannot distinguish `in_review` from `resolved` or `wont_fix` from `duplicate`). Rendered via `FindingStatusBadge` using the same `color-mix` pattern as the generic badge variants, so colors stay correct in both light and dark.

**Severity** reuses the existing `severityVariant` mapping (`info`/`low` → queued, `medium`/`warning` → warning, `critical`/`high` → error). No new severity tokens.

**Triage chrome** composes existing primitives — the inspector is a `Card`; action buttons are `button.secondary`; the `target_executor` selector and inline-edit inputs are `input-select-textarea.default`; the active table row uses `data-table.row-selected`. The `finding-triage` group records these compositions so the surface is discoverable without duplicating primitive tokens.

Interaction rules:
- Invalid status transitions are disabled client-side as defense-in-depth; the server is the authority (HTTP 422 `INVALID_TRANSITION`).
- Optimistic mutations update the list cache immediately and roll back on error; no conflict modal (last-writer-wins, single-author triage).
- `target_executor` is an assignment hint, not an auto-trigger — re-running a preset stays a deliberate canvas/CLI action.
- Status is never color alone: every badge carries a humanized text label.

### Creator Memory Review-Loop (V1.78)

The Control-Room gains a creator-scoped Memory surface that closes the capture → review → internalize loop. Token values: see frontmatter `memory-pending-count`, `memory-task-kind-*`, `memory-fragment-summary`, `memory-fragment-id`, `memory-inspector-*`, `memory-review-button`, `memory-fragment-filter-input`.

**Pending-review count badge** (`memory-pending-count`) — a red numeric indicator on the pending-reviews header showing the live count from `GET /v1/local/memory/pending-review/count`. Red signals "items awaiting your review".

**Task-kind chips** (`memory-task-kind-*`) — `task_kind` is a free-form string on the wire, so five known values map to distinct color accents (reusing the V1.77 `severityVariant` / `findingStatusClasses` `color-mix` pattern) and any unrecognized value falls back to a neutral chip rendered verbatim:

| `task_kind` | Token | Color intent |
| --- | --- | --- |
| `brainstorm` | `memory-task-kind-brainstorm` | Amber — ideation / creative |
| `outline` | `memory-task-kind-outline` | Blue — planning / structure |
| `chapter` | `memory-task-kind-chapter` | Teal — writing / content |
| `research` | `memory-task-kind-research` | Purple — inquiry / knowledge |
| `unknown` (and unrecognized) | `memory-task-kind-unknown` | Gray — neutral |

**Fragment browser** (`memory-fragment-summary`, `memory-fragment-id`, `memory-fragment-filter-input`) — a read-only list of long-term memory fragments produced only by the `review` route. `fragment_id` renders in monospace; `summary` renders as body copy; the keyword filter is a standard search input. No CRUD — fragments are produced only by reviewing pending captures.

**Inspector chrome** (`memory-inspector-header`, `memory-inspector-field-label`, `memory-inspector-field-value`) — the side inspector (matching the V1.77 `FindingDetailPanel` pattern) shows all 6 `PendingReviewInfo` fields. Absent `world_id` reads as "(none)"; `created_at` is RFC 3339 rendered in the author's local time; `raw_digest` is a scrollable preformatted area.

**Review & Summarize CTA** (`memory-review-button`) — a primary accent button (reuses `button.primary` basis) enabled only when `count > 0`; shows a processing state then surfaces `promoted`/`fragmented`/`dropped` counters in a confirmation toast.

Interaction rules:
- The surface is review/consume-only — `createPendingReview` stays CLI/producer-only (the session-end capture pipeline owns creation), mirroring V1.77's `createFinding` CLI-only decision.
- Optimistic delete removes the row and decrements the count badge before the server responds; rolls back on error.
- Token names are preserved verbatim — no consumer (`tailwind.config.ts`, `index.css`, or Memory page components) invents a name not in this frontmatter (V1.69 invariant continues).

### Author Reflection Token Stubs (V1.79)

V1.79 P0 filled the Track A reading-surface tokens with concrete light values; only the Track B SOUL visualization token stubs remain for P1 to replace with concrete light/dark values.
