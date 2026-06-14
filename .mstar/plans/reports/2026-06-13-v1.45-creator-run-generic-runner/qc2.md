---
report_kind: qc-review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-13-v1.45-creator-run-generic-runner
secondary_plan_ids: [2026-06-13-v1.45-delete-bespoke-run-subcommands, 2026-06-13-v1.45-creator-bootstrap-and-works-migration]
verdict: Approve
generated_at: 2026-06-14T05:12:00Z
review_range: "merge-base: 76a9eb79; tip: HEAD (ad7b5565); equivalent: git diff 76a9eb79...HEAD"
working_branch: iteration/v1.45
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
---

# Code Review Report — QC #2 (Security / Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (CLI parsing surface, preset loader contract, input validation, error safety, state machine invariants, DEPRECATED deletion, daemon↔CLI wire contract)
- Report Timestamp: 2026-06-14T05:12:00Z

## Scope
- plan_id: 2026-06-13-v1.45-creator-run-generic-runner (primary); secondary: 2026-06-13-v1.45-delete-bespoke-run-subcommands, 2026-06-13-v1.45-creator-bootstrap-and-works-migration
- Review range / Diff basis: merge-base: 76a9eb79 (= origin/main V1.44); tip: HEAD (ad7b5565 on iteration/v1.45); equivalent: git diff 76a9eb79...HEAD
- Working branch (verified): iteration/v1.45
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 38 changed (4171 insertions, 2648 deletions)
- Commit range: 76a9eb79...ad7b5565 (B1 atomic merge commits + harness flip commits)
- Tools run:
  - `cargo +nightly fmt --all -- --check` (clean)
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo test -p nexus42 --lib` (665 passed)
  - `cargo test -p nexus42 --test command_surface_contract` (37 passed)
  - `cargo test -p nexus-orchestration --lib preset` (207 passed)
  - Manual diff + targeted file reads on CLI surface, contracts, orchestration validation, user preset loader, works atomic commands, legacy audit paths

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- (W-1, correctness, low impact) `work_id` (wrk_...) format is a documented convention (V1.41) but is accepted as opaque `String` / `Option<String>` in clap structs (`RunCommand.work_id`, all `WorksCommand` variants, `BootstrapArgs` indirect via resolution). No regex enforcement or prefix validation at CLI layer; daemon contract is expected to reject malformed IDs on `/works/{id}` and schedule paths. This is consistent with prior surface (no regression introduced by B1). Tests use only well-formed `wrk_*` values. Not a security vector (no path traversal or injection via work_id in new code), but worth noting for future typed wrapper if desired.
  - Source: `crates/nexus42/src/commands/creator/run.rs:42-43` (RunCommand), `works/mod.rs:42,73,90,...` (multiple), `resolve_work_id` at 183-204 (pool fallback query).
  - Evidence: No `unwrap`/`panic` on the value; passed verbatim to daemon HTTP paths. Daemon-side validation (existing) is the trust boundary.

- (W-2, security hygiene, low impact) `--idea` (in `BootstrapArgs`, the P2-migrated composite) is a free `String` with no explicit length bound or control-char sanitization in the new surface (unlike `--reason`/`--gate-reason` which have 512-char + control-char checks in both generic and legacy paths). The value is used for title truncation (60 chars) and passed as `initial_idea` in the Work creation payload. No `format!` or exec surface in the reviewed CLI code; daemon-side limits and prompt templating apply. Not a new regression (1:1 with V1.44 `run start --idea`), but the asymmetry with reason hardening is observable.
  - Source: `crates/nexus42/src/commands/creator/bootstrap.rs:27-29` (BootstrapArgs.idea), `run.rs:532-588` (legacy Start handler showing prior usage; generic path has no --idea).
  - Evidence: Reason sanitization present at `handle_run:82-94`, `handle_run_legacy:555-567`, stage advance 1627-1640. Idea path has none beyond title derivation.

### 🟢 Suggestion
- (S-1, maintainability) Legacy `handle_audit_chapter`, `AuditMode`, `resolve_audit_body_path`, `validate_body_path`, and the entire `LegacyRunCommand` enum + match arms remain under `#[allow(dead_code)]` for P1/P2 reference (as documented in module header). This is intentional per plan and compass. When P2 migration is complete and no more reference is needed, a follow-up can remove the dead code + the still-present embedded `novel-manuscript-audit*` preset dirs (which are intentionally kept for daemon execution of the split review/extract presets). Command-surface contract test still exercises the old "audit-chapter" token for compat surface (expected).
  - Source: `run.rs:13-14` (module doc), 300-472 (LegacyRunCommand + AuditMode), 1385-1595 (handle_audit_chapter + path validators), 2240 (legacy test), `command_surface_contract.rs:1081`.
  - Evidence: P1 plan explicitly states "loader references cleaned" — CLI dispatch variants are gone; embedded presets and dead handlers are the remaining artifacts.

- (S-2, consistency) `PresetCliArg` validation (`check_cli_args` in orchestration) runs at preset load time (embedded + user). CLI `parse_preset_cli_args` performs a second, independent structural parse (kebab lookup, type coercion, required/default application) before constructing `AddScheduleRequest`. This is correct per spec (CLI does not filter; daemon validates on schedule create), but the two surfaces duplicate some rules (name shape, required+default). A future shared validator could reduce drift risk.
  - Source: `crates/nexus-orchestration/src/preset/validation.rs:927-988` (check_cli_args), `run.rs:210-296` (parse_preset_cli_args), `contracts preset.rs:118-145` (PresetCliArg schema).

- (S-3, observability) `resolve_work_id` (generic path) and the equivalent logic in `works status` / `works inspire` etc. both query the same `/v1/local/works?limit=1&status=active` shape and take the first `work_id`. No algorithmic drift observed in this diff, but the two call sites are textually separate. A small shared helper would make future changes obviously consistent.
  - Source: `run.rs:183-204`, `works/mod.rs` (multiple `resolve_work_id` or equivalent client.get calls).

## Source Trace
- Finding ID: QC2-B1-001 (W-1)
- Source Type: manual-reasoning + grep on diff
- Source Reference: `git diff 76a9eb79...HEAD -- crates/nexus42/src/commands/creator/{run.rs,works/mod.rs}`; `resolve_work_id` implementation
- Confidence: High

- Finding ID: QC2-B1-002 (W-2)
- Source Type: manual-reasoning + targeted read
- Source Reference: `bootstrap.rs:27`, `run.rs:82-94` (reason checks), legacy Start path
- Confidence: High

- Finding ID: QC2-B1-003 (S-1)
- Source Type: git-diff + code inspection
- Source Reference: `run.rs:13-14,300-472,1385-1595`; P1 plan text on DEPRECATED deletion
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## CI Gates (re-run in review cwd)
All mandated gates executed on this checkout (HEAD ad7b5565 on iteration/v1.45) and passed cleanly:
- `cargo +nightly fmt --all -- --check`
- `cargo clippy --all -- -D warnings`
- `cargo test -p nexus42 --lib` (665 passed)
- `cargo test -p nexus42 --test command_surface_contract` (37 passed)
- `cargo test -p nexus-orchestration --lib preset` (207 passed)

No pre-existing or introduced CI failures in scope.

## Specific Security / Correctness Checklist (per assignment)
- CLI parsing surface (`RunCommand` struct + `trailing_var_arg`): clap correctly captures globals before preset trailing args; unknown preset_id rejected at resolve time with actionable message; unknown `--flag` for a preset rejected with explicit allowed list; no shell/exec surface. ✅
- `--idea` / `BootstrapArgs`: no injection vector in reviewed paths (plain payload field); length/control sanitization present only for `--reason` family (pre-existing asymmetry, not introduced here). ✅ (with W-2 noted)
- `works` subcommands (inspire/reopen/resume-chain/reconcile-chapters): `work_id` is opaque string; no path-like treatment; pool active resolution matches prior algorithm. ✅
- Preset loader / `PresetCliArg`: schema validation at load (name shape, no dups, required+default conflict); CLI parse happens before `AddScheduleRequest`; daemon validates on schedule create per spec. User preset dir safely skips `_`/`.` prefixes and uses validated loader. ✅
- DEPRECATED `novel-manuscript-audit` deletion: bespoke RunCommand variants and direct handlers removed from dispatch; legacy code dead under `#[allow(dead_code)]`; embedded preset files remain (intentional for daemon execution of split review/extract). Command surface test retains token for compat. No orphan references in active loader paths. ✅
- Hint strings (P1 T5): static clap derives; updates were in docs/plans, no user-controlled content interpolated into help output. ✅
- Error message safety: errors are `CliError::Config(...)` with bounded/reasonable detail; no internal paths or stack traces leaked in new paths. ✅
- Backward compat: hard delete — old `creator run audit-chapter ...` etc. now resolve as unknown preset (clear error). No silent fallback. ✅
- Daemon↔CLI contract (`AddScheduleRequest`): generic path constructs with `preset_id` (validated), `input` (from typed cli_args parse), `force_gates`+`reason`, `creator_id`. No legacy field leakage. ✅
- State machine / FL-E: generic runner dispatches FL-E presets to `stage_advance` with `force=false` (ordering enforced); `force_gates`/`reason` threaded; runtime lock serialization remains daemon concern (unchanged). ✅
- Red flags scanned: no `format!` with unsanitized user paths in scheduling hot paths; no `unwrap`/`panic` on user input in CLI handlers; no `unsafe`; no user closures to spawn; no SQL concat (client only); file IO for user presets is on fixed safe base dir + validated loader; pool active resolution consistent; cli_args required fields / defaults typed correctly; no stage-advance race introduced (uses existing gate + lock machinery). ✅

## Revalidation Notes
N/A — initial QC #2 wave for B1 atomic merge.

## Residual / Follow-up (non-blocking)
- The two low-impact Warnings (W-1 work_id convention, W-2 --idea asymmetry) are pre-existing patterns, not regressions from this change set. They may be addressed in a future hardening pass if typed wrappers or uniform input sanitization are desired.
- Legacy dead code + embedded audit presets will naturally be cleaned when P2 migration reference is no longer needed (plan already tracks this).

**Final verdict for security / correctness surface: Approve.**
