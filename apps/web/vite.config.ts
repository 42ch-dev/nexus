import path from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Vite config for the Nexus local Web UI.
//
// Dev: the SPA runs on the Vite dev server and proxies Local API requests to
// the running daemon (default http://127.0.0.1:8420, the daemon HTTP transport
// default — see crates/nexus-daemon-runtime/src/boot.rs). Override the target
// with VITE_DAEMON_URL, e.g. VITE_DAEMON_URL=http://127.0.0.1:9000 pnpm dev.
//
// Release: the built dist/ is embedded into the nexus42 binary (plan P3,
// rust-embed); no Node runtime ships. The proxy is dev-only.
const daemonUrl = process.env.VITE_DAEMON_URL ?? 'http://127.0.0.1:8420';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  // Modern-only target: the local-first app ships in a current browser or a
  // V1.65 Tauri system webview, so esbuild need not lower syntax. esbuild 0.28
  // (pinned via the workspace override) fails its destructuring transform on
  // the default `modules` target; `esnext` skips that pass everywhere esbuild
  // runs (build, dev source transform, and dep pre-bundling).
  esbuild: { target: 'esnext' },
  build: {
    target: 'esnext',
    rollupOptions: {
      // Split large vendor trees into named chunks so no single minified chunk
      // exceeds Vite's 500 kB warning ceiling (`R-V175QC3-S001`). App + route
      // code returns `undefined` and stays under Rollup's default route-level
      // splitting. Group by dependency tree (TipTap pulls ProseMirror; React
      // Markdown pulls unified/remark/micromark) so each chunk is cache-stable.
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined;
          if (
            id.includes('prosemirror') ||
            id.includes('@tiptap') ||
            id.includes('tiptap-markdown')
          ) {
            return 'tiptap';
          }
          if (id.includes('@xyflow')) return 'xyflow';
          if (id.includes('@tanstack')) return 'query';
          if (id.includes('react-router')) return 'router';
          if (
            id.includes('react-markdown') ||
            id.includes('remark-') ||
            id.includes('micromark') ||
            id.includes('mdast') ||
            id.includes('hast-') ||
            id.includes('unified')
          ) {
            return 'markdown';
          }
          if (id.includes('lucide-react')) return 'icons';
          if (
            id.includes('/react-dom/') ||
            id.includes('/react/') ||
            id.includes('/scheduler/')
          ) {
            return 'react';
          }
          return undefined;
        },
      },
    },
  },
  optimizeDeps: {
    esbuildOptions: { target: 'esnext' },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 5173,
    proxy: {
      // Local API — keyless on loopback (V1.20 model). BrowserClient is
      // same-origin against this proxy in dev; in release it is same-origin
      // against the daemon port that serves the embedded SPA.
      '/v1/local': {
        target: daemonUrl,
        changeOrigin: false,
      },
    },
  },
});
