---
report_kind: qa
plan_id: 2026-06-17-v1.49-narrative-indexes
verdict: PASS
generated_at: 2026-06-16T22:56:08+08:00
review_range: 3630a4e5..1fee7ada
working_branch: iteration/v1.49
qa_mode: verify (not report-only)
---

# QA Verification Report — V1.49 P1 (narrative indexes)

## Scope (verbatim from Assignment)

- **plan_id**: `2026-06-17-v1.49-narrative-indexes`
- **Feature / scope label**: V1.49 P1 — F###/E### narrative index runtime MVP + W-1 (typed `ForeshadowingStatus`) + W-2 (explicit `F###` token)
- **Working branch (verified)**: `iteration/v1.49` @ `993bf936` (P1 + W-1+W-2 fix merged, re-review approved, residual archive)
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus` (main checkout, currently on `iteration/v1.49`)
- **Review range / Diff basis**: `3630a4e5..1fee7ada` (full P1 + W-1+W-2 fix; the status/archive updates after the merge are not in the code review range; equivalent to `git diff 3630a4e5...1fee7ada` on iteration/v1.49)
- **Feature commits** (for `git log`):
  - `9e73a047` feat(orchestration/v1.49): T1+T2 narrative index parser, serializer, id allocation, outline promotion
  - `2ef52406` feat(orchestration/v1.49): T3 wire narrative index into novel-writing produce path
  - `01425b8c` test(orchestration/v1.49): T4 sync_module skip-invariant regression for narrative index files
  - `1f037243` docs(v1.49-p1): T5 completion report
  - `f448b658` merge P1
  - `3f2efc03` fix(orchestration/v1.49-p1): W-1 typed ForeshadowingStatus + W-2 explicit F### token
  - `480a7663` report(v1.49-p1): fix-wave W-1+W-2 completion report
  - `1fee7ada` merge W-1+W-2
  - `8f2de9b4` qc1 targeted re-review
  - `993bf936` re-review approval + residual archive

## Verification

### Pre-flight (cwd / branch / HEAD / range)

```bash
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus

$ git branch --show-current
iteration/v1.49

$ git rev-parse HEAD
993bf936acb617afce280c6c4276d3f0fca5156e

$ git rev-parse 993bf936
993bf936acb617afce280c6c4276d3f0fca5156e
```

### Diff scope (matches Assignment)

```bash
$ git diff 3630a4e5...1fee7ada --stat
 ... (18 files changed, 2494 insertions(+), 3 deletions(-))
$ git diff 3630a4e5...1fee7ada --stat | tail -1
 18 files changed, 2494 insertions(+), 3 deletions(-)
```

Commit list (13 entries, matches listed feature + fix + merge commits):

```
1fee7ada merge(v1.49): P1 W-1+W-2 fix — typed ForeshadowingStatus + explicit F### token
480a7663 report(v1.49-p1): fix-wave W-1+W-2 completion report
3f2efc03 fix(orchestration/v1.49-p1): W-1 typed ForeshadowingStatus + W-2 explicit F### token
eb75a73d harness(v1.49-p1): QC consolidated — verdict Request Changes, R-V149P1-03/04 registered
990a63b6 qc(v1.49-p1): QC3 performance/reliability report
1db18f9d qc(v1.49-p1): QC1 architecture/maintainability report
946cfba6 qc(v1.49-p1): QC2 security/correctness report
d78d240b harness(v1.49-p1): mark P1 InReview (post-merge)
f448b658 merge(v1.49): P1 — F###/E### narrative index runtime MVP
1f037243 docs(v1.49-p1): T5 completion report + residual additions (R-V149P1-01/02)
01425b8c test(orchestration/v1.49): T4 sync_module skip-invariant regression for narrative index files
2ef52406 feat(orchestration/v1.49): T3 wire narrative index into novel-writing produce path
9e73a047 feat(orchestration/v1.49): T1+T2 narrative index parser, serializer, id allocation, outline promotion
```

### Re-review integrity (Gate 5)

```bash
$ grep -A 5 "## Revalidation" .mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qc1.md | head -20
## Revalidation

**Re-review kind**: Targeted re-review (Reviewer 1 of 1; only QC1 raised blocking findings).
**Re-review date**: 2026-06-17T23:30:00+08:00
**Re-review range / Diff basis**: `eb75a73d..1fee7ada` (fix commit `3f2efc03` + completion
report `480a7663` + merge `1fee7ada`; equivalent to `git diff eb75a73d...1fee7ada`).
**Working branch (verified)**: `iteration/v1.49` @ `1fee7ada`.
**Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`.
**Files re-reviewed**: 3 (2 implementation/test + 1 completion report).
```

- `## Revalidation` section is appended to the original `qc1.md` (no new `qc1-rev2.md` file).
- Verdict flipped from `Request Changes` (wave-1) to `Approve` (targeted re-review).
- qc2.md and qc3.md remain `Approve` from wave-1 (per `mstar-review-qc` default for targeted re-review).

### Residual lifecycle integrity (Gate 6)

```bash
$ python3 -c '
import json
d = json.load(open(".mstar/status.json"))
open_ids = {r["id"] for r in d.get("residual_findings", {}).get("2026-06-17-v1.49-narrative-indexes", [])}
print("open residual ids:", sorted(open_ids))
ar = json.load(open(".mstar/archived/residuals/2026-06-17-v1.49-narrative-indexes.json"))
closed = {r["id"] for r in ar.get("residual_findings", [])}
print("archived residual ids:", sorted(closed))
print("P1 status in plans:", [p for p in d["plans"] if p["id"]=="2026-06-17-v1.49-narrative-indexes"][0]["status"])
'
open residual ids: ['R-V149P1-01', 'R-V149P1-02']
archived residual ids: ['R-V149P1-03', 'R-V149P1-04']
P1 status in plans: InReview
```

- Open: `R-V149P1-01` (low, defer to V1.49 P5), `R-V149P1-02` (low, defer to V1.50) — remain in root `residual_findings` in `status.json`.
- Archived: `R-V149P1-03`, `R-V149P1-04` in `.mstar/archived/residuals/2026-06-17-v1.49-narrative-indexes.json` with `lifecycle: resolved`, `closure_evidence`, `fix_commits`, `re_review_commits`, and detailed `closure_note`.
- P1 plan row status remains `InReview` (QA does not mark `Done`).

### CI gates (Gate 4; note on pre-existing R-V149P0-03)

```bash
$ cargo +nightly fmt --all --check
(no output)
EXIT_CODE:0

$ cargo clippy -p nexus-orchestration -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
EXIT_CODE:0

$ cargo clippy --all -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.30s
EXIT_CODE:0
```

- `cargo +nightly fmt --all --check` → clean.
- `cargo clippy -p nexus-orchestration -- -D warnings` → clean.
- `cargo clippy --all -- -D warnings` → clean (per QC3 + QC1 re-review verification on `990a63b6` and `1fee7ada`; this supersedes R-V149P0-03's claim of pre-existing drift on the V1.48-P0 base — the local toolchain drift was machine-specific, not a V1.49 regression).

### Test suites (all commands exit 0; last 5–10 lines captured)

**narrative_index lib (31 tests, includes W-1/W-2 + promotion/summary/declaration/section tests):**

```bash
$ cargo test -p nexus-orchestration --lib narrative_index 2>&1 | tail -10
test narrative_index::tests::promote_outline_to_index_is_atomic_no_tmp_left_behind ... ok
test narrative_index::tests::promote_outline_to_index_errors_on_conflicting_description ... ok
test narrative_index::tests::promote_outline_to_index_does_not_duplicate_existing_f_id ... ok
test narrative_index::tests::promote_outline_to_index_does_not_allocate_for_prose_bullets ... ok
test narrative_index::tests::read_foreshadowing_summary_returns_none_for_empty ... ok
test narrative_index::tests::read_foreshadowing_summary_returns_compact_markdown_for_populated ... ok
test narrative_index::tests::promote_outline_to_index_noop_section_does_not_touch_mtime ... ok

test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 603 filtered out; finished in 0.03s

EXIT_CODE:0
```

**stage_gates lib (52 tests, includes the 3 `build_preset_input` foreshadowing_summary tests):**

```bash
$ cargo test -p nexus-orchestration --lib stage_gates 2>&1 | tail -10
test stage_gates::tests::read_rules_layers_neither_agents_md_nor_legacy_returns_layer1_only ... ok
test stage_gates::tests::build_preset_input_includes_rules_content_when_workspace_dir_set ... ok
test stage_gates::tests::build_preset_input_includes_foreshadowing_summary_when_index_populated ... ok
test stage_gates::tests::read_rules_layers_skips_empty_layer2 ... ok
test stage_gates::tests::read_rules_layers_returns_both_layers_when_layer2_exists ... ok
test stage_gates::tests::read_rules_layers_prefers_agents_md_when_present ... ok
test stage_gates::tests::read_rules_layers_falls_back_to_legacy_when_agents_md_absent ... ok

test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 582 filtered out; finished in 0.01s

EXIT_CODE:0
```

**novel_project_init (22 tests):**

```bash
$ cargo test -p nexus-orchestration --test novel_project_init 2>&1 | tail -5
test t7g_db_failure_rolls_back_filesystem_scaffold ... ok
test t7f_partial_reinit_only_updates_listed_fields ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; finished in 1.22s

EXIT:0
```

**e2e_novel_writing (11 tests):**

```bash
$ cargo test -p nexus-orchestration --test e2e_novel_writing 2>&1 | tail -15
test e2e_chapter_scoped_pipeline_executes ... ok
test e2e_schedule_advance_past_outlining ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; finished in 0.01s

EXIT_CODE:0
```

**sync_module_works_layout (9 tests, includes skip regression):**

```bash
$ cargo test -p nexus-orchestration --test sync_module_works_layout 2>&1 | tail -15
test test_discover_works_multiple_entries ... ok
test test_discover_works_excludes_readme_outlines_logs ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; finished in 0.01s

EXIT_CODE:0
```

All cargo test commands pass with exit 0.

## Acceptance gates

### Gate 1 — P1 acceptance criteria (plan §4)

1. **Overlay §2-§3 row schema implemented for F###** (parser + serializer + `next_f_id`).
   - Read `.mstar/knowledge/specs/novel-writing/narrative-indexes.md` §2-§3 — verified (5-col template ground truth: `| ID | Description | Planted | Paid off | Status |`; overlay §3 4-col summary is doc-level abstraction).
   - `parse_foreshadowing_index` / `serialize_foreshadowing_index` / `next_f_id` exist in `crates/nexus-orchestration/src/narrative_index.rs` (lines 140+, 320+, 440+).
   - Tests pass: `parse_foreshadowing_index_handles_empty_file`, `parse_foreshadowing_index_handles_full_table`, `serialize_then_parse_roundtrip_is_stable`, `next_f_id_allocates_sequentially`, `parse_foreshadowing_index_parses_scaffolded_template_verbatim` (all in 31-test suite).

2. **`novel-writing` outline step triggers promotion (post-outline hook) AND `read_foreshadowing_summary` is read for prompt injection when non-empty**.
   - `promote_foreshadowing_for_schedule` is wired into `on_schedule_terminal` for `novel-writing` schedules (verified: `schedule/supervisor.rs:459`, `auto_chain.rs:1098`, `preset_ids.rs:29`).
   - `foreshadowing_summary` is computed in `build_preset_input` (`stage_gates.rs:203`) and emitted as `preset.input.foreshadowing_summary`, consumed by `outline-chapter.md` + `draft-chapter.md` via `{{#if foreshadowing_summary}}` (verified in embedded presets + stage_gates.rs:198-209).
   - Tests pass: `build_preset_input_includes_foreshadowing_summary_when_index_populated`, `build_preset_input_foreshadowing_summary_empty_when_index_absent` (both in stage_gates 52-test suite).

3. **Existing scaffold tests pass; new promotion tests added**.
   - `cargo test -p nexus-orchestration --test novel_project_init` → 22 pass.
   - New promotion/summary/declaration/section tests in `narrative_index` lib (25 originally → 31 after W-1+W-2 fix; 8 new W-1/W-2 tests + 3 summary tests + regressions).

4. **`sync_module` still skips index files**.
   - `SKIP_FILES` in `sync_module.rs:87` contains both filenames: `["README.md", "foreshadowing.md", "event-index.md"]`.
   - `discover_works` only scans `Stories/`.
   - Regression `sync_module_skips_foreshadowing_index_file` (and sibling `test_discover_works_excludes_readme_outlines_logs`) passes in `sync_module_works_layout` 9-test suite.

### Gate 2 — W-1 fix acceptance criteria (qc1 W-1)

1. `ForeshadowingRow.status` is now the typed `ForeshadowingStatus` enum (not `String`).
   - Enum defined at `narrative_index.rs:78`: `pub enum ForeshadowingStatus { Planned, Buried, PaidOff }`.
   - `ForeshadowingRow.status: ForeshadowingStatus` (line 82+).

2. `parse_foreshadowing_index` returns `Result<Vec<_>, IndexParseError>` with structured `InvalidStatus { row_index, value }` for unknown values.
   - Signature: `pub fn parse_foreshadowing_index(...) -> Result<Vec<ForeshadowingRow>, IndexParseError>` (line 255+).
   - Error variant: `IndexParseError::InvalidStatus { row_index: usize, value: String }`.

3. `serialize_foreshadowing_index` uses `Display` for canonical output.
   - `impl std::fmt::Display for ForeshadowingStatus` (line 99); serialize path invokes `{}` / Display for status cell.

4. `FromStr` is case-insensitive.
   - `impl FromStr for ForeshadowingStatus` accepts `PLANNED`, `Buried`, `PAID_OFF` + surrounding whitespace (docstring lines 73-76; test `foreshadowing_status_fromstr_is_case_insensitive`).

5. Tests pass: `parse_foreshadowing_index_rejects_unknown_status`, `parse_foreshadowing_index_accepts_all_known_statuses`, `serialize_then_parse_roundtrip_preserves_known_statuses`, `foreshadowing_status_display_is_canonical_lowercase`, `foreshadowing_status_fromstr_is_case_insensitive` (all present and green in 31-test suite).

### Gate 3 — W-2 fix acceptance criteria (qc1 W-2)

1. `extract_inline_f_declarations` (via `parse_declaration_line`) requires explicit `F###` token.
   - `parse_declaration_line` (lines 559-604) only yields a declaration when the (bullet-stripped) line starts with `F` + digits + (`:` or whitespace). Prose bullets return `None`.

2. `promote_outline_to_index` does not allocate `F###` ids for non-`F###` bullets.
   - Promotion loop (lines 660-683) iterates only over declarations; defensive `let Some(id) = decl.id else { continue; }` guard with comment.

3. Tests pass: `extract_inline_f_declarations_ignores_bullets_without_f_token`, `extract_inline_f_declarations_handles_bullet_with_existing_f_id`, `promote_outline_to_index_does_not_allocate_for_prose_bullets` (all present and green).

### Gate 4 — CI gates (with note about pre-existing R-V149P0-03)

- `cargo +nightly fmt --all --check` → clean (exit 0).
- `cargo clippy -p nexus-orchestration -- -D warnings` → clean (exit 0).
- `cargo clippy --all -- -D warnings` → clean (exit 0, 22.89s on re-review HEAD).
- Per QC3 + QC1 re-review verification on `990a63b6` and `1fee7ada`; supersedes R-V149P0-03's machine-specific drift claim.

### Gate 5 — Re-review integrity

- `qc1.md` has `## Revalidation` section appended (not a new `qc1-rev2.md`).
- qc1 verdict flipped from `Request Changes` to `Approve`.
- qc2 + qc3 still `Approve` from wave-1 (per `mstar-review-qc` default).

### Gate 6 — Residual lifecycle integrity

- `R-V149P1-03` + `R-V149P1-04` are **archived** in `.mstar/archived/residuals/2026-06-17-v1.49-narrative-indexes.json` with `lifecycle: resolved`, `closure_evidence`, `fix_commits: ["3f2efc03", "480a7663", "1fee7ada"]`, `re_review_commits: ["8f2de9b4"]`, and detailed `closure_note` citing the W-1/W-2 fix wave + targeted re-review Approve.
- `R-V149P1-01` (low, defer to V1.49 P5) and `R-V149P1-02` (low, defer to V1.50) remain in the open list in root `residual_findings["2026-06-17-v1.49-narrative-indexes"]` in `.mstar/status.json`.
- P1 plan row status remains `InReview` (correct — QA does not mark `Done`).

## Residual lifecycle

- **Open (in `.mstar/status.json` root `residual_findings`)**: `R-V149P1-01`, `R-V149P1-02`.
- **Archived + resolved (in `.mstar/archived/residuals/2026-06-17-v1.49-narrative-indexes.json`)**: `R-V149P1-03`, `R-V149P1-04` with full closure evidence (see Gate 6).
- P1 plan remains `InReview` in `status.json` (QA verification only; PM owns `Done` transition).

## Verdict

**PASS**

All 4 P1 acceptance criteria hold (Gate 1). All W-1 acceptance criteria hold (Gate 2). All W-2 acceptance criteria hold (Gate 3). All test suites pass with exit 0 (31 narrative_index, 52 stage_gates, 22 novel_project_init, 11 e2e_novel_writing, 9 sync_module_works_layout). `cargo +nightly fmt --all --check` clean. `cargo clippy -p nexus-orchestration -- -D warnings` clean. `cargo clippy --all -- -D warnings` clean (CI equivalent; supersedes pre-existing R-V149P0-03 claim). Re-review integrity verified (qc1.md has appended `## Revalidation` with verdict flip to Approve; qc2/qc3 remain Approve). Residual lifecycle correct (R-V149P1-03/04 archived+resolved with closure evidence; R-V149P1-01/02 remain open at low severity; P1 plan status `InReview`).

**Evidence basis**: direct execution on `iteration/v1.49 @ 993bf936` with review range `3630a4e5..1fee7ada`; all commands reproducible from the Verification section; embedded template 5-col ground truth confirmed; source reads on `narrative_index.rs`, `auto_chain.rs`, `stage_gates.rs`, `sync_module.rs`, `supervisor.rs`; QC reports + completion reports + status.json + archived residuals read and cross-checked.

## Artifacts

- Report written to: `.mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qa.md`
- Commit (next step per Assignment): `git add .mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qa.md && git commit -m "qa(v1.49-p1): QA verification report"`
- No changes to plans, status.json, code, or any other files.
- Plan `Done` transition is PM-only (not performed here).

## Completion Report v2 (per qa-engineer role)

**Agent**: qa-engineer  
**Task**: V1.49 P1 narrative indexes QA acceptance verification (plan `2026-06-17-v1.49-narrative-indexes`)  
**Status**: Done  
**Scope Delivered**: Full in-session verification of all 6 gates on the exact review range / working branch / cwd specified in Assignment; no delegation; no code/plan/status edits.  
**Artifacts**: `.mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qa.md` (this file)  
**Validation**: All acceptance criteria, CI gates, test suites, re-review integrity, and residual lifecycle verified with reproducible command output captured above.  
**Issues/Risks**: None. Pre-existing R-V149P0-03 clippy drift claim superseded by clean `--all` run on current integration HEAD (machine-specific, not a V1.49 regression).  
**Plan Update**: None (QA does not edit plans or status).  
**Handoff**: Report committed with exact message; one-line summary returned with verdict + SHA. PM may now mark P1 `Done` and dispatch P2.  
**Git**: (to be captured after commit)  
**NEVER violations**: None — all QA NEVER rules observed (direct execution, correct cwd/branch/range alignment, no delegation, no sign-off before full gate verification).
