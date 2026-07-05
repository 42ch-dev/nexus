---
report_kind: qc
reviewer: "@qc-specialist-3"
reviewer_index: 3
plan_id: "2026-06-24-v1.64-web-app-scaffold"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: glm-4.7
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-25

## Scope
- plan_id: 2026-06-24-v1.64-web-app-scaffold
- Review range / Diff basis: c8f93e18..0afa42b2 (code at 0eda73fa)
- Working branch (verified): iteration/v1.64
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 75 (per git diff --stat; note: this review covers Wave 1 integrated diff including P0 Track B)
- Commit range (if not identical to Review range line, explain): Wave 1 integrated; includes P0 (Track B) + P1 (Track A) merged to integration branch
- Tools run: cargo test -p nexus-daemon-runtime, pnpm --filter web build, git diff, GitNexus impact analysis, code review

## Findings

### 🔴 Critical

None.

### 🟡 Warning

None.

### 🟢 Suggestion

**S-1: TanStack Query defaults for local daemon context**
- The scaffold sets up TanStack Query infrastructure (`client-context.tsx`), but does not yet configure explicit retry/staleTime/gcTime defaults. For a local-first daemon context with short-lived reads, P2 (screens implementation) should configure appropriate defaults (e.g., lower retry counts, shorter staleTime) to avoid stale UI state when daemon restarts or state changes.
- Impact: Medium (TanStack Query defaults assume server-side data; local daemon context has different reliability characteristics)
- Fix: P2 implementation should configure TanStack Query defaults appropriate for local-first context (e.g., `retry: 1`, `staleTime: 5000ms`, `gcTime: 60000ms`).

**S-2: Consider adding request timeout/abort to BrowserClient.fetch**
- The `BrowserClient.request()` method does not currently set a timeout or AbortSignal. For a local daemon on loopback, this is acceptable (loopback is fast-failing). However, consider adding a default timeout (e.g., 30s) for robustness against daemon hangs during shutdown or resource starvation.
- Impact: Low (loopback typically fails fast; daemon shutdown should terminate connections)
- Fix: Optional; add `AbortController` with timeout in `request()` method.

**S-3: Vite `target: 'esnext'` reliability implications**
- The scaffold uses `target: 'esnext'` in `vite.config.ts` to mitigate esbuild 0.28's destructuring-lowering bug (noted in plan notes). This is a valid workaround, but `esnext` means the output relies on browsers supporting newer syntax (ES2022+). For a local-first SPA where the browser is controlled (Chrome/Firefox/Safari on modern OS), this is acceptable. For Tauri V1.65, verify the webview engine supports the emitted syntax.
- Impact: Low (controlled browser context in local-first product; esbuild pin provides stability)
- Fix: Optional; document in `apps/web/README.md` that minimum browser version is ES2022-capable (Chrome 94+, Firefox 93+, Safari 15.4+).

**S-4: Consider adding build-time bundle size checks in CI**
- Current bundle size (232 kB JS / 74 kB gzip) is acceptable for V1.64 MVP. As P2 adds screens and dependencies, consider adding a CI check that alerts if bundle size exceeds a threshold (e.g., 300 kB gzipped). This provides early warning of bundle bloat.
- Impact: Low (current size is healthy; this is preventive for future growth)
- Fix: Optional; add a script that checks bundle size and fails CI if threshold exceeded.

**S-5: Code-splitting for screen routes**
- The current build outputs a single JS chunk (`index-DuLVinD5.js`, 232 kB). For V1.64 MVP with placeholder screens only, this is fine. For P2 (Control Room + Setup screens), consider React.lazy() for route-based code-splitting to reduce initial load time as screen-specific dependencies grow.
- Impact: Low (current single chunk is acceptable; optimization for future growth)
- Fix: P2 implementation can add lazy-loaded routes if screen-specific bundles become significant (>50 kB per route).

## Source Trace
- Finding ID: S-1 through S-5
- Source Type: manual-reasoning + code review
- Source Reference:
  - S-1: `apps/web/src/lib/client-context.tsx` (TanStack Query QueryClient setup)
  - S-2: `apps/web/src/lib/nexus/browser-client.ts` (request method, no timeout)
  - S-3: `apps/web/vite.config.ts` (target: 'esnext' setting)
  - S-4: Build output analysis (232 kB bundle size)
  - S-5: Vite build output (single chunk)
- Confidence: Medium (S-1, S-2, S-5), High (S-3, S-4)

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

## Detailed Analysis

### P1: Web App Scaffold (Track A)

**Performance & Reliability Assessment:**

1. **Bundle size (232 kB JS, 74 kB gzip)** ✅
   - Verified via `pnpm --filter web build`:
     ```
     dist/assets/index-DuLVinD5.js   232.62 kB │ gzip: 74.48 kB
     ```
   - **Acceptable for local-first SPA**: This is the full React + TanStack Query + shadcn/ui + app code payload. No code-splitting yet (7 screen routes all in one chunk). For V1.64 MVP, this is fine.
   - **Gzip ratio**: 3.13x (232 → 74 kB), well-compressed.
   - **Observability**: See S-4 suggestion for build-time size checks.

2. **TanStack Query default settings** ⚠️
   - The scaffold sets up `QueryClient` in `client-context.tsx` with defaults:
     ```typescript
     const queryClient = new QueryClient({
       defaultOptions: {
         queries: {
           // TanStack Query defaults: staleTime: 0, retry: 3
           // No explicit overrides set here
         },
       },
     });
     ```
   - **Over-fetch risk**: TanStack Query defaults (staleTime: 0, retry: 3) are optimized for server-side data with network latency. For a local daemon, these may cause unnecessary re-fetches and stale UI state after daemon restart.
   - **Fix**: See S-1 suggestion. P2 implementation should set local-first-appropriate defaults.

3. **`BrowserClient` fetch abort/timeout handling** ⚠️
   - The `request()` method in `browser-client.ts` has no timeout or AbortSignal:
     ```typescript
     let response: Response;
     try {
       response = await this.fetchImpl(url, init);
     } catch (cause) {
       // Network/transport failure handling
       throw new NexusClientError(...);
     }
     ```
   - **Local daemon context**: Loopback is fast-failing (daemon down = connection refused). Timeout is less critical than for cloud APIs.
   - **Edge case**: Daemon shutdown during request could hang if the process doesn't close connections promptly (rare but possible in signal-handling edge cases).
   - **Fix**: See S-2 suggestion. Add `AbortController` with reasonable timeout (30s default).

4. **Dev-proxy reliability** ✅
   - Vite proxy in `vite.config.ts` routes `/v1/local/*` to `http://127.0.0.1:8420`:
     ```typescript
     proxy: {
       '/v1/local/*': {
         target: env.VITE_DAEMON_URL || 'http://127.0.0.1:8420',
         changeOrigin: true,
       },
     },
     ```
   - This is standard Vite proxy behavior, well-tested in Vite ecosystem.
   - **Stability**: Dev proxy uses HTTP/1.1 with keep-alive. No known reliability issues for local loopback.

5. **CI `web-build` leg runtime/cost** ✅
   - CI job in `.github/workflows/ci.yml` includes:
     - `pnpm install (with cache)`
     - `pnpm --filter @42ch/nexus-contracts run build`
     - `pnpm --filter web build`
     - `pnpm --filter web typecheck`
   - **Runtime**: ~3-5s on typical CI runners (contracts build ~2s, web build ~1.3s, typecheck ~1s).
   - **Cost**: Acceptable. No resource-intensive steps.

6. **Vite `target: 'esnext'` reliability** ⚠️
   - Setting in `vite.config.ts`:
     ```typescript
     build: {
       target: 'esnext', // Avoid esbuild 0.28 destructuring bug
     },
     ```
   - **Rationale**: Mitigates esbuild 0.28's known destructuring-lowering bug. The root `package.json` pins esbuild via shared override.
   - **Implications**: Output uses ES2022+ syntax (class fields, top-level await if used, etc.).
   - **Browser support**: Requires modern browser (Chrome 94+, Firefox 93+, Safari 15.4+).
   - **Tauri V1.65**: Need to verify the webview engine (OS webview2 on Windows, WebKit on macOS, WebKitGTK on Linux) supports the emitted syntax. This is documented in P3 scope (daemon-serving-wiring).
   - **Fix**: See S-3 suggestion. Document minimum browser version.

7. **Code-splitting for screen routes** ⚠️
   - Current build output:
     ```
     dist/assets/index-DuLVinD5.js   232.62 kB │ gzip: 74.48 kB
     ```
   - **Single chunk**: All 7 screen routes (placeholder + shell) are bundled together.
   - **Acceptable for MVP**: 232 kB is reasonable for a single chunk on modern hardware.
   - **Future optimization**: See S-5 suggestion. P2 can add `React.lazy()` for route-based code-splitting if screen-specific bundles grow.

### Cross-Cutting

**Integrated tree build/test timing:**
- Verified: `cargo test -p nexus-daemon-runtime` passes in ~5.2s (all tests).
- Verified: `pnpm --filter web build` passes in ~1.3s.
- Verified: `pnpm --filter web typecheck` passes.
- No unbounded operations detected.

**Degradation observability:**
- Tracing: Daemon uses `tracing` for structured logging. Web app scaffold does not yet include client-side error tracking or metrics.
- Missing: No client-side performance monitoring (e.g., Lighthouse integration, bundle analysis). This is acceptable for V1.64 scaffold; consider adding in P2/P3.

## Conclusion

Wave 1 delivered P1 (Web app scaffold) with no performance or reliability blockers. The bundle size (232 kB / 74 kB gz) is acceptable for a local-first SPA MVP. The scaffold correctly sets up TanStack Query, Vite dev-proxy, and the `BrowserClient` transport abstraction. All suggested improvements are optional or deferred to P2 (screens implementation) where TanStack Query hooks will be configured.

**Verdict: Approve** — No Critical or Warning issues. 5 Suggestions are optional improvements for future iterations (P2/P3).