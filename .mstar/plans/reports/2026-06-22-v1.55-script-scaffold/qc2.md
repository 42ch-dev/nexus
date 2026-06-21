---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.55-script-scaffold"
verdict: "Approve"
generated_at: "2026-06-21"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (path validation, profile isolation, ValidationMode gates, scaffold transaction atomicity, file write safety)
- Report Timestamp: 2026-06-22

## Scope
- plan_id: 2026-06-22-v1.55-script-scaffold
- Review range / Diff basis: merge-base: origin/main + tip: iteration/v1.55 HEAD (c30cdd48); P3 commits only
- Working branch (verified): iteration/v1.55
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 5 (core P3 changes in scope: script_scaffold.rs, game_bible_scaffold.rs (transaction backport), validation.rs, bootstrap.rs, script-profile.md + preset + schema)
- Commit range: 59ad649a (ValidationMode::Script + taxonomy), 4eb88c20 (script scaffold + CLI), 08f2c37c (ScaffoldTransaction on game_bible), 4a545ab1 (docs/status), c30cdd48 (merge)
- Tools run: git diff/log/rev-parse, read (source + specs), grep (pattern search for validation/path/profile), cargo test (nexus-kb validation + orchestration compile), bash (tree verification)

## Findings

### 🔴 Critical
- **C-001: script.project_scaffold performs no `work_ref` path validation before FS operations**  
  `script_scaffold.rs:216`: `let work_dir = self.works_root.join(&inp.work_ref);` then unconditional `create_dir_all` + `fs::write` for README, Scripts/script.md, Beats/beat-sheet.md, Characters/characters.md.  
  No call to `validate_work_ref` (contrast `novel_scaffold.rs:272`: `let work_ref = validate_work_ref(&inp.work_ref)?;`).  
  `work_ref` originates from untrusted grill-me / preset input (CLI bootstrap + `script-init` preset).  
  Upstream guards exist (nexus-home-layout `validate_workspace_path_safe`, daemon handlers, auto_chain canonicalize checks, novel_scaffold_sanitize), but the scaffold capability itself lacks self-defense. A malicious or corrupted `work_ref` containing `..`, `/`, `\`, or control chars could escape `Works/` before any caller sanitization.  
  AC: "Script scaffold path validation prevents traversal/escapes" — not satisfied in P3 implementation.  
  **Severity**: critical (security: path traversal on FS write surface; correctness: violates defense-in-depth for non-novel scaffold).  
  **Fix**: Add `use super::novel_scaffold_sanitize::validate_work_ref;` + `let work_ref = validate_work_ref(&inp.work_ref)?;` (then re-bind) exactly as novel does. Apply symmetrically to game_bible_scaffold if not already.  
  Source Trace: git diff 9f5298e..c30cdd48 on script_scaffold.rs + novel_scaffold_sanitize.rs; direct join at L216; no sanitize import.

### 🟡 Warning
- **W-001: `ValidationMode::Script` rejects `novel_category`, but explicit negative test coverage for Script mode is thin**  
  Code is correct: `validation.rs:266` dispatches `ValidationMode::Script => validate_script_body(...)`; `validate_script_body:440-456` explicitly checks `if attrs.get("novel_category").is_some() { return Err(InvalidNovelCategory) }` (and same for game_bible_category) before requiring `script_category`.  
  However, the KB test suite runs (37+ validation tests) show coverage for Novel and GameBible rejections (`novel_mode_rejects_*`, `game_bible_mode_rejects_novel_category`), but no dedicated `script_mode_rejects_novel_category` or `script_mode_rejects_game_bible_category` test case was exercised in filtered runs. The positive `validate_script_body` path is present; negative cross-profile leakage test is missing or not asserted under Script mode.  
  AC: "`ValidationMode::Script` rejects `novel_category` in `body.attributes`" — implementation exists, but verification evidence is incomplete for this reviewer focus.  
  **Severity**: high (correctness: profile isolation guarantee; security-adjacent: category leakage could corrupt KB invariants for script Works).  
  **Fix**: Add two negative tests under `ValidationMode::Script` (one for novel_category, one for game_bible_category) mirroring the GameBible test at L734.  
  Source Trace: validation.rs:439 (rejection), 266 (dispatch), test mod around L734 (only game_bible case); cargo test -p nexus-kb -- validation (no script negative surfaced).

- **W-002: ScaffoldTransaction provides rollback guard but does not use temp-file + atomic rename for content writes**  
  Both `script_scaffold.rs:251-275` and `game_bible_scaffold.rs` use direct `std::fs::write(path, content)` + `create_dir_all`, then `tx.commit()` after DB PATCH.  
  `ScaffoldTransaction` Drop (L118-146) does best-effort `remove_file` / `remove_dir` for items this invocation created.  
  AC explicitly lists "File write atomicity (temp+rename pattern)". Current approach relies on process-local guard + single-user daemon assumption (documented at L9-14). Partial write + crash before commit leaves partial tree that Drop cleans on next run, but no per-file atomicity.  
  **Severity**: medium (correctness: atomicity guarantee weaker than stated AC; reliability under crash).  
  **Fix**: Either (a) implement temp+rename inside the scaffold writes (as some other FS paths in repo do), or (b) update AC / residual to document that the transaction guard is the chosen mechanism and why temp+rename was not applied for scaffolds.  
  Source Trace: script_scaffold.rs:224 (tx = new), 251 (direct write), 288 (commit); identical pattern in game_bible; tests at L363 (rollback) and L392 (commit).

### 🟢 Suggestion
- **S-001: No `is_script_profile` helper for symmetry**  
  `nexus-local-db/src/works.rs` has `is_novel_profile` and `is_game_bible_profile`. No `is_script_profile`. Routing in bootstrap.rs and preset uses string `work_profile == "script"` or ValidationMode. Correct today, but increases future drift risk when adding script-specific behavior.  
  **Severity**: low (maintainability).  
  **Fix**: Add `pub fn is_script_profile(profile: Option<&str>) -> bool { profile == Some("script") }` + test (mirrors existing).  

- **S-002: ScaffoldTransaction duplicated across novel/game-bible/script**  
  Identical struct + Drop impl now lives in novel_scaffold.rs, game_bible_scaffold.rs (P3 backport), and script_scaffold.rs.  
  **Severity**: nit (maintainability).  
  **Fix**: Extract to shared module under `capability/builtins/` (e.g. `scaffold_transaction.rs`) once the pattern stabilizes.

- **S-003: Script layout isolation documented and implemented correctly**  
  `script-profile.md:46-73` explicitly forbids `Stories/`, `Outlines/`, `Drafts/`, `Design/`, `work_chapters`. Implementation matches: Scripts/, Beats/, Characters/, Logs/{write,review}/ only. Preset gates `work_profile in [null, script]`. No novel-path leakage observed.  
  **Severity**: nit (positive). Keep.

## Source Trace
- Finding C-001: git diff + read on script_scaffold.rs:216 vs novel_scaffold.rs:272 + novel_scaffold_sanitize.rs:32
- Finding W-001: grep + read validation.rs:440 (reject block), 266 (dispatch), test section L606+
- Finding W-002: read script_scaffold.rs:88-147 (tx) + 251 (writes), game_bible_scaffold.rs:296-347
- CI / verification: cargo test -p nexus-kb (validation 36+ passed), cargo check -p nexus-orchestration, git rev-parse on iteration/v1.55 @ c30cdd48
- GitNexus: attempted impact on ScriptProjectScaffold / validate_work_ref / ScaffoldTransaction (low/0 direct upstream in indexed graph; ambiguous on dupe struct name)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 (1 resolved in fix-wave) |
| 🟡 Warning | 0 (2 resolved in fix-wave) |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Evidence Summary (per assignment)
- cwd + branch + commit range: verified via `git rev-parse --show-toplevel`, `--abbrev-ref HEAD`, `merge-base origin/main iteration/v1.55`, `git log ... | head` (P3 commits only).
- GitNexus: low-risk symbols; no high blast-radius callers for core scaffold entry in current index.
- CI gates: `cargo test -p nexus-kb -- validation` (all relevant pass), `cargo check -p nexus-orchestration` (clean), scoped test runs for scaffold_transaction + tree creation.
- All standard qc-specialist-2 checklist items covered (path safety, profile gates, ValidationMode correctness, transaction rollback, SQL/FS safety).
- Findings use machine enum severity mapping (critical/high/medium/low/nit per mstar-plan-artifacts/references/status-and-residuals.md).

---

## Revalidation (Wave 2, targeted re-review after P3 fix-wave commit 21908cdb)

**Review range for revalidation**: merge-base: c30cdd48 + tip: iteration/v1.55 HEAD (post-fix 21908cdb + 376ef43a merge)

**Fix-wave commit under review**: `21908cdb` — "fix(v1.55-P3): P3 fix-wave — ScaffoldTransaction safety, path validation, Script tests, daemon boot count"

### Revalidation Scope (qc2 focus per assignment)
- C-001: `validate_work_ref` call in `script.project_scaffold` + traversal regression test (`../etc/passwd`)
- W-001: `ValidationMode::Script` direct unit tests (positive + 2+ negative + edge cases)
- W-002: `ScaffoldTransaction` temp+rename atomic pattern + crash-mid-transaction regression test
- All original findings disposition

### Evidence from fix-wave (21908cdb)

**C-001 (Critical — path validation) → RESOLVED**
- `script_scaffold.rs:28`: `use super::novel_scaffold_sanitize::validate_work_ref;`
- `script_scaffold.rs:310`: `let work_ref = validate_work_ref(&inp.work_ref)?;` (before any path join)
- Same pattern applied to `game_bible_scaffold.rs:330`
- Regression test: `script_scaffold_rejects_path_traversal_in_work_ref` (L549-578) — passes `"../etc/passwd"`, asserts `is_err()` + error contains `path-traversal` / `invalid character` / `must start with`
- Verified execution: `cargo test -p nexus-orchestration --lib script_scaffold_rejects_path_traversal_in_work_ref` → **PASS**
- Identical test exists for game_bible (symmetry)

**W-001 (Warning — ValidationMode::Script tests) → RESOLVED**
- `validation.rs` now contains 8 dedicated `script_mode_*` tests (L1049-1167+):
  - Positive: `script_mode_accepts_all_three_categories` (Dialogue/Beat/Act + script_category)
  - Negative (cross-profile leakage): `script_mode_rejects_novel_category`, `script_mode_rejects_game_bible_category`
  - Negative (missing/invalid): `script_mode_rejects_missing_script_category`, `script_mode_rejects_invalid_script_category`, `script_mode_rejects_non_string_script_category`
  - Edge: `script_mode_rejects_missing_body`, `script_mode_rejects_missing_attributes`
  - Structured error verification: `script_missing_body_returns_structured_kind`, `script_missing_category_returns_structured_kind`
  - Utility + display tests also added for `is_valid_script_category`, `default_block_type_for_script_category`, `ValidationMode::Script` + `ValidationKind` variants
- Verified execution: `cargo test -p nexus-kb -- validation::tests::script_mode` → **8 passed**

**W-002 (Warning — atomic writes) → RESOLVED**
- `ScaffoldTransaction` rewritten (L95-247):
  - `create_dir` / `write_file` helpers track create vs overwrite
  - `write_file` uses temp+rename: writes `<path>.tmp`, then `std::fs::rename` to final path
  - `temp_files`, `created_files`, `overwritten_files`, `created_dirs` tracked separately
- Drop rollback (L196-246):
  - Cleans `temp_files` (partial writes)
  - Restores `overwritten_files` from snapshot
  - Deletes only `created_files`
  - Removes `created_dirs` in reverse order
- Regression test: `crash_mid_transaction_leaves_no_half_written_file` (L583-616) — simulates temp file present without rename, asserts temp cleaned + final path never created
- Verified execution: `cargo test -p nexus-orchestration --lib crash_mid_transaction_leaves_no_half_written_file` → **PASS** (both script + game_bible)
- Additional regression: `rollback_preserves_pre_existing_user_content` (L510-545) — pre-creates user README, scaffold overwrites, rollback restores original
- Verified: `cargo test -p nexus-orchestration --lib rollback_preserves_pre_existing_user_content` → **PASS**

### Revalidation verdict per finding
- C-001: **Resolved** — `validate_work_ref` called before path join; `../etc/passwd` regression test passes and executes.
- W-001: **Resolved** — 8 direct `ValidationMode::Script` tests (positive + negative cross-profile + edge + structured error kinds) present and passing.
- W-002: **Resolved** — temp+rename atomic write + explicit crash-mid-transaction regression test present and passing; pre-existing content preservation also covered.

### Standard checklist re-check (post-fix)
- Path traversal self-defense now present in scaffold capability itself (defense-in-depth).
- ValidationMode::Script negative tests cover novel/game_bible leakage explicitly.
- File write atomicity via temp+rename + rollback tracking implemented; crash safety tested.
- No behavior change to commit paths on success (atomic rename is transparent).
- All prior CI gates re-verified in scope (nexus-kb validation, nexus-orchestration lib tests).

### Disposition
All blocking findings (C-001, W-001, W-002) from initial qc2 review are resolved by fix-wave 21908cdb with passing regression tests. Suggestions (S-001..S-003) remain non-blocking (maintainability).

**Updated Verdict**: Approve

**Revalidation timestamp**: 2026-06-21 (post-fix 21908cdb on iteration/v1.55 @ 376ef43a)

---

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: QC targeted re-review (security/correctness) Wave 2 — V1.55 P3 script scaffold (fix-wave 21908cdb)  
**Status**: Done  
**Scope Delivered**: Revalidation of fix-wave 21908cdb on iteration/v1.55 (merge-base c30cdd48). Verified C-001 (validate_work_ref + traversal test), W-001 (ValidationMode::Script positive + 2+ negative + edge tests), W-002 (temp+rename atomic + crash-mid-transaction test). All original blocking findings resolved.  
**Artifacts**: `.mstar/plans/reports/2026-06-22-v1.55-script-scaffold/qc2.md` (updated in-place; same file as wave 1)  
**Validation**: cwd/branch/range verified; regression tests executed and passing (`script_scaffold_rejects_path_traversal_in_work_ref`, 8× `script_mode_*` tests, `crash_mid_transaction_leaves_no_half_written_file`, `rollback_preserves_pre_existing_user_content`); GitNexus not re-run (no new symbols); code + docs cross-checked against fix diff.  
**Issues/Risks**: None blocking for qc2 focus. All critical/warning items from initial review resolved with evidence. Suggestions remain open for future maintainability work.  
**Plan Update**: N/A (reviewer; no plan edits).  
**Handoff**: qc2 re-review complete. Approve. PM may proceed to consolidated decision.  
**Git**: 74d06ee6 qc(v1.55-p3): qc2 revalidation — C-001/W-001/W-002 resolved in fix-wave 21908cdb; Approve

**Reviewer alignment note**: Review performed in strict leaf-executor mode per mstar-dispatch-gates + qc-specialist-shared. No subagent dispatch, no code changes, no status.json writes. Report only under `{PLAN_DIR}/reports/.../qc2.md`. Targeted re-review updates same file (no `-rev2`).
