import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { useTheme } from '@/components/theme-provider';

/* ------------------------------------------------------------------ */
/*  Data — token inventory from SSOT                                    */
/* ------------------------------------------------------------------ */

interface ColorToken {
  label: string;
  varName: string;
}

interface TokenGroup {
  title: string;
  /** Optional Chronos / usage note rendered under the group title. */
  hint?: string;
  tokens: ColorToken[];
}

const COLOR_GROUPS: TokenGroup[] = [
  {
    title: 'Brand',
    hint:
      'Chronos dual-role anchors: deep-blue is ink structure (titlebar, light text links); brand-cyan is the cobalt signal — light primary fill (blue-700 ≡ brand-cyan), dark brightens to blue-700 on dark. Extended steps and alphas recalibrated for the new signal.',
    tokens: [
      { label: 'brand-deep-blue', varName: '--color-brand-deep-blue' },
      { label: 'brand-deep-blue-800', varName: '--color-brand-deep-blue-800' },
      { label: 'brand-deep-blue-900', varName: '--color-brand-deep-blue-900' },
      { label: 'brand-deep-blue-1000', varName: '--color-brand-deep-blue-1000' },
      { label: 'brand-cyan', varName: '--color-brand-cyan' },
      { label: 'brand-cyan-800', varName: '--color-brand-cyan-800' },
      { label: 'brand-cyan-900', varName: '--color-brand-cyan-900' },
      { label: 'brand-cyan-1000', varName: '--color-brand-cyan-1000' },
      { label: 'brand-deep-blue-alpha-100', varName: '--color-brand-deep-blue-alpha-100' },
      { label: 'brand-deep-blue-alpha-200', varName: '--color-brand-deep-blue-alpha-200' },
      { label: 'brand-cyan-alpha-100', varName: '--color-brand-cyan-alpha-100' },
      { label: 'brand-cyan-alpha-200', varName: '--color-brand-cyan-alpha-200' },
      { label: 'brand-white', varName: '--color-brand-white' },
    ],
  },
  {
    title: 'Overlay scrim',
    hint:
      'Backdrop fill only — text never lives directly on scrim; an opaque dialog/popover plane sits above it (DESIGN.md §Colors).',
    tokens: [{ label: 'scrim', varName: '--color-scrim' }],
  },
  {
    title: 'Background',
    hint:
      'Silver-neutral planes on light; graphite dark planes on dark. Shell chrome reads from these tokens.',
    tokens: [
      { label: 'background-100', varName: '--color-background-100' },
      { label: 'background-200', varName: '--color-background-200' },
      { label: 'background-300', varName: '--color-background-300' },
    ],
  },
  {
    title: 'Gray (solid)',
    tokens: [
      { label: 'gray-100', varName: '--color-gray-100' },
      { label: 'gray-200', varName: '--color-gray-200' },
      { label: 'gray-300', varName: '--color-gray-300' },
      { label: 'gray-400', varName: '--color-gray-400' },
      { label: 'gray-500', varName: '--color-gray-500' },
      { label: 'gray-600', varName: '--color-gray-600' },
      { label: 'gray-700', varName: '--color-gray-700' },
      { label: 'gray-800', varName: '--color-gray-800' },
      { label: 'gray-900', varName: '--color-gray-900' },
      { label: 'gray-1000', varName: '--color-gray-1000' },
    ],
  },
  {
    title: 'Gray alpha',
    tokens: [
      { label: 'gray-alpha-100', varName: '--color-gray-alpha-100' },
      { label: 'gray-alpha-200', varName: '--color-gray-alpha-200' },
      { label: 'gray-alpha-300', varName: '--color-gray-alpha-300' },
      { label: 'gray-alpha-400', varName: '--color-gray-alpha-400' },
      { label: 'gray-alpha-500', varName: '--color-gray-alpha-500' },
      { label: 'gray-alpha-600', varName: '--color-gray-alpha-600' },
    ],
  },
  {
    title: 'Blue',
    hint:
      'Interactive cobalt scale (light and dark). Light primary/active chrome uses blue-1000 ≡ brand-cyan-1000; dark CTA uses blue-700 ≡ brand-cyan. Focus rings: blue-1000 light / blue-700 dark. Not body-link ink on light (use brand-deep-blue).',
    tokens: ['700', '800', '900', '1000', '1100'].map((s) => ({
      label: `blue-${s}`,
      varName: `--color-blue-${s}`,
    })),
  },
  {
    title: 'Red',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `red-${s}`,
      varName: `--color-red-${s}`,
    })),
  },
  {
    title: 'Amber',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `amber-${s}`,
      varName: `--color-amber-${s}`,
    })),
  },
  {
    title: 'Green',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `green-${s}`,
      varName: `--color-green-${s}`,
    })),
  },
  {
    title: 'Teal',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `teal-${s}`,
      varName: `--color-teal-${s}`,
    })),
  },
  {
    title: 'Purple',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `purple-${s}`,
      varName: `--color-purple-${s}`,
    })),
  },
  {
    title: 'Pink',
    tokens: ['700', '800', '900', '1000'].map((s) => ({
      label: `pink-${s}`,
      varName: `--color-pink-${s}`,
    })),
  },
  {
    title: 'Component surfaces',
    hint:
      'Semantic component fills — DESIGN.md components.data-table.row-protected (protected-row tint, purple) and components.launch-daemon.main-banner (banner fill, amber). Both are translucent color-mix tints that re-resolve on theme flip (dark raises the mix percentage).',
    tokens: [
      { label: 'data-table-row-protected', varName: '--color-data-table-row-protected' },
      { label: 'main-banner-background', varName: '--color-main-banner-background' },
    ],
  },
  {
    title: 'State surface fills & borders',
    hint:
      'Semantic state surfaces (DESIGN.md components.states.{error,success,warning,info}) — translucent tint fills with matching 30% borders. Error/empty/loading and validation states consume these; each has an opaque text plane over the fill.',
    tokens: [
      { label: 'error-surface', varName: '--color-error-surface' },
      { label: 'error-surface-border', varName: '--color-error-surface-border' },
      { label: 'success-surface', varName: '--color-success-surface' },
      { label: 'success-surface-border', varName: '--color-success-surface-border' },
      { label: 'warning-surface', varName: '--color-warning-surface' },
      { label: 'warning-surface-border', varName: '--color-warning-surface-border' },
      { label: 'info-surface', varName: '--color-info-surface' },
      { label: 'info-surface-border', varName: '--color-info-surface-border' },
    ],
  },
  {
    title: 'Finding status pills',
    hint:
      'Finding-status pill fills/text/borders (DESIGN.md components.finding-status-pill) — open/triaged/in-review/resolved/wont-fix/duplicate. Semantics preserved; values resolve from the semantic scales.',
    tokens: [
      ...['open', 'triaged', 'in-review', 'resolved', 'wont-fix', 'duplicate'].flatMap((s) => [
        { label: `finding-status-${s}-bg`, varName: `--color-finding-status-${s}-bg` },
        { label: `finding-status-${s}-text`, varName: `--color-finding-status-${s}-text` },
        { label: `finding-status-${s}-border`, varName: `--color-finding-status-${s}-border` },
      ]),
    ],
  },
  {
    title: 'Memory task-kind chips',
    hint:
      'Memory task-kind chip fills/text/borders (DESIGN.md components.memory-task-kind-*) — brainstorm/outline/chapter/research/unknown.',
    tokens: [
      ...['brainstorm', 'outline', 'chapter', 'research', 'unknown'].flatMap((s) => [
        { label: `memory-task-kind-${s}-bg`, varName: `--color-memory-task-kind-${s}-bg` },
        { label: `memory-task-kind-${s}-text`, varName: `--color-memory-task-kind-${s}-text` },
        { label: `memory-task-kind-${s}-border`, varName: `--color-memory-task-kind-${s}-border` },
      ]),
    ],
  },
  {
    title: 'Reading maturation badges',
    hint:
      'Reading-maturation count badges (DESIGN.md components.reading-maturation-badge) — world-KB density (teal) and open-findings (amber) counts.',
    tokens: [
      { label: 'reading-maturation-kb-density-bg', varName: '--color-reading-maturation-kb-density-bg' },
      { label: 'reading-maturation-kb-density-text', varName: '--color-reading-maturation-kb-density-text' },
      { label: 'reading-maturation-kb-density-border', varName: '--color-reading-maturation-kb-density-border' },
      { label: 'reading-maturation-open-findings-bg', varName: '--color-reading-maturation-open-findings-bg' },
      { label: 'reading-maturation-open-findings-text', varName: '--color-reading-maturation-open-findings-text' },
      { label: 'reading-maturation-open-findings-border', varName: '--color-reading-maturation-open-findings-border' },
    ],
  },
  {
    title: 'nexus-ui Badge soft variants',
    hint:
      'nexus-ui Badge soft fills/text/borders (DESIGN.md components.badge-status-pill.soft) — running/queued/warning/error/preset. Neutral uses gray-alpha directly.',
    tokens: [
      ...['running', 'queued', 'warning', 'error', 'preset'].flatMap((s) => [
        { label: `nexus-ui-badge-soft-${s}-bg`, varName: `--color-nexus-ui-badge-soft-${s}-bg` },
        { label: `nexus-ui-badge-soft-${s}-text`, varName: `--color-nexus-ui-badge-soft-${s}-text` },
        { label: `nexus-ui-badge-soft-${s}-border`, varName: `--color-nexus-ui-badge-soft-${s}-border` },
      ]),
    ],
  },
];

/* ---------- Typography specimens (DESIGN.md frontmatter typography:) ----------
 *
 * Class strings are written out literally so the Tailwind scanner emits them
 * (dynamic `text-${name}` interpolation is invisible to the scanner). Metrics
 * (size / weight / line-height / tracking) are read live from the rendered
 * specimen's computed style — never hardcoded copies of the token values.
 *
 * Voice discipline (DESIGN.md §Design Concept): the display tier is the
 * content voice (offline system sans, `font-display`) — creative-entity titles
 * only; everything else stays interface voice (`font-sans` / `font-mono`).
 * Each specimen's family resolves via its role-scoped `font-<role>` utility,
 * which reads the generated `--text-<role>--font-family` var.
 */

interface TypoSpecimen {
  label: string;
  role: string;
  /** Literal text-* size class from the shared preset. */
  textClass: string;
  /**
   * Literal per-role font family utility (`font-<role>`), which resolves
   * through the generated `--text-<role>--font-family` var. Using the
   * role-scoped utility (not the coarse `font-sans`/`font-mono`) means a
   * DESIGN family edit to one role re-projects that specimen only — the
   * actual family is observable live from the rendered row.
   */
  familyClass: string;
  /** Literal weight utility; display tier bakes weight 600 into text-display-*. */
  weightClass?: 'font-heading' | 'font-semibold' | 'font-medium' | 'font-button';
  sampleText: string;
}

const TYPO_SPECIMENS: TypoSpecimen[] = [
  // ── Content voice (V1.121 v0.4 display tier — sans) ──
  { label: 'display-32', role: 'Content voice · page-level creative titles', textClass: 'text-display-32', familyClass: 'font-display-32', sampleText: 'The Orchard of Small Hours' },
  { label: 'display-24', role: 'Content voice · work / world titles', textClass: 'text-display-24', familyClass: 'font-display-24', sampleText: 'Chapter Six — The Long Descent' },
  { label: 'display-20', role: 'Content voice · card & chapter titles', textClass: 'text-display-20', familyClass: 'font-display-20', sampleText: 'A Field Guide to Tidal Magic' },
  // ── Interface voice (sans) ──
  { label: 'heading-32', role: 'Page / view title', textClass: 'text-heading-32', familyClass: 'font-heading-32', weightClass: 'font-heading', sampleText: 'Heading 32 — The quick brown fox' },
  { label: 'heading-24', role: 'Section title', textClass: 'text-heading-24', familyClass: 'font-heading-24', weightClass: 'font-heading', sampleText: 'Heading 24 — The quick brown fox' },
  { label: 'heading-20', role: 'Card title / dense section', textClass: 'text-heading-20', familyClass: 'font-heading-20', weightClass: 'font-heading', sampleText: 'Heading 20 — The quick brown fox' },
  { label: 'heading-16', role: 'Inline heading', textClass: 'text-heading-16', familyClass: 'font-heading-16', weightClass: 'font-heading', sampleText: 'Heading 16 — The quick brown fox' },
  { label: 'label-14', role: 'Form labels, nav items, table headers', textClass: 'text-label-14', familyClass: 'font-label-14', weightClass: 'font-medium', sampleText: 'Label 14 — The quick brown fox' },
  { label: 'label-12', role: 'Badge labels, compact headers', textClass: 'text-label-12', familyClass: 'font-label-12', weightClass: 'font-semibold', sampleText: 'LABEL 12 — THE QUICK BROWN FOX' },
  { label: 'copy-16', role: 'Primary body copy', textClass: 'text-copy-16', familyClass: 'font-copy-16', sampleText: 'Body 16 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-14', role: 'Default UI copy', textClass: 'text-copy-14', familyClass: 'font-copy-14', sampleText: 'Body 14 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-13', role: 'Dense helper text', textClass: 'text-copy-13', familyClass: 'font-copy-13', sampleText: 'Body 13 — The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.' },
  { label: 'copy-12', role: 'Footnotes, dense metadata, existing utility', textClass: 'text-copy-12', familyClass: 'font-copy-12', sampleText: 'Body 12 — The quick brown fox jumps over the lazy dog. (copy-12 formalizes an existing Tailwind utility.)' },
  { label: 'button-14', role: 'Default button label', textClass: 'text-button-14', familyClass: 'font-button-14', weightClass: 'font-button', sampleText: 'Button 14 — Continue' },
  { label: 'button-12', role: 'Compact button label', textClass: 'text-button-12', familyClass: 'font-button-12', weightClass: 'font-semibold', sampleText: 'BUTTON 12 — SAVE' },
  // ── Interface voice (mono) ──
  { label: 'label-12-mono', role: 'IDs, table figures, code-like values', textClass: 'text-label-12-mono', familyClass: 'font-label-12-mono', weightClass: 'font-medium', sampleText: 'mono-12 — 0xDEAD_BEEF_2024' },
  { label: 'copy-13-mono', role: 'Dense mono body', textClass: 'text-copy-13-mono', familyClass: 'font-copy-13-mono', sampleText: 'mono-13 — const answer = 42; // the quick brown fox' },
];

/* ---------- Spacing scale (DESIGN.md frontmatter spacing:) ----------
 *
 * Bars render at true scale via the token's CSS custom property
 * (`width: var(--space-N)`), projected in tokens.css `:root` from the DESIGN
 * spacing: frontmatter — so the visualization tracks the SSOT, not the
 * Tailwind default scale. The px/rem readout is read live from the rendered
 * bar's computed width.
 */

interface SpacingStep {
  label: string;
  varName: string;
}

const SPACING_SCALE: SpacingStep[] = [
  { label: 'space-1', varName: '--space-1' },
  { label: 'space-2', varName: '--space-2' },
  { label: 'space-3', varName: '--space-3' },
  { label: 'space-4', varName: '--space-4' },
  { label: 'space-6', varName: '--space-6' },
  { label: 'space-8', varName: '--space-8' },
  { label: 'space-10', varName: '--space-10' },
  { label: 'space-16', varName: '--space-16' },
  { label: 'space-24', varName: '--space-24' },
];

/* ---------- Radius scale (DESIGN.md frontmatter rounded:) ---------- */

interface RadiusStep {
  label: string;
  varName: string;
}

const RADIUS_SCALE: RadiusStep[] = [
  { label: 'control', varName: '--radius-control' },
  { label: 'card', varName: '--radius-card' },
  { label: 'popover', varName: '--radius-popover' },
  { label: 'fullscreen', varName: '--radius-fullscreen' },
  { label: 'pill', varName: '--radius-pill' },
];

/* ---------- Elevation scale (DESIGN.md §Elevation, V1.121 v0.4) ---------- */

interface ElevationToken {
  label: string;
  varName: string;
  usage: string;
}

const ELEVATION_LEVELS: ElevationToken[] = [
  { label: 'elevation-0', varName: '--shadow-elevation-0', usage: 'Flat / sunk into surface' },
  { label: 'elevation-1', varName: '--shadow-elevation-1', usage: 'Resting card / canvas node at rest' },
  { label: 'elevation-2', varName: '--shadow-elevation-2', usage: 'Hover / raised (interactive lift)' },
  { label: 'elevation-3', varName: '--shadow-elevation-3', usage: 'Popover / floating (menus, tooltips, command panels)' },
  { label: 'elevation-4', varName: '--shadow-elevation-4', usage: 'Modal / dragging' },
];

/** Legacy alias chain — zero consumer breakage (DESIGN.md §Elevation). */
const ELEVATION_ALIASES = [
  { label: 'shadow-card', varName: '--shadow-card', target: 'elevation-1' },
  { label: 'shadow-popover', varName: '--shadow-popover', target: 'elevation-3' },
  { label: 'shadow-modal', varName: '--shadow-modal', target: 'elevation-4' },
] as const;

/* ---------- Motion tokens (DESIGN.md §Motion, V1.121 v0.4) ---------- */

interface MotionToken {
  label: string;
  varName: string;
  usage: string;
}

const MOTION_DURATIONS: MotionToken[] = [
  { label: 'duration-instant', varName: '--duration-instant', usage: 'Table filtering, data refresh replacement' },
  { label: 'duration-state', varName: '--duration-state', usage: 'Hover / focus / pressed states' },
  { label: 'duration-popover', varName: '--duration-popover', usage: 'Menus, dropdowns, tooltips' },
  { label: 'duration-modal', varName: '--duration-modal', usage: 'Dialog open / close' },
  { label: 'duration-enter', varName: '--duration-enter', usage: 'Entering surfaces (popover content in, toast in)' },
  { label: 'duration-exit', varName: '--duration-exit', usage: 'Dismissing surfaces (exit is faster than enter)' },
];

const MOTION_EASINGS: MotionToken[] = [
  { label: 'ease-standard', varName: '--ease-standard', usage: 'Default UI ease' },
  { label: 'ease-emphasized', varName: '--ease-emphasized', usage: 'Modal / panel enter' },
];

/* ---------- Canvas tokens (V1.121 P3 — DESIGN.md §Canvas Surface) -------
 *
 * Live swatches for the canvas token families. Values resolve through
 * tokens.css `:root` (light) and `.dark` (dark) blocks — every swatch
 * re-reads on theme flip. The accent spine row uses the same border-l-[3px]
 * shape NodeChromeShell renders for `accent="<surface>"`; the ambient row
 * shows the actual dot-grid pattern at the live gap/dot-size metrics.
 */

interface CanvasToken {
  label: string;
  varName: string;
  /** When set, renders the swatch with the literal token as a border color. */
  asBorder?: boolean;
  /** When set, renders the swatch as a transparent fill (color-mix over page). */
  asTint?: boolean;
}

interface CanvasTokenGroup {
  title: string;
  /** Optional blurb shown under the group title. */
  hint?: string;
  tokens: CanvasToken[];
}

const CANVAS_TOKEN_GROUPS: CanvasTokenGroup[] = [
  {
    title: 'Ambient',
    hint:
      'Canvas surface chrome — the dot-grid sits on graphite ink in dark; light sits on silver-white. Grid gap / dot size are live metrics.',
    tokens: [
      { label: 'canvas-surface', varName: '--color-canvas-surface' },
      { label: 'canvas-grid', varName: '--color-canvas-grid' },
      { label: 'canvas-minimap', varName: '--color-canvas-minimap' },
    ],
  },
  {
    title: 'Node chrome',
    hint:
      'Shared node primitives — fill, border, and the selected-border + focus-ring pairing. Selection is never color-only (Draft §4.4 #6).',
    tokens: [
      { label: 'canvas-node-fill', varName: '--color-canvas-node-fill' },
      { label: 'canvas-node-fill-hover', varName: '--color-canvas-node-fill-hover' },
      { label: 'canvas-node-border', varName: '--color-canvas-node-border' },
      { label: 'canvas-node-border-selected', varName: '--color-canvas-node-border-selected' },
    ],
  },
  {
    title: 'Edges & ports',
    hint:
      'Stroke colors for relationship/transition edges and the connection port fill. Edge-hover is reserved for the v0.4 hover affordance.',
    tokens: [
      { label: 'canvas-edge', varName: '--color-canvas-edge' },
      { label: 'canvas-edge-hover', varName: '--color-canvas-edge-hover' },
      { label: 'canvas-port', varName: '--color-canvas-port' },
    ],
  },
  {
    title: 'Accent spines',
    hint:
      'Surface spines (strategy / outline / worldkb). Timeline surface accent lives in Canvas — Timeline accent spine; layer feel lives in Canvas — Layer accents. strategy=purple-700, outline=amber-700, worldkb=teal-700 (DESIGN.md §Canvas Surface).',
    tokens: [
      { label: 'canvas-strategy-accent', varName: '--color-canvas-strategy-accent' },
      { label: 'canvas-outline-accent', varName: '--color-canvas-outline-accent' },
      { label: 'canvas-worldkb-accent', varName: '--color-canvas-worldkb-accent' },
    ],
  },
  {
    title: 'Canvas — Timeline accent spine',
    hint:
      'Surface-level Timeline identity — cobalt signal (blue-700 ≡ brand-cyan). Distinct from per-layer accents.',
    tokens: [
      { label: 'canvas-timeline-accent', varName: '--color-canvas-timeline-accent' },
    ],
  },
  {
    title: 'Canvas — Layer accents',
    hint:
      'Intra-surface Brief / Narrative / Moment feel (V1.123 P4). Used on Timeline node icons and badges.',
    tokens: [
      { label: 'canvas-layer-brief-accent', varName: '--color-canvas-layer-brief-accent' },
      { label: 'canvas-layer-narrative-accent', varName: '--color-canvas-layer-narrative-accent' },
      { label: 'canvas-layer-moment-accent', varName: '--color-canvas-layer-moment-accent' },
    ],
  },
  {
    title: 'Canvas — Outline Timeline pins',
    hint:
      'Outline canvas when-axis pins/markers (not World Timeline card chrome).',
    tokens: [
      { label: 'canvas-outline-timeline-event-pin', varName: '--color-canvas-outline-timeline-event-pin' },
      { label: 'canvas-outline-timeline-marker', varName: '--color-canvas-outline-timeline-marker' },
    ],
  },
  {
    title: 'Soul Viz — Timeline axes',
    hint:
      'Soul visualization timeline axis geometry colors (light/dark differ).',
    tokens: [
      { label: 'soul-viz-timeline-axis-line', varName: '--color-soul-viz-timeline-axis-line' },
      { label: 'soul-viz-timeline-axis-tick', varName: '--color-soul-viz-timeline-axis-tick' },
      { label: 'soul-viz-timeline-axis-label', varName: '--color-soul-viz-timeline-axis-label' },
    ],
  },
  {
    title: 'Canvas — Write states',
    hint:
      'Live write-back indicators (dirty / conflict / success) and the stale background wash (DESIGN.md §Canvas Surface).',
    tokens: [
      { label: 'canvas-write-dirty', varName: '--color-canvas-write-dirty' },
      { label: 'canvas-write-conflict', varName: '--color-canvas-write-conflict' },
      { label: 'canvas-write-success', varName: '--color-canvas-write-success' },
      { label: 'canvas-write-stale-bg', varName: '--color-canvas-write-stale-bg' },
    ],
  },
  {
    title: 'Canvas — Outline surface',
    hint:
      'Outline canvas scene/beat/chapter chrome and volume fill (DESIGN.md §Canvas Outline Tokens).',
    tokens: [
      { label: 'canvas-outline-volume-fill', varName: '--color-canvas-outline-volume-fill' },
      { label: 'canvas-outline-chapter-card-status-pending', varName: '--color-canvas-outline-chapter-card-status-pending' },
      { label: 'canvas-outline-chapter-card-status-drafted', varName: '--color-canvas-outline-chapter-card-status-drafted' },
      { label: 'canvas-outline-chapter-card-status-completed', varName: '--color-canvas-outline-chapter-card-status-completed' },
      { label: 'canvas-outline-foreshadow-edge', varName: '--color-canvas-outline-foreshadow-edge' },
      { label: 'canvas-outline-conflict-marker', varName: '--color-canvas-outline-conflict-marker' },
      { label: 'canvas-outline-scene-fill', varName: '--color-canvas-outline-scene-fill' },
      { label: 'canvas-outline-scene-border', varName: '--color-canvas-outline-scene-border' },
      { label: 'canvas-outline-scene-status-drafted', varName: '--color-canvas-outline-scene-status-drafted' },
      { label: 'canvas-outline-scene-status-completed', varName: '--color-canvas-outline-scene-status-completed' },
      { label: 'canvas-outline-beat-fill', varName: '--color-canvas-outline-beat-fill' },
      { label: 'canvas-outline-beat-border', varName: '--color-canvas-outline-beat-border' },
    ],
  },
  {
    title: 'Canvas — World KB entity cards',
    hint:
      'World KB entity-card fill/stroke pairs including selected states (DESIGN.md components.canvas.worldkb-*).',
    tokens: [
      { label: 'canvas-worldkb-entity-card-fill-default', varName: '--color-canvas-worldkb-entity-card-fill-default' },
      { label: 'canvas-worldkb-entity-card-fill-hover', varName: '--color-canvas-worldkb-entity-card-fill-hover' },
      { label: 'canvas-worldkb-entity-card-fill-selected', varName: '--color-canvas-worldkb-entity-card-fill-selected' },
      { label: 'canvas-worldkb-entity-card-stroke-default', varName: '--color-canvas-worldkb-entity-card-stroke-default' },
      { label: 'canvas-worldkb-entity-card-stroke-selected', varName: '--color-canvas-worldkb-entity-card-stroke-selected' },
      { label: 'canvas-worldkb-focus-ring', varName: '--color-canvas-worldkb-focus-ring' },
      { label: 'canvas-worldkb-nonspatial-row-highlight', varName: '--color-canvas-worldkb-nonspatial-row-highlight' },
    ],
  },
  {
    title: 'Canvas — World KB promotion & anchors',
    hint:
      'World KB promotion states and source-anchor edge/node fills (DESIGN.md components.canvas.worldkb-*).',
    tokens: [
      { label: 'canvas-worldkb-promotion-pending', varName: '--color-canvas-worldkb-promotion-pending' },
      { label: 'canvas-worldkb-promotion-confirmed', varName: '--color-canvas-worldkb-promotion-confirmed' },
      { label: 'canvas-worldkb-promotion-rejected', varName: '--color-canvas-worldkb-promotion-rejected' },
      { label: 'canvas-worldkb-promotion-merged', varName: '--color-canvas-worldkb-promotion-merged' },
      { label: 'canvas-worldkb-source-anchor-edge', varName: '--color-canvas-worldkb-source-anchor-edge' },
      { label: 'canvas-worldkb-source-anchor-node', varName: '--color-canvas-worldkb-source-anchor-node' },
      { label: 'canvas-worldkb-computable-badge', varName: '--color-canvas-worldkb-computable-badge' },
    ],
  },
  {
    title: 'Canvas — World KB relationships',
    hint:
      'World KB relationship edges (default / symmetric / custom), confidence tints, and grounded/asserted badges (DESIGN.md components.canvas.worldkb-relationship-*).',
    tokens: [
      { label: 'canvas-worldkb-relationship-edge', varName: '--color-canvas-worldkb-relationship-edge' },
      { label: 'canvas-worldkb-relationship-edge-default', varName: '--color-canvas-worldkb-relationship-edge-default' },
      { label: 'canvas-worldkb-relationship-edge-symmetric', varName: '--color-canvas-worldkb-relationship-edge-symmetric' },
      { label: 'canvas-worldkb-relationship-edge-custom', varName: '--color-canvas-worldkb-relationship-edge-custom' },
      { label: 'canvas-worldkb-relationship-confidence-low', varName: '--color-canvas-worldkb-relationship-confidence-low' },
      { label: 'canvas-worldkb-relationship-confidence-mid', varName: '--color-canvas-worldkb-relationship-confidence-mid' },
      { label: 'canvas-worldkb-relationship-confidence-high', varName: '--color-canvas-worldkb-relationship-confidence-high' },
      { label: 'canvas-worldkb-relationship-grounded-badge', varName: '--color-canvas-worldkb-relationship-grounded-badge' },
      { label: 'canvas-worldkb-relationship-asserted-badge', varName: '--color-canvas-worldkb-relationship-asserted-badge' },
      { label: 'canvas-worldkb-relationship-inspector-fill', varName: '--color-canvas-worldkb-relationship-inspector-fill' },
    ],
  },
  {
    title: 'Canvas — World KB conflict markers',
    hint:
      'World KB conflict marker + its translucent fill (DESIGN.md components.canvas.worldkb-*).',
    tokens: [
      { label: 'canvas-worldkb-conflict-marker', varName: '--color-canvas-worldkb-conflict-marker' },
      { label: 'canvas-worldkb-conflict-marker-fill', varName: '--color-canvas-worldkb-conflict-marker-fill' },
    ],
  },
  {
    title: 'Soul Viz — Keyword cluster nodes',
    hint:
      'Soul keyword-cluster node fill/stroke/label (DESIGN.md components.soul-viz-keyword-cluster-node).',
    tokens: [
      { label: 'soul-viz-keyword-cluster-node-fill', varName: '--color-soul-viz-keyword-cluster-node-fill' },
      { label: 'soul-viz-keyword-cluster-node-stroke', varName: '--color-soul-viz-keyword-cluster-node-stroke' },
      { label: 'soul-viz-keyword-cluster-node-label', varName: '--color-soul-viz-keyword-cluster-node-label' },
    ],
  },
  {
    title: 'Soul Viz — Drift band',
    hint:
      'Soul drift-band fills (six hue bands), step stroke and label (DESIGN.md components.soul-viz-drift-band).',
    tokens: [
      ...['', '-2', '-3', '-4', '-5', '-6'].map((s) => ({
        label: `soul-viz-drift-band-fill${s}`,
        varName: `--color-soul-viz-drift-band-fill${s}`,
      })),
      { label: 'soul-viz-drift-band-step-stroke', varName: '--color-soul-viz-drift-band-step-stroke' },
      { label: 'soul-viz-drift-band-label', varName: '--color-soul-viz-drift-band-label' },
    ],
  },
  {
    title: 'Soul narrative & growth curve',
    hint:
      'Soul narrative prose color and growth-curve stroke (DESIGN.md §Canvas Surface / SOUL).',
    tokens: [
      { label: 'soul-narrative-prose', varName: '--color-soul-narrative-prose' },
      { label: 'soul-growth-curve-stroke', varName: '--color-soul-growth-curve-stroke' },
    ],
  },
];

/** Node-width family (P0 + P3) — five --canvas-node-width-* slots. */
interface CanvasWidthToken {
  label: string;
  /** Tailwind utility name; the chip's min-width resolves through the var. */
  utilityClass: string;
  varName: string;
}

const CANVAS_NODE_WIDTHS: CanvasWidthToken[] = [
  { label: 'strategy-root', utilityClass: 'min-w-canvas-node-strategy-root', varName: '--canvas-node-width-strategy-root' },
  { label: 'strategy-primary', utilityClass: 'min-w-canvas-node-strategy-primary', varName: '--canvas-node-width-strategy-primary' },
  { label: 'strategy-secondary', utilityClass: 'min-w-canvas-node-strategy-secondary', varName: '--canvas-node-width-strategy-secondary' },
  { label: 'outline-scene-beat', utilityClass: 'min-w-canvas-node-outline-scene-beat', varName: '--canvas-node-width-outline-scene-beat' },
  { label: 'default', utilityClass: 'min-w-canvas-node-default', varName: '--canvas-node-width-default' },
];

/* ---------- Structural scalars (DESIGN.md components.*) ----------
 *
 * Non-color sizing/geometry projections: footer-profile, setup-wizard,
 * sidebar-nav, dialog/sheet/listbox, reading-annotation/chrome. Read live via
 * probe elements so the reader sees the actual generated value per theme.
 */

interface ScalarToken {
  label: string;
  varName: string;
  usage: string;
}

const FOOTER_PROFILE_SCALARS: ScalarToken[] = [
  { label: 'avatar-size', varName: '--color-footer-profile-avatar-size', usage: 'Avatar diameter (px)' },
  { label: 'avatar-rounded', varName: '--color-footer-profile-avatar-rounded', usage: 'Avatar corner radius' },
  { label: 'avatar-bg', varName: '--color-footer-profile-avatar-bg', usage: 'Avatar resting fill' },
  { label: 'avatar-bg-hover', varName: '--color-footer-profile-avatar-bg-hover', usage: 'Avatar hover fill' },
  { label: 'avatar-bg-active', varName: '--color-footer-profile-avatar-bg-active', usage: 'Avatar selected fill' },
  { label: 'avatar-text', varName: '--color-footer-profile-avatar-text', usage: 'Avatar label text' },
  { label: 'avatar-text-active', varName: '--color-footer-profile-avatar-text-active', usage: 'Avatar selected text' },
  { label: 'avatar-fallback-bg', varName: '--color-footer-profile-avatar-fallback-bg', usage: 'Avatar fallback initials fill' },
  { label: 'avatar-fallback-text', varName: '--color-footer-profile-avatar-fallback-text', usage: 'Avatar fallback initials text' },
  { label: 'add-button-bg', varName: '--color-footer-profile-add-button-bg', usage: 'Add-profile button fill' },
  { label: 'add-button-border', varName: '--color-footer-profile-add-button-border', usage: 'Add-profile button border' },
  { label: 'add-button-text', varName: '--color-footer-profile-add-button-text', usage: 'Add-profile button text' },
  { label: 'add-button-hover-bg', varName: '--color-footer-profile-add-button-hover-bg', usage: 'Add-profile hover fill' },
  { label: 'add-button-hover-border', varName: '--color-footer-profile-add-button-hover-border', usage: 'Add-profile hover border' },
  { label: 'add-button-hover-text', varName: '--color-footer-profile-add-button-hover-text', usage: 'Add-profile hover text' },
  { label: 'gap', varName: '--color-footer-profile-gap', usage: 'Inter-avatar gap' },
];

const SETUP_WIZARD_STEP_SCALARS: ScalarToken[] = [
  { label: 'step-row-height', varName: '--color-setup-wizard-step-row-height', usage: 'Step row height' },
  { label: 'step-circle-size', varName: '--color-setup-wizard-step-circle-size', usage: 'Status circle diameter' },
  { label: 'step-circle-active-bg', varName: '--color-setup-wizard-step-circle-active-bg', usage: 'Active step circle fill' },
  { label: 'step-circle-active-text', varName: '--color-setup-wizard-step-circle-active-text', usage: 'Active step circle text' },
  { label: 'step-circle-complete-bg', varName: '--color-setup-wizard-step-circle-complete-bg', usage: 'Complete step fill' },
  { label: 'step-circle-complete-text', varName: '--color-setup-wizard-step-circle-complete-text', usage: 'Complete step text' },
  { label: 'step-circle-pending-bg', varName: '--color-setup-wizard-step-circle-pending-bg', usage: 'Pending step fill' },
  { label: 'step-circle-pending-text', varName: '--color-setup-wizard-step-circle-pending-text', usage: 'Pending step text' },
  { label: 'step-connector', varName: '--color-setup-wizard-step-connector', usage: 'Step connector stroke' },
  { label: 'step-label-active-color', varName: '--color-setup-wizard-step-label-active-color', usage: 'Active step label' },
  { label: 'step-label-pending-color', varName: '--color-setup-wizard-step-label-pending-color', usage: 'Pending step label' },
  { label: 'step-label-typography', varName: '--color-setup-wizard-step-label-typography', usage: 'Step label font-size' },
];

const SETUP_WIZARD_SURFACE_SCALARS: ScalarToken[] = [
  { label: 'card-bg', varName: '--color-setup-wizard-surface-card-bg', usage: 'Wizard card fill' },
  { label: 'card-border', varName: '--color-setup-wizard-surface-card-border', usage: 'Wizard card border' },
  { label: 'step-panel-width', varName: '--color-setup-wizard-surface-step-panel-width', usage: 'Left step-panel width' },
  { label: 'step-panel-right-divider', varName: '--color-setup-wizard-surface-step-panel-right-divider', usage: 'Step-panel divider' },
  { label: 'step-panel-padding-x', varName: '--color-setup-wizard-surface-step-panel-padding-x', usage: 'Step-panel horizontal padding' },
  { label: 'step-panel-padding-y', varName: '--color-setup-wizard-surface-step-panel-padding-y', usage: 'Step-panel vertical padding' },
  { label: 'content-panel-padding-x', varName: '--color-setup-wizard-surface-content-panel-padding-x', usage: 'Content-panel horizontal padding' },
  { label: 'content-panel-padding-y', varName: '--color-setup-wizard-surface-content-panel-padding-y', usage: 'Content-panel vertical padding' },
  { label: 'input-row-bg', varName: '--color-setup-wizard-surface-input-row-bg', usage: 'Path/input row fill' },
  { label: 'input-row-border', varName: '--color-setup-wizard-surface-input-row-border', usage: 'Path/input row border' },
  { label: 'input-row-gap', varName: '--color-setup-wizard-surface-input-row-gap', usage: 'Input-row gap' },
  { label: 'input-row-min-height', varName: '--color-setup-wizard-surface-input-row-min-height', usage: 'Input-row min height' },
  { label: 'input-row-rounded', varName: '--color-setup-wizard-surface-input-row-rounded', usage: 'Input-row radius' },
  { label: 'input-row-label-color', varName: '--color-setup-wizard-surface-input-row-label-color', usage: 'Input-row label' },
  { label: 'input-row-path-color', varName: '--color-setup-wizard-surface-input-row-path-color', usage: 'Input-row path text' },
  { label: 'input-row-icon-color', varName: '--color-setup-wizard-surface-input-row-icon-color', usage: 'Input-row icon' },
  { label: 'input-row-padding-x', varName: '--color-setup-wizard-surface-input-row-padding-x', usage: 'Input-row horizontal padding' },
  { label: 'input-row-padding-y', varName: '--color-setup-wizard-surface-input-row-padding-y', usage: 'Input-row vertical padding' },
  { label: 'cta-primary-max-width', varName: '--color-setup-wizard-surface-cta-primary-max-width', usage: 'Primary CTA max width' },
  { label: 'cta-container-gap', varName: '--color-setup-wizard-surface-cta-container-gap', usage: 'CTA container gap' },
];

const CHROME_SCALARS: ScalarToken[] = [
  { label: 'sidebar-nav-width', varName: '--sidebar-nav-width', usage: 'App sidebar width' },
  { label: 'sidebar-nav-item-height', varName: '--sidebar-nav-item-height', usage: 'Sidebar nav item height' },
  { label: 'dialog-width', varName: '--dialog-width', usage: 'Dialog width (calc-based)' },
  { label: 'dialog-max-height', varName: '--dialog-max-height', usage: 'Dialog max height' },
  { label: 'dialog-max-width', varName: '--color-dialog-max-width', usage: 'Dialog max width' },
  { label: 'sheet-width', varName: '--sheet-width', usage: 'Sheet width (min-based)' },
  { label: 'listbox-max-height', varName: '--color-listbox-max-height', usage: 'Listbox max height' },
];

const READING_CHROME_SCALARS: ScalarToken[] = [
  { label: 'reading-annotation-inspector-background', varName: '--color-reading-annotation-inspector-background', usage: 'Annotation inspector surface' },
  { label: 'reading-annotation-inspector-border', varName: '--color-reading-annotation-inspector-border', usage: 'Annotation inspector border' },
  { label: 'reading-annotation-inspector-text', varName: '--color-reading-annotation-inspector-text', usage: 'Annotation inspector text' },
  { label: 'reading-selection-toolbar-background', varName: '--color-reading-selection-toolbar-background', usage: 'Selection toolbar surface' },
  { label: 'reading-selection-toolbar-border', varName: '--color-reading-selection-toolbar-border', usage: 'Selection toolbar border' },
  { label: 'reading-selection-toolbar-text', varName: '--color-reading-selection-toolbar-text', usage: 'Selection toolbar text' },
  { label: 'reading-annotation-highlight-yellow-background', varName: '--color-reading-annotation-highlight-yellow-background', usage: 'Yellow highlight fill' },
  { label: 'reading-annotation-highlight-yellow-text', varName: '--color-reading-annotation-highlight-yellow-text', usage: 'Yellow highlight text' },
  { label: 'reading-annotation-highlight-blue-background', varName: '--color-reading-annotation-highlight-blue-background', usage: 'Blue highlight fill' },
  { label: 'reading-annotation-highlight-blue-text', varName: '--color-reading-annotation-highlight-blue-text', usage: 'Blue highlight text' },
  { label: 'reading-annotation-highlight-green-background', varName: '--color-reading-annotation-highlight-green-background', usage: 'Green highlight fill' },
  { label: 'reading-annotation-highlight-green-text', varName: '--color-reading-annotation-highlight-green-text', usage: 'Green highlight text' },
  { label: 'reading-annotation-highlight-pink-background', varName: '--color-reading-annotation-highlight-pink-background', usage: 'Pink highlight fill' },
  { label: 'reading-annotation-highlight-pink-text', varName: '--color-reading-annotation-highlight-pink-text', usage: 'Pink highlight text' },
];

interface ReadingChromeSpecimen {
  label: string;
  /** Flattened --reading-chrome-* property name in generated tokens.css. */
  varName: string;
  sample: string;
  className: string;
}

const READING_CHROME_SPECIMENS: ReadingChromeSpecimen[] = [
  {
    label: 'novel · chapter-title',
    varName: '--reading-chrome-novel-chapter-title-color',
    sample: 'Chapter Twelve — The Tidal Gate',
    className: 'font-display text-[28px] font-semibold',
  },
  {
    label: 'novel · epigraph',
    varName: '--reading-chrome-novel-epigraph-color',
    sample: '“Water remembers every shore.”',
    className: 'italic text-right',
  },
  {
    label: 'essay · section-heading',
    varName: '--reading-chrome-essay-section-heading-color',
    sample: 'The Long Descent',
    className: 'font-medium',
  },
  {
    label: 'essay · blockquote',
    varName: '--reading-chrome-essay-blockquote-color',
    sample: '“Precision is patience, applied.”',
    className: 'italic',
  },
  {
    label: 'essay · footnote-marker',
    varName: '--reading-chrome-essay-footnote-marker-color',
    sample: '1',
    className: 'align-super text-xs',
  },
  {
    label: 'script · character-name',
    varName: '--reading-chrome-script-character-name-color',
    sample: 'MARSH',
    className: 'font-bold text-sm uppercase',
  },
  {
    label: 'script · parenthetical',
    varName: '--reading-chrome-script-parenthetical-color',
    sample: '(glances at the folded chart)',
    className: 'italic',
  },
  {
    label: 'script · scene-heading',
    varName: '--reading-chrome-script-scene-heading-color',
    sample: 'EXT. TIDEPOOL — NIGHT',
    className: 'font-bold text-sm uppercase',
  },
  {
    label: 'game-bible · term-link',
    varName: '--reading-chrome-game-bible-term-link-color',
    sample: 'tide-mark',
    className: 'underline decoration-dotted',
  },
  {
    label: 'game-bible · category-badge',
    varName: '--reading-chrome-game-bible-category-badge-color',
    sample: 'FLORA',
    className: 'font-semibold',
  },
];

/* ------------------------------------------------------------------ */
/*  Helpers                                                             */
/* ------------------------------------------------------------------ */

/**
 * Read the computed background-color that results from applying a CSS
 * custom property to a DOM element.  This resolves *through* var()
 * chains (e.g. `var(--nexus-brand-deep-blue)`) and returns the final
 * rgb / rgba string the browser paints.
 */
function resolveSwatchColor(varName: string): string {
  const el = document.createElement('div');
  el.style.backgroundColor = `var(${varName})`;
  el.style.display = 'none';
  document.body.appendChild(el);
  const computed = getComputedStyle(el).backgroundColor;
  document.body.removeChild(el);
  return computed;
}

/**
 * Read the computed box-shadow for a shadow CSS custom property, resolving
 * through the alias chain (`--shadow-card` → `var(--shadow-elevation-1)`)
 * to the value the browser actually paints.
 */
function resolveBoxShadow(varName: string): string {
  const el = document.createElement('div');
  el.style.boxShadow = `var(${varName})`;
  el.style.display = 'none';
  document.body.appendChild(el);
  const computed = getComputedStyle(el).boxShadow;
  document.body.removeChild(el);
  return computed;
}

/**
 * Read the computed width that results from assigning a CSS custom property
 * carrying a *length* (not a color) to a probe element. Resolves through
 * var() chains and returns the final px string the browser computes, matching
 * the existing length readouts (background-size / metrics) rather than forcing
 * a length into a color property.
 */
function resolveSwatchLength(varName: string): string {
  const el = document.createElement('div');
  el.style.width = `var(${varName})`;
  el.style.display = 'none';
  document.body.appendChild(el);
  const computed = getComputedStyle(el).width;
  document.body.removeChild(el);
  return computed;
}

/**
 * Read a computed property produced by assigning a CSS custom property to a
 * probe element — live from the token's declared value, not a hardcoded copy.
 * Returns '' when the var is not defined (e.g. jsdom without CSS).
 */
function useComputedVarValue(
  varName: string,
  property: 'transitionDuration' | 'transitionTimingFunction',
): string {
  const [value, setValue] = useState('');
  useEffect(() => {
    const el = document.createElement('div');
    el.style[property] = `var(${varName})`;
    el.style.display = 'none';
    document.body.appendChild(el);
    const computed = getComputedStyle(el)[property];
    document.body.removeChild(el);
    setValue(computed ?? '');
  }, [varName, property]);
  return value;
}

/** Live prefers-reduced-motion state (drives the demo honesty note). */
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduced(mql.matches);
    update();
    mql.addEventListener('change', update);
    return () => mql.removeEventListener('change', update);
  }, []);
  return reduced;
}

/** Format a px value as rem (1rem = 16px). */
function pxToRem(px: number): string {
  return `${px / 16}rem`;
}

/** Trim a ratio to at most `decimals` places without trailing zeros. */
function trimRatio(value: number, decimals: number): string {
  return String(Number(value.toFixed(decimals)));
}

/* ------------------------------------------------------------------ */
/*  Sub-components                                                      */
/* ------------------------------------------------------------------ */

function SectionHeading({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h3 id={id} className="text-heading-20 font-semibold text-gray-1000 mb-4 pt-8 scroll-mt-16">
      {children}
    </h3>
  );
}

function ColorSwatch({ token }: { token: ColorToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveSwatchColor(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      // Defer to next frame so CSS vars have been swapped.
      const rafId = requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div className="flex flex-col gap-2" data-testid={`color-swatch-${token.label}`}>
      <div
        className="w-full aspect-[3/2] rounded-card border border-gray-alpha-400"
        style={{ backgroundColor: `var(${token.varName})` }}
      />
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000 truncate">{token.label}</span>
        <span className="text-copy-13 text-gray-700 truncate font-mono">{computed}</span>
      </div>
    </div>
  );
}

/**
 * Typography specimen row. Renders the specimen with the literal token
 * classes and reads font-size / weight / font-family / line-height /
 * letter-spacing back from the computed style (live) for the metrics line.
 */
function TypoRow({ specimen }: { specimen: TypoSpecimen }) {
  const { resolvedTheme } = useTheme();
  const specimenRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState('');

  useEffect(() => {
    const el = specimenRef.current;
    if (!el) return;
    // Re-read after the theme swap (next frame) so a light→dark change with
    // differing typography metrics/family re-projects the live readout and
    // keeps the labels agreeing with the rendered specimen.
    const rafId = requestAnimationFrame(() => {
      const cs = getComputedStyle(el);
      const fontSize = cs.fontSize ?? '';
      const sizePx = parseFloat(fontSize);
      if (!fontSize.endsWith('px') || Number.isNaN(sizePx) || sizePx === 0) {
        setMetrics('');
        return;
      }
      const parts: string[] = [fontSize];
      if (cs.fontWeight) parts.push(`weight ${cs.fontWeight}`);
      const family = cs.fontFamily ?? '';
      if (family) parts.push(`family ${family}`);
      const lineHeight = cs.lineHeight ?? '';
      if (lineHeight.endsWith('px')) {
        parts.push(`line-height ${trimRatio(parseFloat(lineHeight) / sizePx, 2)} (${lineHeight})`);
      } else if (lineHeight && lineHeight !== 'normal') {
        parts.push(`line-height ${lineHeight}`);
      }
      const tracking = cs.letterSpacing ?? '';
      if (tracking.endsWith('px')) {
        parts.push(`tracking ${trimRatio(parseFloat(tracking) / sizePx, 3)}em`);
      } else if (tracking === 'normal') {
        parts.push('tracking 0');
      }
      setMetrics(parts.join(' · '));
    });
    return () => cancelAnimationFrame(rafId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme]);

  const className = [
    specimen.textClass,
    specimen.familyClass,
    specimen.weightClass ?? '',
    'text-gray-1000',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div
      data-testid={`typo-row-${specimen.label}`}
      className="flex flex-col sm:flex-row sm:items-baseline gap-2 py-4 border-b border-gray-alpha-200 last:border-b-0"
    >
      <div className="w-44 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{specimen.label}</span>
        <span className="text-label-12-mono font-mono text-gray-500">{specimen.familyClass}</span>
        <span className="text-copy-13 text-gray-600">{specimen.role}</span>
      </div>
      <div className="flex-1 min-w-0">
        <div ref={specimenRef} className={className}>
          {specimen.sampleText}
        </div>
        {metrics && (
          <div className="text-copy-13-mono font-mono text-gray-500 mt-1">{metrics}</div>
        )}
      </div>
    </div>
  );
}

/**
 * Spacing bar rendered at true scale — the bar's width is the token's CSS
 * variable itself, so what you see is the token. The px/rem readout is read
 * live from the rendered bar.
 */
function SpacingBar({ step }: { step: SpacingStep }) {
  const barRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState('');

  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    setWidth(getComputedStyle(el).width);
  }, []);

  const px = parseFloat(width);
  const hasValue = width.endsWith('px') && !Number.isNaN(px) && px > 0;

  return (
    <div data-testid={`spacing-row-${step.label}`} className="flex items-center gap-4 py-2">
      <div className="w-32 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{step.label}</span>
        <span className="text-copy-13-mono font-mono text-gray-500">{step.varName}</span>
      </div>
      <div className="flex-1 flex items-center gap-3">
        <div
          ref={barRef}
          className="h-6 bg-blue-700 rounded-control"
          style={{ width: `var(${step.varName})` }}
        />
        {hasValue && (
          <span className="text-copy-13 text-gray-500 font-mono shrink-0">
            {width} / {pxToRem(px)}
          </span>
        )}
      </div>
    </div>
  );
}

/** Radius swatch — the box's corner radius is the token's CSS variable. */
function RadiusBox({ step }: { step: RadiusStep }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const [radius, setRadius] = useState('');

  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    setRadius(getComputedStyle(el).borderRadius);
  }, []);

  return (
    <div data-testid={`radius-box-${step.label}`} className="flex flex-col items-center gap-3">
      <div
        ref={boxRef}
        className="w-20 h-20 bg-gray-100 border border-gray-alpha-400"
        style={{ borderRadius: `var(${step.varName})` }}
      />
      <div className="flex flex-col items-center gap-0.5">
        <span className="text-label-14 text-gray-1000">{step.label}</span>
        <span className="text-copy-13-mono font-mono text-gray-500">{step.varName}</span>
        {radius && <span className="text-copy-13 text-gray-600 font-mono">{radius}</span>}
      </div>
    </div>
  );
}

/** Elevation swatch — shadow applied via the live CSS variable. */
function ElevationCard({ token }: { token: ElevationToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveBoxShadow(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const rafId = requestAnimationFrame(() => setComputed(resolveBoxShadow(token.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div data-testid={`elevation-swatch-${token.label}`} className="flex flex-col gap-3">
      <div
        className="w-full aspect-[16/10] rounded-card bg-background-100 border border-gray-alpha-200 flex items-center justify-center"
        style={{ boxShadow: `var(${token.varName})` } as CSSProperties}
      >
        <span className="text-copy-14 text-gray-500 font-mono">{token.label}</span>
      </div>
      <div className="flex flex-col gap-0.5">
        <span className="text-label-14 text-gray-1000">{token.label}</span>
        <span className="text-copy-13 text-gray-600">{token.usage}</span>
        <span className="text-copy-13 text-gray-500 font-mono break-all">{computed}</span>
      </div>
    </div>
  );
}

/** Alias-chain row — proves the legacy name resolves onto the scale. */
function ElevationAliasRow({ alias }: { alias: (typeof ELEVATION_ALIASES)[number] }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveBoxShadow(alias.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const rafId = requestAnimationFrame(() => setComputed(resolveBoxShadow(alias.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, alias.varName]);

  return (
    <div className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-2 border-b border-gray-alpha-200 last:border-b-0">
      <span className="text-label-14 font-medium text-gray-1000 w-40 shrink-0">
        {alias.label}
      </span>
      <span className="text-copy-13-mono font-mono text-gray-600 w-40 shrink-0">
        → {alias.target}
      </span>
      <span className="text-copy-13 text-gray-500 font-mono break-all">{computed}</span>
    </div>
  );
}

/** Motion token row — value read live from the token's CSS variable. */
function MotionRow({
  token,
  property,
}: {
  token: MotionToken;
  property: 'transitionDuration' | 'transitionTimingFunction';
}) {
  const value = useComputedVarValue(token.varName, property);
  return (
    <div
      data-testid={`motion-row-${token.label}`}
      className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-2 border-b border-gray-alpha-200 last:border-b-0"
    >
      <span className="text-label-14 font-medium text-gray-1000 w-44 shrink-0">{token.label}</span>
      <span className="text-copy-13-mono font-mono text-gray-700 w-56 shrink-0 break-all">
        {value || token.varName}
      </span>
      <span className="text-copy-13 text-gray-600">{token.usage}</span>
    </div>
  );
}

/**
 * Canvas color swatch — resolves through tokens.css `:root` (light) and
 * `.dark` (dark) blocks. Re-reads on theme flip so the visual reader can
 * verify both themes from the same gallery. The optional `asBorder` /
 * `asTint` modes let the swatch represent tokens that are typically used
 * as borders or translucent tints (so the reader sees the token in its
 * realistic context, not as a flat fill).
 */
function CanvasColorSwatch({ token }: { token: CanvasToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveSwatchColor(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const rafId = requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  const swatchStyle: CSSProperties = {
    backgroundColor: `var(${token.varName})`,
  };

  return (
    <div className="flex flex-col gap-2">
      <div
        className="w-full aspect-[3/2] rounded-card border border-gray-alpha-400"
        style={swatchStyle}
      />
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000 truncate">{token.label}</span>
        <span className="text-copy-13 text-gray-700 truncate font-mono">{computed}</span>
      </div>
    </div>
  );
}

/**
 * Ambient dot-grid swatch — renders the actual canvas dot-grid pattern at
 * the live grid-gap / grid-dot-size metrics. The reader sees the same
 * texture the App canvas paints, in both themes.
 */
function CanvasAmbientGridSwatch() {
  const { resolvedTheme } = useTheme();
  const [gap, setGap] = useState('20px');
  const [dot, setDot] = useState('1.5px');

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const rafId = requestAnimationFrame(() => {
      setGap(resolveSwatchLength('--color-canvas-grid-gap') || '20px');
      setDot(resolveSwatchLength('--color-canvas-grid-dot-size') || '1.5px');
    });
    return () => cancelAnimationFrame(rafId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme]);

  return (
    <div className="flex flex-col gap-2" data-testid="canvas-ambient-grid-swatch">
      <div
        className="w-full aspect-[3/2] rounded-card border border-gray-alpha-400 bg-canvas-surface"
        style={{
          backgroundImage: `radial-gradient(var(--color-canvas-grid) ${dot}, transparent ${dot})`,
          backgroundSize: `${gap} ${gap}`,
        }}
      />
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000">canvas dot-grid</span>
        <span className="text-copy-13 text-gray-700 font-mono">
          gap {gap} · dot {dot}
        </span>
      </div>
    </div>
  );
}

/**
 * Accent spine swatch — renders the surface's accent token as the same
 * 3px border-l stripe NodeChromeShell applies for `accent="<surface>"`.
 * Mirrors the spine shape so the visual reader sees the token in its
 * realistic context.
 */
function CanvasAccentSpineSwatch({ token }: { token: CanvasToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState<string>(() => resolveSwatchColor(token.varName));

  useEffect(() => {
    if (typeof window !== 'undefined') {
      const rafId = requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
      return () => cancelAnimationFrame(rafId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div className="flex flex-col gap-2">
      <div
        className="w-full aspect-[3/2] rounded-card bg-background-100 shadow-card"
        style={{
          borderLeft: `3px solid var(${token.varName})`,
        }}
      />
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000 truncate">{token.label}</span>
        <span className="text-copy-13 text-gray-700 truncate font-mono">{computed}</span>
      </div>
    </div>
  );
}

/**
 * Node-width utility chip — the chip's `min-width` resolves through the
 * --canvas-node-width-* CSS variable via the named utility class. The
 * rendered min-width is read live so the reader sees the actual px value
 * of the token.
 */
function CanvasNodeWidthSwatch({ token }: { token: CanvasWidthToken }) {
  const chipRef = useRef<HTMLDivElement>(null);
  const [minWidth, setMinWidth] = useState('');

  useEffect(() => {
    const el = chipRef.current;
    if (!el) return;
    setMinWidth(getComputedStyle(el).minWidth);
  }, []);

  return (
    <div
      data-testid={`canvas-node-width-swatch-${token.label}`}
      className="flex flex-col gap-2"
    >
      <div
        ref={chipRef}
        className={[
          'h-12 rounded-control border border-gray-alpha-300 bg-background-100',
          'flex items-center justify-center px-3',
          'text-label-12 font-mono text-gray-700',
          token.utilityClass,
        ].join(' ')}
      >
        {token.label}
      </div>
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000 truncate">
          {token.utilityClass}
        </span>
        <span className="text-copy-13 text-gray-700 truncate font-mono">
          {token.varName} {minWidth && `· ${minWidth}`}
        </span>
      </div>
    </div>
  );
}

const DEMO_BUTTON_CLASS =
  'px-3 py-1.5 rounded-control border border-gray-alpha-400 bg-background-100 text-button-14 font-button text-gray-1000 hover:bg-gray-alpha-100 transition-colors duration-state ease-standard motion-reduce:transition-none';

/**
 * Card hover-lift recipe (DESIGN.md §Motion / §Elevation): rest elevation-1,
 * hover elevation-2 + translateY(-1px) over 160ms ease-standard, pressed
 * returns to elevation-1. Reduced motion: instant state change, no
 * transform/opacity animation (motion-reduce guards).
 */
function HoverLiftDemo() {
  return (
    <div
      data-testid="motion-demo-lift"
      tabIndex={0}
      className="rounded-card border border-gray-alpha-300 bg-background-100 p-5 shadow-elevation-1 transition-all duration-popover ease-standard hover:-translate-y-px hover:shadow-elevation-2 focus-visible:-translate-y-px focus-visible:shadow-elevation-2 active:translate-y-0 active:shadow-elevation-1 motion-reduce:transition-none motion-reduce:transform-none"
    >
      <p className="text-label-14 font-medium text-gray-1000 mb-1">Card hover lift</p>
      <p className="text-copy-13 text-gray-600">
        Rest <code>elevation-1</code> → hover <code>elevation-2</code> + <code>translateY(-1px)</code>,
        160ms <code>ease-standard</code>; pressed returns to <code>elevation-1</code>.
      </p>
    </div>
  );
}

/**
 * Popover enter/exit recipe: enter opacity + scale(0.98 → 1) with
 * duration-enter (200ms) ease-standard; exit fades with duration-exit
 * (140ms). Reduced motion collapses both to an instant state change.
 */
function EnterExitDemo() {
  const [visible, setVisible] = useState(true);

  const replay = () => {
    setVisible(false);
    // Outlasts duration-exit (140ms) so the exit completes before re-enter.
    window.setTimeout(() => setVisible(true), 280);
  };

  return (
    <div>
      <div className="flex flex-wrap gap-2 mb-4">
        <button type="button" data-testid="motion-demo-replay" className={DEMO_BUTTON_CLASS} onClick={replay}>
          Replay enter
        </button>
        <button
          type="button"
          data-testid="motion-demo-dismiss"
          className={DEMO_BUTTON_CLASS}
          onClick={() => setVisible(false)}
        >
          Dismiss
        </button>
      </div>
      <div
        data-testid="motion-demo-enter-exit"
        className={[
          'rounded-popover border border-gray-alpha-300 bg-background-100 p-4 shadow-elevation-3',
          'transition-all ease-standard motion-reduce:transition-none motion-reduce:transform-none',
          visible ? 'opacity-100 scale-100 duration-enter' : 'opacity-0 scale-[0.98] duration-exit',
        ].join(' ')}
      >
        <p className="text-label-14 font-medium text-gray-1000 mb-1">Popover enter / exit</p>
        <p className="text-copy-13 text-gray-600">
          Enter: opacity + <code>scale(0.98 → 1)</code>, <code>duration-enter</code> (200ms){' '}
          <code>ease-standard</code>. Exit: opacity out, <code>duration-exit</code> (140ms).
        </p>
      </div>
    </div>
  );
}

/**
 * Disabled-wash chip (DESIGN.md §States, R-V1182P0-002 / v1.183 P0 AR-1) —
 * renders the shared disabled-state wash at the live
 * --color-states-disabled-opacity value (NOT a COLOR_GROUPS swatch:
 * ColorSwatch assumes color-valued vars; this is a scalar opacity). The
 * chip is a realistic disabled-button context so the reader sees the wash
 * the way EmptyCreateCard and the disabled:opacity-disabled utility paint
 * it. Re-resolves on theme flip (the wash is theme-independent, but the
 * chip's surface colors are not).
 */
function DisabledWashDemo() {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState('');

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const el = document.createElement('div');
    el.style.opacity = 'var(--color-states-disabled-opacity)';
    el.style.display = 'none';
    document.body.appendChild(el);
    const value = getComputedStyle(el).opacity;
    document.body.removeChild(el);
    const rafId = requestAnimationFrame(() => setComputed(value));
    return () => cancelAnimationFrame(rafId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme]);

  return (
    <div className="flex flex-col gap-2" data-testid="states-disabled-wash-chip">
      <button
        type="button"
        disabled
        className="cursor-not-allowed rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-14 font-button text-gray-1000"
        style={{ opacity: 'var(--color-states-disabled-opacity)' }}
      >
        Disabled action
      </button>
      <div className="flex flex-col gap-0.5 min-w-0">
        <span className="text-label-14 text-gray-1000 truncate">
          states-disabled-opacity
        </span>
        <span className="text-copy-13 text-gray-700 truncate font-mono">
          {computed || 'var(--color-states-disabled-opacity)'}
        </span>
      </div>
    </div>
  );
}

/**
 * Scalar token row — reads the live computed value of a structural CSS var
 * (length / color / radius) via a probe element, re-resolving on theme flip.
 */
function ScalarRow({ token }: { token: ScalarToken }) {
  const { resolvedTheme } = useTheme();
  const [computed, setComputed] = useState('');
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const el = document.createElement('div');
    el.style.backgroundColor = `var(${token.varName})`;
    el.style.display = 'none';
    document.body.appendChild(el);
    const bg = getComputedStyle(el).backgroundColor;
    document.body.removeChild(el);
    let value = '';
    if (bg && bg !== 'transparent' && bg !== 'rgba(0, 0, 0, 0)') {
      value = bg;
    } else {
      const el2 = document.createElement('div');
      el2.style.width = `var(${token.varName})`;
      el2.style.display = 'none';
      document.body.appendChild(el2);
      const w = getComputedStyle(el2).width;
      document.body.removeChild(el2);
      if (w.endsWith('px')) value = w;
    }
    const rafId = requestAnimationFrame(() => setComputed(value || ''));
    return () => cancelAnimationFrame(rafId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, token.varName]);

  return (
    <div
      className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-2 border-b border-gray-alpha-200 last:border-b-0"
      data-testid={`scalar-row-${token.label}`}
    >
      <span className="text-label-14 font-medium text-gray-1000 w-56 shrink-0 truncate">{token.label}</span>
      <span className="text-copy-13-mono font-mono text-gray-600 w-56 shrink-0 break-all">
        {computed || token.varName}
      </span>
      <span className="text-copy-13 text-gray-600">{token.usage}</span>
    </div>
  );
}

/** Reading-chrome specimen — sample text styled with the flattened css var. */
function ReadingChromeSpecimenRow({ spec }: { spec: ReadingChromeSpecimen }) {
  const { resolvedTheme } = useTheme();
  const [color, setColor] = useState('');
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rafId = requestAnimationFrame(() => setColor(getComputedStyle(el).color));
    return () => cancelAnimationFrame(rafId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme]);

  return (
    <div
      className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-2 border-b border-gray-alpha-200 last:border-b-0"
      data-testid={`reading-chrome-specimen-${spec.label.replace(/[^a-z0-9]/gi, '-')}`}
    >
      <div className="w-60 shrink-0 flex flex-col gap-0.5">
        <span className="text-label-14 font-medium text-gray-1000">{spec.label}</span>
        <span className="text-copy-13-mono font-mono text-gray-500 break-all">{spec.varName}</span>
      </div>
      <div className="flex-1 min-w-0 flex items-baseline gap-2">
        <span
          ref={ref}
          className={spec.className}
          style={{ color: `var(${spec.varName})` }}
        >
          {spec.sample}
        </span>
        {color && <span className="text-copy-13-mono font-mono text-gray-500 ml-auto shrink-0">{color}</span>}
      </div>
    </div>
  );
}

/**
 * Structural scalar section (DESIGN.md components.*) — footer-profile,
 * setup-wizard step/surface, sidebar-nav/dialog/sheet/listbox chrome metrics,
 * and reading annotation/chrome scalar families. Rows read live generated
 * values so both themes are observable in one gallery.
 */
function StructuralSection() {
  return (
    <section data-testid="tokens-structural">
      <SectionHeading id="tokens-structural">Structural scalars & reading chrome</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Non-color component scalars and flattened reading/annotation chrome families from DESIGN.md
        components.*. Values are the actual generated CSS custom properties, read live per theme.
      </p>

      <div className="mb-8 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-1">Footer profile</h4>
        <p className="text-copy-13 text-gray-600 mb-3 max-w-prose">
          Avatar and add-profile chrome scalars (DESIGN.md components.footer-profile).
        </p>
        {FOOTER_PROFILE_SCALARS.map((t) => (
          <ScalarRow key={t.varName} token={t} />
        ))}
      </div>

      <div className="mb-8 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-1">Setup wizard — step</h4>
        <p className="text-copy-13 text-gray-600 mb-3 max-w-prose">
          Step circle/row/label scalars (DESIGN.md components.setup-wizard-step).
        </p>
        {SETUP_WIZARD_STEP_SCALARS.map((t) => (
          <ScalarRow key={t.varName} token={t} />
        ))}
      </div>

      <div className="mb-8 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-1">Setup wizard — surface</h4>
        <p className="text-copy-13 text-gray-600 mb-3 max-w-prose">
          Wizard card / step-panel / content-panel / input-row scalars (DESIGN.md
          components.setup-wizard-surface).
        </p>
        {SETUP_WIZARD_SURFACE_SCALARS.map((t) => (
          <ScalarRow key={t.varName} token={t} />
        ))}
      </div>

      <div className="mb-8 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-1">Chrome metrics</h4>
        <p className="text-copy-13 text-gray-600 mb-3 max-w-prose">
          Sidebar-nav width/item-height, dialog width/max-height/max-width, sheet width, listbox
          max-height (DESIGN.md components.sidebar-nav / dialog / sheet / listbox).
        </p>
        {CHROME_SCALARS.map((t) => (
          <ScalarRow key={t.varName} token={t} />
        ))}
      </div>

      <div className="mb-8 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-1">Reading annotations & toolbar</h4>
        <p className="text-copy-13 text-gray-600 mb-3 max-w-prose">
          Annotation inspector / selection toolbar surfaces and four highlight fills + text
          (DESIGN.md components.reading-annotation-* / reading-selection-toolbar).
        </p>
        {READING_CHROME_SCALARS.map((t) => (
          <ScalarRow key={t.varName} token={t} />
        ))}
      </div>

      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-1">Reading chrome specimens</h4>
        <p className="text-copy-13 text-gray-600 mb-3 max-w-prose">
          Flattened reading-chrome family colors applied to representative prose (DESIGN.md
          components.reading-chrome-*).
        </p>
        {READING_CHROME_SPECIMENS.map((s) => (
          <ReadingChromeSpecimenRow key={s.varName} spec={s} />
        ))}
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Sections                                                            */
/* ------------------------------------------------------------------ */

function SubNav() {
  const items = [
    { label: 'Colors', href: '#tokens-colors' },
    { label: 'Type', href: '#tokens-typography' },
    { label: 'Space', href: '#tokens-spacing' },
    { label: 'Radius', href: '#tokens-radius' },
    { label: 'Elevation', href: '#tokens-elevation' },
    { label: 'Motion', href: '#tokens-motion' },
    { label: 'Canvas', href: '#tokens-canvas' },
    { label: 'States', href: '#tokens-states' },
    { label: 'Structural', href: '#tokens-structural' },
  ];

  return (
    <nav aria-label="Token sub-sections" className="flex flex-wrap gap-1 mb-8">
      {items.map(({ label, href }) => (
        <a
          key={href}
          href={href}
          className="px-3 py-1.5 rounded-md text-label-14 text-gray-700 hover:text-gray-1000 hover:bg-gray-alpha-100 transition-colors no-underline"
        >
          {label}
        </a>
      ))}
    </nav>
  );
}

function ColorsSection() {
  return (
    <section data-testid="tokens-colors">
      <SectionHeading id="tokens-colors">Colors</SectionHeading>
      <p
        data-testid="tokens-chronos-note"
        className="text-copy-14 text-gray-700 mb-6 max-w-prose"
      >
        Chronos dual-role: <strong className="font-medium text-gray-1000">cobalt signal</strong>{' '}
        (<code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">blue-1000</code> /{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">brand-cyan-1000</code> on
        light fills; <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">blue-700</code> /{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">brand-cyan</code> on dark){' '}
        for interactive affordances;{' '}
        <strong className="font-medium text-gray-1000">deep ink</strong> (
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">brand-deep-blue</code>) for
        structure and light-theme body links. Toggle light/dark — blue-* stays cobalt in both
        themes (lighter in dark).
      </p>
      {COLOR_GROUPS.map((group) => (
        <div
          key={group.title}
          className="mb-8"
          data-testid={`color-group-${group.title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`}
        >
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">{group.title}</h4>
          {group.hint ? (
            <p className="text-copy-13 text-gray-600 mb-4 max-w-prose">{group.hint}</p>
          ) : null}
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
            {group.tokens.map((t) => (
              <ColorSwatch key={t.varName} token={t} />
            ))}
          </div>
        </div>
      ))}
    </section>
  );
}

function TypographySection() {
  return (
    <section data-testid="tokens-typography">
      <SectionHeading id="tokens-typography">Typography</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        The display tier (<code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">font-display</code>,
        offline system sans) is the <strong>content voice</strong> — creative-entity titles only, never nav,
        buttons, tables, badges, or labels. Everything else is the interface voice (sans / mono).
        Metrics are read live from each rendered specimen.
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        {TYPO_SPECIMENS.map((s) => (
          <TypoRow key={s.label} specimen={s} />
        ))}
      </div>

      <h4 className="text-heading-16 font-semibold text-gray-900 mt-8 mb-2">Reading metrics</h4>
      <p className="text-copy-13 text-gray-600 mb-4 max-w-prose">
        Main reading-prose constraints from DESIGN.md typography — the reading prose measure (66ch)
        and paragraph spacing/line-height, applied live so a DESIGN edit re-reads it.
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="flex flex-col gap-3">
          <div className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3">
            <span className="text-label-14 font-medium text-gray-1000 w-44 shrink-0">reading-prose-measure</span>
            <span className="text-copy-13-mono font-mono text-gray-600" data-testid="reading-prose-measure">
              <ReadingMetricValue varName="--reading-prose-measure" />
            </span>
          </div>
          <div className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3">
            <span className="text-label-14 font-medium text-gray-1000 w-44 shrink-0">reading-prose-line-height</span>
            <span className="text-copy-13-mono font-mono text-gray-600">
              <ReadingMetricValue varName="--reading-prose-line-height" />
            </span>
          </div>
          <div className="flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3">
            <span className="text-label-14 font-medium text-gray-1000 w-44 shrink-0">reading-prose-paragraph-spacing</span>
            <span className="text-copy-13-mono font-mono text-gray-600">
              <ReadingMetricValue varName="--reading-prose-paragraph-spacing" />
            </span>
          </div>
        </div>
      </div>

      <h4 className="text-heading-16 font-semibold text-gray-900 mt-8 mb-2">Bilingual long text</h4>
      <p className="text-copy-13 text-gray-600 mb-4 max-w-prose">
        Latin + CJK in the same specimen with the shared offline system sans stack. No network font
        request — Latin and Simplified Chinese both render from the named system fallbacks (PingFang
        SC / Hiragino Sans GB / Microsoft YaHei UI / Noto Sans CJK SC).
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6 space-y-5">
        <div>
          <div className="text-display-24 font-display-24 text-gray-1000">
            The Orchard of Small Hours · 时间的果园
          </div>
          <div className="text-copy-14 text-gray-700 mt-2">
            故事从一场深夜的雷雨开始。The story begins with a thunderstorm past midnight — the
            quick brown fox jumps over the lazy dog, 中文排版与西文在同一行流畅混排，标点与基线对齐。
          </div>
        </div>
        <div>
          <div className="text-display-20 font-display-20 text-gray-1000">
            精密创作工作台 · Precision Creative Tool
          </div>
          <div className="text-copy-16 text-gray-800 mt-2 max-w-prose" style={{ maxWidth: 'var(--reading-prose-measure)' }}>
            中性平面、石墨暗面、钴蓝交互构成了工作台的视觉语言。Neutral planes, graphite dark
            surfaces, and cobalt interaction define the workbench language — 长句连续阅读时保持
            一致的度量与行距，标点不因字体回退而跳动。
          </div>
        </div>
        <div>
          <div className="text-copy-13-mono font-mono text-gray-700">
            mono-13 — const 中文 = 「变量名」 // the quick brown fox · CJK falls back to the named sans in the mono stack
          </div>
          <div className="text-label-12-mono font-mono text-gray-600 mt-1">
            LABEL-12-MONO — 0xDEAD_BEEF_2024 · 中文 id 亦可读
          </div>
        </div>
      </div>
    </section>
  );
}

/** Read a CSS custom property's declared value from the document root, live per theme. */
function ReadingMetricValue({ varName }: { varName: string }) {
  const { resolvedTheme } = useTheme();
  const [value, setValue] = useState('');
  useEffect(() => {
    const rafId = requestAnimationFrame(() => {
      const v = getComputedStyle(window.document.documentElement)
        .getPropertyValue(varName)
        .trim();
      setValue(v);
    });
    return () => cancelAnimationFrame(rafId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedTheme, varName]);
  return <>{value || varName}</>;
}

function SpacingSection() {
  return (
    <section data-testid="tokens-spacing">
      <SectionHeading id="tokens-spacing">Spacing</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Base unit 4px. Bars render at true scale — each bar's width is the token's{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--space-*</code> CSS
        variable, with the computed px/rem read live.
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        {SPACING_SCALE.map((s) => (
          <SpacingBar key={s.label} step={s} />
        ))}
      </div>
    </section>
  );
}
function StatesSection() {
  return (
    <section data-testid="tokens-states">
      <SectionHeading id="tokens-states">States</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Shared disabled-state wash (DESIGN.md §States) — the{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--color-states-disabled-opacity</code>{' '}
        scalar consumed by the Tailwind <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">opacity-disabled</code>{' '}
        utility from the shared preset. The chip applies the live variable so what you see is the
        token, and re-resolves on theme flip.
      </p>
      <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
        <DisabledWashDemo />
      </div>
    </section>
  );
}

function RadiusSection() {
  return (
    <section data-testid="tokens-radius">
      <SectionHeading id="tokens-radius">Radius</SectionHeading>
      <div className="flex flex-wrap items-end gap-8 p-6 border border-gray-alpha-300 rounded-card bg-background-100">
        {RADIUS_SCALE.map((s) => (
          <RadiusBox key={s.label} step={s} />
        ))}
      </div>
    </section>
  );
}

function ElevationSection() {
  return (
    <section data-testid="tokens-elevation">
      <SectionHeading id="tokens-elevation">Elevation</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Two-part shadows (tight ambient + soft key), ink-tinted in light, pure-black in dark.
        Swatches apply the live <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--shadow-elevation-*</code>{' '}
        variables and re-read on theme flip.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-6">
        {ELEVATION_LEVELS.map((t) => (
          <ElevationCard key={t.varName} token={t} />
        ))}
      </div>
      <div data-testid="elevation-aliases" className="mt-6 border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Alias chain (no consumer breakage)</h4>
        <p className="text-copy-13 text-gray-600 mb-3">
          Legacy names resolve onto the scale. <code>elevation-2</code> has no legacy alias — consume it
          directly (<code>shadow-elevation-2</code>) for hover states.
        </p>
        {ELEVATION_ALIASES.map((a) => (
          <ElevationAliasRow key={a.varName} alias={a} />
        ))}
      </div>
    </section>
  );
}

function MotionSection() {
  const reduced = usePrefersReducedMotion();
  return (
    <section data-testid="tokens-motion">
      <SectionHeading id="tokens-motion">Motion</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        Short and standard-eased (120–220ms). Durations and easings are read live from their{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--duration-*</code> /{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">--ease-*</code> CSS
        variables. Every recipe honors{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">prefers-reduced-motion: reduce</code>{' '}
        by collapsing to an instant state change.
      </p>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
        <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Durations</h4>
          {MOTION_DURATIONS.map((t) => (
            <MotionRow key={t.label} token={t} property="transitionDuration" />
          ))}
        </div>
        <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Easings</h4>
          {MOTION_EASINGS.map((t) => (
            <MotionRow key={t.label} token={t} property="transitionTimingFunction" />
          ))}
        </div>
      </div>

      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-4">Recipes</h4>
        {reduced && (
          <p data-testid="motion-reduced-note" className="text-copy-13 text-gray-600 mb-4">
            <code>prefers-reduced-motion: reduce</code> is active — these demos render as instant state
            changes with no transform/opacity animation.
          </p>
        )}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <HoverLiftDemo />
          <EnterExitDemo />
        </div>
      </div>
    </section>
  );
}

/**
 * Canvas section (V1.121 P3 T4) — live swatches for every canvas token
 * family: ambient (surface + grid + minimap + dot-grid pattern), node
 * chrome (fill / fill-hover / border / border-selected), edges & ports,
 * per-surface accent spines, and the five --canvas-node-width-* utility
 * slots. Every swatch re-resolves on theme flip so the gallery covers
 * both light and dark.
 */
function CanvasSection() {
  return (
    <section data-testid="tokens-canvas">
      <SectionHeading id="tokens-canvas">Canvas</SectionHeading>
      <p className="text-copy-14 text-gray-700 mb-4 max-w-prose">
        V1.121 v0.4 canvas token families from DESIGN.md §Canvas Surface — the same{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">canvas-*</code> /
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">canvas-node-*</code> /
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">canvas-{`{surface}`}-accent</code>{' '}
        tokens the three App canvases consume. Swatches read live CSS variable values and re-resolve on
        theme flip; the dot-grid swatch uses the live grid gap / dot-size metrics.
      </p>

      {CANVAS_TOKEN_GROUPS.map((group, idx) => (
        <div key={group.title} className="mb-8" data-testid={`canvas-token-group-${idx}`}>
          <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">{group.title}</h4>
          {group.hint && (
            <p className="text-copy-13 text-gray-600 mb-4 max-w-prose">{group.hint}</p>
          )}
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
            {group.tokens.map((t) =>
              group.title === 'Accent spines' ||
              group.title === 'Canvas — Timeline accent spine' ? (
                <CanvasAccentSpineSwatch key={t.varName} token={t} />
              ) : (
                <CanvasColorSwatch key={t.varName} token={t} />
              ),
            )}
            {/* Ambient group also shows the live dot-grid pattern. */}
            {group.title === 'Ambient' && <CanvasAmbientGridSwatch />}
          </div>
        </div>
      ))}

      <div className="mb-8" data-testid="canvas-token-group-widths">
        <h4 className="text-heading-16 font-semibold text-gray-900 mb-2">Node widths</h4>
        <p className="text-copy-13 text-gray-600 mb-4 max-w-prose">
          Five <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            --canvas-node-width-*
          </code>{' '}
          utility slots from DESIGN.md{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components.canvas.node-width
          </code>
          . Each chip's <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">min-width</code>{' '}
          resolves through the named Tailwind utility, with the live computed px read out below.
        </p>
        <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-5 gap-4">
          {CANVAS_NODE_WIDTHS.map((t) => (
            <CanvasNodeWidthSwatch key={t.varName} token={t} />
          ))}
        </div>
      </div>

      <p className="text-copy-13 text-gray-500 mt-6">
        Accent spine shape mirrors{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">NodeChromeShell</code>{' '}
        — the same{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">border-l-[3px]</code>{' '}
        recipe the App graph renders. The{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">/surfaces/canvas</code>{' '}
        page mirrors the App canvas surfaces (Outline / Strategy / World KB) consuming these tokens.
      </p>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function TokensPage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">Tokens</h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        All scalar design scales from the DESIGN SSOT — colors, typography (incl. the display tier),
        spacing, radius, elevation, and motion. Values are read live from CSS custom properties and
        generated utility classes, and update when the theme toggles. Chronos projects{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">blue-*</code> as the
        cobalt interactive scale — see Colors for the ink-vs-signal split.
      </p>
      <SubNav />

      <ColorsSection />
      <TypographySection />
      <SpacingSection />
      <RadiusSection />
      <ElevationSection />
      <MotionSection />
      <CanvasSection />
      <StatesSection />
      <StructuralSection />

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        Every gallery reads live values: colors, shadows, spacing, radius, motion, and canvas
        tokens from CSS custom properties (re-resolved on theme flip where theme-dependent);
        typography from the computed style of elements carrying the token&apos;s utility class.
      </p>
    </div>
  );
}
