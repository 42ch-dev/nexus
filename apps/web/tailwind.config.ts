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
  // Scan the @42ch/nexus-ui package source so package-exclusive utilities
  // (e.g. `appearance-none`, `ps-3`, `pe-8`, `dark:bg-brand-cyan`) are emitted.
  // Without this, only classes that also happen to appear under apps/web/src
  // are generated, so the native Select kept its UA arrow alongside the custom
  // chevron overlay (V1.120 P1 T2 / AC-P1-3). Mirrors apps/design-studio.
  content: [
    './index.html',
    './src/**/*.{ts,tsx}',
    '../../packages/nexus-ui/src/**/*.{ts,tsx}',
  ],
  theme: {
    extend: {
      // V1.113 P1: app-local Tailwind utility. design-studio's mirrored
      // gallery does not use the listbox cap; keeping it here avoids widening
      // the shared preset for a single consumer.
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
