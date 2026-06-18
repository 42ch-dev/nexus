---
report_kind: qc_review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.51-llm-extraction
verdict: Approve
generated_at: 2026-06-18T14:12:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-18T14:12:00Z

## Scope
- plan_id: 2026-06-18-v1.51-llm-extraction
- Review range / Diff basis: iteration/v1.51...HEAD (= ca494f03...deed03ff)
- Working branch (verified): feature/v1.51-llm-extraction
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p0
- Files reviewed: 25 (per `git diff --stat`)
- Commit range: ca494f03...deed03ff (9 commits, +2292/-91)
- Tools run:
  - `cargo test -p nexus-orchestration -- llm_extract` (15 passed)
  - `cargo test -p nexus-orchestration --test novel_review_master` (3 passed)
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (12 passed)
  - `cargo clippy --all -- -D warnings` (clean)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (unresolved).

### 🟢 Suggestion
- S-001: The review-time extraction hook (`extract_kb_candidates_for_review`) is intentionally best-effort and non-blocking (errors logged, terminal transition still succeeds). This matches the pre-existing review-findings hook pattern and is documented in `quality_loop.rs`. No outer transaction wraps the per-candidate `insert_pending_with_llm` batch. If a future requirement demands atomicity across the entire candidate set for a chapter, an explicit `BEGIN`/`COMMIT` or batched insert would be needed. Current design is acceptable per plan acceptance criteria.
- S-002: `parse_extract_response` + `normalize_candidate` are defensive (code-fence stripping, bare-array fallback, confidence clamp, missing-field defaults). They correctly treat malformed LLM output as "no candidates" (warn-level log). Consider adding a bounded retry or structured error surfacing in a future iteration if LLM noise becomes operationally visible.
- S-003: The `novel-review-master` preset now lists `nexus.llm.extract` in `requires_capabilities`. This is the correct capability-registry gate. No other presets were modified in scope.

## Source Trace

**Primary security/correctness paths reviewed (trace from untrusted chapter text to DB + adopt surface):**

1. **Input surface (untrusted author content)**:
   - Chapter prose is read from the workspace filesystem via `load_chapter_prose` (relative path from `work_chapters.body_path`).
   - Passed as `chapter_prose` (data) into the LLM prompt; never interpolated into executable context or used for control flow / privileged operations.

2. **LLM call contract (worker identity boundary — SEC-V131-01)**:
   - `extract_kb_candidates_for_review` → `extract_via_llm` (or `LlmExtractTask.evaluate`).
   - Capability input is built with **context-injected** `_creator_id` and `_session_id` only.
   - Raw `creator_id`/`session_id` fields from user/preset input are ignored (explicit test: `llm_extract_raw_creator_id_ignored_on_spoof_attempt`).
   - `LlmExtract::run` forwards only the injected identity to `call_acp_prompt(creator_id, session_id, ..., "deny_all")`.
   - Tool policy is `deny_all` (read-only extraction; no side effects).
   - Mirrors the existing `judge.llm` / `LlmJudgeTask` pattern.

3. **Capability registration & preset authority**:
   - `nexus.llm.extract` is registered in all three `CapabilityRegistry` constructors.
   - `novel-review-master` v3 lists it in `requires_capabilities`.
   - Capability name is a constant (`LLM_EXTRACT_CAPABILITY`); not user-controllable at runtime.

4. **DB write safety**:
   - Migration `202606180006_kb_extract_jobs_llm_payload.sql` is **additive only** (`ALTER TABLE ... ADD COLUMN` for `llm_confidence` and `llm_source_quote`; nullable; legacy rows default NULL).
   - DAO: `insert_pending_with_llm` uses fully parameterized `sqlx::query` (11 bind params). No string concatenation.
   - `persist_candidates` calls this for LLM candidates; heuristic path passes `None, None`.
   - Idempotency guard (`is_idempotent`) + existing-name filter run before every insert.
   - No change to the promotion state machine (`pending` → `confirmed`/`rejected`).

5. **Adoption flow (author identity enforcement)**:
   - `creator world kb adopt` path (`kb_adopt` + `check_world_owner`) is unchanged.
   - Still reads `narrative_worlds.owner_creator_id` and compares to the active creator.
   - Returns `WORLD_KB_FORBIDDEN` on mismatch (stable code).
   - `confidence` + `source_quote` are read (dedicated columns first, then JSON fallback) and emitted for display / `--json` only. They do not drive any privileged action or bypass the owner check.

6. **Transaction / partial-failure boundaries**:
   - Each `insert_pending_with_llm` is its own statement (atomic).
   - The hook is explicitly best-effort (documented in plan, code, and `quality_loop` docs). A failure on one candidate does not abort the chapter or the terminal transition.
   - No orphan-row risk beyond the pre-existing design (idempotency + promotion_status guard the pending table).

7. **No new injection / path-traversal / permission surfaces**:
   - Chapter prose is treated as data inside a framed prompt.
   - Workspace paths come from the DB (not user-supplied in this flow).
   - No new filesystem writes, process spawns, or privileged operations in the diff.
   - `source_quote` is a verbatim excerpt stored for audit; it is not executed.

8. **R-V150KBED-01 closure evidence**:
   - `status.json` residual for the predecessor plan shows `lifecycle: resolved`, `closure_evidence` listing specific commits + 29 test names across 5 modules (capability, tasks, quality_loop, migration, adopt tests). Not vague.

**Tests executed (workflow gate)**:
- All 15 `llm_extract` unit tests pass (including SEC-V131-01 spoof test, malformed JSON handling, worker-unavailable fallback).
- All 3 `novel_review_master` E2E tests pass (LLM pathway writes the 4 keys; idempotent; heuristic fallback when no registry).
- All 4 V1.51 migration + DAO round-trip tests pass (`v151_*`).
- Clippy clean.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (unresolved) |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Verdict Reasoning

All security and correctness focus areas required by the assignment were satisfied:

- **LLM prompt injection surface**: Chapter text is data only. The prompt template is internal (hardcoded in the hook + capability). `deny_all` tool policy. No privileged operations are driven by untrusted content.
- **Worker identity boundary**: SEC-V131-01 is mirrored (context-injected `_creator_id`/`_session_id`; raw spoofing rejected; explicit regression test).
- **`novel-review-master` preset authority**: `nexus.llm.extract` correctly listed in `requires_capabilities`.
- **DB write safety**: Parameterized SQL only; additive migration; no unique-index violations introduced; idempotency guard preserved.
- **Capability registry**: Properly registered; not exposed to untrusted callers.
- **`confidence` + `source_quote`**: Display-only in the adopt surface; do not bypass author identity checks.
- **Transaction boundaries**: Single-statement inserts with pre-existing idempotency + best-effort contract (consistent with prior review-time hooks).
- **No injection / path-traversal / permission issues** in the new paths.
- **R-V150KBED-01 closure_evidence**: Concrete (commit hashes + named tests).
- **Adoption flow**: `check_world_owner` (world `owner_creator_id` gate) is untouched and still enforced.

CI gates passed (targeted tests + workspace clippy). The design reuses the proven `LlmJudgeTask` / `judge.llm` pattern with the expected safety seams. No blocking findings.

The three Suggestions are non-blocking observations about existing best-effort semantics and defensive parsing; they do not require changes for this plan.
