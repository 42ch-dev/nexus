/**
 * Tailwind config for the Nexus local Web UI.
 *
 * DESIGN.md is the design-token SSOT (Standard completeness). This config
 * CONSUMES those tokens — it does not invent them. Token categories mapped:
 *
 *   • Color   → CSS custom properties (light defaults on :root; dark overrides
 *               under `.dark`). Theme-dependent, so values live in index.css.
 *   • Elevation (shadow) → CSS custom properties (differ light/dark).
 *   • Typography / Spacing / Motion / Shapes / Breakpoints → theme-independent
 *               direct values from DESIGN.md.
 *
 * Spacing note: DESIGN.md's spacing scale (space-1..space-24) is exactly the
 * Tailwind default 4px scale (1=4px … 24=96px), so the default `spacing` theme
 * already covers it — no override needed.
 *
 * See apps/web/DESIGN.md §Implementation Mapping for P1.
 *
 * Dark mode: `class` strategy. A `.dark` class on <html> swaps the color +
 * shadow CSS variables; token names are identical in both themes.
 */
import type { Config } from 'tailwindcss';

/** CSS var helper for color tokens. */
const cv = (token: string): string => `var(--color-${token})`;

const config: Config = {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    // Breakpoints — DESIGN.md §Breakpoints (px-based).
    screens: {
      sm: '401px',
      md: '601px',
      lg: '961px',
      xl: '1200px',
      '2xl': '1400px',
    },
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
        // Canvas surface — DESIGN.md §Canvas Surface (V1.70). Infinite-canvas
        // graph primitives (background, grid, node fill/border, edges, ports,
        // minimap, Strategy accent). Theme-dependent → CSS vars in index.css.
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
        },
      },
      fontFamily: {
        sans: 'var(--font-sans)',
        mono: 'var(--font-mono)',
      },
      // Type scale — DESIGN.md §Typography/Type Scale.
      // Each token carries size + line-height + letter-spacing; weight is applied
      // via the fontWeight entries below or utility classes.
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
      },
      fontWeight: {
        // DESIGN.md uses 550 / 650 (non-standard but valid).
        medium: '500',
        semibold: '600',
        heading: '650',
        button: '550',
      },
      // Elevation — DESIGN.md §Elevation (theme-dependent → CSS vars).
      boxShadow: {
        card: 'var(--shadow-card)',
        popover: 'var(--shadow-popover)',
        modal: 'var(--shadow-modal)',
      },
      // Motion — DESIGN.md §Motion (theme-independent).
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
      // Shapes — DESIGN.md §Shapes (theme-independent).
      borderRadius: {
        control: '6px',
        card: '8px',
        popover: '12px',
        fullscreen: '16px',
        pill: '9999px',
      },
    },
  },
  plugins: [],
};

export default config;
