/**
 * Tailwind config for the Nexus Design Studio.
 *
 * Consumes the shared @nexus/design-tokens preset (single theme.extend source
 * shared with apps/web). Dark mode uses the `class` strategy — the theme toggle
 * adds/removes `.dark` on <html>.
 *
 * No duplicate theme.extend token block; app-specific overrides only.
 */
import type { Config } from 'tailwindcss';
import preset from '@nexus/design-tokens/tailwind.preset';

const config: Config = {
  presets: [preset],
  darkMode: 'class',
  content: [
    './index.html',
    './src/**/*.{ts,tsx}',
    '../web/src/components/ui/**/*.{ts,tsx}',
    '../web/src/components/setup/**/*.{ts,tsx}',
    '../web/src/components/layout/presentational/**/*.{ts,tsx}',
    '../web/src/components/settings/presentational/**/*.{ts,tsx}',
    '../web/src/components/canvas/presentational/**/*.{ts,tsx}',
    '../web/src/components/global-timeline/presentational/**/*.{ts,tsx}',
    '../../packages/nexus-ui/src/**/*.{ts,tsx}',
  ],
  theme: {
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
