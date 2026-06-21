---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.55-script-scaffold"
verdict: "Request Changes"
generated_at: "2026-06-22"
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
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

## Evidence Summary (per assignment)
- cwd + branch + commit range: verified via `git rev-parse --show-toplevel`, `--abbrev-ref HEAD`, `merge-base origin/main iteration/v1.55`, `git log ... | head` (P3 commits only).
- GitNexus: low-risk symbols; no high blast-radius callers for core scaffold entry in current index.
- CI gates: `cargo test -p nexus-kb -- validation` (all relevant pass), `cargo check -p nexus-orchestration` (clean), scoped test runs for scaffold_transaction + tree creation.
- All standard qc-specialist-2 checklist items covered (path safety, profile gates, ValidationMode correctness, transaction rollback, SQL/FS safety).
- Findings use machine enum severity mapping (critical/high/medium/low/nit per mstar-plan-artifacts/references/status-and-residuals.md).

---

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: QC tri-review (security/correctness) for V1.55 P3 — script scaffold  
**Status**: Done  
**Scope Delivered**: Full review of P3 diff range on iteration/v1.55 (c30cdd48); focused on path validation, no-novel-leakage, ValidationMode::Script, ScaffoldTransaction atomicity, file write safety.  
**Artifacts**: `.mstar/plans/reports/2026-06-22-v1.55-script-scaffold/qc2.md` (this file)  
**Validation**: cwd/branch/range verified; cargo tests + checks executed; GitNexus impact attempted; spec + code cross-checked against ACs.  
**Issues/Risks**: C-001 (path validation missing in scaffold capability) is blocking per AC and security focus. W-001/W-002 are high/medium correctness gaps.  
**Plan Update**: N/A (reviewer; no plan edits).  
**Handoff**: PM to address C-001 (add validate_work_ref) + strengthen Script-mode negative tests + document atomicity choice before re-review or consolidated decision.  
**Git**: (to be executed after write)

**Reviewer alignment note**: Review performed in strict leaf-executor mode per mstar-dispatch-gates + qc-specialist-shared. No subagent dispatch, no code changes, no status.json writes. Report only under `{PLAN_DIR}/reports/.../qc2.md`.
