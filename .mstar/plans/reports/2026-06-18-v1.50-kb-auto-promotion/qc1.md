---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-18-v1.50-kb-auto-promotion"
working_branch: "feature/v1.50-kb-auto-promotion"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion"
review_range: "merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..8eec12e5dac2a023a4b4115483505534119c630c"
verdict: "Approve"
generated_at: "2026-06-17T11:19:24Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-17T11:19:24Z

## Scope
- plan_id: 2026-06-18-v1.50-kb-auto-promotion
- Review range / Diff basis: merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..8eec12e5dac2a023a4b4115483505534119c630c
- Working branch (verified): feature/v1.50-kb-auto-promotion
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion
- Files reviewed: 14 (3 new source files + 1 migration + 3 test files + 7 modified)
- Commit range (identical to Review range): 0ea2995f..8eec12e5 (4 feature commits: c616dc11, 841ec302, 13494027, 8eec12e5)
- Tools run: `git diff`/`git show`, `cargo clippy --all -- -D warnings` (clean), `cargo +nightly fmt --all --check` (clean), `cargo test` for 3 suites + quality_loop unit tests (26 tests total, all pass)

### Alignment context (per Assignment scope)

- **Iteration compass**: `.mstar/iterations/v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md` §0.1 decisions 6, 16 (review-time-only trigger; state model `manual|pending|confirmed|rejected`).
- **Primary spec**: `.mstar/knowledge/specs/entity-scope-model.md` §5.5 (Draft V1.50) — promotion state machine §5.5.1–§5.5.5.
- **Plan**: `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md` (T1–T8 all marked done).

## Findings
### 🔴 Critical

_None._

### 🟡 Warning

_None._

### 🟢 Suggestion

#### S-001 — `work_entry_id` semantic overload for promotion rows
- **Location**: `crates/nexus-local-db/src/kb_extract_job.rs:587` (`insert_pending` binds `work_entry_id = canonical_name_guess`).
- **Observation**: To reuse the V1.29 unique index `idx_kb_extract_jobs_idempotent` on `(creator_id, work_entry_id, world_id) WHERE status NOT IN ('failed')` as a DB-level guard against duplicate promotion candidates, `insert_pending` stores the `canonical_name_guess` in the `work_entry_id` column (which semantically means "work-scope KB entry ID" in the V1.29 schema). The overload is documented in a code comment at line 570–575 and the `KbExtractPromotion` struct does not expose `work_entry_id`, bounding the confusion at the API boundary. However, a future developer inspecting raw `kb_extract_jobs` rows (e.g. via `sqlite3` or the V1.29 `list_extract_jobs` CLI) will see canonical names in the `work_entry_id` column for promotion rows — a schema-level surprise that is only discoverable by reading the `insert_pending` comment.
- **Recommendation**: When T-B P2 (refreshable scan) or V1.51+ touches this table, add a dedicated unique index such as `UNIQUE (work_id, canonical_name_guess) WHERE promotion_status IN ('pending','confirmed')` and stop overloading `work_entry_id`. Not blocking for V1.50 — the current guard is correct and the struct encapsulation prevents API-level confusion.
- **Disposition**: defer to T-B P2 / V1.51+.

#### S-002 — Misleading error message in `kb_adopt` race branch
- **Location**: `crates/nexus42/src/commands/creator/world/kb.rs:480–488`.
- **Observation**: The adopt flow inserts the `KeyBlock` first (line 472 `store.insert_key_block(kb)`), then flips the promotion row via `mark_confirmed` (line 480). If `mark_confirmed` returns `Ok(false)` (the row was already confirmed/rejected by a concurrent operation), the error message reads `"KeyBlock was not duplicated."` — but the KeyBlock **was** already inserted at line 472 and remains in the world. The comment at lines 477–479 correctly acknowledges this ("the KeyBlock insert above is still valid"), but the user-facing message contradicts the actual behavior. This branch is **not** covered by tests: `double_adopt_is_rejected` exercises the sequential case where the second adopt fails early at `load_pending_candidate` (line 645, "not pending"), never reaching the insert-then-flip path. The race branch is only reachable under true concurrent CLI invocations on the same candidate (narrow for a single-author local tool), and the orphan KeyBlock is a valid `confirmed` block (not corruption).
- **Recommendation**: Reword the error to reflect reality, e.g. `"Candidate '{id}' state was already resolved (race). A KeyBlock was still created and is visible in 'creator world kb list'."` Optional: add a `#[cfg(test)]` race-flavored test that pre-flips the row before the adopt call to cover the branch. Not blocking — the trigger is narrow and the impact is bounded.
- **Disposition**: defer (low probability, bounded impact); reword on next touch.

#### S-003 — Heuristic is English-only (Title Case regex)
- **Location**: `crates/nexus-orchestration/src/quality_loop.rs:69–77` (`capitalized_phrase_regex`).
- **Observation**: The regex `\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,3})\b` matches Title Case English/Latin noun phrases. Non-English prose without capitalization (e.g. Chinese, Japanese, Korean fiction) will yield zero candidates. The stopword list is also English-only. This is consistent with compass §0.1 decision 6 (V1.50 ships heuristic-only extraction; LLM-driven extraction is deferred to V1.51+) and the novel profile's English-fiction bias, but is worth recording so the V1.51 LLM-extraction roadmap explicitly covers CJK and other scripts.
- **Recommendation**: Note in the T-B P2 / V1.51+ plan that LLM extraction must replace this heuristic for non-English profiles. No V1.50 action.
- **Disposition**: defer to V1.51+ (LLM extraction).

#### S-004 — `adopt` does not surface a `--block-type` override
- **Location**: `crates/nexus42/src/commands/creator/world/kb.rs:105–111` (`Adopt` variant) and line 454 (`parse_block_type_cli` consumes `block_type_guess` with no CLI override).
- **Observation**: The heuristic defaults every candidate to `block_type_guess='character'` (R-V150KBED-01). The `adopt` command has no `--block-type` flag, so an author adopting a non-character candidate (place, organization, item) must adopt it as `character` and then call `creator world kb edit` to correct the type — two steps where one would do. The spec §5.5.3 also lists an optional `--with-merge` flag (merge into existing KB row) that is not implemented; the plan §2 Goals did not require it, so its absence is in-scope, but the missing `--block-type` override is a small UX gap that compounds R-V150KBED-01.
- **Recommendation**: When addressing R-V150KBED-01 (V1.51+ LLM extraction), add `--block-type <wire-value>` and optionally `--with-merge` to `Adopt` in the same change. Not blocking for V1.50.
- **Disposition**: defer to V1.51+; couples with R-V150KBED-01.

## Source Trace
- **Finding ID**: S-001
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `crates/nexus-local-db/src/kb_extract_job.rs:570–598` (`insert_pending`); `crates/nexus-local-db/migrations/20260527_kb_extract_jobs.sql:25–27` (unique index definition)
  - Confidence: High
- **Finding ID**: S-002
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:472–488` (insert-then-flip ordering); `crates/nexus42/tests/world_kb_promotion_cli.rs:242–263` (`double_adopt_is_rejected` covers early gate, not race branch)
  - Confidence: High
- **Finding ID**: S-003
  - Source Type: git-diff + doc-rule
  - Source Reference: `crates/nexus-orchestration/src/quality_loop.rs:69–77, 84–163`; compass §0.1 decision 6
  - Confidence: High
- **Finding ID**: S-004
  - Source Type: git-diff + doc-rule
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:105–111, 454`; entity-scope-model §5.5.3; R-V150KBED-01 in plan
  - Confidence: High

## Reviewer-perspective assessment (architecture coherence + maintainability)

### Deviation 1 — column `promotion_status` (not `status`) — **clean**
The existing `kb_extract_jobs.status` column carries the V1.29/V1.40 extraction-queue CHECK `('queued','running','done','failed')`. SQLite cannot ALTER an existing CHECK in place, so reusing `status` for the promotion lifecycle would either (a) conflate two orthogonal lifecycles in one column (schema smell) or (b) require a table rebuild. The new `promotion_status TEXT NOT NULL DEFAULT 'pending' CHECK (promotion_status IN ('pending','confirmed','rejected'))` column is the pragmatic choice and **cleanly maps** to entity-scope-model §5.5.1:
- `pending` ↔ §5.5.1 `pending` (review extracted, awaiting confirm)
- `confirmed` ↔ §5.5.1 `confirmed` (adopted into KB)
- `rejected` ↔ §5.5.1 `rejected` (dismissed, archived)

The `manual` state from §5.5.1 is not represented as a `promotion_status` value because manual rows are inserted directly as `KeyBlock`s via `creator world kb edit` (T-B P0) — they never pass through `kb_extract_jobs`. This is consistent with §5.5.5 ("governs **how** a row enters the World, not **what** it contains") and the §5.5.2 transition diagram (`manual → confirmed | rejected` is the direct-insert path, outside the promotion table).

**Consumer confusion check**: the V1.29/V1.40 extraction consumers (`KbExtractJob` struct + `enqueue`/`claim_job`/`mark_running`/`mark_done`/`mark_failed`/`next_queued`) are fully separated from the V1.50 promotion consumers (`KbExtractPromotion` struct + `insert_pending`/`list_pending_for_world`/`get_promotion`/`mark_confirmed`/`mark_rejected`/`is_idempotent`). The two structs share no fields that would let a promotion consumer accidentally read the queue `status` or vice versa. Promotion rows are inserted with `status='done'`, so the V1.40 `kb.extract_work` worker (which claims `status='queued'` rows via `next_queued`) will never pick them up. No consumer confusion found. (See S-001 for the `work_entry_id` overload, which is the only residual schema-level surprise.)

### Deviation 2 — reuse `work_id` (not new `source_work_id`) — **clean**
The V1.40 P3 `work_id` column (TEXT, nullable) already carries the source-work semantics. The plan §2 Goal 1 mentioned `source_work_id` as a candidate name, but reusing `work_id` avoids a redundant column. I checked `entity-scope-model.md` §5.5 and the compass §0.1: neither document names `source_work_id` as a future field. The spec §5.5.1–§5.5.5 refers only to "the work" abstractly. No contract or doc mentions `source_work_id`. The deviation is semantically equivalent and documented in the migration header. Clean.

### T-A P2 coordination — **well-documented**
The `// COORDINATE-WITH-T-A-P2` contract is documented in **three** places:
1. `crates/nexus-orchestration/src/quality_loop.rs:25–33` (module doc explaining the hook fires for any `novel-review-master` schedule reaching the supervisor, including the V1.39 stale-findings path and manual `creator run`, so the pipeline is independently testable before T-A P2 wires the per-Work `review` cron role).
2. `crates/nexus-orchestration/src/schedule/supervisor.rs:469–487` (inline comment at the hook call site, explaining the non-fatal semantics and the T-A P2 dependency).
3. `crates/nexus-orchestration/src/preset_ids.rs:38–53` (`NOVEL_REVIEW_MASTER_PRESET_ID` doc lists all three consumers: `enqueue_review_master_schedule`, `extract_kb_candidates_for_review`, `ScheduleSupervisor::on_schedule_terminal`).

The contract is clear enough for the T-A P2 implementer: wire the per-Work `review` cron role to enqueue `novel-review-master` schedules, and the hook in `quality_loop.rs` will fire on those schedules' terminal transitions. The hook is gated on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID` (not on a T-A-P2-specific schedule flag), so it is decoupled from the cron wiring and will work unchanged once T-A P2 lands.

### Heuristic extraction — **cleanly replaceable**
`extract_candidates_from_text` is a pure function (`&str → Vec<KbCandidate>`) with no I/O, making it trivially replaceable by an LLM extraction call in V1.51+. The call site in `extract_kb_candidates_for_review` (line 270) is a single line; the rest of the hook (load context, filter existing names, idempotency guard, persist pending) is LLM-agnostic and will work with any candidate source. The `MAX_CANDIDATES_PER_PASS = 20` cap and the `is_idempotent` guard are independent of the extraction method. Good separation.

### Module reuse — **no parallel surface**
- Reuses the T-B P0 `creator world kb` module (`crates/nexus42/src/commands/creator/world/kb.rs`) — the new `Pending`/`Adopt`/`Reject` variants are added to the existing `WorldKbCommand` enum, not a new CLI group. Consistent with §5.5.1 "Visible to author via" column which lists `creator world kb pending`.
- Reuses the V1.40 P1 `SqliteKbStore::with_validation_mode(ValidationMode::Novel)` for body validation on adopt (line 464) — per §5.5.5, the promotion state machine does not change `BlockType` or `ValidationMode` constraints. Correct.
- Reuses the existing `WORLD_KB_FORBIDDEN_CODE` (`WORLD_KB_FORBIDDEN`) from T-B P0 for the cross-author 403, satisfying AC3. No new error code proliferation.
- Reuses the V1.39 `novel-review-master` preset id (now promoted to a `pub const` in `preset_ids.rs` with a frozen-value guard test).

### Surgical changes — **confirmed**
The diff is +2197/−13 across 14 files. The only files touched outside the new T-B P1 surface are:
- `crates/nexus-local-db/src/lib.rs` (+4 lines: re-exports for the new DAO functions, following the existing alias convention).
- `crates/nexus-orchestration/src/lib.rs` (+1 line: `pub mod quality_loop;`).
- `crates/nexus-orchestration/src/preset_ids.rs` (+22 lines: new `pub const` + frozen-value test — the const was previously inlined as a string literal).
- `crates/nexus-orchestration/src/schedule/supervisor.rs` (+34 lines: the hook call site, clearly fenced with V1.50 T-B P1 comments).
- `Cargo.lock` (+1 line: `regex` dependency for `nexus-orchestration`, already a workspace dep).
- `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md` (plan status + completion report — harness artifact).

No unrelated refactoring, no piggyback changes, no handwritten DTO parallel to contracts. The change set matches the plan §5 task list (T1–T8) exactly.

## Verification (re-run by reviewer)

| Check | Command | Result |
|-------|---------|--------|
| AC1 migration | `forward_migration_adds_promotion_columns` + `promotion_status_defaults_to_pending_with_check_constraint` | ✅ pass (CHECK rejects 'bogus') |
| AC2 review-time hook | `ac2_extraction_inserts_pending_candidates` | ✅ pass (≥1 candidate inserted for review-master schedule) |
| AC3 CLI round-trip + 403 | `pending_cross_author_returns_403`, `adopt_cross_author_returns_403`, `reject_cross_author_returns_403` | ✅ pass (all assert `WORLD_KB_FORBIDDEN`) |
| AC4 adopt → confirmed KeyBlock | `adopt_creates_confirmed_key_block` | ✅ pass (KeyBlock `status='confirmed'` via `SqliteKbStore`) |
| AC5 reject → log + rejected | `reject_marks_rejected_and_writes_log` | ✅ pass (`promotion_status='rejected'` + audit log file exists) |
| AC6 idempotency | `ac6_rerun_does_not_duplicate_pending` + `idempotency_guard_blocks_duplicate_pending` + `idempotency_survives_confirm_but_not_reject` + `reject_allows_re_extraction` | ✅ pass (second run inserts 0; `is_idempotent` correctly handles pending/confirmed/rejected transitions) |
| AC7 nightly fmt | `cargo +nightly fmt --all --check` | ✅ clean (exit 0) |
| AC8 clippy --all | `cargo clippy --all -- -D warnings` | ✅ clean (no warnings) |

Test totals: `kb_extract_jobs_migration` 7/7, `review_time_extraction` 5/5, `world_kb_promotion_cli` 8/8, `quality_loop` unit tests 6/6 → 26/26 pass.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: **Approve**

No Critical or Warning-level unresolved findings. All four Suggestions are deferred to T-B P2 / V1.51+ with clear disposition; two of them (S-003, S-004) are already tracked as plan residuals R-V150KBED-01 and R-V150KBED-02. The two deviations from plan §5 (column name `promotion_status`, reuse of `work_id`) are well-documented in the migration header and the Completion Report, map cleanly to the §5.5 state machine, and introduce no consumer confusion (verified by grep across all `kb_extract_job` consumers). The architecture is coherent, the heuristic is cleanly replaceable by LLM extraction, the T-A P2 coordination contract is documented in three places, and module reuse (T-B P0 CLI group, V1.40 P1 Novel validation, V1.39 preset id) follows the "prefer reuse over parallel logic" guideline. Changes are surgical and match the plan task list exactly.
