---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-27-v1.68-cli-relocation-and-product-surface-formalization"
verdict: "Request Changes"
generated_at: "2026-06-27"
---
# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance/reliability, scope boundary, CI/wire-contract integrity, historical-record integrity, missed live references
- Report Timestamp: 2026-06-27T09:42:06Z

## Scope
- **plan_id**: `2026-06-27-v1.68-cli-relocation-and-product-surface-formalization`
- **Review range / Diff basis**: `4606395e..2a4e5577` (origin/main → iteration/v1.68 HEAD; substantive implement is commit `2a4e5577`; earlier commits are Prepare docs)
- **Working branch**: `iteration/v1.68`
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus` (verify with `git rev-parse --show-toplevel` + `git branch --show-current` first)
- **QC gate**: initial full tri-review
- Working branch (verified): `iteration/v1.68`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 126 files in `4606395e..2a4e5577`; 123 files in substantive commit `2a4e5577`.
- Commit range: `4606395e..2a4e5577` (substantive implementation commit: `2a4e5577cdc9ea2c76a67304ba3ff4b9e8fd1fe7`)
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD && git log --oneline -5`
  - `git status --short`
  - `git show --stat 2a4e5577 | grep -E '\.mstar/(iterations|plans)/.*\.md'`
  - `git show --name-status --oneline 2a4e5577`
  - `git show --name-only --format= 2a4e5577 -- schemas/ packages/ crates/nexus-contracts/`
  - `git show 2a4e5577 -- schemas/ packages/ crates/nexus-contracts/`
  - `git grep -n 'crates/nexus42' -- ':!.mstar' ':!*.lock'`
  - `git grep -n -E 'crates/nexus42|--manifest-path[ =].*nexus42|path:.*crates/nexus42|externalBin.*crates/nexus42' -- .github/workflows/`
  - `git grep -n -E 'crates/\*\*|apps/\*\*|crates/nexus42|apps/nexus42|nexus42' -- .github/workflows/`
  - `cargo metadata --no-deps --format-version 1`
  - `cargo test -p nexus42`
  - `bash tooling/check-schema-drift.sh`
  - `cargo +nightly-2026-06-26 fmt --all --check`
  - `cargo clippy --all -- -D warnings`
  - `git diff --check 4606395e..2a4e5577`

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- **W-001 — Desktop Build PR path filter omits `apps/nexus42/**`, so CLI-only sidecar changes will not trigger the desktop bundle CI job.** The relocation made `apps/nexus42` the product-surface source for the bundled `nexus42` sidecar, but `.github/workflows/desktop-build.yml` still triggers on `apps/web/**`, `apps/desktop/**`, `packages/nexus-contracts/**`, and `crates/**` only. A PR that changes only `apps/nexus42/**` (for example a sidecar behavior change) can skip `desktop-build`, despite the job building and bundling `nexus42` via `pnpm -w run sidecar` / `cargo build --release -p nexus42`. This is a CI coverage regression introduced by the new location. -> Add `apps/nexus42/**` to both `push.paths` and `pull_request.paths` in `.github/workflows/desktop-build.yml` (or broaden to `apps/**` if intended), then rerun path-filter/CI checks.

### 🟢 Suggestion
- **S-001 — `apps/AGENTS.md` contains Markdown hard-break trailing spaces.** `git diff --check 4606395e..2a4e5577` reports trailing whitespace on `apps/AGENTS.md:13-14`. This is documentation-only and does not affect runtime/CI in this repo, but cleaning it would keep future diffs lint-friendly if whitespace checks are added.

## Required Assignment Checks

### Scope boundary / historical records
- `git show --stat 2a4e5577 | grep -E '\.mstar/(iterations|plans)/.*\.md'` returned **no output**. The substantive commit did **not** edit `.mstar/iterations/*.md` or historical plan/report markdown.
- `git show --name-only --format= 2a4e5577 -- .mstar/` returned exactly:
  - `.mstar/knowledge/specs/acp-client-tech-spec.md`
  - `.mstar/knowledge/specs/concurrency.md`
  - `.mstar/knowledge/specs/desktop-shell.md`
  - `.mstar/knowledge/specs/novel-writing/workflow-profile.md`
  - `.mstar/knowledge/specs/orchestration-engine.md`
  - `.mstar/plans/2026-06-27-v1.68-cli-relocation-and-product-surface-formalization.md`
  - `.mstar/status.json`
- Classification: the five `knowledge/specs/*.md` files and `status.json` match the assignment's allowed live edits. The P0 plan file was also updated only to tick implementation/verification checkboxes for this active plan; no past compass/plan/QC report markdown was edited. No scope-violation Warning from historical rewrites.

### `wire_contracts_changed: FALSE` integrity
- `git show --name-only --format= 2a4e5577 -- schemas/ packages/ crates/nexus-contracts/` returned **no output**.
- `git show 2a4e5577 -- schemas/ packages/ crates/nexus-contracts/` returned **no output**.
- `git diff --name-status 4606395e..2a4e5577 -- schemas/ packages/nexus-contracts/ crates/nexus-contracts/` returned **no output**.
- `bash tooling/check-schema-drift.sh` passed all checks, including Rust/TypeScript `LATEST_SCHEMA_VERSION` parity. `wire_contracts_changed: FALSE` is supported by evidence.

### Missed live `crates/nexus42` references
- Required sweep `git grep -n 'crates/nexus42' -- ':!.mstar' ':!*.lock'` returned **no output**.
- Broader sweep `git grep -n -E 'crates/nexus42|apps/nexus42|--manifest-path|externalBin|rust-analyzer|\.cargo/config|\.gitmodules' -- ':!.mstar' ':!*.lock'` found no missed old-path live reference; the remaining non-historical hits are either updated `apps/nexus42` references, Tauri `externalBin` binary-name references, or skill examples under `.agents/`.
- `.gitmodules` has only `.agents/skills/greptile` and `.agents/skills/react-flow`; no CLI path coupling. No `.cargo/config*` or rust-analyzer config files were found.

### CI / PR readiness
- `.github/workflows/ci.yml` has no `crates/nexus42` hardcoded path. The Rust tests matrix builds CLI by package name (`-p nexus42`, lines 251-253), and the Rust fmt/clippy job pins `FMT_NIGHTLY: "nightly-2026-06-26"` with `cargo +"${FMT_NIGHTLY}" fmt --all -- --check` (lines 96-126).
- `.github/workflows/desktop-build.yml` has no `crates/nexus42` hardcoded path, but its PR/push path filters omit `apps/nexus42/**` (lines 7-16 and 20-29). This is W-001.
- Verification run locally:
  - `cargo metadata --no-deps --format-version 1` succeeded.
  - `cargo test -p nexus42` passed: 762 unit tests + all integration test files passed; 3 expected ignored host-call smoke tests; doc-tests 1 passed / 1 ignored.
  - `cargo +nightly-2026-06-26 fmt --all --check` passed.
  - `cargo clippy --all -- -D warnings` passed.
  - `bash tooling/check-schema-drift.sh` passed.

## Source Trace
- Finding ID: W-001
  - Source Type: manual-reasoning + CI config review
  - Source Reference: `.github/workflows/desktop-build.yml:7-16`, `.github/workflows/desktop-build.yml:20-29`, `scripts/fetch-sidecar.sh:36-40`, `apps/desktop/src-tauri/tauri.conf.json:31-34`
  - Confidence: High
- Finding ID: S-001
  - Source Type: static-analysis
  - Source Reference: `git diff --check 4606395e..2a4e5577` -> `apps/AGENTS.md:13` and `apps/AGENTS.md:14` trailing whitespace
  - Confidence: High
- Evidence ID: E-001
  - Source Type: git-diff
  - Source Reference: `git show --stat 2a4e5577 | grep -E '\.mstar/(iterations|plans)/.*\.md'` -> no output
  - Confidence: High
- Evidence ID: E-002
  - Source Type: git-diff
  - Source Reference: `git show --name-only --format= 2a4e5577 -- schemas/ packages/ crates/nexus-contracts/` -> no output
  - Confidence: High
- Evidence ID: E-003
  - Source Type: static-analysis
  - Source Reference: `git grep -n 'crates/nexus42' -- ':!.mstar' ':!*.lock'` -> no output
  - Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes
