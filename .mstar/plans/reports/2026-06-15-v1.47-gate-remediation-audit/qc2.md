---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-15-v1.47-gate-remediation-audit"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-15

## Scope
- plan_id: `2026-06-15-v1.47-gate-remediation-audit`
- Review range / Diff basis: `merge-base: 6acb5ae680c5c7f11050c82df6f0e4156c33f78e + tip: HEAD`
- Working branch (verified): `feature/v1.47-gate-remediation-audit`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p1-remediation`
- Files reviewed: 6 (preset_gates.rs + 4 CLI/daemon files + errors.rs; 283 insertions, 33 deletions)
- Commit range: single commit `9a3ac5a9` (fix(v1.47-P1): gate remediation cites executable commands, not raw .mstar/ paths)
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git diff --stat 6acb5ae680c5c7f11050c82df6f0e4156c33f78e..HEAD`, full targeted reads of changed files + remediation helpers, `cargo +nightly fmt --all -- --check` (clean), `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings` (clean), `cargo test -p nexus42 -- works` (all pass), `cargo test -p nexus-orchestration --lib -- gate` (all pass, including new intake test), `./target/debug/nexus42 --help` (creator surface present), grep for remediation strings and `creator (bootstrap|run)` patterns.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

#### S-1: New remediation tests remain string-contains snapshots (maintainability)
**Triggering condition**: The V1.47 P1 tests added/updated in `preset_gates.rs` (e.g., `intake_status_remediation_cites_executable_bootstrap`, `no_gate_remediation_embeds_raw_dotmstar_paths`, plus the four pre-existing remediation_* tests) and the routing-hint regression test in `works/mod.rs` use `contains("creator bootstrap")`, `!contains(".mstar/")`, `!contains("--preset creative-brief-intake")`, `contains("→ write")`, etc. These are still literal string checks against the remediation text produced by the three helper functions (`work_field_remediation`, `filesystem_remediation`, `previous_preset_remediation`) and the error paths.

**Impact**: Same pattern noted in the prior hygiene plan's qc2 (S-1/S-2). If any remediation literal or the intake wording changes, the corresponding test must be updated. Low risk — the tests are co-located with the production helpers, small, and the strings are compile-time data with no runtime generation. No security or correctness impact; the assertions correctly validate AC1–AC4. Not blocking.

**Suggested fix**: None required for this plan. Acceptable for a small set of user-facing advisory strings whose sole purpose is to point at stable commands and spec sections. A future hygiene pass could centralize the remediation table, but that is out of scope.

**Source Reference**: `crates/nexus-orchestration/src/preset_gates.rs:1104–1265` (new V1.47 intake + blanket .mstar guard tests) and `crates/nexus42/src/commands/creator/works/mod.rs:1761–1788` (v146 routing hint guard); production remediation fns at lines 351–411.

**Confidence**: High

#### S-2: Minor duplication of "see the X spec" phrasing across CLI help, errors, and gate remediation (cosmetic)
**Triggering condition**: The sweep touched `creator/mod.rs` (Bootstrap/Works doc comments), `run.rs` (produce-stage error), `works/mod.rs` (stale findings + completion banners), `errors.rs` (daemon-not-reachable), `schedules.rs` (completion guard + two error paths), and the three gate helpers. All now use the short form ("See the creator-run-preset-entry spec" / "See novel-author-experience §3") instead of raw `.mstar/...` paths.

**Impact**: Slight increase in the number of sites that must be kept in sync if the canonical short names ever change. Negligible security/correctness surface — these are purely advisory strings with no interpolation of untrusted input. The prior qc2 for the hygiene plan already flagged the snapshot nature; this is the same family of strings now applied to the gate remediation path.

**Suggested fix**: None for P1. If a later pass extracts a small const table or a helper for "see spec X", the sites can be unified. Not blocking.

**Source Reference**: All six changed files in `9a3ac5a9`; remediation construction sites in `preset_gates.rs:351–411` and schedules error paths.

**Confidence**: High

## Source Trace
- **Finding ID: S-1**
  - Source Type: manual code review + test-vs-production cross-check (security/correctness lens)
  - Source Reference: `git show 9a3ac5a9 -- crates/nexus-orchestration/src/preset_gates.rs` (new intake test 1104–1154 and blanket guard 1156–1265); runtime reads of remediation helpers (351–411) and the routing-hint test (1761–1788)
  - Confidence: High

- **Finding ID: S-2**
  - Source Type: manual code review + string hygiene audit
  - Source Reference: `git diff 6acb5ae680c5c7f11050c82df6f0e4156c33f78e..HEAD --stat` + targeted reads of the six files; grep for remediation patterns
  - Confidence: High

- Mechanical AC verification (plan §4 + assignment):
  - AC1 (R-V146P1-QC3-S1 repro fixed): new dedicated test `intake_status_remediation_cites_executable_bootstrap` (and the blanket `no_gate_remediation_embeds_raw_dotmstar_paths`) + explicit comment block in `work_field_remediation` explaining why `--preset creative-brief-intake` was wrong. Test passes (`cargo test -p nexus-orchestration --lib -- intake_status_remediation`).
  - AC2 (no raw .mstar/ paths): every remediation helper now uses short spec names; all new/updated tests assert `!contains(".mstar/")`; the blanket test exercises every branch (work_field, filesystem, previous_preset, and forced intake_status). Also verified in CLI help texts, errors, and schedules.rs.
  - AC3 (executable commands for intake/scaffold): `intake_status` now says "Intake runs automatically during `nexus42 creator bootstrap`"; scaffold paths continue to cite `creator bootstrap --init-preset novel-project-init`; previous_preset branches cite the same. `nexus42 --help` surface confirms `creator` (and code confirms `bootstrap` / `run` subcommands exist).
  - AC4 (no regression on V1.46 routing_hint): new regression test `v146_routing_hint_behavior_unchanged` asserts each per-finding `routing_hint` appears verbatim and that no blanket `novel-chapter-review` footer is injected. Test passes as part of `cargo test -p nexus42 -- works`.

- CI / lint gates (all clean, no scope-attributable failures):
  - `cargo +nightly fmt --all -- --check` → clean (no output, exit 0)
  - `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings` → clean
  - `cargo test -p nexus42 -- works` → 7 + 6 + 1 + 0 (all pass)
  - `cargo test -p nexus-orchestration --lib -- gate` → 80 relevant tests pass (including the new intake remediation test)

- Diff scope: exactly one commit on the assigned branch; review range reproduces cleanly; only advisory string content + the two new regression tests + comments changed. No new command-construction logic, no user-field interpolation into remediation strings, no shell metacharacters introduced.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

All four acceptance criteria (AC1–AC4) are satisfied with tests or explicit guards. The single commit is a narrow, well-scoped hygiene + correctness fix: it replaces the incorrect intake remediation command (the R-V146P1-QC3-S1 repro case), removes all remaining raw `.mstar/knowledge/specs/...` citations from user-facing remediation and error strings (AC2), ensures gate failures for intake/scaffold now cite real executable `creator bootstrap` forms (AC3), and adds an explicit regression test that V1.46 per-finding `routing_hint` behavior is untouched (AC4).

From the security/correctness perspective (reviewer #2 focus):
- Remediation text is produced exclusively by compile-time match arms in three small helper functions (and a handful of error strings). There is **no** interpolation of untrusted user input (work title, chapter number, etc.) into the suggested commands or spec references.
- No shell injection surface: the strings contain only literal command names, flags, and short spec titles. No `&&`, `;`, backticks, `$()`, or other metacharacters appear.
- The `intake_status` fix is semantically correct per the comments and the `creator-run-preset-entry.md` §3.2 reference: intake is a side-effect of `creator bootstrap`, not a separate `--preset` override. The old suggestion would have created a new Work instead of advancing the existing one.
- The schedules.rs changes (completion guard + two error paths) are pure string updates; the underlying SQL and control flow are unchanged.
- The new `routing_hint` test directly protects the AC4 guarantee and the Grill #7 design (per-finding hints, no blanket footer).
- All touched crates pass clippy (`-D warnings`) and the relevant test subsets. Nightly fmt is clean.

The two Suggestions are low-impact maintainability notes about the pre-existing snapshot-test pattern for advisory strings. They do not affect the ability of users (or automation) to follow the remediation instructions, nor do they introduce security or correctness risk.

Per `mstar-review-qc` gate rule (Critical = 0 and Warning = 0 ⇒ Approve), and because all explicit acceptance criteria in the plan and assignment are met with evidence, this seat returns **Approve**.

## Revalidation
N/A — initial wave for this plan. No prior qc2 report exists for `2026-06-15-v1.47-gate-remediation-audit`. The target residuals (R-V146P1-QC3-S1, R-V146P1-QC3-S4) originated in the V1.46 P1 hygiene work and are explicitly closed by the changes under review.

## Evidence (verification-before-completion)
- Assignment fields verified on-disk: `git rev-parse --show-toplevel`, `git branch --show-current`, `git diff --stat` (exact range), single commit `9a3ac5a9`.
- All required lint + test commands executed and clean (see Source Trace).
- Full diff + targeted file reads of every changed remediation site and every new test.
- `nexus42 --help` executed to confirm creator command surface.
- Grep across the tree for `remediation` + `creator (bootstrap|run)` to cross-check command naming.
- Report will be committed (only this path) before emitting Completion Report v2.
