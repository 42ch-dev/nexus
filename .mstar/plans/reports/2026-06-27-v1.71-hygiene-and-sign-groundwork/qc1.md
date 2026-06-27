---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-27-v1.71-hygiene-and-sign-groundwork"
verdict: "Approve"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: k2p7
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-28T05:30:00Z

## Scope
- plan_id: `2026-06-27-v1.71-hygiene-and-sign-groundwork`
- Review range / Diff basis: `39493026..63e52fa3` (P1 topic branch `feature/v1.71-hygiene-and-sign-groundwork` vs. `iteration/v1.71` at P-1 lock)
- Working branch (verified): `iteration/v1.71`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 32 changed files, ~1,183 insertions, ~132 deletions
- Commit range (if not identical to Review range line, explain): `394930269d733855dc26e46ac6153b4a23020591..63e52fa322960392c95a4dab8227526be4743cbf`
- Tools run:
  - `cargo +nightly-2026-06-26 fmt --all --check`
  - `cargo clippy --all -- -D warnings`
  - `cargo test --all`
  - `pnpm --filter web typecheck`
  - `pnpm --filter web test`
  - `./scripts/served-ui-smoke.sh`

## Findings

### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion

- **S-001 — `tauri.conf.json` `signingIdentity: null` is misleading next to env-var signing**
  - **Scope:** `apps/desktop/src-tauri/tauri.conf.json` line 45
  - **Evidence:** Tauri docs state `APPLE_SIGNING_IDENTITY` overwrites `tauri.conf.json > bundle > macOS > signingIdentity`. The current config explicitly sets `signingIdentity: null`, which is functionally overridden by the env var but reads as "signing disabled".
  - **Fix:** Either omit `signingIdentity` from the config object, or add a comment explaining that the env var (`APPLE_SIGNING_IDENTITY`) takes precedence and that `null` is intentional. Update `apps/desktop/SIGNING.md` if the config choice changes.
  - **Risk if deferred:** Low — future maintainer may incorrectly believe signing is hard-disabled and add a duplicate config value that conflicts with the env var.

- **S-002 — `__internal daemon-run` references left in comments after spawn path changed**
  - **Scope:** `apps/nexus42/src/commands/daemon_run.rs` module doc; `crates/nexus-daemon-runtime/src/boot.rs` comment near `try_init`
  - **Evidence:** `start_daemon` now spawns `daemon-run` directly (commit `835e4762`), but `daemon_run.rs` still says "Hidden `__internal daemon-run` command" and `boot.rs` still references "the `__internal daemon-run` subprocess".
  - **Fix:** Update both comments to reflect that `daemon-run` is a hidden subcommand invoked directly by `nexus42 daemon start`, not via `__internal`.
  - **Risk if deferred:** Low — comment drift; no runtime impact.

- **S-003 — `ChapterPage` defaults `can_edit_outline` to `true` when the field is absent**
  - **Scope:** `apps/web/src/pages/chapter-page.tsx` line 70
  - **Evidence:** `const canEditOutline = chapter.data?.can_edit_outline ?? true;`. The backend always populates the field today, but if a mock, cached response, or future contract change omits it, the editor falls back to editable.
  - **Fix:** Consider `?? false` for defense-in-depth, or assert/validate the contract shape at the API boundary. If the current default is intentional (backward compatibility), add a comment.
  - **Risk if deferred:** Low — current backend always sets the field; this is a hardening suggestion.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S-001 | doc-rule + manual-reasoning | Tauri v2 docs (`APPLE_SIGNING_IDENTITY` overwrites config); `apps/desktop/src-tauri/tauri.conf.json` | High |
| S-002 | manual-reasoning | `git diff 39493026..63e52fa3 -- apps/nexus42/src/commands/daemon/mod.rs`; `apps/nexus42/src/commands/daemon_run.rs`; `crates/nexus-daemon-runtime/src/boot.rs` | High |
| S-003 | manual-reasoning | `git diff 39493026..63e52fa3 -- apps/web/src/pages/chapter-page.tsx` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

P1 delivers the intended Track B scope cleanly: all 13 planned low-severity residuals are either resolved or explicitly deferred with reason; the desktop signing infrastructure is correctly env-gated; the admission-gate UI note and served-UI smoke script are present and tested; `can_edit_outline` is now probed against the workspace path guard and respected in the chapter editor; the shared sort comparator removes duplicated `compare_*` closures; and clippy/format/test gates all pass. The three suggestions above are non-blocking doc/consistency/hardening nits.

## Verification evidence

- Rust formatting: `cargo +nightly-2026-06-26 fmt --all --check` — passed
- Rust lint: `cargo clippy --all -- -D warnings` — passed
- Rust tests: `cargo test --all` — 762 passed in `nexus42` plus all other workspace crates; 0 failed
- Web typecheck: `pnpm --filter web typecheck` — passed
- Web tests: `pnpm --filter web test` — 17 files, 139 tests passed
- Served-UI smoke: `./scripts/served-ui-smoke.sh` — passed (built web SPA, built release `nexus42`, started daemon on ephemeral port, health + root HTML checks green)
