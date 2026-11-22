import path from 'node:path';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

/**
 * Vitest config for the Nexus Design Studio.
 *
 * Mirrors the resolve alias + oxc target from vite.config.ts so source
 * transforms in tests match the dev/build pipeline. The test environment is
 * jsdom. No msw is needed — the studio is a read-only gallery with no daemon
 * transport, so we don't ship a mock server in setup.
 */
export default defineConfig({
  plugins: [react()],
  oxc: { target: 'esnext' },
  resolve: {
    alias: {
      // general `@` -> design-studio/src alias catches it (matches vite.config.ts).
      '@/lib/utils': path.resolve(import.meta.dirname, '../web/src/lib/utils'),
      '@/lib/status-variant': path.resolve(
        import.meta.dirname,
        '../web/src/lib/status-variant',
      ),
      // instance (matches vite.config.ts).
      '@web-locales/en': path.resolve(import.meta.dirname, '../web/src/locales/en'),
      // apps/web UI primitives via @/components/ui/*. Mirror that alias so
      // Studio tests can import the presentational module without duplicating it.
      '@/components/ui': path.resolve(import.meta.dirname, '../web/src/components/ui'),
      '@': path.resolve(import.meta.dirname, './src'),
      '@web-ui': path.resolve(import.meta.dirname, '../web/src/components/ui'),
      '@web-setup': path.resolve(import.meta.dirname, '../web/src/components/setup'),
      '@web-layout': path.resolve(import.meta.dirname, '../web/src/components/layout/presentational'),
      '@web-settings': path.resolve(import.meta.dirname, '../web/src/components/settings/presentational'),
      '@web-canvas': path.resolve(import.meta.dirname, '../web/src/components/canvas/presentational'),
      '@web-harness': path.resolve(
        import.meta.dirname,
        '../web/src/components/harness/presentational',
      ),
      '@web-global-timeline': path.resolve(
        import.meta.dirname,
        '../web/src/components/global-timeline/presentational',
      ),
      '@web-lib/utils': path.resolve(import.meta.dirname, '../web/src/lib/utils'),
      '@web-shell': path.resolve(
        import.meta.dirname,
        '../web/src/components/selection-submenu',
      ),
      '@web-developer': path.resolve(
        import.meta.dirname,
        '../web/src/components/developer/presentational',
      ),
      '@web-works': path.resolve(
        import.meta.dirname,
        '../web/src/components/works/presentational',
      ),
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
