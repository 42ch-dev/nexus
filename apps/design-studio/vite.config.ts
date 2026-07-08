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
      '@': path.resolve(__dirname, './src'),
      '@web-ui': path.resolve(__dirname, '../web/src/components/ui'),
      '@web-lib/utils': path.resolve(__dirname, '../web/src/lib/utils'),
    },
  },
  server: {
    port: 5174,
  },
});
