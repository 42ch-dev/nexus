---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-10-v1.40-world-kb-extract-binding"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk
- Report Timestamp: 2026-06-10T22:15:00Z

## Scope
- plan_id: 2026-06-10-v1.40-world-kb-extract-binding
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-kb-extract-binding (equivalently b172dfa5..<HEAD>)
- Working branch (verified): feature/v1.40-world-kb-extract-binding
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 16 (implementation + prior QC reports + DF tracker; focused on 11 core changed files)
- Commit range: b172dfa5..22192833 (13 commits total; 7 implementation + fixes + 2 prior QC + 1 docs + fmt/clippy)
- Tools run: git diff, git log, git grep, git rev-parse, manual source review (read full key modules), grep for isolation/parameterized patterns

## Findings

### 🔴 Critical
(None)

### 🟡 Warning

#### W-001: Dead code — `build_child_kb_extract_schedule` defined but never invoked (carry-forward from QC #1)
- **File**: `crates/nexus-orchestration/src/stage_gates.rs:276-320` (added in T8)
- **Evidence**: `git grep -rn "build_child_kb_extract_schedule"` (restricted to review range) returns only the definition. No call sites in Rust, YAML presets, or tests. T8 plan text and commit message claim "schedule.enqueue_child + novel-review-master sync_world_kb", but the actual implementation in `novel-review-master/preset.yaml:88-103` drives `kb.extract_work` directly via a preset `enter` capability action (no schedule, no `depends_on`, no `enqueue_child`). The dead function also hard-codes `profile_hint`/`source_kind` that are already supplied in the preset YAML call.
- **Impact (correctness + maintainability)**: 76 lines of unused code will trigger `dead_code` under pedantic clippy. More importantly, it documents an architecture (child schedule with parent dependency) that was not delivered; future maintainers will be confused about the intended orchestration model for World KB promotion after review.
- **-> Fix**: Either (a) delete the dead function and update plan/DF-63 to reflect that `sync_world_kb` uses direct capability invocation from the preset state machine, or (b) implement a real `schedule.enqueue_child` capability (register it, wire it from the preset or from a post-`await_decision` hook), pass the parent schedule id, and use `depends_on` for ordering. If (b), also add cycle/parent validity checks in the scheduler.

#### W-002: `sync_world_kb` state in novel-review-master is not a no-op for legacy V1.39 worldless Works (carry-forward from QC #1 W-002; "fix" commit 5c3b4c01 only touched test)
- **File**: `crates/nexus-orchestration/embedded-presets/novel-review-master/preset.yaml:88-103`
- **Evidence**: The state unconditionally executes an `enter` capability action calling `kb.extract_work` with `world_id: "{{preset.input.world_id}}"`. When the Work is worldless (V1.39 legacy, `preset.input.world_id` absent), Handlebars renders this as the empty string `""`. No `{{#if}}` guard, no `Conditional` enter action, and no early `exit_when` that would short-circuit before the capability call. The plan text says "No-op for worldless". Commit 5c3b4c01 ("update review-master test for sync_world_kb state") updated test expectations but left the preset YAML unchanged. The subsequent state `exit_when: kind: rule` (always-true) is never reached if the capability errors.
- **Impact (correctness + data integrity)**: Worldless Works will hit the capability, which will attempt to create a `KeyBlock` with `world_id == ""`. This will fail (either validation, FK on worlds table, or downstream KeyBlock uniqueness). The review flow will surface an error instead of completing cleanly to `done`. Violates the mandatory binding semantics ("legacy V1.39 worldless Works skip World promotion — no orphan KeyBlocks").
- **-> Fix**: Add an explicit guard before the capability call so that absent/empty `world_id` produces a true no-op (either a Handlebars conditional around the enter action if supported, a `Conditional` action kind that short-circuits, or make the capability itself return success/no-op for empty world_id while still marking the parent review job complete). Update the test to cover the worldless skip path through `sync_world_kb`.

#### W-003: Runtime `sqlx::query_as` + shared `JOB_COLUMNS` string replaces compile-time checked `query_as!` macros (carry-forward from QC #1 W-003)
- **File**: `crates/nexus-local-db/src/kb_extract_job.rs:57-91` (JOB_COLUMNS + fetch_* helpers) and all call sites (lines 162-172, 217-225, 269-277, 292-300, 342-349, etc.)
- **Evidence**: The refactor (T2) introduces a 14-column `JOB_COLUMNS` const and does `sqlx::query_as::<_, KbExtractJob>(&format!("SELECT {JOB_COLUMNS} ...")) .bind(...)`. All SELECT paths (enqueue idempotency check, get, list, next_queued, claim_job) now use this. `nexus-local-db/AGENTS.md` (and the daemon-runtime reference it points to) states: "Compile-time checked queries only — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment." SAFETY comments are present but do not restore compile-time column/type verification.
- **Impact (correctness + future schema safety)**: A future migration that adds/renames a column will not be caught at compile time; it will surface as a runtime deserialization or "no such column" error during job queue operations. The previous `query_as!` pattern with explicit `as "field!"` aliases gave stronger guarantees. The stated rationale (avoid drift across many similar queries) is reasonable but the chosen mitigation violates the crate rule.
- **-> Fix**: Revert to `sqlx::query_as!()` with explicit per-query column lists (accepting minor duplication), or add an explicit documented exception in `nexus-local-db/AGENTS.md` + `nexus-daemon-runtime/AGENTS.md` with justification and a requirement that integration tests (the new e2e + claim_job tests) cover every query path. The latter is acceptable only if the tests are hermetic and run on every schema change.

#### W-004: CLI `--chapter N` sugar performs no range validation and does not resolve a real `body_path`
- **File**: `crates/nexus42/src/commands/creator/kb.rs:825-836` (inside `kb_queue_extract`)
- **Evidence**: When `--chapter N` is supplied, the code does:
  ```rust
  let locator = format!("chapter:{ch_label}");
  (Some("work_chapter"), Some(locator), Some("novel"))
  ```
  No check that `N >= 1` (or even `N > 0`). `chapter:00` or `chapter:-1` are accepted and written to `source_locator`. The comment says "Best-effort... The exact path is resolved later by the capability from work_chapters." However, neither `kb.extract_work` nor `finalize_extract` nor the preset call site performs any lookup in a `work_chapters` table to turn the locator into a real filesystem path or excerpt; the locator string is passed straight into `SourceAnchor::from_excerpt(source_locator)`.
- **Impact (correctness + UX)**: Users can queue nonsensical chapter numbers. The resulting `SourceAnchor` will contain a synthetic "chapter:NN" string rather than actual chapter text or a verifiable path, weakening provenance. Plan T6 description ("resolves body_path") is not delivered.
- **-> Fix**: Add `if chapter < 1 { return Err(...) }`. Either (a) implement real resolution against the work's chapter list here (preferred for the sugar), or (b) change the locator format to something the downstream can actually use (e.g. a stable chapter id) and document that body content is supplied by the orchestrator at prompt time. Update the plan/DF tracker if the resolution was intentionally deferred.

#### W-005: `kb.extract_work` does not re-validate creator/workspace ownership when an explicit `job_id` is supplied
- **File**: `crates/nexus-orchestration/src/capability/builtins/kb_extract_work.rs:209-236` (Phase 1)
- **Evidence**:
  - If `job_id` present: `nexus_local_db::get_extract_job(...)` then status check. No assertion that `job.creator_id == input_creator_id` or that the caller's workspace matches `job.workspace_id`.
  - If `job_id` absent: it calls `next_queued_extract_job(pool, creator_id)` (creator-scoped claim).
  - The job row carries both `creator_id` and `workspace_id`; the capability input only requires `creator_id`.
- **Impact (security + correctness)**: A caller who can supply (or guess) a `job_id` belonging to another creator could drive extraction for that job (the prompt phase would return the job's world/work_entry, and the finalize phase would insert a KeyBlock into the job's world). While job_ids are not currently exposed in a way that makes this trivial, the missing ownership check after load is a latent cross-creator / cross-workspace isolation hole. Enqueue paths (CLI + orchestration) do enforce creator/workspace at insert time, but the execution path does not re-enforce on the explicit-id fast path.
- **-> Fix**: Immediately after loading the job by id, add:
  ```rust
  if job.creator_id != creator_id {
      return Err(CapabilityError::InputInvalid("job creator mismatch".into()));
  }
  ```
  Optionally also surface and check workspace when the capability is called in a workspace-aware context. Add a test that attempts a cross-creator job_id claim and expects InputInvalid.

#### W-006: `kb.extract_work` marks the job `done` *before* `finalize_extract` / `insert_key_block` succeeds (QC #3 S-002 confirmed)
- **File**: `crates/nexus-orchestration/src/capability/builtins/kb_extract_work.rs:361-380` (Phase 4b / 5)
- **Evidence**: Explicit sequence:
  ```rust
  nexus_local_db::mark_extract_job_done(pool, &job_id).await?;   // <--- done first
  let insert_result = nexus_kb::finalize_extract(&store, finalize_input).await
      .map_err(|e| { tracing::error!(... "KeyBlock insert failed after job marked done — extraction content lost"); ... })?;
  ```
  The comment above it explains the rationale (if mark_done fails we want no orphan KeyBlock; a done job without a block is "recoverable").
- **Impact (correctness + data loss risk)**: On any failure inside `finalize_extract` (P1 validation error on canonical_name/novel_category, Duplicate, store I/O error, etc.) the job is left in `done` state with no corresponding KeyBlock. There is no automatic transition back to `failed`, no DLQ, and the only signal is a tracing error log. Subsequent `sync_world_kb` or manual retry will see a done job and skip re-extraction. The KeyBlock content from the (expensive) LLM extraction is lost for that job.
- **-> Fix**: Preferred: move the `mark_done` *after* successful insert (accept the small window where a running job could have an orphaned KeyBlock on crash). Alternative: on insert failure, call `mark_failed` with the error and propagate so the preset state machine can surface it to the user or retry. At minimum, add a periodic "done jobs with no KeyBlock" audit query + alert, and document the recovery procedure (re-enqueue a new job for the same work_entry+world).

#### W-007: `sync_world_kb` passes the literal string `"auto"` as `work_entry_id`; plan promised `schedule.enqueue_child` with parent dependency
- **File**: `crates/nexus-orchestration/embedded-presets/novel-review-master/preset.yaml:95` + `kb_extract_work.rs:226-236`
- **Evidence**: Preset supplies `work_entry_id: "auto"`. In the capability, absence of `job_id` causes a call to `next_queued_extract_job(creator_id)` (the `work_entry_id` value is never read in the claim path). No code treats the token `"auto"` specially. The T8 implementation commit and plan text describe `schedule.enqueue_child` + `depends_on` so the child kb-extract runs *after* the review-master parent completes. The delivered mechanism is a direct capability call inside the preset state (no schedule, no dependency edge, no separate enqueue).
- **Impact (correctness + architectural drift)**: The state machine will attempt extraction immediately upon entering `sync_world_kb` (subject to the world_id guard). There is no parent-child scheduling relationship, no visibility in the schedule list, and no failure propagation back to the parent review flow. The `"auto"` value is noise that will confuse anyone reading the preset or logs.
- **-> Fix**: Align plan vs. implementation. Either (a) implement the real child-schedule path using (or fixing) `build_child_kb_extract_schedule` + a scheduler that respects `depends_on`, and call it from a post-decision hook instead of a preset state, or (b) update the plan, DF-63, and preset comments to clearly describe the chosen design (direct capability invocation from the review-master preset after decisions, with the world_id guard providing the legacy skip). Remove the magic `"auto"` or document what it means.

### 🟢 Suggestion

#### S-001: `enqueue_with_artifact` returns `Pin<Box<dyn Future>>` via `Box::pin(async move { ... })` (carry from QC #1 S-001)
- **File**: `crates/nexus-local-db/src/kb_extract_job.rs:204-244`
- **-> Fix**: Convert to a normal `async fn` for consistency with the sibling `enqueue` function unless there is a documented reason for the indirection (e.g., future trait object requirements).

#### S-002: `world_refs_validate` module exported from `builtins/mod.rs` but never registered in `CapabilityRegistry` (pre-existing, noted by QC #3)
- **File**: `crates/nexus-orchestration/src/capability/builtins/mod.rs:41`
- **-> Fix**: Register or remove the export. Out of scope for this plan; track as residual if desired.

#### S-003: `SourceAnchor` for chapter flow receives the locator string via `from_excerpt` (semantics may be stretched)
- **File**: `kb_extract_work.rs:348-350` and `extract_finalize.rs:84-86` (test usage)
- **Evidence**: When `source_locator` is present, `SourceAnchor::from_excerpt(source_locator)` is used. `from_excerpt` was originally intended for a short text excerpt of the source material. For the artifact-locator case it now holds a synthetic path-like string (`chapter:03` or `Works/.../03.md`). This is not wrong for provenance, but it may surprise code that later renders or searches anchors expecting human-readable excerpts.
- **-> Fix**: Consider adding a `SourceAnchor::from_artifact_locator(kind, locator)` constructor (or a dedicated field) and update documentation. Or keep the current approach and add a comment that "excerpt" is overloaded for locator mode.

#### S-004: No test coverage visible for duplicate chapter extraction (same chapter → idempotent or Duplicate error)
- **Evidence**: The new e2e (`kb_extract_binding_e2e.rs`) covers happy-path persist→extract→query and the novel_category validation path, but does not re-enqueue the same `(work_entry_id, world_id)` chapter and assert either "existing job returned" (idempotency at enqueue) or "Duplicate KeyBlock" at finalize time.
- **-> Fix**: Add a test case that enqueues the same chapter twice (or finalizes twice with the same canonical_name) and verifies the expected duplicate/idempotent behavior. This would also exercise the `mark_done` before insert path under conflict.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-001 | git-grep + manual-reasoning | `git grep -rn build_child_kb_extract_schedule` (range-limited) + preset.yaml diff showing direct capability | High |
| W-002 | preset diff + prior QC1 report + commit 5c3b4c01 message | `novel-review-master/preset.yaml:88-103` (unconditional enter action); no `if` guard added | High |
| W-003 | crate AGENTS.md rule + diff | `nexus-local-db/AGENTS.md:11` vs `kb_extract_job.rs:57-91` (JOB_COLUMNS + all query_as::<_, > sites) | High |
| W-004 | code review of kb_queue_extract | `crates/nexus42/src/commands/creator/kb.rs:825-836` (format!("chapter:{ch_label}") with no range check) | High |
| W-005 | code review of capability run | `kb_extract_work.rs:209-225` (load by job_id, no creator/workspace re-check) | High |
| W-006 | explicit code + comment | `kb_extract_work.rs:361-380` ("mark job done BEFORE inserting KeyBlock") + error log message | High |
| W-007 | preset YAML + capability claim path | `preset.yaml:95` (`work_entry_id: "auto"`) + `kb_extract_work.rs:226-236` (next_queued path ignores the value) | High |
| S-001 | diff + prior QC | `kb_extract_job.rs:204` (`Box::pin(async move)`) vs sibling `async fn enqueue` | Medium |
| S-003 | manual reasoning on SourceAnchor usage | `kb_extract_work.rs:348` (`from_excerpt(source_locator)`) | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 7 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**:
- Three Warnings (W-001, W-002, W-003) were already raised by QC #1 and remain open in the current diff (the intervening "fix" commit only updated a test; the dead function, the unconditional worldless path in the preset, and the runtime-sqlx pattern are still present).
- Four additional security/correctness Warnings (W-004–W-007) were identified under this review's focus:
  - CLI chapter sugar lacks validation and does not deliver the documented body_path resolution.
  - Explicit `job_id` path in `kb.extract_work` lacks creator/workspace re-validation (latent cross-tenant risk).
  - `mark_done` before `insert_key_block` creates a window for "done job, lost extraction content" with only a log as signal.
  - Plan vs. implementation mismatch on child scheduling (`enqueue_child` + `depends_on` advertised, direct preset capability delivered) plus magic `"auto"` token.
- The schema migration itself is clean (additive only). `WorkFields.world_id` threading and legacy NULL handling are correct. P1 validation is present and unit-tested in `extract_finalize`. E2E test coverage for the happy path is good. These positives are noted but do not outweigh the open blocking items for a security/correctness reviewer.

All prior QC #1 and QC #3 findings that are still relevant have been incorporated or cross-referenced. No new Criticals were found.

## Positive notes (non-blocking)
- Migration is purely additive (`ALTER TABLE ... ADD COLUMN` four times); existing V1.29-era rows receive NULLs for the new columns and continue to work via the legacy `enqueue` path.
- `nexus_kb::finalize_extract` + `validate_canonical_name` / `validate_body(ValidationMode::Novel)` correctly enforce the P1 grammar and `novel_category` requirement with structured errors.
- `WorkFields.world_id: Option<String>` is additive; `build_preset_input` and auto-chain only inject it when present, preserving the V1.39 worldless skip behavior at the persist layer.
- New e2e test (`kb_extract_binding_e2e.rs`) covers the full persist → enqueue-with-artifact → finalize → queryable KeyBlock path, plus novel_category validation and idempotency at the enqueue layer.
- DF-63 tracker was updated to mark the relevant workstream "Shipped".

**Next step recommendation for PM**: Resolve W-001/W-002/W-003 (either by code changes or explicit documented exceptions + test requirements) plus the four new W- items before re-dispatching targeted re-review or moving to QA.

---

**Commit after report write (to be executed)**:
`git -C /Users/bibi/workspace/organizations/42ch/nexus add .mstar/plans/reports/2026-06-10-v1.40-world-kb-extract-binding/qc2.md && git -C /Users/bibi/workspace/organizations/42ch/nexus commit -m "qc(2026-06-10-v1.40-world-kb-extract-binding): QC #2 security-correctness review (verdict: Request Changes)"`
