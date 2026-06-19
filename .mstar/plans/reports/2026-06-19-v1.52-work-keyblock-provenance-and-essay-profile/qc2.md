---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-19-v1.52-work-keyblock-provenance-and-essay-profile"
verdict: "Request Changes"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-19T

## Scope
- plan_id: 2026-06-19-v1.52-work-keyblock-provenance-and-essay-profile
- Review range / Diff basis: b97ec0d9..09837535
- Working branch (verified): feature/v1.52-work-keyblock-provenance-and-essay-profile
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p2/
- Files reviewed: 47
- Commit range: b97ec0d9..09837535
- Tools run: git diff (targeted), cargo test (scoped), cargo clippy --all, manual code review

## Findings

### 🔴 Critical

**C1: `require_world_owner` does NOT consult `source_work_id` — R-V150KBED-02 remains open in practice**

- **Location**: `crates/nexus42/src/commands/creator/world/kb.rs:1261-1286` (the `require_world_owner` function), and callers at `kb_edit`/`kb_delete` paths.
- **Issue**: The auth gate for edit/delete reads **only** `narrative_worlds.owner_creator_id`. The new `source_work_id` / `source_provenance_kind` columns are **written** during adopt (lines 771-777, 991-993) but **never read** during authorization decisions.
- **Impact**: If a Work is owned by creator A but its World is owned by creator B, a cross-author edit/delete against the KB row will be gated by World owner, not Work owner — violating the explicit design goal stated in the plan ("when KB row has `source_work_id`, the auth gate for edit/delete check `source_work_id`'s owner first").
- **Evidence**:
  - Migration `202606190003_kb_key_blocks_provenance.sql` adds `source_work_id` + CHECK constraint.
  - `kb_adopt` and `kb_adopt_auto` populate `source_work_id` from `candidate.work_id`.
  - **No code path** in `kb_edit`, `kb_delete`, or `require_world_owner` reads `source_work_id` or joins to `works.creator_id`.
  - Existing authz tests (`world_kb_authz.rs`, `world_kb_cli.rs`) only test World ownership, not Work provenance.
- **Deferred items compound risk**: T6/T7/T9 (provenance tests) are explicitly deferred — no test would have caught this.
- **Fix required**: Either (a) implement Work-owner pre-check when `source_work_id` is present (preferred), or (b) document that provenance is **informational only** and the plan's stated security goal is deferred to a future plan.

**C2: `creator bootstrap --profile essay` CLI surface does NOT exist (T13 deferred)**

- **Location**: No implementation in `crates/nexus42/src/commands/creator/` for `--profile essay` or `bootstrap` subcommand.
- **Issue**: Per plan scope and deferred items list, T13 ("creator bootstrap --profile essay CLI surface") is deferred. The essay scaffold capability (`essay.project_scaffold`) exists in orchestration, but there is **no CLI entry point** to invoke it.
- **Impact**:
  - Users cannot create essay-profile Works via the documented CLI.
  - The scaffold's DB PATCH (`UPDATE works SET work_profile = 'essay'`) is unreachable from CLI.
  - Cross-author essay creation attack surface cannot be validated — no surface means no auth gate test.
- **Security implication**: If a future CLI path is added that calls the scaffold with `--author-id <other>`, there is no existing pattern to audit. The scaffold itself has no auth logic (it takes `creator_id` from input JSON).
- **Fix required**: Either implement T13 (with auth gate mirroring Work ownership), or explicitly mark the essay profile as **orchestration-only** (not CLI-exposed) until a later plan.

### 🟡 Warning

**W1: Three `sqlx::query` (runtime) instead of `sqlx::query!` for provenance writes — documented but still a correctness surface**

- **Location**: `crates/nexus-local-db/src/kb_store.rs:174-209` (insert in tx), `335-366` (insert), `376-417` (get_key_block).
- **Issue**: The SAFETY comments correctly note that sqlx compile-time verification cannot see the new columns until the migration is applied. However:
  - Parameter binding is manual (`.bind(&kb.source_work_id)` etc.) — typo or column rename would only be caught at runtime.
  - The `source_provenance_kind` CHECK constraint in the migration (`'manual' | 'review_time_extract' | ... | 'author_explicit'`) is **not enforced at the Rust type level** — any string can be passed.
  - `get_key_block` uses `sqlx::query_as::<_, KeyBlockRow>` (runtime) instead of `query_as!`.
- **Risk**: Medium. The code is correct today, but the pattern increases the chance of future drift when more provenance columns are added.
- **Recommendation**: Add a compile-time test (feature-flagged or integration) that exercises the full round-trip with the new columns once the migration is applied in the test harness. Consider a typed enum for `source_provenance_kind` in Rust that `Display`s to the exact CHECK values.

**W2: `source_provenance_kind` enum coverage is incomplete across surfaces**

- **Location**: Migration CHECK constraint (5 values), `kb_adopt` (sets `review_time_extract` or `manual`), `kb_adopt_auto` (hardcodes `author_explicit`).
- **Issue**:
  - `cross_chapter_rescan` and `finalize_time_extract` are in the CHECK but have **no code path** that writes them in this diff.
  - The plan states "does it cover all extraction paths?" — the answer is "the enum is defined, but only 3 of 5 values are exercised."
  - `author_explicit` is used for `--auto` (which is an LLM path), while manual review-time uses `review_time_extract`. The distinction is subtle and not documented in comments.
- **Impact**: Future extraction surfaces (rescan, finalize) may pick the wrong kind or invent new strings that violate the CHECK.
- **Recommendation**: Add a `#[derive(Display)]` or `FromStr` enum in `nexus-kb` or `nexus-local-db` so the 5 values are the **only** legal strings. Gate the auto vs manual decision behind a clear function.

**W3: Migration `202606190003` has no rollback / idempotency test in the reviewed diff**

- **Location**: `crates/nexus-local-db/migrations/202606190003_kb_key_blocks_provenance.sql`.
- **Issue**:
  - Columns are added as nullable (`TEXT`, `INTEGER`, `TEXT`) with no `DEFAULT` — correct for additive.
  - Index is `CREATE INDEX IF NOT EXISTS` — idempotent on create.
  - No `down` migration or test that applies → inserts with provenance → reverts → verifies old schema still works.
  - The work_profile migration (004) does a full table recreate — higher risk, but the diff shows it copies all rows and recreates indexes.
- **Risk**: If a user has an existing DB with pending KB rows and the migration partially applies, `source_work_id` could be NULL in ways that break future auth logic.
- **Recommendation**: Add a migration roundtrip test (similar to `kb_extract_jobs_migration.rs`) that exercises the provenance columns before/after.

**W4: Essay scaffold writes `work_profile` via raw `sqlx::query` without FK/world validation**

- **Location**: `crates/nexus-orchestration/src/capability/builtins/essay_scaffold.rs:95-100`.
- **Issue**:
  - The scaffold does `UPDATE works SET work_profile = 'essay', work_ref = ? WHERE work_id = ?`.
  - No check that the `creator_id` in input actually owns the `work_id`.
  - `world_id` is optional in input and is **never used** in the scaffold body (only logged).
  - The `work_ref` uniqueness constraint (`idx_works_unique_story_ref`) is not checked before the PATCH — a collision would fail at runtime with a DB error, not a clean `CapabilityError`.
- **Impact**: If the scaffold is ever invoked by a compromised or misconfigured orchestrator with a forged `creator_id`, it can mutate any Work's profile. The current CLI surface is missing (T13), which reduces immediate blast radius, but the capability itself is unauthenticated.
- **Recommendation**: Either (a) pass the calling creator through a trusted context (not user JSON), or (b) add an ownership check inside the capability before the PATCH.

**W5: Cross-author adopt provenance is recorded but not validated**

- **Location**: `kb_adopt:704` calls `require_world_owner` (World check), then writes `candidate.work_id` into `source_work_id`.
- **Issue**: The `candidate` comes from `kb_extract_jobs`, which may have been created by a different creator than the current caller. The code does **not** verify that `candidate.creator_id == creator_id` before adopting.
  - `load_pending_candidate` is not shown in the diff, but the flow allows a world owner to adopt a job that was extracted by someone else.
  - The provenance will record the **extractor's** `work_id`, but the **adopter** is the world owner.
- **Correctness question**: Is this intentional (world owner can "claim" KB from any work in their world)? If so, the provenance semantics ("who authored this KB") are now ambiguous.
- **Recommendation**: Clarify in entity-scope-model whether adopt provenance records the **extractor** or the **adopter**. If the latter, the code should overwrite `source_work_id` with the adopter's active work, not the candidate's.

**W6: Auto-promote audit log path uses `work_ref` resolved from `works.story_ref`, but the column may be NULL**

- **Location**: `write_auto_promoted_log:1133-1136`, `resolve_work_ref_for_log` (not fully shown).
- **Issue**: The code does `.unwrap_or_else(|| "unknown-work".to_string())`. If many Works have `story_ref = NULL` (possible per schema), the audit logs will pile up under `Works/unknown-work/...`, which is both a correctness and a potential path traversal / collision risk if an attacker can influence `work_id` values.
- **Recommendation**: Require `story_ref` for Works that participate in KB provenance, or use `work_id` (sanitized) as a fallback directory name with a clear marker.

### 🟢 Suggestion

**S1: Add a helper `provenance_kind_for_adopt(llm_confidence: Option<f64>) -> &'static str`**

Centralize the ternary at `kb_adopt:773` and `kb_adopt_auto:993` so future extraction paths cannot drift.

**S2: Consider a view or helper for "effective owner of a KeyBlock"**

If future code will consult `source_work_id` for auth, a small DB view or Rust helper that joins `kb_key_blocks` → `works` (when present) → `narrative_worlds` would make the precedence rule explicit and testable.

**S3: Document that `source_chapter` is advisory, not a hard FK**

The column is `INTEGER` with no foreign key to `chapters` or `manuscript_chapters`. If a chapter is deleted, the provenance becomes dangling. Add a comment in the KeyBlock struct and the migration.

**S4: The essay scaffold hardcodes templates; consider making them overridable by profile**

The current implementation writes fixed content. If essay profiles later support custom templates (like novel-writing), the scaffold will need a registry. Note for future plan.

## Source Trace
- Finding C1: git diff on `kb.rs` + `require_world_owner`; absence of `source_work_id` reads in edit/delete paths.
- Finding C2: exhaustive search for `bootstrap.*essay` and `--profile` in CLI diff; plan deferred-items list.
- Finding W1: `kb_store.rs` SAFETY comments + manual `.bind` for provenance columns.
- Finding W2: migration CHECK values vs. only 3 written strings in `kb_adopt`/`kb_adopt_auto`.
- Finding W4: `essay_scaffold.rs:95` raw UPDATE with no ownership guard.
- Finding W5: `kb_adopt:699-704` (load then world check, no creator_id match on candidate).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 6 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

## Revalidation Notes (N/A — initial wave)

This is the initial tri-review wave. No prior qc2.md exists for this plan_id.

## Attachments
- None (report-only; no screenshots or artifacts generated during review).
