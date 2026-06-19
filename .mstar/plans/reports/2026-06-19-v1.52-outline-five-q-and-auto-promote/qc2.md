---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-19-v1.52-outline-five-q-and-auto-promote"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (auto-promote gate, authz, audit trail, tx atomicity, injection surface, migration safety)
- Report Timestamp: 2026-06-19

## Scope
- plan_id: 2026-06-19-v1.52-outline-five-q-and-auto-promote
- Review range / Diff basis: b97ec0d9..431aca4c
- Working branch (verified): feature/v1.52-outline-five-q-and-auto-promote
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p0/
- Files reviewed: 18 changed (1327 insertions, 192 deletions)
- Commit range: b97ec0d9..431aca4c
- Tools run:
  - `git diff b97ec0d9..431aca4c --stat`
  - Targeted `git diff` on `kb.rs`, `kb_extract_job.rs`, migration, `quality_loop.rs`, `creator_world_kb.rs`
  - `cargo test -p nexus42 --test creator_world_kb -- adopt_auto_promote` (2 tests: adopt_auto_promote, cross-author 403)
  - `cargo test -p nexus-local-db --lib kb_extract_job::tests::auto_promote_columns_default_to_null_and_record_on_flip`
  - `cargo test -p nexus-orchestration --lib quality_loop::tests::outline_five_q` (4 tests)
  - `cargo clippy --all -- -D warnings` (clean)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S1 (maintainability)**: The `0.95` auto-promote threshold is a literal in `kb_adopt_auto`. Consider extracting to a documented `const AUTO_PROMOTE_CONFIDENCE_THRESHOLD: f64 = 0.95;` with a comment referencing the plan/spec for future tuning rationale.
- **S2 (observability)**: Audit log write failures are intentionally non-fatal (consistent with `kb_reject`). The `--json` output already distinguishes `promoted` vs `skipped`; operators monitoring the directory will see missing logs only via tracing. Acceptable for V1.52 T-A P0, but a future enhancement could add a `log_write_failed` count in the JSON summary if operational visibility becomes a requirement.

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: manual code review + test execution
- Source Reference: `kb_adopt_auto` (kb.rs:892), `mark_auto_promoted_in_tx_with_cas` (kb_extract_job.rs:1095), `outline_five_q_check` (quality_loop.rs:1212), migration `202606190002`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Detailed Security / Correctness Review (per assignment)

### 1. Auto-promote threshold bypass
- Gate conditions are evaluated **before** any DB mutation:
  - `llm_confidence >= 0.95` (explicit float compare)
  - `llm_source_quote` non-empty after trim
  - `source_chapter_id.is_some()`
  - `insert_key_block_in_tx` runs under `ValidationMode::Novel` (enforces `validation_clean`)
  - Duplicate `canonical_name` check inside the same transaction (early rollback + skip)
- No path allows a candidate with `confidence < 0.95`, missing provenance, or validation failure to reach `mark_auto_promoted_in_tx_with_cas`.
- Heuristic (non-LLM) candidates are skipped with reason "no LLM confidence".
- Edge: empty world → `list_pending_for_world` returns empty → zero promotions, clean.
- Edge: LLM worker unavailable at extract time → candidates were never created with high confidence; auto-promote path never calls LLM.

### 2. Authorization
- `require_world_owner(pool, world_id, creator_id).await?` is the **first** statement in `kb_adopt_auto`, identical to the single-adopt path and to `kb_reject`.
- Cross-author attempt yields `CliError::Api { status: 403, message: containing WORLD_KB_FORBIDDEN_CODE }`.
- Test `adopt_auto_promote_cross_author_returns_403` asserts exactly this behavior.
- Ownership is resolved against `narrative_worlds.owner_creator_id` (world-scoped, per entity-scope-model §1.2/§5.1). No regression on `works.creator_id` resolution for this surface.

### 3. Audit trail integrity
- `auto_promoted_at`, `auto_promoted_reason`, `auto_promoted_by` are written in the **same UPDATE** that flips `promotion_status = 'confirmed'` inside `mark_auto_promoted_in_tx_with_cas`.
- The UPDATE is guarded by `WHERE ... promotion_status = 'pending' AND version = ?` (CAS).
- If the CAS UPDATE affects 0 rows, the tx is rolled back before commit; no partial state.
- `auto_promoted_by` is constructed as `format!("nexus42:cli:kb-adopt-auto:{creator_id}")` from the **authenticated** `creator_id` parameter, not from any field in the candidate row. Spoofing via crafted `kb_extract_jobs` row is impossible.
- Concurrent calls: the CAS + `version` increment serializes promotions for the same row. Second caller will see either "no longer pending" or `VersionMismatch`.
- Columns are nullable in the schema; freshly inserted rows correctly default to NULL (test-verified).

### 4. SQL injection / path traversal
- All SQL uses parameterized queries (`sqlx::query(...).bind(...)`); no string concatenation into SQL.
- Audit log path: `Works/<work_ref>/Logs/kb/auto-promoted/<date>-<job_id>.md`
  - `work_ref` is resolved via the existing `resolve_work_ref_for_log` helper (hardened in V1.50 for R-V150KBED-05) which reads `works.story_ref`.
  - `job_id` is the DB PK (opaque string from prior extraction); used only as filename component after the directory is created with `create_dir_all`.
  - No user-controlled path segments are accepted.
- `resolve_work_ref_for_log` returns `Err` (aborting the log write) rather than falling back to `work_id` when `story_ref` is absent — consistent with the reject path.

### 5. Transaction boundaries
- Per-candidate: `pool.begin()` → `store.insert_key_block_in_tx` → `mark_auto_promoted_in_tx_with_cas` → `commit`.
- On any error (duplicate, validation failure, CAS failure, commit failure) → `tx.rollback().await.ok();` then record skip reason and `continue`.
- INSERT success + CAS failure → full rollback (no orphan KeyBlock, no flipped row).
- The CAS pre-image check (`expected_version`) is passed from the row read in the same loop iteration; TOCTOU between list and adopt is closed by the version guard.

### 6. LLM judge injection (outline 五问)
- The `outline_five_q_check` function is a **pure deterministic heuristic** on the author-supplied outline string.
- It performs simple string contains / line counting / suffix checks. No LLM output is parsed for the gate decision in this code path.
- The preset's `llm_judge` (in `outline-exit.md`) is a separate, later stage; its verdict is not used to drive auto-promote.
- Dimension scores are booleans derived from the input text the author controls; there is no "crafted outline response" that can cause auto-promotion of a KB candidate.
- Pacing bound `(80..=2000)` chars is a simple range check.

### 7. Migration risk
- `202606190002_kb_extract_jobs_auto_promote.sql` contains only three `ALTER TABLE ... ADD COLUMN` statements.
- All new columns are nullable `TEXT` with no `NOT NULL` / `DEFAULT` requirements.
- Existing rows (pending/confirmed/rejected) remain valid; columns default to NULL.
- No data backfill, no constraint changes, no index changes.
- The Rust struct `KbExtractPromotion` adds the fields with `#[serde(skip_serializing_if = "Option::is_none")]`.
- All SELECT sites were updated to project the new columns (compile-time verified by sqlx in tests).

## Verification Evidence
- All targeted tests passed (see Tools run above).
- `cargo clippy --all -- -D warnings` produced zero diagnostics.
- Manual inspection of the CAS UPDATE, auth gate ordering, eligibility checks, and work_ref resolution confirms the design matches the stated invariants.

## Revalidation Notes
N/A (initial tri-review wave).

---

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: V1.52 T-A P0 tri-review (qc2) — security/correctness focus on auto-promote and outline 五问 gate
**Status**: Done
**Scope Delivered**: Full review of diff range b97ec0d9..431aca4c in worktree `.worktrees/v1.52-ta-p0/`. Executed required test matrix and clippy. Manual analysis of threshold bypass, authz, audit integrity, tx boundaries, injection surfaces, and migration. Produced qc2.md report.
**Artifacts**:
- Report: `.mstar/plans/reports/2026-06-19-v1.52-outline-five-q-and-auto-promote/qc2.md`
- Git commit of report (see below)
**Validation**:
- `cargo test` (targeted) × 3 suites: all green
- `cargo clippy --all -- -D warnings`: clean
- No Critical or Warning findings
**Issues/Risks**: None blocking. Two low-impact Suggestions recorded.
**Plan Update**: N/A (reviewer does not mutate plans)
**Handoff**: Report committed per workflow. PM may now proceed to consolidate or targeted re-review if other reviewers raise items.
**Git**: (populated after commit)
