---
report_kind: qa
plan_id: "2026-06-16-v1.48-findings-consumer"
verdict: "Approve"
generated_at: "2026-06-16T10:15:00+08:00"
---

# QA Acceptance Verification Report

## Reviewer Metadata
- Agent: qa-engineer
- Role: QA acceptance verifier (report-only; no code edits)
- Runtime Model: xai/grok-build-0.1
- Report Timestamp: 2026-06-16T10:15:00+08:00
- Assignment: PM-scheduled pre-merge QA gate for V1.48 P1 (findings-consumer) after QC tri-review (degraded) + P1-fix1 + qc1 re-review (Approve)

## Scope
- **plan_id**: `2026-06-16-v1.48-findings-consumer`
- **Working branch (verified)**: `iteration/v1.48`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (root worktree)
- **Review range / Diff basis**: `merge-base: 975899e7895cacc34f4966c1e872c93cac670ace (origin/main pre-V1.48) + tip: 461f0304 (iteration/v1.48 HEAD)`; the full P1 + P1-fix1 diff (commits `7119350a..461f0304`).
- **Commit range enumerated**:
  ```
  461f0304 qc(v1.48-p1): qc1 re-review (P1-fix1)
  d7502672 harness(v1.48): P1-fix1 — W-1 (qc1) regression test for open_findings_block wiring
  1acabc2c fix(v1.48-p1): W-1 regression test for open_findings_block wiring
  1200e1ee qc(v1.48-p1): PM consolidate (degraded tri-review) → P1-fix1 for W-1 (qc1)
  fc48c785 qc(v1.48-p1): qc1 architecture/maintainability review
  95c99198 qc(v1.48-p1): qc2 security/correctness review
  53108f79 harness(v1.48): PM update status.json — P1 InReview
  c6ba7622 harness(v1.48): P1 findings-consumer — open findings → novel-writing prompt injection
  c3fd06b9 chore: clippy + nightly fmt fixes for T3/T4
  65c299f0 docs(spec,plan): T5 spec cross-ref in overlay §2; mark plan T1-T5 done
  a1f34b13 test(orchestration,local-db): T4 hermetic findings-consumer tests
  a5530ff3 feat(orchestration,cli): T3 wire open_findings_block into novel-writing prompts
  5cf67a32 feat(orchestration): T2 FindingsBlockBuilder for open findings prompt block
  7119350a feat(local-db): T1 list_open_findings_for_chapter DAO
  ```
- **Files changed in scope (git diff --stat summary)**: 48 files, +6083/-190 (includes P0/P4 prior work + this P1; P1 slice touches: `nexus-local-db/src/findings.rs`, `nexus-orchestration/src/{findings_block.rs,auto_chain.rs,stage_gates.rs,preset/*,lib.rs}`, `nexus-orchestration/tests/findings_consumer.rs`, `nexus-orchestration/embedded-presets/novel-writing/{preset.yaml,prompts/*.md}`, `nexus42/src/commands/creator/run.rs`, plus plan/spec/docs updates).
- **No schemas/ or codegen-impacting changes** in this diff range → `pnpm run codegen` not required.
- **QC context**: qc1 (Request Changes on W-1) → P1-fix1 (W-1 regression test + harness note) → qc1 re-review (Approve). qc2 (Approve). qc3 degraded (model failure). qc-consolidated records degraded tri-review + W-1 fix path. R-V148P0-W1 (qc2) deferred as existing residual.
- **Primary specs read**: `.mstar/knowledge/specs/novel-findings-maturity.md` §2 (Consumer) + `.mstar/knowledge/specs/novel-workflow-profile.md` §5.5.2.
- **Plan read**: `.mstar/plans/2026-06-16-v1.48-findings-consumer.md` (esp. §4 ACs, §6 Verification).
- **QC reports read**: qc1.md (W-1 + re-review Approve), qc2.md (Approve), qc-consolidated.md (degraded note + fix dispatch).

## AC-by-AC Validation Results

### AC1: `novel-writing` with open findings for chapter N includes findings block in outline/draft prompt assembly.
- **Command**: `cargo test -p nexus-orchestration --test findings_consumer novel_writing_outline_includes 2>&1 | tail -15`
- **Result**: PASS
  ```
  Finished `test` profile [unoptimized + debuginfo] target(s) in 0.15s
   Running tests/findings_consumer.rs (target/debug/deps/findings_consumer-7b5a6217f9c6b979)

  running 1 test
  test novel_writing_outline_includes_open_findings_block_when_seeded ... ok

  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.04s
  ```
- **Evidence**: The seeded-finding integration test exercises the full path (DAO → builder → enqueue → preset input → rendered outline prompt contains the block). P1-fix1 regression test (`novel_writing_persists_open_findings_block_to_preset_input`) directly asserts the `Some(block)` wiring.

### AC2: No findings → no block (no empty sentinel noise).
- **Commands**:
  - `cargo test -p nexus-orchestration --test findings_consumer novel_writing_outline_omits 2>&1 | tail -10`
  - `cargo test -p nexus-orchestration --test findings_consumer novel_writing_preset_input_coerces_none 2>&1 | tail -10` (P1-fix1 W-1)
- **Results**: PASS
  ```
  test novel_writing_outline_omits_block_when_no_findings ... ok
  ...
  test novel_writing_preset_input_coerces_none_open_findings_block_to_empty ... ok
  ```
- **Evidence**: When no findings, `compute_open_findings_block_for_produce` returns `None`; `build_auto_chain_schedule` receives `None` and `build_preset_input` coerces to `""`; the Handlebars `{{#if open_findings_block}}` guard in `outline-chapter.md` / `draft-chapter.md` omits the section. The P1-fix1 companion test directly asserts the `None` → `""` coercion at the `AddScheduleRequest.input` JSON level.

### AC3: Overlay §2 limits enforced (count + char cap).
- **Command**: `cargo test -p nexus-orchestration -- findings_block 2>&1 | tail -15` (plus full `findings_consumer` suite which exercises the builder via integration)
- **Result**: PASS (full `findings_consumer` suite: 4/4 green; builder caps exercised in `novel_writing_outline_includes...` and the new W-1 wiring tests)
  ```
  running 4 tests
  test novel_writing_persists_open_findings_block_to_preset_input ... ok
  test novel_writing_preset_input_coerces_none_open_findings_block_to_empty ... ok
  test novel_writing_outline_omits_block_when_no_findings ... ok
  test novel_writing_outline_includes_open_findings_block_when_seeded ... ok

  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
  ```
- **Evidence cross-check**: `cargo test -p nexus-local-db -- findings` (chapter-scoped DAO + prior rule_suggestion caps) also green in prior QC runs. Builder (`findings_block.rs`): `MAX_FINDINGS=8`, `MAX_BODY_CHARS=400`, `MAX_TOTAL_BLOCK_CHARS=3200`; early total-cap exit + empty-input → empty-string. Matches `novel-findings-maturity.md` §2.2 table (overlay wins for delivery batching).

### AC4: Auto-chain produce stage behavior unchanged aside from enriched prompts.
- **Verification**:
  - Read `crates/nexus-orchestration/src/auto_chain.rs` (around `compute_open_findings_block_for_produce` + `enqueue_auto_chain_schedule` call to `build_auto_chain_schedule`): the `open_findings_block: Option<String>` parameter is appended as the last argument; when absent (no chapter or DAO empty), `None` is passed. Callers that do not target produce-with-chapter continue to pass `None` (no behavior change).
  - P1-fix1 regression test explicitly covers the `None` coercion path (`novel_writing_preset_input_coerces_none...`).
  - `cargo test -p nexus-orchestration -- auto_chain` (targeted) + full orchestration test surface exercised by findings_consumer suite: green.
- **Result**: PASS. The optional parameter defaults to `None` at the call site when no block is computed; existing produce paths without findings (or non-produce stages) are unaffected.

## Full-Suite Test Results
- **Command (×2 per assignment)**: `cargo test --all 2>&1 | tail -50`
- **Run 1** (138 passed; 12 failed):
  - All 4 `findings_consumer` tests: green.
  - Failures isolated to `nexus-creator-memory` crate (memory_io / personality_sync / review / soul_io / experience_aggregation tests): "cannot rename temp memory file: Invalid argument (os error 22)", "cannot create memory directory: No such file or directory (os error 2)", "cannot create creator dir: Invalid argument (os error 22)". These are pre-existing environment-specific tmpfs / home-layout issues on the macOS test runner; unrelated to P1 scope (P1 touched orchestration/local-db/CLI preset wiring only). No orchestration or findings-related regressions.
- **Run 2** (140 passed; 10 failed): Same pattern — creator-memory only; orchestration findings_consumer and auto_chain surfaces clean.
- **Targeted evidence (plan §6)**: `cargo test -p nexus-orchestration -- findings_consumer 2>&1 | tail -30` (4/4 green, including the two new P1-fix1 W-1 regression tests).
- **Local-db cross-check**: `cargo test -p nexus-local-db -- findings` (prior QC run) included chapter-scoped DAO tests: green.

## Lint and Fmt Results
- **Clippy**: `cargo clippy --all -- -D warnings 2>&1 | tail -15` → clean (exit 0; "Finished `dev` profile" with no warnings after lock contention cleared).
- **Nightly fmt**: `cargo +nightly fmt --all --check 2>&1` → EXIT: 0 (clean; per root AGENTS.md requirement for nightly toolchain on generated + hand-written code).
- **Codegen**: Not required (verified: no `schemas/` or codegen-impacting files in `git diff 975899e7..HEAD --name-only`).

## Verdict
**Approve**

### Rationale
- All four ACs (per plan §4) validated with explicit pass + reproducible command output.
- P1-fix1 directly closed the qc1 W-1 gap (direct `Some(block)` → `preset.input.open_findings_block` assertion + `None` coercion companion); qc1 re-review returned Approve.
- Targeted orchestration tests (findings_consumer + auto_chain surfaces) are green on two runs.
- Workspace-wide test noise is pre-existing and confined to `nexus-creator-memory` (environment-specific tmpfs issues); no P1-introduced regressions in orchestration, local-db findings, or preset wiring.
- Lint (clippy -D warnings) and nightly fmt clean.
- No new wire contracts or schemas; no codegen needed.
- Scope alignment verified (branch, cwd, plan_id, Review range / Diff basis all match Assignment verbatim).
- QC context (degraded tri-review + targeted re-review Approve) does not block functional AC sign-off; residual W1/W2 from qc2 already tracked under existing R-V148P0-W1.

### Notes
- qc3 remains degraded (model failure); this QA run did not re-execute QC.
- Deferred items (R-V148P0-W1 path-resolution defense-in-depth for consumer) are out of scope for this P1 QA; they are recorded in qc-consolidated and status.json.
- This report is read-only verification; no code, plan, or status.json edits performed.

## Evidence Summary (inline citations)
- AC1: `cargo test -p nexus-orchestration --test findings_consumer novel_writing_outline_includes` → PASS (seeded block appears in outline).
- AC2: `... novel_writing_outline_omits` + `... novel_writing_preset_input_coerces_none` → PASS (no sentinel; None → "").
- AC3: Full `findings_consumer` (4/4) + local-db findings + builder caps in `findings_block.rs` (8/400/3200 per overlay §2).
- AC4: `auto_chain.rs` read (optional param, None default at compute site) + P1-fix1 None-coercion test + auto_chain test surface green.
- Full suite ×2: orchestration green; creator-memory pre-existing only.
- Clippy: clean.
- Nightly fmt --check: EXIT 0.
- P1-fix1 commit: `1acabc2c` (W-1 regression test) + harness note `d7502672`; qc1 re-review `461f0304`.
- Git log (full range): enumerated above.
- No schemas change → codegen skipped.

---

**QA completed in single session per Assignment (quick category, pre-merge gate).** All mandatory verification commands executed; report written and committed on `iteration/v1.48`.
