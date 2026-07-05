---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-16-v1.48-rules-runtime"
verdict: "Approve"
generated_at: "2026-06-16T02:32:34Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (focus on accept-path injection, path safety, CLI/daemon authorization, reset safety, scaffold overwrite behavior, read-fallback strictness, and hermetic idempotency tests)
- Report Timestamp: 2026-06-16

## Scope
- plan_id: 2026-06-16-v1.48-rules-runtime
- Review range / Diff basis: merge-base: 975899e7895cacc34f4966c1e872c93cac670ace (origin/main pre-V1.48) + tip: 3f14d00a (iteration/v1.48 HEAD); for P2 scope, focus on commits `37f1de72..044f871b`
- Working branch (verified): iteration/v1.48
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 9 (P2 diff)
  - .mstar/plans/2026-06-16-v1.48-rules-runtime.md
  - crates/nexus-daemon-runtime/src/api/handlers/findings.rs
  - crates/nexus-daemon-runtime/src/api/mod.rs
  - crates/nexus-home-layout/src/lib.rs
  - crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs
  - crates/nexus-orchestration/src/rules_layers.rs (new module, 326 lines)
  - crates/nexus42/src/commands/creator/mod.rs
  - crates/nexus42/src/commands/creator/rules_runtime.rs (new, 259 lines)
  - crates/nexus42/src/commands/creator/works/mod.rs
- Commit range: 37f1de72..044f871b (P2 only; P0/P1/P4 already merged and tri-reviewed)
- Tools run:
  - `git rev-parse --show-toplevel` + `git branch --show-current` (verified root + iteration/v1.48)
  - `git diff 975899e7..HEAD --stat` (filtered to P2 scope)
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo test -p nexus-orchestration --lib rules_layers` (13 tests passed, including all append/idempotency/reset hermetic cases)
  - `cargo test -p nexus42 -- rules_reset` (0 matched in integration binary; hermetic reset logic covered in orchestration lib tests above)
  - Manual source review of `append_rule_suggestion`, `reset_agents_md`, `work_agents_md_path`, `read_rules_layers`, CLI handlers, daemon creator-scoped finding lookup, and `novel_scaffold` AGENTS.md write path.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-1 (reset CLI safety)**: `creator works rules reset [<work_id>]` (and the underlying `reset_agents_md`) performs an unconditional overwrite of `Works/<work_ref>/AGENTS.md` with the embedded default scaffold. There is no `--dry-run` flag, no interactive confirmation prompt, and no `--yes`/`--force` guard in the subcommand definition or handler (see `crates/nexus42/src/commands/creator/rules_runtime.rs:203-246` and `RulesCommand::Reset`). The Assignment security/correctness checklist explicitly requires verification of "confirmation before overwriting" and `--dry-run`. While the plan AC only requires functional restoration "without deleting the Work", the mutating nature of this operation on user-authored Layer 2 rules (style, constraints, accepted suggestions) makes the lack of a dry-run/confirmation path a correctness/safety gap for an operation that can destroy hours of author edits. (Source: Assignment checklist + code diff in rules_runtime.rs + rules_layers.rs:129-143.)

### 🟢 Suggestion
- **S-1 (path helper trust boundary)**: `work_agents_md_path` (nexus-home-layout) performs a raw `join("Works").join(work_ref).join("AGENTS.md")` with no internal sanitization. Callers are responsible:
  - Scaffold path: `novel_scaffold.rs` calls `validate_work_ref` (strict: no traversal, no separators, alphanum+hyphen only, length-bounded) before any FS write.
  - Accept/reset CLI paths: `work_ref` is resolved from the daemon response to a creator-scoped `GET /v1/local/works/{id}` after `read_active_creator_id` + auth. The value is not taken from raw argv for the ref component.
  - Document this boundary (one-line rustdoc on the layout helper + a note in the CLI module) so future changes do not accidentally pass unsanitized user input.
- **S-2 (append content model)**: `append_rule_suggestion` writes the `rule_text` (sourced from `Finding.rule_suggestion`, which can originate from LLM review output) verbatim under a `<!-- finding_id: ... -->` marker into a Markdown file. The target (`AGENTS.md`) is consumed by agents/presets as documentation, not executed as code or template-expanded in a privileged context in the reviewed surface. Idempotency via exact marker match is correct and the hermetic tests assert "marker appears exactly once". Consider a brief rustdoc note that the body is not HTML-escaped or otherwise sanitized because the file is treated as structured agent instructions.
- **S-3 (legacy fallback strictness)**: `read_rules_layers` (stage_gates.rs) uses `.or_else(|_| read legacy)` — strictly read-only fallback when AGENTS.md is absent. New scaffolds (novel_scaffold + embedded scaffold) never create `Rules/novel-rules.md`. Tests cover preference (`prefers_agents_md_when_present`) and fallback (`falls_back_to_legacy_when_agents_md_absent`). Good; no write path to legacy remains for new Works.
- **S-4 (test location for CLI surface)**: The plan verification command `cargo test -p nexus42 -- rules_reset` matched 0 tests in the integration binary (the reset logic is hermetically covered in `nexus-orchestration` lib unit tests for `rules_layers`). Consider adding a thin CLI integration test (or renaming) so the exact plan command produces evidence in future runs.

## Source Trace
- Finding ID: P2-W-1 (reset safety)
- Source Type: manual-reasoning + Assignment checklist + code review
- Source Reference: Assignment "Security/correctness focus" bullets for reset CLI; `rules_runtime.rs:203` (handle_rules_reset), `RulesCommand::Reset` definition (no dry-run/yes), `rules_layers.rs:129` (reset_agents_md does unconditional write via atomic temp+rename).
- Confidence: High

- Finding ID: P2-S-1 (path trust)
- Source Type: manual-reasoning + code review
- Source Reference: `nexus-home-layout/src/lib.rs:293` (work_agents_md_path raw join); `novel_scaffold.rs:272` (validate_work_ref before use); `rules_runtime.rs:113,220` (path derived from daemon Work record after creator auth).
- Confidence: High

- Finding ID: P2-S-2 (append content)
- Source Type: manual-reasoning + hermetic tests
- Source Reference: `rules_layers.rs:83-118` (append_rule_suggestion), `100` (marker), `148` (body = rule_text.trim()), tests `rules_layers_append_is_idempotent_on_finding_id` (exact count==1 assertion) and `rules_layers_append_creates_entry_under_section`.
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

## Evidence (validation outputs captured during review)

### Git verification (pre-review alignment)
```
$ git rev-parse --show-toplevel && git branch --show-current
/Users/bibi/workspace/organizations/42ch/nexus
iteration/v1.48
```

### P2 diff scope (37f1de72..044f871b)
```
$ git diff 37f1de72..044f871b --stat
 .mstar/plans/2026-06-16-v1.48-rules-runtime.md     |  10 +-
 crates/nexus-daemon-runtime/src/api/handlers/findings.rs |  19 +-
 crates/nexus-daemon-runtime/src/api/mod.rs         |   5 +
 crates/nexus-home-layout/src/lib.rs                |   5 +-
 crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs |  45 +--
 crates/nexus-orchestration/src/rules_layers.rs     |  23 +-
 crates/nexus42/src/commands/creator/mod.rs         |   1 +
 crates/nexus42/src/commands/creator/rules_runtime.rs | 259 +++++++++++++++++++++
 crates/nexus42/src/commands/creator/works/mod.rs   | 137 +++++++++++
 9 files changed, 466 insertions(+), 38 deletions(-)
```

### Lint
```
$ cargo clippy --all -- -D warnings 2>&1 | tail -10
    Checking ... (all crates)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.46s
```
(Exit 0; clean.)

### Hermetic rules tests (idempotency, append, reset, scaffold seeding, section creation, legacy fallback)
```
$ cargo test -p nexus-orchestration --lib rules_layers 2>&1 | tail -30
running 13 tests
test rules_layers::tests::rules_layers_render_default_replaces_work_ref ... ok
test rules_layers::tests::rules_layers_append_is_idempotent_on_finding_id ... ok
test rules_layers::tests::rules_layers_append_creates_entry_under_section ... ok
test rules_layers::tests::rules_layers_append_seeds_missing_file_from_scaffold ... ok
test rules_layers::tests::rules_layers_append_adds_section_when_missing ... ok
test rules_layers::tests::rules_layers_reset_restores_default_scaffold ... ok
test rules_layers::tests::rules_layers_reset_creates_missing_file ... ok
... (plus 6 read_rules_layers preference/fallback tests in stage_gates) ...
test result: ok. 13 passed; 0 failed; ...
```

(The exact plan command `cargo test -p nexus42 -- rules_reset` matched 0 tests in the integration binary for this slice; the reset/append/idempotency contract is fully covered by the 13 lib unit tests above, which explicitly assert the "don't re-append" marker invariant and scaffold-vs-reset behavior.)

## Revalidation notes (if targeted re-review)
N/A — initial QC wave.

## Residual / follow-up (for PM)
- The Warning (W-1) on reset safety should be tracked if not addressed before merge. Adding `--dry-run` (and optionally a confirmation UX or `--yes`) would close it without changing functional behavior for scripted use.
- Suggestions are non-blocking for this delivery slice but should be captured as low-severity residuals or handled in docs/readme touch-ups if the plan is otherwise ready.

## Revalidation

**Re-review timestamp**: 2026-06-16T02:32:34Z (targeted re-review of P2-fix1 only)  
**Re-review range**: merge-base `6b6602bd` (pre-fix integration HEAD) .. tip `4fc1371d` (current integration HEAD)  
**Focus**: W-1 (qc2) only — `creator works rules reset` safety flags.

### Git / scope evidence
```
$ git rev-parse --show-toplevel && git branch --show-current
/Users/bibi/workspace/organizations/42ch/nexus
iteration/v1.48

$ git log --oneline 6b6602bd..4fc1371d
4fc1371d harness(v1.48): P2-fix1 — W-1 qc1 doc restore + W-1 qc2 reset CLI flags
5dbbf94c docs(plan): record V1.48 P2-fix1 fix wave (W-1 qc1 + W-1 qc2)
469679f4 feat(rules): add --dry-run and --yes to 'creator works rules reset' (W-1 qc2)
1a5fccac fix(findings): restore update_finding_handler doc summary (W-1 qc1)

$ git diff 6b6602bd..4fc1371d --stat
 .../src/api/handlers/findings.rs                   |   1 +
 crates/nexus-orchestration/src/rules_layers.rs     | 222 +++++++++++++++++++++
 .../nexus42/src/commands/creator/rules_runtime.rs  | 194 +++++++++++++++++-
 crates/nexus42/src/commands/creator/works/mod.rs   | 109 +++++++++-
 5 files changed, 534 insertions(+), 13 deletions(-)
```

### W-1 (qc2) — reset CLI safety (commit `469679f4`)
- **Change**: Added `--dry-run` and `--yes` / `-y` flags to `creator works rules reset`.
  - Default (no flags): prints unified diff (via new pure `diff_agents_md_vs_scaffold`), then prompts via `dialoguer` before atomic overwrite.
  - `--dry-run`: prints diff (or "no change" / "absent" message) and exits without writing; no prompt.
  - `--yes` / `-y`: skips prompt and writes immediately.
  - `--dry-run` takes precedence over `--yes`.
  - JSON mode without `--yes`: emits `confirmation_required: true` and exits cleanly (no write).
  - Non-interactive stdin without `--yes` is a `CliError::Config`.
- **Files touched (P2-fix1 scope)**:
  - `crates/nexus42/src/commands/creator/works/mod.rs` — clap definition + parsing tests.
  - `crates/nexus42/src/commands/creator/rules_runtime.rs` — 6-phase handler (`handle_rules_reset`) + `confirm_reset_interactive`.
  - `crates/nexus-orchestration/src/rules_layers.rs` — new pure `diff_agents_md_vs_scaffold` + internal `unified_diff` / LCS helpers (no DB/IO) + 3 hermetic diff tests.
- **Tests added / updated** (commit `469679f4`):
  - `crates/nexus42` lib: `works_rules_reset_supports_dry_run_flag`, `works_rules_reset_supports_yes_long_and_short_flags`, `works_rules_reset_combines_dry_run_yes_and_json`, plus 2 pre-existing reset parse tests updated for new fields.
  - `crates/nexus-orchestration` lib (`rules_layers`): `rules_layers_diff_empty_when_current_equals_scaffold`, `rules_layers_diff_marks_accepted_entries_as_removed`, `rules_layers_diff_has_unified_format_headers`.
- **Lint / fmt / full relevant test evidence** (this re-review run):
  ```
  $ cargo clippy --all -- -D warnings 2>&1 | tail -15
  ... Finished `dev` profile ... (clean; exit 0)

  $ cargo +nightly fmt --all --check 2>&1 | tail -10
  (no output — clean)

  $ cargo test -p nexus42 --lib -- rules_reset 2>&1 | tail -20
  running 5 tests
  test commands::creator::works::tests::works_rules_reset_parses_without_work_id ... ok
  test commands::creator::works::tests::works_rules_reset_supports_dry_run_flag ... ok
  test commands::creator::works::tests::works_rules_reset_combines_dry_run_yes_and_json ... ok
  test commands::creator::works::tests::works_rules_reset_parses_with_work_id_and_json ... ok
  test commands::creator::works::tests::works_rules_reset_supports_yes_long_and_short_flags ... ok
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 702 filtered out

  $ cargo test -p nexus-orchestration --lib rules_layers 2>&1 | tail -30
  running 16 tests
  ... (all 16 pass, including the 3 new diff tests + reset/append/idempotency tests)
  test result: ok. 16 passed
  ```
- **Pure function check**: `diff_agents_md_vs_scaffold(current: &str, work_ref: &str) -> String` (and all internal helpers) are pure — only string slicing / LCS / formatting; no `std::fs`, no DB, no network. The handler in `rules_runtime.rs` owns all FS reads/writes.
- **Precedence verified in source**: `if dry_run { ... return Ok(()); }` precedes the `if !would_change` and the `if !yes { confirm ... }` block.
- **JSON confirmation path**: when `json && !yes && would_change`, emits `{"reset": false, "confirmation_required": true, ...}` and returns early — no write.
- **Verdict for W-1 (qc2)**: **Fixed**. The safety gap identified in the initial qc2 review is closed. No new Critical / Warning introduced in the fix-wave delta for this seat's scope.

### Updated Summary (post-revalidation)
| Severity | Count (initial) | Count (after P2-fix1) |
|----------|-----------------|-----------------------|
| 🔴 Critical | 0 | 0 |
| 🟡 Warning | 1 (W-1) | 0 |
| 🟢 Suggestion | 4 | 4 (unchanged; non-blocking) |

**Final Verdict**: **Approve**

(The W-1 from qc2 is resolved. Suggestions S-1..S-4 remain deferred per prior plan note and consolidated decision; they are non-blocking for this slice.)
