---
module: tooling
date: 2026-08-14
problem_type: knowledge
category: conventions
severity: medium
plan_id: 2026-08-14-v1.164-p1-spoke-010-upgrade-observation-passthrough
tags: [pnpm, toolchain, supply-chain, minimumReleaseAge, lockfile, ci-pin, allowBuilds]
last_updated: 2026-08-15
applies_when: Installing or upgrading npm deps (especially same-day releases, spoke lockstep bumps); any pnpm install failure mentioning ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION, ERR_PNPM_IGNORED_BUILDS, or MODULE_NOT_FOUND after a partial install
---

# pnpm 11 Toolchain Pin, Workspace Settings, and Supply-Chain Age Policy

## Context

CI and local both run pnpm **11** (`.github/actions/setup-monorepo/action.yml` default `pnpm-version: "11"`; `engines.pnpm >= 11`). Two pnpm-11 behaviors are load-bearing for this repo:

1. **Settings moved to `pnpm-workspace.yaml`.** pnpm 11 stopped reading the `package.json` `pnpm` field entirely (it warns "no longer read by pnpm"). Security/version `overrides` live in `pnpm-workspace.yaml` — keep them there; re-adding a `pnpm` field to package.json silently does nothing.
2. **Dependency lifecycle scripts are blocked by default.** Without an `allowBuilds` allowlist, `pnpm install` fails with `ERR_PNPM_IGNORED_BUILDS` (exit 1 — breaks `--frozen-lockfile` in CI, not just a warning). `allowBuilds: { esbuild: true, msw: true }` preserves the pnpm-9 behavior; esbuild needs its postinstall to place the platform binary (vite/tsup break without it), msw's postinstall is a guarded no-op.

Historical note (why this pin lagged): until 2026-08-15 CI pinned pnpm 9 while dev machines ran 11, and a local pnpm 11 `minimumReleaseAge` supply-chain policy rejected same-day publishes (e.g. `@42ch/spoke-*@0.10.0`), with failed installs sometimes partially wiping `node_modules/.pnpm`. Hit 3× in V1.164 before the pin caught up.

## Guidance

1. **Ambient pnpm is now the CI pin** — plain `pnpm install` / `pnpm run` is correct; no more `npx -y pnpm@9` shims.
2. **On `ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`** (local policy, currently unset on this machine but can return): don't retry the same command and don't edit the lockfile — temporarily relax the local policy (`pnpm config set minimumReleaseAge 0`, revert after) or wait out the window.
3. **On `ERR_PNPM_IGNORED_BUILDS`**: a new dep with a postinstall needs an `allowBuilds` entry in `pnpm-workspace.yaml`. Deliberate — review what the script does before allowing it.
4. **On `MODULE_NOT_FOUND` after any failed install**: assume partial wipe — `rm -rf node_modules && pnpm install --frozen-lockfile`, then re-run typecheck.
5. **Lockfile updates**: any pnpm 11 writes `lockfileVersion: 9.0` (same as 9.x); no cross-version churn.

## Why This Matters

The `minimumReleaseAge` failure mode masquerades as a broken dependency (the just-published package looks "missing"), sending you to investigate the registry rather than the local toolchain; the partial-wipe collateral makes it look like repo-level breakage. The `pnpm`-field removal masquerades as "overrides stopped working" (stale vulnerable transitive versions pass unnoticed).

## When to Apply

- Any dependency bump; new dep ships a postinstall script.
- CI is green locally red (or vice versa) on install/typecheck — first check `pnpm --version` vs the CI pin and `pnpm config get minimumReleaseAge`.
- After any interrupted pnpm operation.

## Examples

V1.164 P1: `@42ch/spoke-schemas@0.10.0` + `@42ch/spoke-operations@0.10.0` npm bump under the old pin — QA agent's pnpm 11 rejected the fresh packages; `npx -y pnpm@9 run typecheck` (then-pin) passed all 8 workspace projects. P3 fixture task: pnpm 11 failed deps-check wiped `node_modules/.pnpm`; restored via frozen pnpm@9 install (then-pin). 2026-08-15 pin-upgrade PR: overrides migration verified byte-identical by a clean frozen install (zero lockfile drift); `allowBuilds` discovered via ERR_PNPM_IGNORED_BUILDS exit 1 on the very same frozen install — note `pnpm approve-builds <pkgs>` (non-interactive) writes the `allowBuilds` map for you.
