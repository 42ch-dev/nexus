/**
 * @nexus/design-tokens — Shared Tailwind preset.
 *
 * Extracted from apps/web/tailwind.config.ts theme.extend for all Nexus
 * product surfaces (apps/web, apps/design-studio). Single source of truth
 * for theme-dependent token mappings; no per-app `theme.extend` duplication.
 *
 * Consumers:
 * ```ts
 * import preset from '@nexus/design-tokens/tailwind.preset';
 * export default { presets: [preset], content: [...] };
 * ```
 *
 * DESIGN SSOT: repo-root DESIGN.md / DESIGN.dark.md
 * CSS variables: @nexus/design-tokens/tokens.css
 */
import type { Config } from 'tailwindcss';

/** CSS var helper for color tokens. */
const cv = (token: string): string => `var(--color-${token})`;

/** CSS var helper for structural (non-color) tokens — e.g. `--canvas-node-width-*`. */
const sv = (token: string): string => `var(--${token})`;

const preset: Partial<Config> = {
  theme: {
    extend: {
      colors: {
        // Background scale — DESIGN.md §Colors/Background.
        background: {
          100: cv('background-100'),
          200: cv('background-200'),
          300: cv('background-300'),
        },
        // Solid gray scale — DESIGN.md §Colors/Gray (solid).
        gray: {
          100: cv('gray-100'),
          200: cv('gray-200'),
          300: cv('gray-300'),
          400: cv('gray-400'),
          500: cv('gray-500'),
          600: cv('gray-600'),
          700: cv('gray-700'),
          800: cv('gray-800'),
          900: cv('gray-900'),
          1000: cv('gray-1000'),
        },
        // Gray alpha scale — DESIGN.md §Colors/Gray Alpha.
        'gray-alpha': {
          100: cv('gray-alpha-100'),
          200: cv('gray-alpha-200'),
          300: cv('gray-alpha-300'),
          400: cv('gray-alpha-400'),
          500: cv('gray-alpha-500'),
          600: cv('gray-alpha-600'),
        },
        // Overlay scrim — DESIGN.md §Elevation (backdrop fill behind overlays).
        scrim: cv('scrim'),
        // Status surface family — DESIGN.md components.states.{error,success,warning,info}
        // (V1.121: error landed P1 T2; success/warning/info added P2 T4 to
        // tokenize preset-validation status cards + canvas live-session banner;
        // theme-aware via .dark overrides).
        'error-surface': cv('error-surface'),
        'error-surface-border': cv('error-surface-border'),
        'success-surface': cv('success-surface'),
        'success-surface-border': cv('success-surface-border'),
        'warning-surface': cv('warning-surface'),
        'warning-surface-border': cv('warning-surface-border'),
        'info-surface': cv('info-surface'),
        'info-surface-border': cv('info-surface-border'),
        // Finding status pill — DESIGN.md components.finding-status-pill
        // (V1.121 P1 T3; tinted fill + semantic text + border per status).
        'finding-status': {
          open: {
            bg: cv('finding-status-open-bg'),
            text: cv('finding-status-open-text'),
            border: cv('finding-status-open-border'),
          },
          triaged: {
            bg: cv('finding-status-triaged-bg'),
            text: cv('finding-status-triaged-text'),
            border: cv('finding-status-triaged-border'),
          },
          'in-review': {
            bg: cv('finding-status-in-review-bg'),
            text: cv('finding-status-in-review-text'),
            border: cv('finding-status-in-review-border'),
          },
          resolved: {
            bg: cv('finding-status-resolved-bg'),
            text: cv('finding-status-resolved-text'),
            border: cv('finding-status-resolved-border'),
          },
          'wont-fix': {
            bg: cv('finding-status-wont-fix-bg'),
            text: cv('finding-status-wont-fix-text'),
            border: cv('finding-status-wont-fix-border'),
          },
          duplicate: {
            bg: cv('finding-status-duplicate-bg'),
            text: cv('finding-status-duplicate-text'),
            border: cv('finding-status-duplicate-border'),
          },
        },
        // Memory task-kind chips — DESIGN.md components.memory-task-kind-*.
        'memory-task-kind': {
          brainstorm: {
            bg: cv('memory-task-kind-brainstorm-bg'),
            text: cv('memory-task-kind-brainstorm-text'),
            border: cv('memory-task-kind-brainstorm-border'),
          },
          outline: {
            bg: cv('memory-task-kind-outline-bg'),
            text: cv('memory-task-kind-outline-text'),
            border: cv('memory-task-kind-outline-border'),
          },
          chapter: {
            bg: cv('memory-task-kind-chapter-bg'),
            text: cv('memory-task-kind-chapter-text'),
            border: cv('memory-task-kind-chapter-border'),
          },
          research: {
            bg: cv('memory-task-kind-research-bg'),
            text: cv('memory-task-kind-research-text'),
            border: cv('memory-task-kind-research-border'),
          },
          unknown: {
            bg: cv('memory-task-kind-unknown-bg'),
            text: cv('memory-task-kind-unknown-text'),
            border: cv('memory-task-kind-unknown-border'),
          },
        },
        // Reading maturation count badges — DESIGN.md
        // components.reading-maturation-badge.
        'reading-maturation': {
          'kb-density': {
            bg: cv('reading-maturation-kb-density-bg'),
            text: cv('reading-maturation-kb-density-text'),
            border: cv('reading-maturation-kb-density-border'),
          },
          'open-findings': {
            bg: cv('reading-maturation-open-findings-bg'),
            text: cv('reading-maturation-open-findings-text'),
            border: cv('reading-maturation-open-findings-border'),
          },
        },
        // Brand primitives — DESIGN.md + @42ch/nexus-ui/theme.css.
        brand: {
          'deep-blue': cv('brand-deep-blue'),
          cyan: cv('brand-cyan'),
          white: cv('brand-white'),
        },
        // V1.94 Footer Profile Switcher + Setup Wizard Step Chrome tokens.
        'footer-profile': {
          'avatar-size': cv('footer-profile-avatar-size'),
          'avatar-rounded': cv('footer-profile-avatar-rounded'),
          'avatar-bg': cv('footer-profile-avatar-bg'),
          'avatar-bg-hover': cv('footer-profile-avatar-bg-hover'),
          'avatar-bg-active': cv('footer-profile-avatar-bg-active'),
          'avatar-text': cv('footer-profile-avatar-text'),
          'avatar-text-active': cv('footer-profile-avatar-text-active'),
          'avatar-fallback-bg': cv('footer-profile-avatar-fallback-bg'),
          'avatar-fallback-text': cv('footer-profile-avatar-fallback-text'),
          'add-button-bg': cv('footer-profile-add-button-bg'),
          'add-button-border': cv('footer-profile-add-button-border'),
          'add-button-text': cv('footer-profile-add-button-text'),
          'add-button-hover-bg': cv('footer-profile-add-button-hover-bg'),
          'add-button-hover-border': cv('footer-profile-add-button-hover-border'),
          'add-button-hover-text': cv('footer-profile-add-button-hover-text'),
          gap: cv('footer-profile-gap'),
        },
        'setup-wizard-step': {
          'circle-active-bg': cv('setup-wizard-step-circle-active-bg'),
          'circle-active-text': cv('setup-wizard-step-circle-active-text'),
          'circle-complete-bg': cv('setup-wizard-step-circle-complete-bg'),
          'circle-complete-text': cv('setup-wizard-step-circle-complete-text'),
          'circle-pending-bg': cv('setup-wizard-step-circle-pending-bg'),
          'circle-pending-text': cv('setup-wizard-step-circle-pending-text'),
          connector: cv('setup-wizard-step-connector'),
          'label-active-color': cv('setup-wizard-step-label-active-color'),
          'label-pending-color': cv('setup-wizard-step-label-pending-color'),
        },
        // V1.96 Setup Wizard Surface
        'setup-wizard-surface': {
          'card-bg': cv('setup-wizard-surface-card-bg'),
          'card-border': cv('setup-wizard-surface-card-border'),
          'step-panel-right-divider': cv('setup-wizard-surface-step-panel-right-divider'),
          'input-row-bg': cv('setup-wizard-surface-input-row-bg'),
          'input-row-border': cv('setup-wizard-surface-input-row-border'),
          'input-row-label-color': cv('setup-wizard-surface-input-row-label-color'),
          'input-row-path-color': cv('setup-wizard-surface-input-row-path-color'),
          'input-row-icon-color': cv('setup-wizard-surface-input-row-icon-color'),
        },
        // Accent scales — DESIGN.md §Colors/Accent Scales.
        blue: {
          700: cv('blue-700'),
          800: cv('blue-800'),
          900: cv('blue-900'),
          1000: cv('blue-1000'),
        },
        red: {
          700: cv('red-700'),
          800: cv('red-800'),
          900: cv('red-900'),
          1000: cv('red-1000'),
        },
        amber: {
          700: cv('amber-700'),
          800: cv('amber-800'),
          900: cv('amber-900'),
          1000: cv('amber-1000'),
        },
        green: {
          700: cv('green-700'),
          800: cv('green-800'),
          900: cv('green-900'),
          1000: cv('green-1000'),
        },
        teal: {
          700: cv('teal-700'),
          800: cv('teal-800'),
          900: cv('teal-900'),
          1000: cv('teal-1000'),
        },
        purple: {
          700: cv('purple-700'),
          800: cv('purple-800'),
          900: cv('purple-900'),
          1000: cv('purple-1000'),
        },
        pink: {
          700: cv('pink-700'),
          800: cv('pink-800'),
          900: cv('pink-900'),
          1000: cv('pink-1000'),
        },
        // Canvas surface — DESIGN.md §Canvas Surface (V1.70).
        canvas: {
          surface: cv('canvas-surface'),
          grid: cv('canvas-grid'),
          'node-fill': cv('canvas-node-fill'),
          'node-fill-hover': cv('canvas-node-fill-hover'),
          'node-border': cv('canvas-node-border'),
          'node-border-selected': cv('canvas-node-border-selected'),
          edge: cv('canvas-edge'),
          'edge-hover': cv('canvas-edge-hover'),
          port: cv('canvas-port'),
          minimap: cv('canvas-minimap'),
          'strategy-accent': cv('canvas-strategy-accent'),
          // V1.121 P3 T2: per-surface accent spines (DESIGN.md §Canvas Surface).
          'outline-accent': cv('canvas-outline-accent'),
          'worldkb-accent': cv('canvas-worldkb-accent'),
          'write-dirty': cv('canvas-write-dirty'),
          'write-conflict': cv('canvas-write-conflict'),
          'write-success': cv('canvas-write-success'),
          'write-stale-bg': cv('canvas-write-stale-bg'),
          // V1.73 World KB canvas-write tokens
          'worldkb-entity-card-fill-default': cv('canvas-worldkb-entity-card-fill-default'),
          'worldkb-entity-card-fill-hover': cv('canvas-worldkb-entity-card-fill-hover'),
          'worldkb-entity-card-fill-selected': cv('canvas-worldkb-entity-card-fill-selected'),
          'worldkb-entity-card-stroke-default': cv('canvas-worldkb-entity-card-stroke-default'),
          'worldkb-entity-card-stroke-selected': cv('canvas-worldkb-entity-card-stroke-selected'),
          'worldkb-promotion-pending': cv('canvas-worldkb-promotion-pending'),
          'worldkb-promotion-confirmed': cv('canvas-worldkb-promotion-confirmed'),
          'worldkb-promotion-rejected': cv('canvas-worldkb-promotion-rejected'),
          'worldkb-promotion-merged': cv('canvas-worldkb-promotion-merged'),
          'worldkb-source-anchor-edge': cv('canvas-worldkb-source-anchor-edge'),
          'worldkb-source-anchor-node': cv('canvas-worldkb-source-anchor-node'),
          'worldkb-computable-badge': cv('canvas-worldkb-computable-badge'),
          'worldkb-conflict-marker': cv('canvas-worldkb-conflict-marker'),
          'worldkb-conflict-marker-fill': cv('canvas-worldkb-conflict-marker-fill'),
          'worldkb-nonspatial-row-highlight': cv('canvas-worldkb-nonspatial-row-highlight'),
          'worldkb-focus-ring': cv('canvas-worldkb-focus-ring'),
          'worldkb-relationship-edge': cv('canvas-worldkb-relationship-edge'),
          'worldkb-relationship-edge-default': cv('canvas-worldkb-relationship-edge-default'),
          'worldkb-relationship-edge-symmetric': cv('canvas-worldkb-relationship-edge-symmetric'),
          'worldkb-relationship-edge-custom': cv('canvas-worldkb-relationship-edge-custom'),
          'worldkb-relationship-confidence-low': cv('canvas-worldkb-relationship-confidence-low'),
          'worldkb-relationship-confidence-mid': cv('canvas-worldkb-relationship-confidence-mid'),
          'worldkb-relationship-confidence-high': cv('canvas-worldkb-relationship-confidence-high'),
          'worldkb-relationship-grounded-badge': cv('canvas-worldkb-relationship-grounded-badge'),
          'worldkb-relationship-asserted-badge': cv('canvas-worldkb-relationship-asserted-badge'),
          'worldkb-relationship-inspector-fill': cv('canvas-worldkb-relationship-inspector-fill'),
          // Outline canvas — DESIGN.md §Canvas Outline Tokens (V1.72; consumed V1.108).
          'outline-volume-fill': cv('canvas-outline-volume-fill'),
          'outline-chapter-card-status-pending': cv('canvas-outline-chapter-card-status-pending'),
          'outline-chapter-card-status-drafted': cv('canvas-outline-chapter-card-status-drafted'),
          'outline-chapter-card-status-completed': cv('canvas-outline-chapter-card-status-completed'),
          'outline-timeline-event-pin': cv('canvas-outline-timeline-event-pin'),
          'outline-foreshadow-edge': cv('canvas-outline-foreshadow-edge'),
          'outline-timeline-marker': cv('canvas-outline-timeline-marker'),
          'outline-conflict-marker': cv('canvas-outline-conflict-marker'),
        },
      },
      fontFamily: {
        sans: 'var(--font-sans)',
        mono: 'var(--font-mono)',
        // V1.121 v0.4 — content-voice editorial serif (DESIGN.md typography.font-display).
        display: 'var(--font-display)',
      },
      fontSize: {
        // V1.121 v0.4 display tier — content-voice titles (serif metrics;
        // fontWeight 600 baked per DESIGN.md typography.display-* contract).
        // Metrics consume the --text-display-* vars projected in tokens.css —
        // no handwritten duplicate literals here (single projection chain).
        'display-32': [
          sv('text-display-32'),
          {
            lineHeight: sv('text-display-32--line-height'),
            letterSpacing: sv('text-display-32--letter-spacing'),
            fontWeight: sv('text-display-32--font-weight'),
          },
        ],
        'display-24': [
          sv('text-display-24'),
          {
            lineHeight: sv('text-display-24--line-height'),
            letterSpacing: sv('text-display-24--letter-spacing'),
            fontWeight: sv('text-display-24--font-weight'),
          },
        ],
        'display-20': [
          sv('text-display-20'),
          {
            lineHeight: sv('text-display-20--line-height'),
            letterSpacing: sv('text-display-20--letter-spacing'),
            fontWeight: sv('text-display-20--font-weight'),
          },
        ],
        'heading-32': ['32px', { lineHeight: '1.18', letterSpacing: '-0.025em' }],
        'heading-24': ['24px', { lineHeight: '1.25', letterSpacing: '-0.02em' }],
        'heading-20': ['20px', { lineHeight: '1.3', letterSpacing: '-0.015em' }],
        'heading-16': ['16px', { lineHeight: '1.4', letterSpacing: '-0.01em' }],
        'label-14': ['14px', { lineHeight: '1.35' }],
        'label-12': ['12px', { lineHeight: '1.35', letterSpacing: '0.02em' }],
        'copy-16': ['16px', { lineHeight: '1.6' }],
        'copy-14': ['14px', { lineHeight: '1.55' }],
        'copy-13': ['13px', { lineHeight: '1.5' }],
        'button-14': ['14px', { lineHeight: '1' }],
        'button-12': ['12px', { lineHeight: '1', letterSpacing: '0.01em' }],
        'label-12-mono': ['12px', { lineHeight: '1.4' }],
        'copy-13-mono': ['13px', { lineHeight: '1.5' }],
        'setup-wizard-step-label-typography': [
          cv('setup-wizard-step-label-typography'),
          { lineHeight: '1' },
        ],
      },
      fontWeight: {
        medium: '500',
        semibold: '600',
        heading: '650',
        button: '550',
      },
      boxShadow: {
        card: 'var(--shadow-card)',
        popover: 'var(--shadow-popover)',
        modal: 'var(--shadow-modal)',
        // V1.121 v0.4 elevation scale (DESIGN.md §Elevation) — additive;
        // legacy card/popover/modal keys above keep resolving via the alias chain.
        // Flat keys: Tailwind v3 does not flatten nested boxShadow maps into
        // dash-separated utilities (compile-verified).
        'elevation-0': 'var(--shadow-elevation-0)',
        'elevation-1': 'var(--shadow-elevation-1)',
        'elevation-2': 'var(--shadow-elevation-2)',
        'elevation-3': 'var(--shadow-elevation-3)',
        'elevation-4': 'var(--shadow-elevation-4)',
      },
      transitionDuration: {
        // DESIGN.md §Motion scale — values projected from --duration-* vars.
        instant: sv('duration-instant'),
        state: sv('duration-state'),
        popover: sv('duration-popover'),
        modal: sv('duration-modal'),
        // V1.121 v0.4 directional pair (DESIGN.md §Motion) — additive.
        enter: 'var(--duration-enter)',
        exit: 'var(--duration-exit)',
      },
      transitionTimingFunction: {
        // DESIGN.md §Motion easings — values projected from --ease-* vars.
        standard: sv('ease-standard'),
        emphasized: sv('ease-emphasized'),
      },
      borderRadius: {
        // DESIGN.md rounded: scale — values projected from --radius-* vars.
        control: sv('radius-control'),
        card: sv('radius-card'),
        popover: sv('radius-popover'),
        fullscreen: sv('radius-fullscreen'),
        pill: sv('radius-pill'),
        'setup-wizard-surface-input-row-rounded': cv('setup-wizard-surface-input-row-rounded'),
      },
      spacing: {
        // DESIGN.md spacing: scale (4px base) — the nine DESIGN steps override
        // the matching Tailwind default numeric steps so the utilities resolve
        // through the --space-* projection chain (values identical to the
        // defaults; non-DESIGN fractional steps stay on the Tailwind scale).
        '1': sv('space-1'),
        '2': sv('space-2'),
        '3': sv('space-3'),
        '4': sv('space-4'),
        '6': sv('space-6'),
        '8': sv('space-8'),
        '10': sv('space-10'),
        '16': sv('space-16'),
        '24': sv('space-24'),
        'setup-wizard-step-circle-size': cv('setup-wizard-step-circle-size'),
        'setup-wizard-step-row-height': cv('setup-wizard-step-row-height'),
        'setup-wizard-surface-step-panel-width': cv('setup-wizard-surface-step-panel-width'),
        'setup-wizard-surface-input-row-min-height': cv('setup-wizard-surface-input-row-min-height'),
        'setup-wizard-surface-input-row-gap': cv('setup-wizard-surface-input-row-gap'),
        'setup-wizard-surface-cta-container-gap': cv('setup-wizard-surface-cta-container-gap'),
        // V1.99 P1: sidebar-nav sizing tokens (DESIGN.md §sidebar-nav)
        'sidebar-nav-width': cv('sidebar-nav-width'),
        'sidebar-nav-item-height': cv('sidebar-nav-item-height'),
      },
      maxWidth: {
        'setup-wizard-step-wizard-max-width': cv('setup-wizard-wizard-max-width'),
        'setup-wizard-surface-cta-primary-max-width': cv('setup-wizard-surface-cta-primary-max-width'),
        // Dialog — DESIGN.md components.dialog.maxWidth (560px).
        dialog: cv('dialog-max-width'),
      },
      // Dialog/Sheet layout metrics — DESIGN.md components.dialog /
      // components.sheet (V1.121 P1 T2; theme-independent). Structural
      // (non-color) tokens — referenced via the bare `--dialog-*` /
      // `--sheet-*` namespace, not the `--color-*` helper.
      width: {
        dialog: sv('dialog-width'),
        sheet: sv('sheet-width'),
      },
      maxHeight: {
        dialog: sv('dialog-max-height'),
      },
      // V1.121 v0.4 canvas node width family (DESIGN.md components.canvas.node-width;
      // registered here, applied to node components in P3 as min-w-canvas-node-*).
      // Structural (non-color) tokens — referenced via the bare
      // `--canvas-node-width-*` namespace, not the `--color-*` helper.
      minWidth: {
        'canvas-node-strategy-root': sv('canvas-node-width-strategy-root'),
        'canvas-node-strategy-primary': sv('canvas-node-width-strategy-primary'),
        'canvas-node-strategy-secondary': sv('canvas-node-width-strategy-secondary'),
        'canvas-node-outline-scene-beat': sv('canvas-node-width-outline-scene-beat'),
        'canvas-node-default': sv('canvas-node-width-default'),
      },
      // V1.105 P2: portrait wizard height cap (DESIGN.md setup-wizard-step.wizard-max-height)
      height: {
        'setup-wizard-wizard-max-height': cv('setup-wizard-wizard-max-height'),
      },
      padding: {
        'setup-wizard-step-wizard-padding': cv('setup-wizard-wizard-padding'),
        'setup-wizard-surface-step-panel-padding-x': cv('setup-wizard-surface-step-panel-padding-x'),
        'setup-wizard-surface-step-panel-padding-y': cv('setup-wizard-surface-step-panel-padding-y'),
        'setup-wizard-surface-content-panel-padding-x': cv('setup-wizard-surface-content-panel-padding-x'),
        'setup-wizard-surface-content-panel-padding-y': cv('setup-wizard-surface-content-panel-padding-y'),
        'setup-wizard-surface-input-row-padding-x': cv('setup-wizard-surface-input-row-padding-x'),
        'setup-wizard-surface-input-row-padding-y': cv('setup-wizard-surface-input-row-padding-y'),
      },
    },
  },
};

export default preset;
