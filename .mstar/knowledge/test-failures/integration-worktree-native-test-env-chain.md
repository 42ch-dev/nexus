---
title: Integration-worktree test environment chain for node:native tests
category: test-failures
track: knowledge
last_updated: 2026-10-17
created: 2026-10-17
status: active
problem_type: test_failure
module: nexus-service / nexus-native
date: 2026-10-17
severity: low
symptoms:
  - "a test green in a built feature worktree fails in a fresh integration worktree with `AssertionError: 2 !== 0` in the `before` hook"
  - "`error TS2688: Cannot find type definition file for 'node'` during the test's `npx tsc` step"
  - "tsc then resolves imports but the suite cancels with `platform native artifact missing at packages/nexus-native-darwin-arm64/native/nexus_core_node.node`"
root_cause: "A freshly created git worktree starts with none of the three artifact layers the node:native test stack needs — node_modules, workspace package dist/ outputs, and the Rust-built native addon. Each missing layer fails differently (tsc exit code, TS2688, then the loader's artifact check), and `pnpm dev:backend:refresh` does not populate the artifact inside a linked worktree."
resolution_type: workflow_improvement
---

# Integration-Worktree Test Environment Chain for node:native Tests

## Context

QA gates that run `node --test apps/nexus-service/tests/world-rules-http.test.mjs` (or any test that boots the standalone TS service over the native addon) must run against a fully prepared checkout. A fresh integration worktree (created with `git worktree add` from the integration branch) looks like a normal checkout but has none of the gitignored build artifacts.

## Guidance

Run the three-step preparation chain at the worktree root before any node:native test:

1. `pnpm install --frozen-lockfile` — restores `node_modules` (missing → the test's `npx tsc` step fails `TS2688`, and the `before` hook's `assert.equal(build.status, 0)` reports the tsc exit code as a confusing numeric mismatch, e.g. `2 !== 0`).
2. `pnpm -r run build` — builds workspace packages whose `dist/` the service's `tsc` step imports (`@42ch/nexus-contracts`, `@42ch/nexus-native`, `@42ch/nexus-provider-acp`; missing → unresolved-import type errors, values typed `unknown`).
3. `node packages/nexus-native/scripts/build.mjs` — builds and installs the platform native addon into `packages/nexus-native-<platform>/native/nexus_core_node.node` (missing → the service loader fails fast with `platform native artifact missing`).

Gotcha: `pnpm dev:backend:refresh` completes successfully but prepares the **primary checkout's** dev backend; it does not guarantee the artifact inside a linked worktree. When a test's own failure message names a build script (`run packages/nexus-native/scripts/build.mjs`), run exactly that script in the worktree.

Quarantine rule for QA gates: a rejection caused by a missing artifact layer is an **environment artifact, not code evidence**. Name the layer, fix the prep chain, and re-run; do not let it count as a code rejection wave (a real chain here cost four QA rounds before the layers were separated).

## Why This Matters

The three failures look like three unrelated bugs (assertion mismatch, TypeScript config, native packaging) but share one root cause. Misreading them as code defects sends QA and implementation into a fix loop against working code.

## When to Apply

- Any QA/verification run of `apps/nexus-service` tests, or any node test that boots `startDomainService`/loads `nexus_core_node.node`, inside a linked worktree (feature, integration, or fix).
- Any time a test that was green in one worktree fails identically-shaped in another.

## Examples

**v1.216 p1** (2026-10-17): the mandatory targeted QA gate for the world-rules HTTP teardown fix was REJECTED four times in the integration worktree — round 1 `2 !== 0` + `TS2688` (no `node_modules`), round 2 unresolved workspace imports (no `dist/`), rounds 3–4 `platform native artifact missing` (no addon; `dev:backend:refresh` had not populated it). After the full three-step chain, the same commands passed (`18 passed` Rust, `7 pass 0 fail` node) with zero code changes between runs.
