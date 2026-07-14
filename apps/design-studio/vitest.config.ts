import path from 'node:path';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

/**
 * Vitest config for the Nexus Design Studio.
 *
 * Mirrors the resolve alias + esbuild target from vite.config.ts so source
 * transforms in tests match the dev/build pipeline. The test environment is
 * jsdom. No msw is needed — the studio is a read-only gallery with no daemon
 * transport, so we don't ship a mock server in setup.
 */
export default defineConfig({
  plugins: [react()],
  esbuild: { target: 'esnext' },
  resolve: {
    alias: {
      // web components import @/lib/utils — resolve it to apps/web before the
      // general `@` -> design-studio/src alias catches it (matches vite.config.ts).
      '@/lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
      // Studio imports web's English source locale catalogs for its own i18next
      // instance (matches vite.config.ts).
      '@web-locales/en': path.resolve(__dirname, '../web/src/locales/en'),
      // Presentational app modules (e.g. daemon-ready-splash) reach into
      // apps/web UI primitives via @/components/ui/*. Mirror that alias so
      // Studio tests can import the presentational module without duplicating it.
      '@/components/ui': path.resolve(__dirname, '../web/src/components/ui'),
      '@': path.resolve(__dirname, './src'),
      '@web-ui': path.resolve(__dirname, '../web/src/components/ui'),
      '@web-setup': path.resolve(__dirname, '../web/src/components/setup'),
      // Gallery-only alias for app-shared layout chrome (V1.107 sidebar/footer/header).
      '@web-layout': path.resolve(__dirname, '../web/src/components/layout/presentational'),
      // Gallery-only alias for app-shared Settings presentational extracts (V1.107 Connection/Setup).
      '@web-settings': path.resolve(__dirname, '../web/src/components/settings/presentational'),
      // Gallery-only alias for app-shared canvas node-chrome extracts (V1.115 NodeChromeShell).
      '@web-canvas': path.resolve(__dirname, '../web/src/components/canvas/presentational'),
      '@web-lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    clearMocks: true,
    restoreMocks: true,
  },
});
