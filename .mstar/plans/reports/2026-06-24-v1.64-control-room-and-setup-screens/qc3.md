---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-24-v1.64-control-room-and-setup-screens"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: zhipuai-coding-plan/glm-4.7
- Review Perspective: Performance and Reliability
- Report Timestamp: 2026-06-25T01:57:43Z

## Scope
- plan_id: 2026-06-24-v1.64-control-room-and-setup-screens
- Review range / Diff basis: 56bf917a..4dd8cbb1
- Working branch (verified): iteration/v1.64
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 67 files (P2 scope: apps/web/src/**, package.json, vite.config.ts)
- Commit range: 56bf917a..4dd8cbb1
- Tools run: git diff, pnpm build, pnpm test, vite config analysis, bundle size audit

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

#### S-001: Consider bundle code-splitting per route for future scale
**Context**: Current bundle is 319KB JS / 98KB gzip (up from P1's 232KB). The 87KB increase is attributed to 7 screens + @radix-ui/dialog. For MVP this is acceptable for local-first usage (all resources served from localhost daemon with no network penalty), but as the UI grows this monolithic chunk will increase initial load time.

**Evidence**:
- `pnpm --filter web build` output shows single JS bundle: `dist/assets/index-CDQAIfH3.js` 319.20 kB │ gzip: 98.17 kB
- Total dist size: 340KB (index.html 0.41KB + CSS 20.70KB + JS 312KB)
- No code-splitting configured in `vite.config.ts` (no `build.rollupOptions.output.manualChunks`)

**Rationale**: Vite's default behavior produces a single bundle. For a local-first app served from embedded assets on localhost, the impact is minimal. However, should the UI scale to 20+ screens (future V1.65+ Tauri webview release), users on constrained systems or using remote daemon access would benefit from route-based code-splitting to reduce initial paint time.

**Recommendation** (non-blocking, defer to V1.65+ Tauri shell planning):
- Evaluate `build.rollupOptions.output.manualChunks` to split by route group (Control Room vs Setup)
- Or wait until Tauri webview use patterns emerge and measure actual startup latency
- Document decision in apps/web/AGENTS.md under "Performance" section

**Affected files**: `apps/web/vite.config.ts`, `apps/web/package.json`

---

#### S-002: Document toast-storm mitigation strategy in use-toast.tsx
**Context**: The QueryCache onError hook surfaces a toast for every failed query. This is appropriate for the current local-first model where the daemon is expected to be running, but in scenarios where many queries fail rapidly (e.g., daemon crash, network disconnection for remote daemon access), users could see a "toast storm" of error notifications.

**Evidence**:
- `apps/web/src/main.tsx` line 44: `queryCache: new QueryCache({ onError })`
- `apps/web/src/lib/use-toast.tsx` lines 48-64: Toast queue implementation with auto-dismiss (6s default)
- No debouncing or deduplication mechanism for identical error messages
- TanStack Query default `retry: 1` (configured in `main.tsx` line 39) mitigates but doesn't prevent multiple distinct queries failing

**Rationale**: Current implementation is correct for MVP: each distinct error should be surfaced. However, for remote daemon access (future V1.66+) where network blips affect multiple queries, consider adding error deduplication (e.g., debounce on error type, or limit toasts visible at once). The implementation already has infrastructure for this (`toasts` array state, dismiss capability).

**Recommendation** (non-blocking, future consideration):
- Add a doc comment in `use-toast.tsx` explaining the current policy: one toast per distinct failure
- Document in apps/web/AGENTS.md that remote access may need error deduplication
- Consider adding a `toast` variant that batches multiple failures if remote patterns emerge

**Affected files**: `apps/web/src/lib/use-toast.tsx`, `apps/web/src/main.tsx`, `apps/web/AGENTS.md`

---

#### S-003: Monitor normalizeList allocation frequency in production
**Context**: The F-P3 adapter (`normalizeList`) allocates a new `{ items, pagination }` object on every fetch. For cursor-paginated Works/Findings, this happens on each page load. While acceptable for MVP (small lists, local daemon), track this if users report jank on very large worksets or slow daemon responses.

**Evidence**:
- `apps/web/src/lib/nexus/adapters.ts` lines 38-45: `normalizeList` creates new object every call
- `apps/web/src/api/queries.ts` lines 35-38, 86-89: Called in `queryFn` for Works and Findings
- Tests verify correctness (10 tests pass) but no performance benchmarks

**Rationale**: The allocation is minimal (object with array reference + pagination copy). For local daemon with <1000 works, this is negligible. However, if the UI grows to support enterprise-scale catalogs (10k+ works) or remote daemon access with 200ms+ latency, consider memoizing the normalization or having the daemon return canonical `items` shape directly (F-P3 closure target V1.66+).

**Recommendation** (non-blocking):
- Add TODO comment in `adapters.ts` linking to F-P3 structural closure plan
- No action needed now; track in P-last metrics if performance issues emerge

**Affected files**: `apps/web/src/lib/nexus/adapters.ts`

---

## Source Trace

### Finding S-001
- Finding ID: S-001
- Source Type: manual-reasoning + build output
- Source Reference: `pnpm --filter web build` output, `apps/web/vite.config.ts`
- Confidence: Medium

### Finding S-002
- Finding ID: S-002
- Source Type: manual-reasoning + code audit
- Source Reference: `apps/web/src/main.tsx:44`, `apps/web/src/lib/use-toast.tsx:48-64`
- Confidence: High

### Finding S-003
- Finding ID: S-003
- Source Type: manual-reasoning + code audit
- Source Reference: `apps/web/src/lib/nexus/adapters.ts:38-45`, `apps/web/src/api/queries.ts:35-38,86-89`
- Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

## Detailed Analysis (Performance & Reliability Focus)

### P2 Performance Assessment

**Bundle Size**: ✅ Acceptable for local-first MVP
- Total dist: 340KB (JS 312KB + CSS 20KB + HTML 0.4KB)
- Gzipped JS: 98KB (well within reasonable range for localhost serving)
- Single-chunk architecture is appropriate for current scope (7 screens)
- No action needed; monitor growth in V1.65+ Tauri planning (see S-001)

**TanStack Query Configuration**: ✅ Conservative defaults appropriate for local daemon
- `retry: 1` — avoids noisy retries when daemon is legitimately down
- `refetchOnWindowFocus: false` — prevents unnecessary refetches for local-first use
- `staleTime: 15_000` — reasonable balance between freshness and cache
- These defaults align with the local loopback model (no network latency)

**Cursor Pagination**: ✅ Correctly implemented
- `useInfiniteQuery` for Works and Findings (the two paginated endpoints per plan)
- `getNextPageParam` correctly checks `has_more` and returns `next_cursor`
- `fetchNextPage` exposed via `LoadMore` component with disabled state
- No unbounded growth: user controls pagination via explicit button click
- 30 tests green (including adapters test coverage)

**F-P3 Adapter Performance**: ✅ Minimal overhead, well-tested
- `normalizeList` is O(n) where n is array length (just rewraps array)
- Allocation per fetch is acceptable for local-first use case
- Tests verify idempotency and edge cases (missing keys, already-normalized payloads)
- See S-003 for future monitoring recommendation

**Toast System**: ✅ Correct error surface pattern; see S-002 for remote access consideration
- QueryCache onError surfaces exactly one toast per failed query
- Auto-dismiss (6s default) prevents UI clutter
- Manual dismiss capability provided
- No deduplication needed for current local-first model

**MSW Test Runtime**: ✅ Clean setup
- `apps/web/src/test/msw-server.ts` provides test isolation
- 30 tests pass in 3.29s (reasonable for the test suite)
- No performance anti-patterns detected in test files

### P2 Reliability Assessment

**Error Handling**: ✅ Robust error-to-toast pipeline
- QueryCache onError catches all read failures (mutations handle their own errors)
- NexusClientError.message parsing provides structured error surface
- Toast queue prevents UI blocking
- No silent failures detected

**Cache Invalidation**: ✅ Correct invalidation patterns
- Mutations invalidate appropriate queries after success
- Example: `usePatchWork` invalidates both `works.lists()` and `work.detail(workId)`

**Type Safety**: ✅ Strict TypeScript with generated contracts
- `pnpm --filter web typecheck` passes
- No `any` usage in query layer (uses generated types from @42ch/nexus-contracts)
- Cursor and pagination types properly typed

**Runtime Stability**: ✅ No detected hot-path issues
- No blocking synchronous operations in query layer
- No infinite loops or unbounded recursions detected
- Adapter functions are pure and side-effect-free

## Conclusion

P2 (Control Room + Setup Screens) delivers acceptable performance and reliability characteristics for a local-first MVP. The bundle size increase (87KB) is justified by the 7 screens + @radix-ui/dialog addition, and serving from localhost daemon eliminates network latency concerns. TanStack Query defaults are well-tuned for the local loopback model. Cursor pagination and adapter implementations are correct and well-tested.

Three suggestions are documented for future scale considerations (code-splitting, toast deduplication for remote access, normalizeList monitoring). These are non-blocking for V1.64 delivery and should be evaluated during V1.65+ Tauri shell planning or F-P3 structural closure work.

**Verdict**: Approve