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
