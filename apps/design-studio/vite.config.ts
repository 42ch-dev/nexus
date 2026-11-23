import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { designTokensPlugin } from './plugins/design-tokens';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');

/**
 * Vite config for the Nexus Design Studio.
 *
 * Studio is a standalone, daemon-independent SPA — no proxy to the daemon API.
 * It shares the @nexus/design-tokens preset with apps/web and imports
 * apps/web UI primitives via @web-ui/* alias (dev gallery only — no product
 * embed).
 *
 * Dev port 5174 to avoid collision with apps/web (5173).
 * esnext target: same rationale as apps/web (no legacy browser support needed).
 */
export default defineConfig({
  plugins: [designTokensPlugin(REPO_ROOT), react()],
  oxc: { target: 'esnext' },
  build: {
    target: 'esnext',
  },
  optimizeDeps: {
    rolldownOptions: { transform: { target: 'esnext' } },
  },
  resolve: {
    alias: {
      '@/lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
      '@/lib/status-variant': path.resolve(__dirname, '../web/src/lib/status-variant'),
      '@web-locales/en': path.resolve(__dirname, '../web/src/locales/en'),
      '@/components/ui': path.resolve(__dirname, '../web/src/components/ui'),
      '@': path.resolve(__dirname, './src'),
        '@web-ui': path.resolve(__dirname, '../web/src/components/ui'),
        '@web-setup': path.resolve(__dirname, '../web/src/components/setup'),
        '@web-layout': path.resolve(__dirname, '../web/src/components/layout/presentational'),
        '@web-settings': path.resolve(__dirname, '../web/src/components/settings/presentational'),
        '@web-canvas': path.resolve(__dirname, '../web/src/components/canvas/presentational'),
        '@web-global-timeline': path.resolve(
          __dirname,
          '../web/src/components/global-timeline/presentational',
        ),
        '@web-lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
        '@web-harness': path.resolve(
          __dirname,
          '../web/src/components/harness/presentational',
        ),
        '@web-shell': path.resolve(
          __dirname,
          '../web/src/components/selection-submenu',
        ),
        '@web-developer': path.resolve(
          __dirname,
          '../web/src/components/developer/presentational',
        ),
        '@web-works': path.resolve(
          __dirname,
          '../web/src/components/works/presentational',
        ),
      },
    },
  server: {
    port: 5174,
  },
});
