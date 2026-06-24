import path from 'node:path';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

// Vitest config for the Nexus local Web UI.
//
// Mirrors the resolve alias + esbuild target from vite.config.ts so source
// transforms in tests match the dev/build pipeline. The test environment is
// jsdom (component + DOM-adapter coverage). msw is wired per test file via
// src/test/setup.ts.
export default defineConfig({
  plugins: [react()],
  esbuild: { target: 'esnext' },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    // Keep the baseline fast and deterministic; no watch by default in CI.
    clearMocks: true,
    restoreMocks: true,
  },
});
