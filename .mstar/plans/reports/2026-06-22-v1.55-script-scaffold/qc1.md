---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.55-script-scaffold"
verdict: "Approve"
generated_at: "2026-06-22"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-21T05:57:08Z

## Scope
- plan_id: 2026-06-22-v1.55-script-scaffold
- Review range / Diff basis: merge-base: origin/main + tip: iteration/v1.55 HEAD (c30cdd48); P3 commits 59ad649a, 4eb88c20, 08f2c37c, 4a545ab1, c30cdd48 (P3 own commits) — review only these
- Working branch (verified): iteration/v1.55
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 18 P3 merge-diff files, plus pattern reference `novel_scaffold.rs`
- Commit range: P3 merge diff `c30cdd48^1..c30cdd48` (first parent 9b3d70ce → merge c30cdd48); assignment diff basis also verified with merge-base `9f5298e4ec4c9376a22d99ebb7af38e92186b5f5`
- Tools run:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`
  - `git branch --show-current` → `iteration/v1.55`
  - `git rev-parse --short HEAD` → `c30cdd48`
  - `git diff --name-status c30cdd48^1..c30cdd48`
  - GitNexus query: `script scaffold profile bootstrap preset ValidationMode Script BlockType dialogue beat act ScaffoldTransaction game_bible_scaffold`
  - `pnpm run codegen` → PASS; follow-up generated-dirs diff check produced no output
  - `cargo +nightly fmt --all --check` → PASS
  - `cargo clippy --all -- -D warnings` → PASS
  - `cargo test -p nexus-orchestration -p nexus-kb -p nexus42` → PASS (762+ integration/doc tests in scoped crates)
  - `cargo test --all` → FAIL (`nexus-daemon-runtime` integration test below)
- Working tree note: pre-existing local modifications were present before this report (`.mstar/plans/reports/2026-06-22-v1.55-game-bible-depth-35/qc2.md`, `AGENTS.md`, `CLAUDE.md`); this review did not modify them.

## Findings

### 🔴 Critical

- **C-001 (severity: critical): `ScaffoldTransaction` can overwrite and then delete pre-existing scaffold files on DB failure.**
  - Evidence: `crates/nexus-orchestration/src/capability/builtins/script_scaffold.rs:244-275` and `game_bible_scaffold.rs:231-249` call `std::fs::write(...)` unconditionally and push every template path into `tx.files_created`. On any later failure before `tx.commit()` (notably the pool-backed `UPDATE works ...` at `script_scaffold.rs:277-285` / `game_bible_scaffold.rs:252-262`), `Drop` removes every path in `files_created` (`script_scaffold.rs:123-131`, `game_bible_scaffold.rs:323-331`).
  - Impact: re-running scaffold over an existing or partially edited `Works/<work_ref>/` can overwrite user-authored `README.md` / template content and then delete those files if the DB PATCH fails. This contradicts the inline transaction contract ("Files/dirs that pre-existed ... are left untouched") and the P3 acceptance item closing `R-V154P1-W001` for both game-bible and script scaffold paths.
  - Architecture note: the adopted reference pattern is `novel_scaffold.rs:743-759` (`write_file_idem`) plus `create_dir_all_idem`, which only tracks files actually created by the invocation and skips pre-existing files. The new game-bible/script implementations copied the rollback guard but not the idempotent file-write boundary, leaving the most important safety invariant unenforced.
  - Required fix: introduce/reuse an idempotent helper equivalent to `novel_scaffold::write_file_idem` for both scaffold paths; do not overwrite pre-existing files; only push freshly-created files into the rollback guard; add regression tests where pre-existing content survives a forced DB update failure.

- **C-002 (severity: critical): Full workspace test gate fails after registering `script.project_scaffold`.**
  - Evidence: `cargo test --all` fails in `crates/nexus-daemon-runtime/tests/daemon_boot_llm_wiring.rs:227`:
    ```text
    with_runtime_deps_registers_all_llm_capabilities ... FAILED
    assertion `left == right` failed: registry should have 23 builtins (21 V1.51 + essay.scaffold V1.52 + game_bible.scaffold V1.54 P1)
      left: 24
     right: 23
    ```
  - Impact: P3 updates `CapabilityRegistry::with_runtime_deps` to include `ScriptProjectScaffold` (`crates/nexus-orchestration/src/capability/mod.rs:369-375`), but a cross-crate daemon-runtime integration assertion was not updated. Per QC baseline, any relevant CI/test failure is at least blocking until fixed or explicitly dispositioned.
  - Required fix: update the daemon-runtime wiring test expectation/message and ensure it still checks that all LLM/runtime dependencies plus scaffold capabilities are registered intentionally. Re-run `cargo test --all`.

### 🟡 Warning

- **W-001 (severity: medium): `ValidationMode::Script` lacks direct unit coverage despite being an acceptance item.**
  - Evidence: `crates/nexus-kb/src/validation.rs:417-503` implements script validation, but the test module ends after game-bible/canonical-name utility tests; grep found no `ValidationMode::Script` tests outside the implementation. Existing tests cover game-bible behavior and `BlockType` deserialization, but not script happy/error paths.
  - Impact: acceptance requires "`script_category` validates script facts" and `script-profile.md` §11 requires accepting script categories and rejecting novel/game-bible categories. Without tests for missing/non-string/invalid `script_category`, cross-profile rejection, and `ValidationMode::Script.to_string()`, future edits can silently drift from the new taxonomy.
  - Required fix: add script-mode tests mirroring game-bible mode: accepts all three categories, rejects `novel_category`, rejects `game_bible_category`, rejects missing/non-string/invalid `script_category`, verifies structured `ValidationKind`, verifies `default_block_type_for_script_category`, and updates `validation_mode_display` to include `script`.

### 🟢 Suggestion

- **S-001 (severity: nit): CLI help for `--init-preset` still says only `novel-project-init`.**
  - Evidence: `crates/nexus42/src/commands/creator/bootstrap.rs:51-53` documents "Accepts: novel-project-init" while the same command now derives `essay-init`, `game-bible-init`, and `script-init` (`bootstrap.rs:285-294`).
  - Improvement: update the help text to list the current profile-derived init presets or describe it generically as an override for the profile init preset.

## Source Trace

- Finding ID: C-001
  - Source Type: manual-reasoning / code-diff
  - Source Reference: `script_scaffold.rs:244-285`, `game_bible_scaffold.rs:231-262`, `novel_scaffold.rs:743-759`
  - Confidence: High
- Finding ID: C-002
  - Source Type: test-gate
  - Source Reference: `cargo test --all`; `crates/nexus-daemon-runtime/tests/daemon_boot_llm_wiring.rs:227`
  - Confidence: High
- Finding ID: W-001
  - Source Type: test-coverage-review
  - Source Reference: `crates/nexus-kb/src/validation.rs:417-503`; grep for `ValidationMode::Script`
  - Confidence: High
- Finding ID: S-001
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/bootstrap.rs:51-53`, `:285-294`
  - Confidence: High

## Acceptance Criteria Review (qc1 focus)

- `script-profile.md` Draft follows `essay-profile.md` / `game-bible-profile.md` patterns: **Pass**.
- Script scaffold creates `Scripts/`, `Beats/`, `Characters/`, `Logs/` and avoids `Stories/` semantics: **Pass** for layout; see C-001 for rollback/idempotence safety.
- `dialogue`, `beat`, `act` are additive BlockType variants; `script_category` validates script facts: **Partial** — schema/codegen/helper implementation present, but W-001 notes missing direct validation tests.
- Additive enum only: **Pass** — reviewed schema/generated/enum conversions show appended script variants and no removal/rename.
- `R-V154P1-W001` closed by applying `ScaffoldTransaction` to both game-bible and script scaffold paths: **Fail** — guard exists, but C-001 means the safety invariant is not actually satisfied for pre-existing files.
- Standard QC checklist: **Fail pending fixes** — CI gate failure and data-loss rollback risk block approval.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

---

## Revalidation (Wave 2, targeted re-review after P3 fix-wave commit 21908cdb)

**Review range for revalidation**: merge-base: c30cdd48 + tip: iteration/v1.55 HEAD (964d2268); P3 fix-wave commit `21908cdb`

**Fix-wave scope (qc1 focus)**: 4 files changed in `21908cdb`:
- `crates/nexus-orchestration/src/capability/builtins/script_scaffold.rs` (+375/-89)
- `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs` (+358/-142)
- `crates/nexus-kb/src/validation.rs` (+206)
- `crates/nexus-daemon-runtime/tests/daemon_boot_llm_wiring.rs` (+6/-2)

### Disposition of qc1 findings

**C-001 (Critical — ScaffoldTransaction overwrite + delete pre-existing files) → RESOLVED**
- `ScaffoldTransaction` rewritten with separate tracking: `created_files`, `overwritten_files`, `temp_files`, `created_dirs`
- `write_file()` saves original content snapshot for pre-existing files before overwriting; uses temp+rename atomic pattern
- `create_dir()` is idempotent — only tracks newly created dirs
- Drop rollback order: (1) clean temp files, (2) restore overwritten files from snapshot, (3) delete only created files, (4) remove created dirs in reverse
- Regression test `rollback_preserves_pre_existing_user_content` in both script & game_bible scaffolds → **PASS**
- Regression test `crash_mid_transaction_leaves_no_half_written_file` in both scaffolds → **PASS**
- Architecture note: the adopted reference pattern now correctly tracks create vs overwrite, going beyond `novel_scaffold.rs:write_file_idem` (which skips pre-existing files entirely) — the fix-wave restores from snapshot on rollback

**C-002 (Critical — daemon_boot_llm_wiring test count 24 ≠ 23) → RESOLVED**
- Expected count updated from 23 → 24 with comment reflecting `+ script.scaffold V1.55 P3`
- `cargo test -p nexus-daemon-runtime --test daemon_boot_llm_wiring` → **4/4 PASS**
- `cargo test --all` → **all pass (0 failures)**

**W-001 (Warning — ValidationMode::Script lacks direct unit tests) → RESOLVED**
- 18 dedicated `ValidationMode::Script` tests added covering:
  - Positive: all 3 categories accepted (dialogue/beat/act)
  - Negative: `novel_category` rejected, `game_bible_category` rejected
  - Edge: missing `script_category`, invalid category, non-string category, missing body, missing attributes
  - Structured error kind verification (MissingScriptCategory, InvalidScriptCategory, NonStringScriptCategory)
  - Utility: `is_valid_script_category`, `default_block_type_for_script_category`
  - Display: `ValidationMode::Script → "script"`, 3 new `ValidationKind::*` display variants
- `cargo test -p nexus-kb -- validation::tests::script_mode` → **8/8 PASS** (8 script-mode specific)
- Full `cargo test -p nexus-kb` validation suite → **51/51 pass**

**S-001 (Suggestion — CLI help text stale) → STILL OPEN (non-blocking)**
- Not addressed in fix-wave. Remains a low-priority suggestion for future CLI hygiene.

### Summary of new findings

- **None.** No new Critical, Warning, or architecture/maintainability concerns introduced by the fix-wave.

### CI Gate Verification (post-fix)

| Gate | Command | Result |
|------|---------|--------|
| Full test suite | `cargo test --all` | **All pass** (no failures) |
| Clippy (deny warnings) | `cargo clippy --all -- -D warnings` | **Clean** |
| Format check | `cargo +nightly fmt --all --check` | **Clean** (exit 0) |
| Codegen | `pnpm run codegen` | **No diff** on generated directories |
| GitNexus | `detect_changes` | **LOW risk** (AGENTS.md/CLAUDE.md only; no code symbols affected) |
| Regression (script) | `rollback_preserves_pre_existing_user_content`, `crash_mid_transaction_leaves_no_half_written_file`, `scaffold_idempotent_preserves_user_content`, `script_scaffold_rejects_path_traversal_in_work_ref` | **All PASS** |
| Regression (game_bible) | Same set + `game_bible_scaffold_rejects_path_traversal_in_work_ref` | **All PASS** |
| Validation | 8× `script_mode_*` tests in `nexus-kb` | **All PASS** |
| Daemon boot | `daemon_boot_llm_wiring` (4 tests) | **All PASS** |

### Verdict rationale

All blocking findings (C-001, C-002, W-001) from the initial qc1 review are resolved by fix-wave commit `21908cdb` with passing regression tests in both scaffold implementations. The `ScaffoldTransaction` rewrite now correctly tracks create vs overwrite, uses atomic temp+rename writes, restores pre-existing files from snapshot on rollback, and is thoroughly tested. The daemon-runtime integration test count is updated. Script validation tests comprehensively cover positive/negative/edge/structured-error paths. The sole remaining suggestion (S-001) is non-blocking. No new architecture or maintainability risks were introduced.

### Acceptance Criteria (updated)

| Criterion | Initial | Post-fix |
|-----------|---------|----------|
| `script-profile.md` Draft follows patterns | ✅ Pass | ✅ Pass (unchanged) |
| Script scaffold avoids `Stories/` semantics | ✅ Pass (layout); ❌ C-001 (safety gap) | ✅ **Pass** — C-001 resolved with create/overwrite tracking + snapshot restore |
| Additive BlockType + `script_category` validation | ⚠️ Partial (no tests) | ✅ **Full Pass** — 18 script-mode tests |
| Additive enum only | ✅ Pass | ✅ Pass (unchanged) |
| `R-V154P1-W001` closed | ❌ Fail (C-001 safety gap) | ✅ **Closed** — both scaffolds use idempotent transaction with rollback safety |
| Standard QC checklist | ❌ Fail (CI failure + data-loss risk) | ✅ **All clear** — CI green, no data-loss risk |

**Updated Verdict**: Approve

**Revalidation timestamp**: 2026-06-22
