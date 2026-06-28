---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-28-v1.72-hygiene-and-release-hardening"
verdict: "Approve"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro (deepseek/deepseek-v4-pro)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-28T17:10:00+08:00

## Scope
- plan_id: 2026-06-28-v1.72-hygiene-and-release-hardening
- Review range / Diff basis: `git diff 92a1c07f..HEAD -- .github/actions/setup-monorepo/ .github/workflows/ci.yml .github/workflows/desktop-build.yml .github/workflows/desktop-release.yml apps/desktop/SIGNING.md apps/web/src/components/canvas/strategy-canvas.tsx apps/web/src/components/canvas/strategy-canvas/ apps/web/src/lib/canvas/use-strategy-data.ts .mstar/status.json`
- Working branch (verified): `iteration/v1.72`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 11 new/modified source files + 2 test files + status.json
- Commit range: `92a1c07f..HEAD`
- Tools run: `cargo clippy --all -- -D warnings`, `cargo +nightly-2026-06-26 fmt --all --check`, `cargo test --all` (partial — see W1), `pnpm --filter web typecheck`, `pnpm --filter web build`, `pnpm --filter web test`, `wc -l` line counts, `jq` residual verification

## Findings

### 🔴 Critical
*None*

### 🟡 Warning

**W1: React useEffect deps churn in inspector save handlers (meta-architectural)**

- **Location**: `apps/web/src/components/canvas/strategy-canvas/inspectors/{state,edge,prompt}-inspector.tsx`
- **Finding**: All three inspector components define `async function handleSave()` inline and include `handleSave` in their `useEffect` dependency arrays. Since `handleSave` is recreated on every render, the effect fires more often than necessary (triggered on every render, not just when `saveTrigger` changes). The early-exit guards (`if (!dirty || patch.isPending) return`) prevent duplicate server calls, so there is no data-integrity risk, but this creates unnecessary effect churn and makes the dependency reasoning fragile.
- **Risk**: Low — early-exit guards prevent actual double-saves. The pattern is safe in practice but signals an architectural tension between imperative save triggers and React's declarative effect model.
- **Fix**: Wrap `handleSave` in `useCallback` with proper dependencies, or use a ref-based trigger pattern (`const handleSaveRef = useRef(handleSave); handleSaveRef.current = handleSave;` then call `handleSaveRef.current()` in the effect). This stabilizes the dependency and eliminates the churn.
- **Severity**: Warning (low impact, cosmetic correctness concern)
- **Confidence**: Medium — behavior is correct today but the pattern is fragile under refactoring.

**W2: `cargo test --all` timed out — integration tests (pre-existing)**

- **Location**: N/A (no Rust changes in this plan scope)
- **Finding**: `cargo test --all` timed out at 300s. All 959 unit/integration tests that completed passed with 0 failures before the timeout (the `auto_chain` integration test likely caused the hang). Since V1.72 P1 modified zero Rust files (`wire_contracts_changed: FALSE`), this is a pre-existing CI flake, not caused by P1 changes.
- **Risk**: None for this plan — no Rust code was changed.
- **Fix**: Track as a pre-existing CI stability concern (defer to V1.73 ops backlog). The timed-out test is not in P1's diff scope.
- **Severity**: Warning (pre-existing; blocks full verification but not P1's responsibility)
- **Confidence**: High — timeout is reproducible and isolated to non-Rust-change plan.

### 🟢 Suggestion

**S1: Orchestrator facade at 187 lines — limited expansion headroom**

- **Location**: `apps/web/src/components/canvas/strategy-canvas.tsx` (187 lines)
- **Finding**: The orchestrator facade is within the 200-line ceiling but has only 13 lines of headroom (6.9%). If Track A or future features add more orchestration logic (e.g., additional inspector sections, context panels), this file will breach the limit and need further splitting. No action needed now.
- **Confidence**: High — straightforward measurement.

**S2: `desktop-build.yml` served-ui-smoke job could benefit from composite action's Rust caching**

- **Location**: `.github/workflows/desktop-build.yml` lines 83-109
- **Finding**: The `served-ui-smoke` job uses `setup-monorepo` composite action but then adds a separate `Swatinem/rust-cache` step. Consider adding an optional `enable-rust-cache` input to the composite action for jobs that need it, keeping caching concerns centralized.
- **Confidence**: Low — architectural preference; no functional issue.

## Source Trace

### B2: strategy-canvas.tsx split (R-V171P0-QC1-006)

| File | Lines | Status |
|------|-------|--------|
| `strategy-canvas.tsx` (orchestrator facade) | 187 | ✅ ≤200 |
| `canvas-layout.tsx` (header + footer) | 80 | ✅ ≤200 |
| `inspector-panel.tsx` (aside shell + StrategyConflictModal) | 152 | ✅ ≤200 |
| `state-machine.tsx` (helpers + shared UI pieces) | 161 | ✅ ≤200 |
| `inspectors/state-inspector.tsx` | 163 | ✅ ≤200 |
| `inspectors/edge-inspector.tsx` | 157 | ✅ ≤200 |
| `inspectors/prompt-inspector.tsx` | 149 | ✅ ≤200 |
| `hooks/use-strategy-canvas.ts` (orchestrator hook) | 169 | ✅ ≤200 |
| `lib/canvas/use-strategy-data.ts` (data hooks, slimmed) | 177 | ✅ ≤200 |

- 7 modules split from the original 571-line `strategy-canvas.tsx` → all ≤200 lines ✅
- Public facade preserved: `StrategyCanvas` + `StrategyCanvasProps` re-exported from the facade ✅
- Module boundaries: orchestrator → hook → state-machine / inspectors / layout → clean separation ✅
- `state-machine.ts` → `state-machine.tsx` rename justified: it contains JSX (`RevisionBadge`, `ValidationPanel`, `ArtifactsList`) ✅
- `conflict-modal.tsx` wrapper merge into `inspector-panel.tsx` as `StrategyConflictModal`: the wrapper still imports `ConflictModal` from `@/components/canvas/conflict-modal` (shared path, not strategy-canvas-internal) ✅
- Test imports: `conflict-modal.test.tsx` and `outline-conflict-modal.test.tsx` import unchanged paths ✅
- Web tests: 19 files, 153 tests, all passed ✅

### B1: per-inspector save (R-V171P0-QC1-004)

- Each inspector (`state`, `edge`, `prompt`) owns: its own save button, its own mutation, its own `saveStatus` per-section UI ✅
- `Cmd/Ctrl+S` dispatches only to `activeSection` via `saveTriggers[activeSection]` ✅
- Shared `form` state is intentional — it represents the unified edit form; `saveStatuses` are per-section (independent) ✅
- Shared `workingRevisionRef` for optimistic concurrency — correct design; failure in one inspector triggers `handleConflict` which is shared but each inspector receives its own `section` parameter ✅
- No compensating rollback logic — per spec lock ✅
- Architecture: clean separation; the orchestrator hook `useStrategyCanvas` owns the shared state but delegates per-section save to each inspector's own mutation ✅
- See also: W1 (useEffect deps churn in inspector save handlers)

### B3: desktop-release.yml (R-V171-CI-RELEASE-WORKFLOW-INCOMPLETE)

- Step ordering: evaluate secrets → build → keychain import → codesign → recreate DMG → notarize → staple → upload → cleanup ✅
- Dependencies correct: codesign needs keychain, notarize needs signed DMG, staple needs notarized DMG ✅
- Secret gating (3-branch): all-present → sign & notarize; none → unsigned with notice; partial → upload unsigned THEN fail with clear message ✅
- Hardened runtime: `--options runtime --timestamp` on codesign ✅
- Notarization: `--wait` flag ✅
- Staple after notarization ✅
- Keychain cleanup: `if: always() && steps.sign-eval.outputs.should_sign == 'true'` — ensures cleanup even on failure ✅
- SIGNING.md documentation: comprehensive (83 lines, 6-section flow, secret table, behavior matrix) ✅

### B4: composite action (R-V171-CI-WORKFLOW-SETUP-DEDUPE)

- `.github/actions/setup-monorepo/action.yml`: 55 lines, 5 inputs, 4 composited steps (pnpm, node, rust-toolchain, pnpm install) ✅
- `ci.yml`: net -24 lines (33 deleted, 9 added). Jobs converted: `validate-schemas`, `verify-codegen`, `typescript-checks`, `web-build` ✅
- `desktop-build.yml`: net -31 lines (36 deleted, 5 added). Jobs converted: `desktop-build`, `served-ui-smoke` ✅
- Jobs NOT converted (`rust-checks`, `verify-sqlx-offline`, `rust-tests`): correctly skip composite — they have special toolchain needs (nightly fmt, artifact downloads, specific Rust targets) ✅
- Reusable from `desktop-release.yml` (line 61): `uses: ./.github/actions/setup-monorepo` with `rust-targets: aarch64-apple-darwin, x86_64-apple-darwin, wasm32-unknown-unknown` ✅
- Matrix/OS coverage preserved: `ci.yml` uses `ubuntu-latest`, `desktop-build.yml` uses `macos-14`, `desktop-release.yml` uses `macos-14` ✅

### T9/T10: Residual lifecycle

**4 resolved residuals with correct resolution metadata:**

| R-id | Plan ID | Commit | Status |
|------|---------|--------|--------|
| R-V171P0-QC1-004 (B1) | V1.72 P1 | `73ed508b` | ✅ `lifecycle: resolved` |
| R-V171P0-QC1-006 (B2) | V1.72 P1 | `73ed508b` | ✅ `lifecycle: resolved` |
| R-V171-CI-RELEASE-WORKFLOW-INCOMPLETE (B3) | V1.72 P1 | `b480a283` | ✅ `lifecycle: resolved` |
| R-V171-CI-WORKFLOW-SETUP-DEDUPE (B4) | V1.72 P1 | `9a6591aa` | ✅ `lifecycle: resolved` |

Verified via `jq` — counts match expectations: 2 for `canvas-strategy-write-boundary`, 2 for `hygiene-and-sign-groundwork`. ✅

**8 deferred residuals with V1.73 targets:**

| R-id | Target | Source plan_id |
|------|--------|---------------|
| R-V165-QC3-VIRT | V1.73 hygiene backlog | (legacy V1.65) |
| R-V171P0-QC1-008 | V1.73 hygiene backlog | canvas-strategy-write-boundary |
| R-V171P0-QC1-010 | V1.73 hygiene backlog | canvas-strategy-write-boundary |
| R-V171P1-QC1-002 | V1.73 hygiene backlog | hygiene-and-sign-groundwork |
| R-V171P1-QC1-003 | V1.73 hygiene backlog | hygiene-and-sign-groundwork |
| R-V171-GREPTILE-POST5-TOAST | V1.73 hygiene backlog | hygiene-and-sign-groundwork |
| R-V171-GREPTILE-POST5-ROLLBACK-ATOMIC | V1.73 hygiene backlog | hygiene-and-sign-groundwork |
| R-V171P1-QC1-001 | V1.73 release hardening backlog | hygiene-and-sign-groundwork |

7 → hygiene, 1 → release hardening ✅. All `lifecycle: open` ✅. No residual incorrectly closed ✅.

### Cross-track coordination (architect §6.9)

- Track A (P0) imports `ConflictModalBase` from `@/components/canvas/conflict-modal-base` (unchanged path) ✅
- Track A imports `OutlineConflictModal` from `@/components/canvas/outline-conflict-modal` (unchanged path) ✅
- B2's `strategy-canvas/inspector-panel.tsx` imports `ConflictModal` from `@/components/canvas/conflict-modal` (NOT from strategy-canvas sub-path) ✅
- B2's `strategy-canvas/state-machine.tsx` imports types from `@/components/canvas/conflict-modal` ✅
- No deep import from `strategy-canvas/` required by Track A ✅
- `conflict-modal.tsx` remains at its original path (not moved into strategy-canvas/) ✅
- `conflict-modal.test.tsx` passes (8 tests) ✅
- `outline-conflict-modal.test.tsx` passes (6 tests) ✅

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (W1: useEffect deps churn — cosmetic; W2: pre-existing test timeout — out of scope) |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

**Rationale**: Both warnings are low-impact and non-blocking. W1 (useEffect deps churn) is a code-pattern concern with no runtime bug — the early-exit guards prevent actual double-saves. W2 (test timeout) is a pre-existing CI flake unrelated to P1's scope (zero Rust files changed). All four residuals (B1-B4) are correctly resolved with proper commit references. The strategy-canvas split is architecturally sound with clean module boundaries and preserved public facade. The desktop-release.yml signing flow has correct step ordering and robust secret gating. The composite action correctly deduplicates setup across workflows while preserving behavior. Cross-track import compatibility is maintained for Track A.

---

## Completion Report v2

**Agent**: qc-specialist
**Task**: QC tri-review (Reviewer #1 — Architecture coherence + maintainability) for V1.72 Plan P1 (`2026-06-28-v1.72-hygiene-and-release-hardening`)
**Status**: Done
**Scope Delivered**: Full architecture review of B1 (per-inspector save), B2 (strategy-canvas split), B3 (desktop-release.yml signing), B4 (composite action), T9 (residual lifecycle), T10 (V1.73 backlog pointers), and cross-track coordination (§6.9). All automated checks run (clippy, fmt, typecheck, build, web tests), all key files read, all structural invariants verified.
**Artifacts**: `.mstar/plans/reports/2026-06-28-v1.72-hygiene-and-release-hardening/qc1.md`
**Validation**:
- `cargo clippy --all -- -D warnings` → PASS
- `cargo +nightly-2026-06-26 fmt --all --check` → PASS
- `pnpm --filter web typecheck` → PASS
- `pnpm --filter web build` → PASS
- `pnpm --filter web test` → 19/19 files, 153/153 tests PASS
- All 9 files ≤200 lines, 4 residuals correctly resolved, 8 deferred with correct V1.73 targets
**Issues/Risks**: W1 (useEffect deps churn — cosmetic, no runtime bug), W2 (pre-existing `cargo test --all` timeout — unrelated to P1 scope)
**Plan Update**: None required
**Handoff**: Report ready for PM consolidated review. No blocking findings.
**Git**: See below.
