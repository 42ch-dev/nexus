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
      },
      fontSize: {
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
      },
      transitionDuration: {
        instant: '0ms',
        state: '120ms',
        popover: '160ms',
        modal: '220ms',
      },
      transitionTimingFunction: {
        standard: 'cubic-bezier(0.16, 1, 0.3, 1)',
        emphasized: 'cubic-bezier(0.2, 0.8, 0.2, 1)',
      },
      borderRadius: {
        control: '6px',
        card: '8px',
        popover: '12px',
        fullscreen: '16px',
        pill: '9999px',
        'setup-wizard-surface-input-row-rounded': cv('setup-wizard-surface-input-row-rounded'),
      },
      spacing: {
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
