---
module: tooling
date: 2026-08-14
problem_type: knowledge
category: conventions
severity: medium
plan_id: 2026-08-14-v1.164-p1-spoke-010-upgrade-observation-passthrough
tags: [pnpm, toolchain, supply-chain, minimumReleaseAge, lockfile, ci-pin, spoke-lockstep]
last_updated: 2026-08-14
applies_when: Installing or upgrading npm deps on the same day they are published (spoke lockstep bumps, fresh releases); any pnpm install/typecheck failure mentioning ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION or MODULE_NOT_FOUND after a partial install
---

# pnpm Toolchain Pin vs Local pnpm 11 Supply-Chain Age Policy

## Context

CI installs pnpm **9** (`.github/actions/setup-monorepo/action.yml` default `pnpm-version: "9"`; `engines.pnpm >= 8`). A developer-local pnpm **11** enforces `minimumReleaseAge` supply-chain policy: it **rejects packages published more recently than the policy window** — which blocks every same-day release (e.g. `@42ch/spoke-*@0.10.0` published the morning of the upgrade). Worse, a failed pnpm 11 `verify-install` can leave `node_modules/.pnpm` partially wiped, so subsequent commands fail with `MODULE_NOT_FOUND` even after the policy issue is bypassed.

Hit three separate times in V1.164 (P1 fix wave, P3 Studio fixture task, P3 App wiring task), each with ~5–10 min lost to diagnosis/repair.

## Guidance

1. **Always run installs through the CI pin:** `npx -y pnpm@9 install --frozen-lockfile` (or `npx -y pnpm@9 run <script>`). Never rely on the ambient pnpm.
2. **On `ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`:** do not retry with the same pnpm, and do not edit the lockfile. Switch to pnpm 9.
3. **On `MODULE_NOT_FOUND` after any failed install:** assume partial wipe — `rm -rf node_modules && npx -y pnpm@9 install --frozen-lockfile`, then re-run typecheck.
4. **Lockfile updates** (dep bumps) also go through pnpm 9; pnpm 11 may honor different `pnpm.overrides` semantics than the CI pin, producing a lockfile CI can't reproduce.

## Why This Matters

The failure mode masquerades as a broken dependency (the just-published package looks "missing"), sending you to investigate the registry or the package rather than the local toolchain. The partial-wipe collateral makes it look like a repo-level breakage.

## When to Apply

- Any same-day dependency bump (spoke lockstep is the recurring case).
- CI is green locally red (or vice versa) on install/typecheck — first check `pnpm --version` vs the CI pin.
- After any interrupted pnpm operation.

## Examples

V1.164 P1: `@42ch/spoke-schemas@0.10.0` + `@42ch/spoke-operations@0.10.0` npm bump — QA agent's pnpm 11 rejected the fresh packages; `npx -y pnpm@9 run typecheck` (after clean reinstall) passed all 8 workspace projects. P3 fixture task: pnpm 11 failed deps-check wiped `node_modules/.pnpm`; restored via frozen pnpm@9 install with zero repo config changes.
