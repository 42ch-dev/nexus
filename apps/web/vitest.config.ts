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
    // Architectural-surface coverage baseline (R-V164-QC1-S1-P1 T5). Scope is
    // deliberately narrow — the adapter boundary, error parsing, theme/provider,
    // and the notification hook — because those are the surfaces P2 builds on.
    // No `thresholds` gate: the plan records the actual number rather than
    // blocking on cosmetic lines. Run via `pnpm --filter web test:coverage`.
    coverage: {
      provider: 'v8',
      include: [
        'src/lib/nexus/browser-client.ts',
        'src/lib/nexus/tauri-client.ts',
        'src/lib/nexus/adapters.ts',
        'src/lib/nexus/errors.ts',
        'src/lib/nexus/types.ts',
        'src/lib/nexus/query-keys.ts',
        'src/lib/client-context.tsx',
        'src/lib/use-toast.tsx',
        'src/api/queries.ts',
        'src/components/status-badge.tsx',
        'src/components/theme-provider.tsx',
        'src/components/ui/tabs.tsx',
        'src/pages/chapters-page.tsx',
        'src/pages/chapter-page.tsx',
      ],
      exclude: ['src/**/*.{test,spec}.{ts,tsx}', 'src/test/**'],
      reporter: ['text', 'text-summary', 'html'],
    },
  },
});
