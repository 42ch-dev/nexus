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
    canvas-write-stale-bg: "rgba(183,110,0,0.08)"
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
