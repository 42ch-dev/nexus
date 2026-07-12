/**
 * Tailwind config for the Nexus local Web UI.
 *
 * DESIGN.md is the unified design-token SSOT at repo root. This config
 * consumes the shared @nexus/design-tokens Tailwind preset + tokens.css.
 * App-specific overrides go here; shared tokens live in the preset.
 *
 * Dark mode: `class` strategy. A `.dark` class on <html> swaps the color +
 * shadow CSS variables; token names are identical in both themes.
 */
import type { Config } from 'tailwindcss';
import preset from '@nexus/design-tokens/tailwind.preset';

const config: Config = {
  presets: [preset],
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      // V1.113 P1: app-local Tailwind utilities. The shared preset is consumed by
      // apps/design-studio too, but only apps/web currently uses disabled opacity and
      // the listbox max-height cap. Keeping them here avoids widening the shared preset
      // for a single consumer; promote to @nexus/design-tokens/tailwind.preset if a
      // second product surface needs them.
      opacity: {
        disabled: 'var(--color-states-disabled-opacity)',
      },
      maxHeight: {
        listbox: 'var(--color-listbox-max-height)',
      },
    },
    // Breakpoints — DESIGN.md §Breakpoints (px-based).
    screens: {
      sm: '401px',
      md: '601px',
      lg: '961px',
      xl: '1200px',
      '2xl': '1400px',
    },
  },
  plugins: [],
};

export default config;
