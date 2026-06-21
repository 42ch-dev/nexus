---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.55-game-bible-depth-35"
verdict: "Approve"
generated_at: "2026-06-21"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-21T04:03:07Z

## Scope
- plan_id: 2026-06-22-v1.55-game-bible-depth-35
- Review range / Diff basis: merge-base: origin/main + tip: iteration/v1.55 HEAD (0718a6fe); review only P2 commits (`fb298429`, `0718a6fe`)
- Working branch (verified): iteration/v1.55
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8 primary P2 files/directories (`quality_loop.rs`, `preset_ids.rs`, `embedded-presets/design-writing/*`, `work_chapters.rs`, `works.rs`, `game-bible-profile.md`, plan stub)
- Commit range: `fb298429^..0718a6fe` for P2-specific diff; assignment diff basis also verified against `origin/main` merge-base `9f5298e4ec4c9376a22d99ebb7af38e92186b5f5`
- Tools run: `git rev-parse`, `git branch --show-current`, `git log`, `git diff --stat`, `git diff --check`; GitNexus impact for `design_five_q_check`, `block_type_to_game_bible_category`, `is_game_bible_design_complete`; `grep`/`read` manual source review; `cargo test -p nexus-orchestration quality_loop::tests::design_five_q`; `cargo test -p nexus-orchestration quality_loop::tests::block_type_to_game_bible_category`; `cargo test -p nexus-local-db work_chapters::tests::test_is_game_bible_design_complete`; `cargo test -p nexus-daemon-runtime get_work`; `cargo +nightly fmt --all --check`; `cargo clippy --all -- -D warnings`; `cargo test --all`

## GitNexus Impact Report
- `design_five_q_check` (`crates/nexus-orchestration/src/quality_loop.rs`): GitNexus returned `Target not found` / `risk: UNKNOWN` (new symbol not present in current index). Manual upstream scan found only local unit-test callers in the P2 diff.
- `block_type_to_game_bible_category` (`crates/nexus-orchestration/src/quality_loop.rs`): GitNexus returned `Target not found` / `risk: UNKNOWN` (new symbol not present in current index). Manual scan found only unit-test callers; see Critical finding F-001.
- `is_game_bible_design_complete` (`crates/nexus-local-db/src/work_chapters.rs`): GitNexus returned `Target not found` / `risk: UNKNOWN` (new symbol not present in current index). Manual upstream scan found daemon `get_work` auto-promotion plus local-db tests.

## Acceptance Criteria Check
- Design 五问 rubric is design-document-specific, not novel-prose 五问: **Pass**. `design-review-exit.md` and `design_five_q_check` use pillars/mechanics/continuity/playability/clarity.
- Section completion detection evaluates critical `Design/*.md` sections + `intake_status`: **Pass**. `is_game_bible_design_complete` checks `overview.md`, `pillars.md`, `mechanics.md`, and `intake_status == complete` with positive/negative tests.
- KB extraction reuses V1.51 `nexus.llm.extract` with no parallel extraction path: **Fail**. No profile-aware reuse path exists for game-bible payloads; the new mapping function is not wired into V1.51 extraction.
- `block_type_to_game_bible_category` mapping is exhaustive + correct: **Partial**. Direct/cross-domain mapping exists, but it is unused in production and therefore cannot make extraction/adoption valid.
- Profile-gate tracing additions are observational only: **Pass**. The added traces in `is_work_completed` and `reconcile_from_filesystem` do not change branch outcomes beyond logging.
- `game-bible-profile.md` Draft body updates are surgical and status remains Draft: **Pass with note**. Header remains Draft; body still contains some V1.54/V1.55 stale language, but that is not the blocking issue here.

## Findings

### 🔴 Critical

- **F-001 (`severity: critical`) — Game-bible KB extraction is not wired; the new category mapping is production-dead.**
  - Evidence: `crates/nexus-orchestration/src/quality_loop.rs:695-729` still has a single `candidate_from_llm_json` path that always emits `attributes.novel_category`, `tags: ["novel", "llm-extracted"]`, and no `game_bible_category`. The new `block_type_to_game_bible_category` at `quality_loop.rs:789-814` is only referenced by tests (`quality_loop.rs:1993-2023`) and is not called by `run_llm_extract`, `extract_via_llm`, `LlmExtractTask`, or any review-time hook. Repository grep for `block_type_to_game_bible_category` confirms no production caller.
  - Impact: Plan acceptance explicitly requires “KB extraction creates/updates World KB facts with correct `game_bible_category` handling.” As implemented, any reused V1.51 extraction path still creates novel-shaped payloads, so `ValidationMode::GameBible` adoption would lack `body.attributes.game_bible_category` and either fail validation or persist semantically wrong novel metadata. Architecturally, this leaves T6 as a unit-tested helper rather than a game-bible extraction path.
  - Fix: Keep reusing V1.51 `nexus.llm.extract`, but make candidate materialization profile-aware (e.g. `candidate_from_llm_json_for_profile` or a game-bible-specific wrapper) so game-bible candidates carry `game_bible_category: block_type_to_game_bible_category(block_type)`, appropriate tags, and no `novel_category`. Add tests that exercise the actual extraction/adoption-facing payload, not only the mapping helper.

### 🟡 Warning

- **F-002 (`severity: high`) — The `design-writing` preset declares an acceptance loop that has no durable section-status transition.**
  - Evidence: `embedded-presets/design-writing/preset.yaml:3-6` says each run drafts a section and “on GO, the `section_status` frontmatter flips to `accepted`,” but the actual states at `preset.yaml:54-83` only inject prompts and run `llm_judge`; there is no file/frontmatter update action, context update, or capability that writes `Design/<section>` or changes `section_status`. `design-section.md:45-46` even tells the agent not to include YAML frontmatter.
  - Impact: Completion detection in `is_game_bible_design_complete` depends entirely on `section_status: accepted`, but the new preset does not own the status transition it documents. That creates an architecture gap between “review GO” and “completion can ever become true” unless a separate manual step is intended and documented.
  - Fix: Either add an explicit durable transition/capability for section acceptance, or narrow the preset/spec comments to state that accepted frontmatter remains manual for P2 and add a follow-up residual. Add at least one regression test or validation fixture proving the chosen flow.

### 🟢 Suggestion

- **S-001 (`severity: low`) — Reconcile game-bible spec language around cross-domain categories during the fix.** `game-bible-profile.md:300` says a game character can use `game_bible_category: "character"`, while §7.2/§7.3 and validation allow only seven game-bible categories. The code maps `character` → `species`, which is defensible, but the spec should not imply an invalid category string.

## Source Trace
- Finding ID: F-001
  - Source Type: git-diff + manual grep
  - Source Reference: `quality_loop.rs:695-729`, `quality_loop.rs:789-814`, `grep block_type_to_game_bible_category` (only tests)
  - Confidence: High
- Finding ID: F-002
  - Source Type: git-diff + manual reasoning
  - Source Reference: `embedded-presets/design-writing/preset.yaml:3-6`, `preset.yaml:54-83`, `prompts/design-section.md:45-46`, `work_chapters.rs:1301-1401`
  - Confidence: Medium
- Finding ID: S-001
  - Source Type: doc-rule + manual reasoning
  - Source Reference: `.mstar/knowledge/specs/game-bible-profile.md:286-308`
  - Confidence: Medium

## Verification Evidence
- cwd/branch: `/Users/bibi/workspace/organizations/42ch/nexus` on `iteration/v1.55`, HEAD `0718a6fe4f65898e67a5fa6145f90f6a9f476d2a`.
- P2 log: `fb298429 feat(v1.55): game-bible Depth 3.5 — design-writing preset, completion detection, KB extraction`; `0718a6fe Merge branch 'feature/v1.55-game-bible-depth-35' into iteration/v1.55`.
- P2 diff whitespace: `git diff --check fb298429^..0718a6fe` passed.
- Scoped tests: design rubric (4), category mapping (3), game-bible completion (4), daemon `get_work` filtered tests — passed.
- Formatting: `cargo +nightly fmt --all --check` passed.
- CI-like gates: `cargo clippy --all -- -D warnings` passed; `cargo test --all` passed.
- Extra strict note: `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus-daemon-runtime --all-targets -- -D warnings` fails on existing/wider test-target lint debt, including P0/reference-source and older local-db test lint surfaces; this is not the repository CI clippy command documented in root `AGENTS.md`.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation

### Revalidation Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-21T00:00:00Z

### Revalidation Scope
- plan_id: `2026-06-22-v1.55-game-bible-depth-35`
- Review range / Diff basis: `merge-base: fb298429^` + `tip: iteration/v1.55 HEAD` (assignment tip `e003363b`; local HEAD during re-review `8fe08564`, qc3 report-only commit on top of `e003363b`)
- Working branch (verified): `iteration/v1.55`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Fix-wave commits reviewed: `798c47a0` (P2 fix-wave) and merge `fc9c1e54`; also checked `e003363b` for `R-V155P2-F002` residual registration and `8fe08564` as qc3 report-only context.
- Local dirty state before report edit: pre-existing unstaged changes in `AGENTS.md`, `CLAUDE.md`, and `qc2.md`; this re-review modifies and commits only this `qc1.md` report.

### Fix-Wave Commit Range Log
```text
8fe08564 qc(v1.55-p2): qc3 revalidation — W-1/W-2/W-3 resolved, Approve
e003363b harness(v1.55): Wave 1 — P0/P2 done + R-V154P1-S002 resolved + R-V155P2-F002 registered
fc9c1e54 Merge branch 'feature/v1.55-game-bible-depth-35' into iteration/v1.55
798c47a0 fix(v1.55): P2 fix-wave — F-001 profile-aware extraction, F-002 preset narrowing, W-1/W-2 tracing/async, S-001 spec reconcile
```

### GitNexus Impact Evidence
| Symbol | Risk | Direct / notable upstream callers | Revalidation use |
| --- | --- | --- | --- |
| `candidate_from_llm_json_for_profile` | HIGH | Direct: `run_llm_extract`, `candidate_from_llm_json`, 4 helper tests; transitive: `extract_via_llm`, `extract_kb_candidates_for_review`, `detect_missing_kb_on_finalize` | Confirms the helper is now called by `run_llm_extract`, but also shows most game-bible coverage is helper-level rather than a production extraction task/hook test. |
| `run_llm_extract` | LOW | Direct: `extract_via_llm`; transitive review/finalize hooks and tests | Confirms signature propagation to the shared extraction core. |
| `extract_via_llm` | LOW | Direct: `extract_kb_candidates_for_review`, `detect_missing_kb_on_finalize` | Confirms the review-time/finalize hooks still call a wrapper that hard-codes `work_profile = "novel"`. |
| `LlmExtractTask` | LOW / ambiguous symbol resolved via Struct+Impl UIDs | No graph upstream callers reported; tests instantiate it directly | Confirms limited graph visibility for task wiring; manual source review remains required. |

### Per-Finding Disposition

#### F-001 (`severity: critical`) — PARTIALLY RESOLVED, still open
- **What is resolved**:
  - `run_llm_extract` now accepts `work_profile: &str` and calls `candidate_from_llm_json_for_profile(c, work_profile)` (`quality_loop.rs:612-663`).
  - `candidate_from_llm_json_for_profile` emits `attributes.game_bible_category`, `tags: ["game-bible", "llm-extracted"]`, and omits `novel_category` for `work_profile == "game_bible"` (`quality_loop.rs:729-799`).
  - `block_type_to_game_bible_category` is now used by the candidate materializer and maps direct plus cross-domain BlockTypes (`quality_loop.rs:839-864`).
  - `LlmExtractTask::evaluate` reads `work_profile` from the graph context with a novel-compatible default and forwards it into `run_llm_extract` (`tasks/mod.rs:573-595`).
- **What remains unresolved**:
  - `extract_via_llm` still hard-codes `"novel"` when invoking `run_llm_extract` (`quality_loop.rs:678-686`). Its callers remain `extract_kb_candidates_for_review` and `detect_missing_kb_on_finalize`, both of which are novel review/finalize hooks today. There is no game-bible review-time hook or schedule path in this diff that passes `work_profile = "game_bible"` into that production hook layer.
  - The new game-bible assertions are helper-only tests in `quality_loop.rs` (`candidate_from_llm_json_for_profile_*`). Existing `LlmExtractTask` tests in `tasks/mod.rs` still exercise the default novel payload only; no test sets graph context `work_profile = "game_bible"` and verifies the production task path returns a game-bible-shaped candidate.
  - Therefore the assignment acceptance criterion “tests cover the production path (not only the helper)” is not met for F-001. The fix establishes the core materializer and one task parameter, but does not yet prove that game-bible Works produce game-bible-shaped KB candidates through an actual extraction task/hook path.
- **Required follow-up**: Add a production-path regression test (minimum: `LlmExtractTask` with mock worker + `work_profile = "game_bible"` context asserting `game_bible_category` and absence of `novel_category`; stronger: a game-bible extraction/review hook test once that hook exists). If P2 intends to defer game-bible review-time hook wiring, record that as a residual rather than claiming F-001 closed.

#### F-002 (`severity: high`) — RESOLVED by Option B
- `design-writing/preset.yaml` now explicitly states that accepted `section_status` frontmatter transition is a manual author step in V1.55 and that the preset does not atomically update `Design/*.md` frontmatter (`preset.yaml:8-11`).
- `R-V155P2-F002` is present in `.mstar/status.json` under root `residual_findings["2026-06-22-v1.55-game-bible-depth-35"]` with machine severity `low`, decision `defer`, owner `@fullstack-dev-2`, target `V1.56+`, and lifecycle `deferred` (`status.json:2119-2132`).
- This satisfies the requested Option B architecture contract: P2 no longer documents an unimplemented durable transition as if it were preset-owned, and the future auto-transition capability is trackable.

#### S-001 (`severity: low`) — RESOLVED
- `game-bible-profile.md` §7.2 now states cross-domain `BlockType` variants are **mapped** to one of the seven valid `game_bible_category` literals and explicitly says `BlockType::Character` maps to `"species"`, not a literal `"character"` category (`game-bible-profile.md:300`).

### Standard QC Checklist Revalidation
- [x] Naming remains clear and consistent for the new helper and mapping functions.
- [x] Responsibilities are improved in the core materializer, but production-path ownership remains incomplete for game-bible extraction; see F-001.
- [x] Error handling remains explicit in `run_llm_extract` (`WorkerUnavailable` vs `CapabilityError`).
- [x] Comments now document intent for F-002 and the profile-aware extraction change; one note: the review-time wrapper comment still describes novel-only extraction and matches the actual code.
- [x] Input/boundary handling is unchanged or tightened: missing canonical names are dropped; confidence remains clamped; unknown game-bible BlockTypes default to `species` with debug trace.
- [x] No new injection/path traversal/privileged-operation surface found in the F-001/F-002/S-001 fix-wave.
- [x] State-transition architecture is coherent after F-002 Option B because the manual accepted-frontmatter step is explicit and residualized.
- [x] Maintainability risk remains in F-001 because helper-level tests can pass while the production task/hook path still lacks a game-bible regression.

### CI Gates Re-run on Current Tree
Command run from `/Users/bibi/workspace/organizations/42ch/nexus` on `iteration/v1.55`:

```text
$ cargo +nightly fmt --all --check && cargo clippy --all -- -D warnings && cargo test --all
exit 0
```

Evidence notes:
- `cargo +nightly fmt --all --check`: passed (no output in chained command before clippy).
- `cargo clippy --all -- -D warnings`: passed (`Finished dev profile ...`).
- `cargo test --all`: passed; final observed summaries include `nexus42` unit tests `762 passed`, CLI/integration test binaries passed, and doc-tests passed. `cargo test` emitted non-fatal rustc warnings in existing test files, but exited 0.
- Additional whitespace check: `git diff --check fb298429^..HEAD` reports pre-existing trailing whitespace in the P2 plan Completion Notes lines 89-90. This is markdown-only and outside the targeted F-001/F-002/S-001 fix criteria, but should be cleaned by the owning PM/dev path before final merge hygiene.

### Updated Findings

#### 🔴 Critical
- **F-001 (`severity: critical`) remains open — production-path coverage/wiring for game-bible extraction is incomplete.** The core helper and `run_llm_extract` are profile-aware, but `extract_via_llm` still passes `"novel"`, no game-bible review-time hook is present, and tests verify only the helper rather than a game-bible production extraction path such as `LlmExtractTask` with `work_profile = "game_bible"`.

#### 🟡 Warning
- (none new). F-002 is resolved and residualized as `R-V155P2-F002` with machine severity `low`.

#### 🟢 Suggestion
- (none new). S-001 is resolved.

### Revalidation Summary
| Severity | Count | Disposition |
|----------|-------|-------------|
| 🔴 Critical | 1 | F-001 still open |
| 🟡 Warning | 0 | F-002 resolved |
| 🟢 Suggestion | 0 | S-001 resolved |

**Verdict**: Request Changes

Rationale: The fix-wave resolves F-002 and S-001, and it materially improves F-001 by adding profile-aware candidate shaping to the shared extraction core. However, the required production-path test/wiring for game-bible extraction is still missing: the only game-bible assertions exercise `candidate_from_llm_json_for_profile` directly, while the actual task/hook path either defaults to novel or lacks a game-bible regression. Under `mstar-review-qc`, an unresolved Critical cannot be approved.

## Revalidation

### 2nd Revalidation Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-21T05:11:07Z

### 2nd Revalidation Scope
- plan_id: `2026-06-22-v1.55-game-bible-depth-35`
- Review range / Diff basis: `merge-base: 798c47a0` + `tip: iteration/v1.55 HEAD` (`f3cec4e6`); PM-narrowed to 2nd fix-wave commits `af987571`, merge `d5f5f8dd`, plan stub addendum `b37d19e5`, merge `f3cec4e6`.
- Working branch (verified): `iteration/v1.55`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- HEAD verified: `f3cec4e6` (`merge(v1.55): plan stub addendum for 2nd fix-wave (b37d19e5)`)
- Local dirty state before this report edit: pre-existing unstaged changes in `AGENTS.md`, `CLAUDE.md`, and `qc2.md`; this re-review modifies and commits only this `qc1.md` report.

### 2nd Fix-Wave Commit Range Log
```text
f3cec4e6 merge(v1.55): plan stub addendum for 2nd fix-wave (b37d19e5)
b37d19e5 docs(v1.55): plan stub addendum — 2nd fix-wave F-001 completion notes
d5f5f8dd merge(v1.55): F-001 2nd fix-wave — production-path coverage (af987571)
af987571 fix(v1.55): F-001 2nd fix-wave — production-path coverage for game-bible extraction
```

### GitNexus Impact Evidence (post-fix)
| Symbol | Risk | Direct / notable upstream callers | Revalidation use |
| --- | --- | --- | --- |
| `extract_via_llm` | LOW | Direct: `extract_kb_candidates_for_review`, `detect_missing_kb_on_finalize`; transitive tests in `missing_kb_detection`, `novel_review_master`, `review_cron_e2e`, `review_time_extraction` | Confirms only two direct production callers; both flow through `ChapterContext`, so the new `work_profile` field is the single pass-through point for review/finalize hooks. |
| `run_llm_extract` | LOW | Direct: `extract_via_llm`; transitive: same review/finalize hooks and tests | Confirms the shared extraction core remains narrowly reused and the signature propagation is contained. Manual source review additionally verified `LlmExtractTask::evaluate` calls it with context `work_profile`. |

### F-001 2nd Revalidation — RESOLVED
- `extract_via_llm` no longer hard-codes `"novel"`: it calls `run_llm_extract(..., &ctx.work_profile)` (`quality_loop.rs:685-693`).
- `ChapterContext` now carries `work_profile: String` with documented default-to-`"novel"` behavior for legacy NULL rows (`quality_loop.rs:873-891`).
- The shared context loader reads the actual Work row via `nexus_local_db::works::get_work` and derives `work.work_profile.filter(|p| !p.is_empty()).unwrap_or_else(|| "novel".to_string())` before constructing `ChapterContext` (`quality_loop.rs:984-1025`). This means both `extract_kb_candidates_for_review` and `detect_missing_kb_on_finalize` pass the actual Work profile through their common `load_context_for_preset` → `extract_via_llm` path rather than a constant.
- `run_llm_extract` remains profile-aware and materializes candidates via `candidate_from_llm_json_for_profile(c, work_profile)` (`quality_loop.rs:618-668`). For `work_profile == "game_bible"`, that materializer emits `attributes.game_bible_category`, `tags: ["game-bible", "llm-extracted"]`, and no `novel_category` (`quality_loop.rs:736-806`).
- New production-path test `tasks::tests::llm_extract_task_with_game_bible_profile_produces_game_bible_candidate` exercises `LlmExtractTask::evaluate` with context `work_profile = "game_bible"`; it does **not** directly call `candidate_from_llm_json_for_profile` (`tasks/mod.rs:2364-2424`). Assertions cover `attributes.game_bible_category == "faction"`, absent `novel_category`, and tags containing `"game-bible"` and `"llm-extracted"`.
- Architecture/maintainability assessment: the fix keeps one shared candidate materialization path (`run_llm_extract` → `candidate_from_llm_json_for_profile`) and one Work-derived profile source (`ChapterContext::work_profile`). This closes the production-path coverage gap without introducing a parallel game-bible extraction pipeline.

### Production-Path Test Invocation
```text
$ cargo test -p nexus-orchestration --lib llm_extract_task_with_game_bible_profile_produces_game_bible_candidate -- --nocapture
running 1 test
test tasks::tests::llm_extract_task_with_game_bible_profile_produces_game_bible_candidate ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 735 filtered out; finished in 0.00s
```

### CI Gates Re-run on Current Tree
Commands run from `/Users/bibi/workspace/organizations/42ch/nexus` on `iteration/v1.55` at HEAD `f3cec4e6`:

```text
$ cargo +nightly fmt --all --check
exit 0

$ cargo clippy --all -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.41s

$ cargo test --all
test result: ok. 762 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.49s
Doc-tests also completed successfully; examples include nexus42 integration suites and crate doc-tests with ignored tests only where marked ignored.
```

### Standard QC Checklist Revalidation
- [x] Naming remains clear and consistent (`work_profile`, `ChapterContext::work_profile`, `candidate_from_llm_json_for_profile`).
- [x] Responsibilities stay separated: Work/profile loading is in the shared context loader; LLM invocation stays in `run_llm_extract`; profile-specific payload shaping stays in the candidate materializer.
- [x] Error handling remains explicit (`WorkerUnavailable` vs `CapabilityError`; DB/load failures flow through existing `AutoChainError`).
- [x] Comments explain intent and compatibility defaults rather than incidental implementation details.
- [x] Input/boundary handling is coherent: empty/NULL work profiles default to novel; unknown game-bible block types default to `species` with trace; candidate cap is unchanged.
- [x] No new injection/path traversal/privileged-operation surface found in the 2nd fix-wave.
- [x] LLM/agent boundary remains unchanged: untrusted LLM JSON is parsed into bounded candidate rows; the fix only selects payload schema by trusted Work/context profile.
- [x] Hot-path and resource behavior are unchanged from the first fix-wave; no new unbounded operations introduced.
- [x] Maintainability improves because production hooks and task extraction share the same profile-aware materializer instead of maintaining parallel novel/game-bible code paths.

### Updated Findings (2nd Revalidation)

#### 🔴 Critical
- (none). Prior F-001 (`severity: critical`) is resolved by production-path profile pass-through plus `LlmExtractTask::evaluate` game-bible regression coverage.

#### 🟡 Warning
- (none new). Prior F-002 remains resolved/residualized as `R-V155P2-F002` with machine severity `low`.

#### 🟢 Suggestion
- (none new). Prior S-001 remains resolved.

### 2nd Revalidation Summary
| Severity | Count | Disposition |
|----------|-------|-------------|
| 🔴 Critical | 0 | F-001 resolved |
| 🟡 Warning | 0 | No new warnings |
| 🟢 Suggestion | 0 | No new suggestions |

**Verdict**: Approve

Rationale: The 2nd fix-wave addresses the remaining F-001 blocker. `extract_via_llm` now forwards the Work-derived `work_profile`, both existing callers use that shared context path, and the new `LlmExtractTask::evaluate` regression proves a real extraction task with `work_profile = "game_bible"` yields a game-bible-shaped candidate (`game_bible_category`, no `novel_category`, `game-bible` tag). Required CI gates are clean on the reviewed tree, and no architecture/maintainability blocker remains for qc1 scope.
