---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-03-v1.87-nexus-ui-component-library"
verdict: "Approve"
generated_at: "2026-07-03"
---

# Code Review Report — V1.87 (QC3: Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk (build/test surface for P0; async/blocking I/O in the P1 handler; CI gate readiness)
- Report Timestamp: 2026-07-03

## Scope
- plan_id: 2026-07-03-v1.87-nexus-ui-component-library
- Review range / Diff basis: `git diff main...iteration/v1.87` (merge-base `ffae19f9` to tip `60916911`; 18 files, +777/-84)
- Working branch (verified): iteration/v1.87
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 18 (2 new components + 2 test files + build/vitest config + P1 handler diff + P1 test additions + web wrapper + plan/status/compass docs)
- Commit range: merge commit `60916911` (Merge feature/v1.87-nexus-ui-component-library into iteration/v1.87); prior tip `4eb26a7c` (P1 hotfix R-V186-QC1-S005)
- Tools run: git checkout verification; scoped `pnpm build/typecheck/test` for `@42ch/nexus-ui` + `web`; `cargo test/clippy -p nexus-daemon-runtime`; `cargo +nightly-2026-06-26 fmt -p nexus-daemon-runtime --check`
- Deep review: **triggered** (signals: multi-module scope [P0 npm package + P0 web wrapper + P1 daemon handler] + new test infra [vitest jsdom config])
- Lenses applied: **Reliability Lens** (build chain, test determinism, async-runtime hygiene, degradation behavior)
- wire_contracts_changed: **false** (`git diff main...iteration/v1.87 -- schemas/ crates/nexus-contracts/src/generated/ packages/nexus-contracts/` produced no output)

## Findings

### Critical
_(none)_

### Warning
_(none)_

### Suggestion

- **S1 — Async-runtime hygiene inconsistency in the V1.87 P1 delegation** (Finding ID: F-001)

  The V1.87 refactor at `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:2135-2142` replaces the previous inline two-branch guard with a direct **synchronous** call to `resolve_guarded_path(workspace_root_path, &body_path, must_exist)`. That helper (`crates/nexus-daemon-runtime/src/api/path_guard.rs:37-101`) performs `std::fs::canonicalize(...)` syscalls, which are blocking. The V1.86 T5 hardening (residual `R-V156P0-M004`) explicitly moved `fs/read_text_file`, `fs/write_text_file`, and `validate_file_path` behind `resolve_guarded_path_async` — a `tokio::task::spawn_blocking` wrapper at `host_tool_handlers.rs:768-786` — precisely to avoid stalling the async runtime on local disk I/O.

  `execute_manuscript_read_range` (V1.87 P1 target), `execute_manuscript_chapter_update` (`:1435`, `:1502`), and `execute_manuscript_write` (`:2280`) call `resolve_guarded_path` synchronously. V1.87 did not introduce the inconsistency for `chapter_update` / `manuscript_write` (they pre-existed), but V1.87's `read_range` delegation was a fresh opportunity to converge on the V1.86 pattern that was not taken. For a single-user local daemon on a fast local FS, `canonicalize()` typically completes in microseconds and this is **not** a hot-path bottleneck — manuscript reads are author-action-frequency, not high-throughput. Impact is therefore low, but the divergence from the V1.86 direction is worth tracking.

  Recommendation: in a follow-up plan, migrate the sync `resolve_guarded_path` call sites in `host_tool_handlers.rs` (lines 1435, 1502, 2136, 2280) plus the two in `outline.rs` and `chapters.rs` to `resolve_guarded_path_async` so all admission paths share the same non-blocking discipline (`R-V156P0-M004` philosophy). Not a blocker for V1.87 merge — the correctness fix (component-wise sibling-escape closure) is fully preserved; only async hygiene is affected.

  Source: `git diff main...iteration/v1.87 -- crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs`; cross-reference V1.86 T5 residual `R-V156P0-M004` and the existing `resolve_guarded_path_async` helper at `host_tool_handlers.rs:773`.

- **S2 — `<NexusMark>` inline SVG allocates on every render** (Finding ID: F-002)

  `packages/nexus-ui/src/components/nexus-mark.tsx:23-58` returns a fresh JSX tree on every render (no `React.memo`, no `useMemo`). The tree is small (~14 SVG child elements, all primitive props), and typical call sites (sidebar / header) do not re-render inside animation loops, so today the impact is negligible. If `<NexusMark>` is later placed in a virtualized list, motion loop, or storyboard grid, wrapping it in `React.memo` would eliminate the subtree reconciliation cost. Ship-blocker: no. Track as a low-priority optimization if usage patterns change.

  Source: manual reasoning against `packages/nexus-ui/src/components/nexus-mark.tsx`.

## Source Trace

- **F-001**
  - Source Type: git-diff + manual-reasoning + doc-rule (V1.86 T5 residual `R-V156P0-M004`)
  - Source Reference: `host_tool_handlers.rs:2135-2142` (V1.87 delegation) vs `host_tool_handlers.rs:563, 624, 773-786, 814` (V1.86 async pattern) vs `path_guard.rs:37-101` (sync helper body)
  - Confidence: High — sync vs async split is directly verifiable via `grep -n resolve_guarded_path crates/nexus-daemon-runtime/src`
- **F-002**
  - Source Type: manual-reasoning
  - Source Reference: `packages/nexus-ui/src/components/nexus-mark.tsx:18-58`
  - Confidence: Medium — impact depends on future consumer usage

## Verification Evidence

### Build & test chain

| Step | Command | Result |
|------|---------|--------|
| nexus-ui build | `pnpm --filter @42ch/nexus-ui run build` | PASS — tsup builds index/tokens `.js/.cjs/.d.ts/.d.cts` (~540 ms DTS) |
| nexus-ui typecheck | `pnpm --filter @42ch/nexus-ui run typecheck` | PASS — `tsc --noEmit` clean (strict + `jsx: react-jsx`) |
| nexus-ui test | `pnpm --filter @42ch/nexus-ui run test` | PASS — **7 / 7 tests** (2 files, 608 ms, jsdom env) |
| web build | `pnpm --filter web run build` | PASS — `prebuild` chains contracts + nexus-ui builds; vite bundles 2492 modules in 3.09 s |
| web typecheck | `pnpm --filter web run typecheck` | PASS — clean (`pretypecheck` hook builds package `dist/` first) |
| web test | `pnpm --filter web run test` | PASS — **387 / 387 tests** across 51 files in 6.72 s |
| daemon-runtime tests | `cargo test -p nexus-daemon-runtime` | PASS — all suites green, incl. `manuscript_read_range_returns_bounded_content`, `manuscript_read_range_rejects_missing_chapter`, `manuscript_read_range_rejects_sibling_escape_body_path`, `manuscript_read_range_accepts_in_bounds_body_path` |
| daemon-runtime clippy | `cargo clippy -p nexus-daemon-runtime -- -D warnings` | PASS — workspace pedantic + nursery lints clean |
| daemon-runtime fmt | `cargo +nightly-2026-06-26 fmt -p nexus-daemon-runtime -- --check` | PASS — no diff |

Full-workspace CI gate (`cargo clippy --all -- -D warnings`, `cargo test --all`) was **not** run per the repo `AGENTS.md` `target/` hygiene rule that scopes iteration builds to `-p <crate>`. The scoped clippy already exercises the transitive dependency graph up to `nexus-daemon-runtime`, and no cross-crate signatures changed in V1.87 (wire contracts untouched). CI-gate readiness inferred green with high confidence.

### Reliability Lens observations

1. **Build ordering**: `apps/web/package.json` `prebuild` / `pretypecheck` invoke `pnpm --filter @42ch/nexus-ui run build` before the web build. V1.87 promoted `@42ch/nexus-ui` from a static tokens package to a React component library, so consumers now depend on the compiled `dist/index.{js,cjs}` + `.d.ts`. The lifecycle hooks are honored, and the CI job (`apps/web/AGENTS.md` §Build/typecheck contract) also builds contracts + nexus-ui first — both local and CI paths verified clean from a fresh state.

2. **Test-infra determinism** (`packages/nexus-ui/vitest.config.ts`): `environment: "jsdom"` + `globals: false` gives each test file an isolated JSDOM. `@testing-library/react` mounts to a fresh container per test with automatic cleanup. The 7 tests are pure DOM assertions with no shared module-level state, no timers, no network, no filesystem, no global registry — no V1.50-style flake surface (`review_report` global registry contention does not apply here). Sub-second total runtime.

3. **P1 handler async hygiene**: see F-001. Behavior change (sibling-escape closure via component-wise `Path::starts_with`) is correct and covered by `manuscript_read_range_rejects_sibling_escape_body_path` (`host_tool_executor_tests.rs:2837-2885`) plus the in-bounds happy path `manuscript_read_range_accepts_in_bounds_body_path` (`:2889-2919`). The refactor preserves functional semantics — missing-in-bounds file still returns `FILE_READ_FAILED`; existing out-of-bounds file returns `invalid_input`. The remaining reliability concern is purely stylistic / defense-in-depth (async runtime hygiene), tracked as S1.

4. **Component hot-path check**:
   - `<NexusLogo>` = single `<img>` — no allocation concern.
   - `<NexusMark>` = ~14 static SVG elements re-created per render but typical parents (`sidebar.tsx`, `header.tsx`) don't re-render tightly. Not a bottleneck (F-002).
   - `resolve_guarded_path` (P1) invoked once per `read_range` call — not an inner loop.

5. **Degradation behavior**:
   - `<NexusLogo>`: `src` is a required TS prop; if a consumer passes an empty string, the browser shows the alt text (`label`, default "Nexus") — graceful.
   - `<NexusMark>`: all sizes/labels have defaults; degrades to the 32 px `logoMinSizePx` glyph with the "Nexus" title.
   - `execute_manuscript_read_range`: unbounded `read_to_string` for the manuscript body still exists (previously flagged in V1.59 QC3 W1; not in V1.87 scope). Line-range slicing after the read means memory footprint equals full file size regardless of requested range. **Not a new regression in V1.87** — carried from V1.59.
   - Path guard error paths remap `chapter_path_*` codes to `invalid_input` field-level errors; consumers get a clear diagnostic without leaking internal chapter-path vocabulary.

### wire_contracts_changed
`false` — verified by `git diff main...iteration/v1.87 -- schemas/ crates/nexus-contracts/src/generated/ packages/nexus-contracts/` producing zero changed files.

## Summary

| Severity | Count |
|----------|-------|
| Critical | 0 |
| Warning  | 0 |
| Suggestion | 2 |

**Verdict**: Approve

Rationale: All three verification tiers (npm package build/test, web build/test, cargo test/clippy/fmt) are green with `Critical = 0`, `Warning = 0`. The V1.87 P0 (React component library promotion) ships with clean tsup + tsc + vitest gates and preserves the `apps/web` `prebuild` ordering. The V1.87 P1 delegation to `resolve_guarded_path` correctly closes R-V186-QC1-S005 with regression tests. Two Suggestions (async-runtime hygiene follow-up S1 for the daemon path guard; optional `React.memo` for `<NexusMark>` S2) are non-blocking and should be tracked as residuals for a future plan.

## Residual candidates for PM

| Proposed id | Title | Severity | Owner suggestion | Trigger |
|-------------|-------|----------|------------------|---------|
| R-V187-QC3-P001 | Migrate remaining sync `resolve_guarded_path` call sites in `host_tool_handlers.rs` (lines 1435, 1502, 2136, 2280), `outline.rs:159/239`, `chapters.rs:209/268` to `resolve_guarded_path_async` for R-V156P0-M004 consistency | Suggestion | daemon-runtime maintainer | Next daemon-runtime performance plan |
| R-V187-QC3-P002 | Consider `React.memo` for `<NexusMark>` if it lands in high-render surfaces (virtualized lists, motion loops) | Suggestion | frontend-dev | Only if usage pattern changes |
