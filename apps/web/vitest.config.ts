import path from 'node:path';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

// V1.84 W003: Narrow suppression of Node 24+ ExperimentalWarning for
// localStorage. Node's experimental localStorage shim emits a warning that is
// harmless under jsdom but clutters test output. We remove Node's default
// 'warning' listener (the one that prints to stderr) and install a single
// filter: drop warnings matching BOTH 'ExperimentalWarning' name AND
// 'localStorage' message; forward everything else to the original listener(s)
// directly. No process.emitWarning from inside the handler, so non-target
// warnings cannot loop.
const NEXUS_LOCALSTORAGE_WARNING_FILTER = Symbol.for(
  'nexus:vitest:localStorageWarningFilter',
);
if (!(globalThis as Record<symbol, unknown>)[NEXUS_LOCALSTORAGE_WARNING_FILTER]) {
  (globalThis as Record<symbol, unknown>)[NEXUS_LOCALSTORAGE_WARNING_FILTER] = true;

  const defaultWarningListeners = process.listeners('warning');

  process.removeAllListeners('warning');
  process.on('warning', (warning: Error) => {
    if (
      warning.name === 'ExperimentalWarning' &&
      warning.message.includes('localStorage')
    ) {
      return; // drop target — Node 24+ experimental localStorage shim
    }

    if (defaultWarningListeners.length > 0) {
      for (const listener of defaultWarningListeners) {
        try {
          listener.call(process, warning);
        } catch {
          // swallow listener errors to avoid breaking test startup
        }
      }
    } else {
      // Fallback for Node versions where the default handler is not exposed via
      // process.listeners('warning'): write the warning to stderr directly.
      process.stderr.write(`${warning.name}: ${warning.message}\n`);
      if (warning.stack) {
        process.stderr.write(`${warning.stack}\n`);
      }
    }
  });
}

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
      '@config': path.resolve(__dirname, './config'),
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
    // V1.162 Phase 5: jsdom + msw + react-query + third-party components
    // (e.g. @xyflow/react) can leave residual timers/sockets that prevent
    // worker forks from exiting cleanly once their event loop drains. If the
    // vitest main process is interrupted (subagent cancel, shell teardown,
    // teardown timeout), worker forks become orphans that never terminate
    // (observed: 27 orphaned ~600MB workers after a `pnpm test` run).
    // `forceExit` makes the main process SIGKILL all workers the instant it
    // finishes, guaranteeing no orphans regardless of residual handles.
    // `maxWorkers` caps the fan-out so even a partial leak is bounded.
    forceExit: true,
    poolOptions: {
      forks: { maxWorkers: '50%' },
    },
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
