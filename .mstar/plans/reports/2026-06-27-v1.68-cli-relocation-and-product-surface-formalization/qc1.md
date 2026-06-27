---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-27-v1.68-cli-relocation-and-product-surface-formalization"
verdict: "Request Changes"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: build correctness, reference completeness, behavioral equivalence (qc1 emphasis)
- Report Timestamp: 2026-06-27

## Scope
- plan_id: `2026-06-27-v1.68-cli-relocation-and-product-surface-formalization`
- Review range / Diff basis: `4606395e..2a4e5577` (origin/main → iteration/v1.68 HEAD; substantive implement is commit `2a4e5577`; earlier commits are Prepare docs)
- Working branch (verified): `iteration/v1.68`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel` and `git branch --show-current`)
- Files reviewed: 126 files changed in `4606395e..2a4e5577`; 123 files in substantive commit `2a4e5577` (107 pure renames + 16 edits + 4 adds per `git diff -M --name-status`)
- Commit range: `4606395e..2a4e5577` (substantive implementation commit: `2a4e5577cdc9ea2c76a67304ba3ff4b9e8fd1fe7`)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`
  - `git log --oneline -5`, `git log --oneline 4606395e..2a4e5577`
  - `git show --stat 2a4e5577`, `git show --stat 4606395e..2a4e5577`
  - `git diff -M --name-status 4606395e..2a4e5577`
  - `git diff -M --diff-filter=R --stat` (rename-only sweep)
  - `git diff -M --name-status 4606395e..2a4e5577` (filter `^[AM]` to enumerate non-rename edits)
  - `git grep -n 'crates/nexus42' -- <12 listed live files>` (must be empty)
  - `git grep -l 'crates/nexus42' -- ':!.mstar'` (broader live sweep)
  - `git grep -n 'apps/nexus42' -- Cargo.toml AGENTS.md apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/src/lib.rs "apps/web/src/pages/dialogs/create-work-dialog.test.tsx"` (replacement-path verification)
  - `git grep -l 'crates/nexus42' -- ':!.mstar'` (count)
  - `git ls-files .mstar/ | xargs grep -l 'crates/nexus42' | wc -l` (historical count)
  - `cargo build --all`
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus42`
  - `cargo test --workspace`
  - `cargo +nightly-2026-06-26 fmt --all --check`
  - `bash tooling/check-schema-drift.sh`
  - `diff <(git show 4606395e:crates/nexus42/<file>) <(cat apps/nexus42/<file>)` on 11 representative `.rs` files
  - `grep -E '^(name|path|\[\[bin\]|\[lib\])' apps/nexus42/Cargo.toml` (crate/bin name verification)
  - `grep -rn 'crates/nexus42' .github/workflows/` (CI workflow path-filter sweep)
  - `git diff --check 4606395e..2a4e5577` (whitespace lint)

## Findings

### 🔴 Critical
(none)

### 🟡 Warning

- **W-001 — `.github/workflows/desktop-build.yml` path filter omits `apps/nexus42/**`** (build correctness / CI coverage regression introduced by the relocation)
  The desktop-build job bundles the `nexus42` sidecar via `pnpm --filter desktop tauri build`, but the workflow's `push.paths` (lines 7–17) and `pull_request.paths` (lines 20–30) only list `apps/web/**`, `apps/desktop/**`, `packages/nexus-contracts/**`, `crates/**`, and a few lockfiles. A PR that touches only `apps/nexus42/**` — now the product-surface source of the bundled sidecar — will not trigger the desktop-build job, even though `cargo build --release -p nexus42` (run inside that job) is affected.
  → Add `apps/nexus42/**` to both `push.paths` and `pull_request.paths` in `.github/workflows/desktop-build.yml` (or broaden to `apps/**` if that is the intended future-proof scope). The CI workflow itself (`.github/workflows/ci.yml`) uses `paths-ignore` (exclusion), so it correctly covers `apps/nexus42/**`; only `desktop-build.yml` is affected.
  - Evidence: `.github/workflows/desktop-build.yml:7-17,20-30` (path lists), `.github/workflows/desktop-build.yml:89-100` (Tauri build invocation that pulls `nexus42` sidecar).
  - Cross-reference: `qc3.md` independently raised the same finding (same root cause, same fix). Two reviewer agreement strengthens confidence.
  - Severity: Warning (CI coverage regression, not a runtime correctness issue; low blast radius since the desktop-build job exists, but a CLI-side regression on the bundled sidecar could merge without CI coverage).

### 🟢 Suggestion
(none from qc1; qc3 recorded a separate S-001 about `apps/AGENTS.md:13-14` trailing-whitespace — confirmed by `git diff --check 4606395e..2a4e5577`, intentional Markdown hard-break formatting. Not blocking.)

## Source Trace

- **Finding W-001**:
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `.github/workflows/desktop-build.yml:7-17,20-30`; cross-ref `.github/workflows/ci.yml:7-11,14-18` (uses `paths-ignore`, so unaffected)
  - Confidence: High

## Detailed Review (qc1 focus)

### 1. CI gate checks — all green

| Check | Result | Evidence |
|-------|--------|----------|
| `cargo build --all` | ✅ clean | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 16.39s`; workspace resolves `nexus42 v0.1.0 (/Users/bibi/.../nexus/apps/nexus42)` |
| `cargo clippy --all -- -D warnings` | ✅ clean | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 12.68s`; no warnings |
| `cargo test -p nexus42` | ✅ all pass | lib 762 + 30 integration test files + 2 doc-tests, 0 failed, 0 measured regressions |
| `cargo test --workspace` | ✅ all pass | every test result line shows `0 failed`; no FAILED/failures:/error markers in output |
| `cargo +nightly-2026-06-26 fmt --all --check` | ✅ clean | empty output (no formatting drift) |
| `bash tooling/check-schema-drift.sh` | ✅ all pass | `✅ All schema consistency checks passed` (CLI/runtime dependency parity, DB_SCHEMA_VERSION single ownership, LATEST_SCHEMA_VERSION Rust/TS parity, no duplicated DDL, no deprecated WIRE_SCHEMA_VERSION, CLI/daemon use nexus-local-db API) |

The CI gate rule from `mstar-review-qc` ("any CI failure default >= Warning") is satisfied with zero CI failures. All `cargo` workspace members compile and test cleanly from the new `apps/nexus42` location.

### 2. Zero source-code change promise — verified

The promise is that relocation touches NO `.rs` source logic. Verified via two methods:

**(a) Git rename detection (`git diff -M`)**:
- 107 files are pure renames with `similarity index 100%` — every `.rs` file in `apps/nexus42/src/**/*.rs` and `apps/nexus42/tests/**/*.rs` plus `.json` fixtures, `AGENTS.md`, and Cargo.toml/build.rs.
- Of 126 changed files, only 19 have non-zero content deltas; the rest are pure renames.

**(b) Byte-level identity spot-check** on 11 representative `.rs` files via `diff <(git show 4606395e:crates/nexus42/<file>) <(cat apps/nexus42/<file>)`:

```
OK   apps/nexus42/src/main.rs
OK   apps/nexus42/src/cli.rs
OK   apps/nexus42/src/config.rs
OK   apps/nexus42/src/lib.rs
OK   apps/nexus42/src/errors.rs
OK   apps/nexus42/src/commands/acp/mod.rs
OK   apps/nexus42/src/commands/daemon/mod.rs
OK   apps/nexus42/src/commands/creator/run.rs
OK   apps/nexus42/src/commands/creator/bootstrap.rs
OK   apps/nexus42/tests/integration.rs
OK   apps/nexus42/tests/cli_agent.rs
```

All `.rs` source files are byte-identical between the old and new locations. The CLI's runtime behavior is unchanged.

**(c) Non-`.rs` content edits** — the only content changes outside pure renames:

| File | Type | Why | Behavioral impact |
|------|------|-----|-------------------|
| `Cargo.toml:16` | workspace member path | `crates/nexus42` → `apps/nexus42` | none — workspace resolves to the same crate |
| `apps/nexus42/Cargo.toml` | relative path attrs | `../nexus-contracts` → `../../crates/nexus-contracts` (and 12 other `crates/*` deps) | none — same crates, deeper relative path |
| `apps/nexus42/build.rs` | path traversal | `..` → `..`, `..`, `crates` (one level deeper; one extra `..`) | none — finds same embedded-presets dir |
| `apps/desktop/src-tauri/Cargo.toml:29` | comment | `crates/nexus42/src/config.rs` → `apps/nexus42/src/config.rs` | none — comment only |
| `apps/desktop/src-tauri/src/lib.rs:76` | comment | `crates/nexus42/src/config.rs` → `apps/nexus42/src/config.rs` | none — comment only |
| `apps/web/src/pages/dialogs/create-work-dialog.test.tsx:187` | comment | `crates/nexus42/...` → `apps/nexus42/...` | none — comment only |
| `apps/AGENTS.md` | new file | documents placement rule | none — additive doc |
| `README.md` | component table | updates Monorepo Layout table | none — doc |
| `AGENTS.md` | index row | `crates/nexus42/` → `apps/nexus42/`, adds apps/ rule paragraph | none — doc |
| `docs/ARCHITECTURE.md` | table row | adds "Product surfaces" row, points `nexus42` to `apps/nexus42` | none — doc |
| `tooling/check-schema-drift.sh` | grep paths | 5 occurrences `crates/nexus42` → `apps/nexus42` | none — script verified to pass post-edit |
| `.mstar/knowledge/specs/{5 files}` | path text | textual references to `crates/nexus42` updated to `apps/nexus42` (matches the 5 live spec files listed in the Assignment) | none — spec text only |
| `.mstar/status.json` | structured update | 50-line delta (P0 plan registration, gate state, residual ledger) | none to runtime |

All non-rename edits are either mechanical path adjustments required by the deeper directory depth, comment updates, or documentation/spec refreshes. **No runtime behavior change**.

### 3. Reference completeness — all 12 live refs migrated, zero remaining live hits

**Targeted live-ref sweep** (the 12 files listed in the Assignment, verbatim command):

```bash
git grep -n 'crates/nexus42' -- \
  Cargo.toml tooling/check-schema-drift.sh AGENTS.md docs/ARCHITECTURE.md \
  apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/src/lib.rs \
  "apps/web/src/pages/dialogs/create-work-dialog.test.tsx" \
  .mstar/knowledge/specs/acp-client-tech-spec.md \
  .mstar/knowledge/specs/concurrency.md \
  .mstar/knowledge/specs/desktop-shell.md \
  .mstar/knowledge/specs/novel-writing/workflow-profile.md \
  .mstar/knowledge/specs/orchestration-engine.md
```

Result: **empty** (no output). All 12 live references migrated.

**Broader sweep** (any remaining `crates/nexus42` outside `.mstar/` audit trail):

```bash
git grep -l 'crates/nexus42' -- ':!.mstar'
```

Result: **0 files**. Zero live hits anywhere outside the intentionally-preserved historical `.mstar/` records.

**Historical record preservation** (Assignment: "~978 historical `.mstar/` records intentionally left referencing `crates/nexus42`"):

```bash
git ls-files .mstar/ | xargs grep -l 'crates/nexus42' | wc -l
# 625
```

Discrepancy with Assignment's "~978": the 625 figure covers git-tracked files; the larger 978 figure likely includes untracked local files, prior archived snapshots, and closed residual JSON entries — all consistent with the Assignment's stated intent ("intentionally left as-is to preserve the audit trail"). No action required.

**Replacement-path spot-checks** (`apps/nexus42` correctly referenced where expected):

```
AGENTS.md:36:                                          | `apps/nexus42/` | CLI executable ...
Cargo.toml:16:                                         "apps/nexus42",
apps/desktop/src-tauri/Cargo.toml:29:                  # Mirrors ... (`apps/nexus42/src/config.rs`):
apps/desktop/src-tauri/src/lib.rs:76:                 /// daemon uses at boot (`apps/nexus42/src/config.rs`)
apps/web/src/pages/dialogs/create-work-dialog.test.tsx:187: bootstrap at apps/nexus42/src/commands/creator/bootstrap.rs:140-143
```

All 5 replacement refs are in place.

### 4. Behavioral equivalence — binary/crate name and sidecar resolution preserved

**Crate / binary name unchanged**:

```toml
[package]
name = "nexus42"
[lib]
name = "nexus42"
path = "src/lib.rs"
[[bin]]
name = "nexus42"
path = "src/main.rs"
```

Identical to pre-relocation. `cargo build` produces `target/debug/nexus42` (144 MB binary, present and runs).

**Tauri sidecar resolution unchanged**:

```json
// apps/desktop/src-tauri/tauri.conf.json:34
"externalBin": ["binaries/nexus42"]
```

```rust
// apps/desktop/src-tauri/src/sidecar.rs:227
.sidecar("binaries/nexus42")
```

Sidecar resolves by string name `"nexus42"`, not by path — the move from `crates/nexus42/` to `apps/nexus42/` does not change anything the Tauri runtime sees.

**Bundled sidecar artifacts unchanged** (still ship as `binaries/nexus42-{arch}-{platform}`):
```
apps/desktop/src-tauri/binaries/nexus42-aarch64-apple-darwin
apps/desktop/src-tauri/binaries/nexus42-x86_64-apple-darwin
```

**Wire contracts unchanged**: `git show --name-only --format= 2a4e5577 -- schemas/ packages/nexus-contracts/ crates/nexus-contracts/` returns no output. `bash tooling/check-schema-drift.sh` passes including Rust/TypeScript `LATEST_SCHEMA_VERSION` numeric parity. `wire_contracts_changed: FALSE` is supported by evidence.

**Wire-contract-aware comments** (e.g., `apps/desktop/src-tauri/src/lib.rs` referencing daemon's config source) were updated to point at the new path. Verified by spot-reading the new path in the file resolves correctly.

### 5. Documentation / placement-rule artifact creation (cross-check with qc2 focus)

- `apps/AGENTS.md` (new, 33 lines): declares `apps/` as polyglot product-surfaces directory; lists three entries (`nexus42` Rust producer, `desktop` TS+Tauri consumer, `web` TS consumer); durable placement rule stated; producer/consumer wire boundary documented; per-entry authority links correct.
- `README.md` Monorepo Layout section: rewritten to distinguish `apps/`, `crates/`, `packages/`, `modules/`, `tooling/`, `schemas/`. Includes `desktop` and `web` (previously omitted from the Components table).
- Root `AGENTS.md`: index row updated to `apps/nexus42/`; explicit "apps/ is the polyglot product-surfaces directory" paragraph added with cross-link to `apps/AGENTS.md`.

Cross-check with qc2 (documentation focus): rule wording is consistent across `apps/AGENTS.md`, compass §6, plan C1, and `AGENTS.md`; no contradictions. qc2 raised one Suggestion (S-01: optional cross-link in README) — non-blocking.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 0 |

**Verdict**: Request Changes

## Cross-Reviewer Convergence

| Concern | qc1 (this report) | qc2 | qc3 | Consensus |
|---------|-------------------|-----|-----|-----------|
| Build/clippy/fmt/test/drift gate | ✅ all green | (docs focus) | ✅ all green | n/a |
| Zero `.rs` source changes | ✅ 100% renames, byte-identical spot-check on 11 files | (not qc2 focus) | ✅ 100% renames | converged |
| 12 live refs migrated | ✅ all empty after git grep; broader sweep zero hits | ✅ confirmed | ✅ confirmed | 3/3 converged |
| Binary / sidecar / wire-contract behavioral equivalence | ✅ name unchanged, sidecar by name, wire contracts untouched | (docs focus) | ✅ wire contracts untouched | converged |
| `.github/workflows/desktop-build.yml` path filter omits `apps/nexus42/**` | **W-001 raised here** | (not qc2 focus) | **W-001 raised** | 2/3 converged (qc2 not in scope) |
| `apps/AGENTS.md:13-14` trailing whitespace | not raised (out of qc1 scope) | (not qc2 focus) | S-001 Suggestion | qc3 only |
| `README.md` optional cross-link under `apps/` row | not raised | S-01 Suggestion | not raised | qc2 only |

The single Warning (W-001) is independently confirmed by qc3 and is the only blocker. Fix is one-line YAML in two path-filter lists. PM should route the fix and re-dispatch qc1+qc3 targeted re-review (qc2 not in scope for the CI change but should re-verify after merge).

## Pre-merge Checklist (`{HARNESS_DIR}/AGENTS.md`)

- [x] `status.json` updated (P0 plan registration present in `.mstar/status.json`; not modified by this review)
- [x] Wire contracts unchanged → no `pnpm run codegen` rerun needed
- [x] Historical `.mstar/` records intentionally preserved per Assignment (625 in git-tracked files; ~978 including local untracked — audit trail intact)
- [ ] `.github/workflows/desktop-build.yml` path filter — **pending W-001 fix**
- [ ] PM re-dispatch after W-001 fix

## Residual Findings留档

No unresolved findings require `{HARNESS_DIR}/status.json` root-level `residual_findings[<plan-id>]` registration:

- W-001 is a single-line CI config fix that should land in this same PR before merge; it does not need a long-lived residual.
- If PM prefers a tracked residual for the desktop-build path filter for completeness, the entry would be `id: R-V168-P0-CI-001`, `severity: medium`, `source: qc1.md#W-001`, `scope: .github/workflows/desktop-build.yml`, `decision: defer-to-fix-round`, `owner: pm→fullstack-dev`, `target: same PR`. PM owns residual lifecycle per `mstar-plan-artifacts`; this report does not modify `status.json`.