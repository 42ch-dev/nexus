import path from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

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
  plugins: [react()],
  esbuild: { target: 'esnext' },
  build: {
    target: 'esnext',
  },
  optimizeDeps: {
    esbuildOptions: { target: 'esnext' },
  },
  resolve: {
    alias: {
      // web components import @/lib/utils — resolve it to apps/web before the
      // general `@` -> design-studio/src alias catches it.
      '@/lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
      // Studio imports web's English source locale catalogs for its own i18next
      // instance. This alias must appear before the general `@` catch-all.
      '@web-locales/en': path.resolve(__dirname, '../web/src/locales/en'),
      // Presentational app modules (e.g. daemon-ready-splash) reach into
      // apps/web UI primitives via @/components/ui/*. Mirror that alias so
      // Studio can import the presentational module without duplicating it.
      '@/components/ui': path.resolve(__dirname, '../web/src/components/ui'),
      '@': path.resolve(__dirname, './src'),
        '@web-ui': path.resolve(__dirname, '../web/src/components/ui'),
        // Gallery-only alias for app-shared setup compositions (V1.101 AgentPicker).
        '@web-setup': path.resolve(__dirname, '../web/src/components/setup'),
        // Gallery-only alias for app-shared layout chrome (V1.107 sidebar/footer/header).
        '@web-layout': path.resolve(__dirname, '../web/src/components/layout/presentational'),
        // Gallery-only alias for app-shared Settings presentational extracts (V1.107 Connection/Setup).
        '@web-settings': path.resolve(__dirname, '../web/src/components/settings/presentational'),
        // Gallery-only alias for app-shared canvas node-chrome extracts (V1.115 NodeChromeShell).
        '@web-canvas': path.resolve(__dirname, '../web/src/components/canvas/presentational'),
        // Gallery-only alias for Global Timeline list chrome (V1.124 P2).
        '@web-global-timeline': path.resolve(
          __dirname,
          '../web/src/components/global-timeline/presentational',
        ),
        '@web-lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
      },
    },
  server: {
    port: 5174,
  },
});
