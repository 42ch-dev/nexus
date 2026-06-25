# Nexus Local Web UI Design System

<!-- COMPLETENESS_LEVEL: 2 — Standard+, last audited 2026-06-25 -->

Nexus Local Web UI is a restrained, author-focused design system for the local-first **Control Room + Setup + Authoring** SPA. It should feel calm and trustworthy: quiet surfaces, dense but readable data, explicit status language, and high-confidence controls for local creative runtime work without making writers feel like they are operating infrastructure.

This file is the light/default theme and the token-name SSOT. Dark values are listed beside light values below using the **same token names** so P1 can map both themes to CSS custom properties or Tailwind tokens from one source. A separate `DESIGN.dark.md` can be split out in V1.66 if the UI graduates to Production completeness.

Product inputs from `.mstar/knowledge/specs/web-ui-design-requirements.md`:

- Primary persona: writers/authors, not engineers; calm and focused over dashboard anxiety.
- Control Room screens are data-dense; Setup screens are form-dense with first-class validation and destructive-action confirmation.
- V1.65 Authoring screens add outline editing, chapter structure tables, and a body read-only context menu. Product-manager design requirements are being amended in parallel; this Standard+ increment assumes browser V1.65 ships `Copy path` only, while `Open with` / `Reveal in file manager` wait for the V1.66 Tauri shell.
- WCAG 2.1 AA is the floor in both light and dark; focus rings, keyboard paths, status text, and reduced motion are non-negotiable.
- Brand voice: helpful, plain, local-first, and consistent with CLI terms (`Work`, `preset`, `stage`, `finding`, `capability`).

---

## Colors

Color tokens follow the Geist-style intent scale: `100` background/quiet, `400` border, `700` solid fill, `900` secondary text, `1000` primary text. Use color for state and hierarchy, not decoration.

### Background

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `background-100` | `#ffffff` | `#0a0a0a` | App/page background, card fill |
| `background-200` | `#fafafa` | `#111111` | Subtle panels, table header |
| `background-300` | `#f5f5f5` | `#1a1a1a` | Hover/selected row background |

### Gray (solid)

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `gray-100` | `#f5f5f5` | `#1f1f1f` | Disabled fill |
| `gray-200` | `#eeeeee` | `#2a2a2a` | Subtle fill hover |
| `gray-300` | `#e0e0e0` | `#3a3a3a` | Active subtle fill |
| `gray-400` | `#c7c7c7` | `#525252` | Default border fallback |
| `gray-500` | `#a3a3a3` | `#737373` | Hover border fallback |
| `gray-600` | `#8a8a8a` | `#8a8a8a` | Active border fallback |
| `gray-700` | `#666666` | `#a3a3a3` | Disabled/tertiary text |
| `gray-800` | `#4a4a4a` | `#c7c7c7` | Strong secondary text |
| `gray-900` | `#333333` | `#e0e0e0` | Secondary text/icons |
| `gray-1000` | `#111111` | `#f5f5f5` | Primary text/icons |

### Gray Alpha

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `gray-alpha-100` | `rgba(0,0,0,0.04)` | `rgba(255,255,255,0.06)` | Hover wash |
| `gray-alpha-200` | `rgba(0,0,0,0.06)` | `rgba(255,255,255,0.08)` | Subtle separator |
| `gray-alpha-300` | `rgba(0,0,0,0.08)` | `rgba(255,255,255,0.10)` | Active wash |
| `gray-alpha-400` | `rgba(0,0,0,0.12)` | `rgba(255,255,255,0.16)` | Default border |
| `gray-alpha-500` | `rgba(0,0,0,0.18)` | `rgba(255,255,255,0.22)` | Hover border |
| `gray-alpha-600` | `rgba(0,0,0,0.24)` | `rgba(255,255,255,0.30)` | Active border / divider |

### Accent Scales

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `blue-700` | `#006bff` | `#52a8ff` | Primary action, links, focus ring |
| `blue-800` | `#0057d9` | `#7bbdff` | Primary hover |
| `blue-900` | `#0046ad` | `#a8d3ff` | Secondary blue text |
| `blue-1000` | `#003680` | `#d6ebff` | Strong blue text |
| `red-700` | `#e5484d` | `#ff6b6b` | Error text, destructive icon |
| `red-800` | `#d11f2a` | `#ff8585` | Destructive fill |
| `red-900` | `#a91520` | `#ffb3b3` | Error secondary text |
| `red-1000` | `#7f1018` | `#ffd6d6` | Error strong text |
| `amber-700` | `#b76e00` | `#ffc043` | Warning text |
| `amber-800` | `#935800` | `#ffd06a` | Warning fill/hover |
| `amber-900` | `#704300` | `#ffe0a3` | Warning secondary text |
| `amber-1000` | `#4d2d00` | `#fff0d0` | Warning strong text |
| `green-700` | `#1f8f4d` | `#54d58a` | Healthy/running status |
| `green-800` | `#18753e` | `#7ae0a3` | Healthy fill hover |
| `green-900` | `#125a30` | `#a6ebc0` | Healthy secondary text |
| `green-1000` | `#0d4023` | `#d4f7df` | Healthy strong text |
| `teal-700` | `#008577` | `#4cd8c8` | Informational status |
| `teal-800` | `#006b60` | `#75e4d7` | Informational hover |
| `teal-900` | `#00524a` | `#a2eee6` | Informational secondary text |
| `teal-1000` | `#003b35` | `#d2f8f4` | Informational strong text |
| `purple-700` | `#7c3aed` | `#b794ff` | Capability/preset accent |
| `purple-800` | `#6d28d9` | `#c5a8ff` | Capability/preset hover |
| `purple-900` | `#581cbd` | `#d8c6ff` | Capability/preset secondary text |
| `purple-1000` | `#3b1686` | `#eee5ff` | Capability/preset strong text |
| `pink-700` | `#db2777` | `#ff8ac2` | Rare highlight, not primary state |
| `pink-800` | `#be185d` | `#ffa6d0` | Highlight hover |
| `pink-900` | `#9d174d` | `#ffc4df` | Highlight secondary text |
| `pink-1000` | `#831843` | `#ffe3f0` | Highlight strong text |

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

Use a system stack by default so the UI works without webfont fetch. If a future build bundles Geist, map `font-sans` to Geist Sans and `font-mono` to Geist Mono with the same token names. Prioritize long-session readability over visual novelty.

### Font Families

| Token | Value | Use |
| --- | --- | --- |
| `font-sans` | `Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` | UI and prose |
| `font-mono` | `"SFMono-Regular", "Cascadia Code", "Roboto Mono", Consolas, monospace` | IDs, ports, code-like values, tabular metrics |

### Type Scale

| Token | Font | Size | Weight | Line | Spacing | Use |
| --- | --- | --- | --- | --- | --- | --- |
| `heading-32` | `font-sans` | `32px` | `650` | `1.18` | `-0.025em` | Page title |
| `heading-24` | `font-sans` | `24px` | `650` | `1.25` | `-0.02em` | View title / detail header |
| `heading-20` | `font-sans` | `20px` | `600` | `1.3` | `-0.015em` | Card section title |
| `heading-16` | `font-sans` | `16px` | `600` | `1.4` | `-0.01em` | Dense section title |
| `label-14` | `font-sans` | `14px` | `500` | `1.35` | `0` | Form labels, nav labels |
| `label-12` | `font-sans` | `12px` | `600` | `1.35` | `0.02em` | Table headers, badge labels |
| `copy-16` | `font-sans` | `16px` | `400` | `1.6` | `0` | Primary body copy |
| `copy-14` | `font-sans` | `14px` | `400` | `1.55` | `0` | Default UI copy |
| `copy-13` | `font-sans` | `13px` | `400` | `1.5` | `0` | Dense helper text |
| `button-14` | `font-sans` | `14px` | `550` | `1` | `0` | Default buttons |
| `button-12` | `font-sans` | `12px` | `600` | `1` | `0.01em` | Compact buttons |
| `label-12-mono` | `font-mono` | `12px` | `500` | `1.4` | `0` | IDs, schema versions, cursor values |
| `copy-13-mono` | `font-mono` | `13px` | `400` | `1.5` | `0` | Code-like inline values |

Numeric columns use `font-variant-numeric: tabular-nums`.

---

## Spacing & Layout

Base unit: **4px**. Prefer mechanical spacing over bespoke values.

### Spacing Scale

| Token | Value | Use |
| --- | --- | --- |
| `space-1` | `4px` | Icon/text nudge |
| `space-2` | `8px` | Inside a control group |
| `space-3` | `12px` | Compact row gap |
| `space-4` | `16px` | Related groups |
| `space-6` | `24px` | Card padding default |
| `space-8` | `32px` | Section spacing |
| `space-10` | `40px` | Major view spacing |
| `space-16` | `64px` | Page vertical rhythm |
| `space-24` | `96px` | Empty-state breathing room |

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

Radii stay tight and utility-oriented.

| Token | Value | Use |
| --- | --- | --- |
| `radius-control` | `6px` | Buttons, inputs, select triggers |
| `radius-card` | `8px` | Cards, table containers |
| `radius-popover` | `12px` | Menus, dropdowns, modals |
| `radius-fullscreen` | `16px` | Fullscreen panels or large sheets |
| `radius-pill` | `9999px` | Badges, status pills, avatars |

Do not mix very rounded and sharp corners in a single view.

---

## Component Primitives

All components must expose visible `:focus-visible` styles using a two-layer ring: `0 0 0 2px var(--color-background-100), 0 0 0 4px var(--color-blue-700)`.

### Button

| Variant | Background | Text | Border | Radius | Height | States |
| --- | --- | --- | --- | --- | --- | --- |
| `primary` | `blue-700` | `#ffffff` | none | `radius-control` | `40px` | hover `blue-800`, active `blue-900` |
| `secondary` | `background-100` | `gray-1000` | `gray-alpha-400` | `radius-control` | `40px` | hover `background-200` + `gray-alpha-500` |
| `tertiary` | `transparent` | `gray-1000` | none | `radius-control` | `40px` | hover `gray-alpha-100` |
| `destructive` | `red-800` | `#ffffff` | none | `radius-control` | `40px` | hover `red-700`, active `red-900` |

Sizes: `small` = `32px` height + `button-12`; `default` = `40px` + `button-14`; `large` = `48px` + `button-14`. Disabled: `gray-100` fill, `gray-700` text, not-allowed cursor.

The preset `Validate` action uses `primary` when it is the main form action, or `secondary` with a `blue-700` leading icon when paired with a separate save action. It must read as reassurance (“is this safe?”), not as a debug-only tool.

### Input / Select / Textarea

| Variant | Background | Text | Border | Radius | Height |
| --- | --- | --- | --- | --- | --- |
| `default` | `background-100` | `gray-1000` | `gray-alpha-400` | `radius-control` | `40px` |
| `error` | `background-100` | `gray-1000` | `red-700` | `radius-control` | `40px` |
| `disabled` | `gray-100` | `gray-700` | `gray-alpha-300` | `radius-control` | `40px` |

Textarea min height: `96px`. Placeholder uses `gray-700`. Helper text uses `copy-13`; error helper uses `red-700`.

### Card

Default card: `background-100`, `gray-alpha-400` border, `radius-card`, `space-6` padding, optional `shadow-card`. Compact card uses `space-4`; hero/status cards can use `space-8`.

### Table

- Header: `background-200`, `label-12`, `gray-900`, bottom border `gray-alpha-400`.
- Rows: `copy-14`, primary text `gray-1000`, secondary `gray-900`; hover `background-200`; selected `background-300`.
- Use `label-12-mono` for IDs/cursors and tabular figures for numeric columns.
- Empty table row: sentence-case helper plus first action if applicable.

### Badge / Status Pill

| Variant | Background | Text | Border |
| --- | --- | --- | --- |
| `neutral` | `gray-alpha-100` | `gray-900` | `gray-alpha-300` |
| `running` | `green-700` at 10% alpha | `green-1000` | `green-700` at 30% alpha |
| `queued` | `teal-700` at 10% alpha | `teal-1000` | `teal-700` at 30% alpha |
| `warning` | `amber-700` at 12% alpha | `amber-1000` | `amber-700` at 30% alpha |
| `error` | `red-700` at 12% alpha | `red-1000` | `red-700` at 30% alpha |
| `preset` | `purple-700` at 10% alpha | `purple-1000` | `purple-700` at 30% alpha |

Height `24px`, horizontal padding `8px`, `radius-pill`, label token `label-12`.

### Toast

Toast: `background-100`, border `gray-alpha-400`, `shadow-popover`, `radius-popover`, width `360px` max. Title uses `label-14`; body uses `copy-13`. Variants use the semantic accent on the leading icon/bar. Toasts name the changed object; no trailing period.

### Sidebar Nav

Sidebar width: `248px`. Background `background-100`; divider `gray-alpha-400`. Nav item height `36px`, radius `radius-control`, `label-14`. Active item uses `gray-alpha-100` fill + `gray-1000` text + optional left bar `blue-700`. Collapsed/mobile nav must keep labels accessible via text, not icon-only navigation.

### Dialog / Popover

Dialog: `background-100`, `radius-popover`, `shadow-modal`, max width `560px`, `space-6` padding. Popover/menu: `background-100`, border `gray-alpha-400`, `shadow-popover`, `radius-popover`, item height `36px`.

### Editor (V1.65 Standard+)

The outline editor is a planning surface, not the body manuscript editor. It should feel closer to an intentional note/workbench than a document processor: compact toolbar, clear save state, and no hidden background writes.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `editor-surface` | `background-100` | `background-100` | Main editor panel |
| `editor-surface-muted` | `background-200` | `background-200` | Toolbar and footer strip |
| `editor-border` | `gray-alpha-400` | `gray-alpha-400` | Editor frame and toolbar divider |
| `editor-border-active` | `blue-700` | `blue-700` | Focused editor frame |
| `editor-toolbar-control-bg` | `transparent` | `transparent` | Default toolbar button |
| `editor-toolbar-control-hover` | `gray-alpha-100` | `gray-alpha-100` | Toolbar button hover |
| `editor-toolbar-control-active` | `gray-alpha-200` | `gray-alpha-200` | Active mark/block button |
| `editor-save-clean` | `green-700` | `green-700` | Saved indicator dot/icon |
| `editor-save-dirty` | `amber-700` | `amber-700` | Unsaved changes indicator |
| `editor-save-error` | `red-700` | `red-700` | Save failed indicator |
| `editor-selection` | `rgba(0,107,255,0.14)` | `rgba(82,168,255,0.24)` | Text selection in editor |

| Element | Token use | Size / rhythm | States |
| --- | --- | --- | --- |
| Editor frame | `editor-surface`, `editor-border`, `radius-card` | Min height `360px`; padding `space-6` | `:focus-within` swaps border to `editor-border-active` and uses global focus ring |
| Toolbar | `editor-surface-muted`, bottom border `editor-border` | Height `44px`; gap `space-1`; horizontal padding `space-2` | Sticky within editor panel if content scrolls |
| Toolbar button | `button-12`, `radius-control` | `32px` square or min-width `32px` | hover `editor-toolbar-control-hover`; active `editor-toolbar-control-active` |
| Save-state indicator | `label-12`, semantic dot | Dot `8px`; gap `space-2` | `Saved` green, `Unsaved` amber, `Save failed` red; always include text, not color alone |
| Markdown helper | `copy-13`, `gray-900` | Footer padding `space-3` | Explain that body writing is read-only/deferred when relevant |

Editor content typography:

- Prose defaults to `copy-16`; headings use `heading-24` / `heading-20` / `heading-16` in descending order.
- Lists use `space-2` vertical rhythm and `space-6` indentation.
- Inline code uses `copy-13-mono`, `gray-alpha-100` background, `radius-control`, horizontal padding `4px`.
- Unknown markdown/frontmatter preservation warnings use `amber-700` icon + `copy-13` text.

### Data Table (V1.65 Standard+)

Chapter structure tables extend the base `Table` primitive with inline-edit and chapter-status semantics.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `table-row-hover` | `background-200` | `background-200` | Row hover |
| `table-row-selected` | `background-300` | `background-300` | Selected/focused chapter row |
| `table-row-edited` | `rgba(183,110,0,0.08)` | `rgba(255,192,67,0.14)` | Row with unsaved inline edits |
| `table-row-protected` | `rgba(124,58,237,0.06)` | `rgba(183,148,255,0.12)` | Finalized/published protected row emphasis |
| `table-cell-edit-bg` | `background-100` | `background-100` | Inline edit control background |
| `table-cell-edit-border` | `blue-700` | `blue-700` | Active inline edit border |
| `table-column-divider` | `gray-alpha-200` | `gray-alpha-200` | Optional dense-column separator |

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

The V1.65 browser context menu is intentionally narrow: **Copy path** only for body/outline path affordances. Native `Open with` and `Reveal in file manager` are V1.66 Tauri-shell capabilities.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `context-menu-bg` | `background-100` | `background-100` | Menu surface |
| `context-menu-border` | `gray-alpha-400` | `gray-alpha-400` | Menu border |
| `context-menu-item-hover` | `gray-alpha-100` | `gray-alpha-100` | Item hover/focus |
| `context-menu-item-active` | `gray-alpha-200` | `gray-alpha-200` | Pressed item |
| `context-menu-item-disabled` | `gray-700` | `gray-700` | Disabled future native actions if shown as roadmap hints |
| `context-menu-shortcut` | `gray-700` | `gray-700` | Shortcut hint text |

| Element | Token use | Size / rhythm | States |
| --- | --- | --- | --- |
| Menu surface | `context-menu-bg`, `context-menu-border`, `shadow-popover`, `radius-popover` | Min width `180px`; padding `space-1` | Opens near pointer/focused row; closes on Escape |
| Menu item | `copy-14`, `gray-1000` | Height `36px`; horizontal padding `space-3`; gap `space-2` | hover/focus `context-menu-item-hover`; active `context-menu-item-active` |
| Path preview | `copy-13-mono`, `gray-900` | Max width `320px`, truncates middle when needed | Read-only; never expose absolute path unless API returns it intentionally |

Copy-path behavior:

- The action label is `Copy Path`; success toast is `Path copied`.
- If clipboard write fails, show `Path not copied. Copy it manually from the details panel.`
- Menu items must be keyboard reachable from the row/body read-only surface.

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
- Shadcn component defaults should read from the component primitive tables above.
- `data-theme="dark"` or a root class may swap values; token names must remain identical.
- Production-level split into `DESIGN.dark.md`, richer component specs, native desktop menu actions, and rendered visual QA are deferred to V1.66.
