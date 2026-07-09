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
      '@': path.resolve(__dirname, './src'),
      '@web-ui': path.resolve(__dirname, '../web/src/components/ui'),
      '@web-setup': path.resolve(__dirname, '../web/src/components/setup'),
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
