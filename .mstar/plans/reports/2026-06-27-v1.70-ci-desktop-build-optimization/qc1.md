---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-27-v1.70-ci-desktop-build-optimization"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: k2p7
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-27T23:57:00+08:00

## Scope
- plan_id: `2026-06-27-v1.70-ci-desktop-build-optimization`
- Review range / Diff basis: `merge-base: 69310a3191d05c80460da227360cba6c9d6539b8 + tip: 1d3d1735c4f9c790af59f924e80dda4ff22b8bbd — focused on .github/workflows/ (equivalent to: git diff 69310a31...HEAD -- .github/workflows/)`
- Working branch (verified): `iteration/v1.70`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 2 (`.github/workflows/desktop-build.yml` modified, `.github/workflows/desktop-release.yml` new)
- Commit range: `69310a31...1d3d1735`
- Tools run: `git branch --show-current`, `git diff 69310a31...HEAD --stat -- .github/workflows/`, `git diff 69310a31...HEAD -- .github/workflows/ci.yml`, `git diff --check 69310a31...HEAD -- .github/workflows/`, file reads

## Findings

### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion
- **S-1 — Deduplicate build setup between `desktop-build.yml` and `desktop-release.yml`**  
  The two workflows duplicate checkout, pnpm/node setup, Rust toolchain install, `rust-cache`, dependency install, and contract-package build steps. Consider introducing a composite action (e.g. `.github/actions/setup-desktop-build`) or a reusable workflow so that future changes to the build environment stay in sync and do not drift between CI and release pipelines.  
  -> Refactor common setup into a shared composite action and call it from both workflows.

## Source Trace
- Finding ID: F-001
- Source Type: manual-reasoning
- Source Reference: `.github/workflows/desktop-build.yml` lines 27–28 and `.github/workflows/desktop-release.yml` lines 27–53
- Confidence: High

## Review Notes

### B1 — Narrowed PR path trigger
Verified that `pull_request.paths` in `desktop-build.yml` now contains only:
- `apps/web/**`
- `apps/desktop/**`
- `.github/workflows/**`

The following paths were intentionally removed per the plan: `apps/nexus42/**`, `packages/nexus-contracts/**`, `crates/**`, `pnpm-lock.yaml`, `package.json`, `pnpm-workspace.yaml`, `Cargo.toml`, `Cargo.lock`. Because `apps/desktop/**` covers `apps/desktop/src-tauri/**`, Tauri-side changes are still included.

### B2 — Retained `push:main` integration safety net
The `push:branches: [main]` trigger still uses the original broad path filter (including crates, lockfiles, manifests, etc.), so CLI-only or lock-only changes are still validated after merge to `main`.

### B3 — New `desktop-release.yml`
The new workflow is correctly gated on `push: tags: ['v*']` and `release: types: [published]`. It performs a single universal Tauri build (`universal-apple-darwin`), packages the `.app` into a zip, and uploads both the zip and `.dmg` files to the GitHub release using `gh release create/upload` with `--clobber` for the release-published path. Permissions are scoped to `contents: write` and `actions: read`.

### B4 — `set -euo pipefail` placement
`set -euo pipefail` is placed at the top of the `Build Tauri desktop bundle (universal, fallback per-arch)` step in `desktop-build.yml`, covering both the universal attempt and the per-arch fallback within the same shell. It is also present in each shell step of `desktop-release.yml`.

### B5 — `ci.yml` untouched
`git diff 69310a31...HEAD -- .github/workflows/ci.yml` produced no output; the test gate is unchanged.

### Path-completeness regression check
Compared against the prior regression `R-V168P0-QC1-W001` (CLI-only PRs no longer triggered desktop-bundle CI because `apps/nexus42/**` was omitted after relocation). The current change is the *inverse and intentional* narrowing: `apps/nexus42/**` and other non-desktop surfaces are removed from the PR trigger, while `push:main` retains them. No silent omission of a desktop-relevant surface was found.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve
