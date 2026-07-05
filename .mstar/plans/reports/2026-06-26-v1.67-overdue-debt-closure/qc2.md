---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-26-v1.67-overdue-debt-closure"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (transaction atomicity for timeline rename, path validation / traversal resistance in is_script_complete, ownership gate deduplication semantics, script.section_status.update capability scoping and guard, N+1 batch pre-fetch correctness, no wire contract change)
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-26-v1.67-overdue-debt-closure (P2 Overdue Debt Closure — 9 residuals)
- Review range / Diff basis: 138a98fd..ae1b960e (P2 code commits) merged at HEAD; diff basis vs 26e477ee
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed (targeted diff + source):
  - `crates/nexus-local-db/src/narrative_write.rs` (append_event_in_tx + is_world_owned extraction)
  - `crates/nexus-orchestration/src/capability/builtins/timeline.rs` (tx-wrapped append + rename + regression test)
  - `crates/nexus-local-db/src/work_chapters.rs` (resolve_script_work_dir + is_script_complete validation + regression test)
  - `crates/nexus-orchestration/src/capability/builtins/script_section_status.rs` (new capability, validate_work_ref guard, Scripts/ scoping)
  - `crates/nexus-orchestration/src/capability/builtins/world.rs` (batch pre-fetch body_json for world.delta.apply + ownership gate usage)
  - `crates/nexus-orchestration/src/capability/mod.rs` (registry registration of script.section_status.update in all three factories)
  - Supporting: prior qc2 (df46) baseline, plan, commits 1e46085d / 14b8dcc8 / 386169bd / ab3ea216 / 8682486b
- Commit range: 138a98fd..ae1b960e (8 commits)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`
  - `git log --oneline 138a98fd..ae1b960e`
  - `git diff 26e477ee..ae1b960e --stat` + targeted per-file diffs
  - `git show <commit> -- <path>` for each priority item
  - Full reads of changed functions + tests
  - `cargo check -p nexus-orchestration -p nexus-local-db`
  - Grep for is_world_owned / ensure_world_owned / append_event_in_tx / resolve_script_work_dir / validate_work_ref / script.section_status.update / batch pre-fetch patterns

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (all assigned P2 items verified closed with regression tests; no new blocking security/correctness issues introduced).

**Verification of assigned items (priority order):**

- **R-V160P0-QC2-W002 (timeline.event.append rename outside tx)**: CLOSED.
  - Prior state (qc2 df46 W-002): append_event succeeded, then a separate UPDATE renamed the id; failure after insert left orphan row under auto-allocated id.
  - Fix (commit 1e46085d): added `append_event_in_tx` (tx-aware variant) + `append_event_core` generic executor. TimelineEventAppend now does `pool.begin()`, explicit collision check inside tx, `append_event_in_tx(&mut tx, ...)`, conditional UPDATE inside same tx, then `tx.commit()`.
  - Atomicity guarantee: rename cannot succeed without the insert (and vice-versa); on any failure the tx rolls back, no orphan row.
  - Regression test: `timeline_event_append_explicit_id_persists_atomically` — supplies explicit `event_id`, asserts final id matches, and `COUNT(*)==1` for the world (no auto-allocated sibling).
  - Evidence: narrative_write.rs:354-402 (append_event_in_tx), 409-459 (core), timeline.rs:130-170 (tx block + commit), test at end of timeline.rs.

- **R-V160P1-QC2-W001 (is_script_complete path validation)**: CLOSED.
  - Prior state: built `workspace_dir.join("Works").join(work_ref).join(rel_path)` with no canonicalize or boundary check inside the function (trusted caller only).
  - Fix (commit 14b8dcc8): new `resolve_script_work_dir` — rejects empty / \ . .. containing-.. work_ref early; `tokio::fs::canonicalize(workspace_dir)`; builds under `Works/`, canonicalizes result, asserts `starts_with(works_boundary)`; returns None (treated as false) on escape. Defensive: any rel_path with ParentDir component also rejected before read.
  - Regression test: `test_is_script_complete_rejects_path_traversal_in_work_ref` — sets `work_ref='../my-script-escape'`, creates a file outside boundary, asserts `is_script_complete` returns false.
  - Evidence: work_chapters.rs:1602-1649 (resolve helper), 1675-1720 (call site + rel_path guard), 3447-3480 (test).

- **R-V160P0-QC2-W001 (ownership gate dedup)**: CLOSED — semantics unchanged.
  - Extraction: `is_world_owned(pool, creator_id, world_id)` in narrative_write.rs:265-280 — single `SELECT world_id FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?`.
  - All call sites (orchestration world/timeline/fork/narrative_compute + daemon host_tool_handlers) now delegate to the shared helper; no inline duplication.
  - Authorization predicate identical to prior inlined checks. No weakening (still fails closed on missing world or wrong owner).
  - Evidence: narrative_write.rs:265 (new helper), world.rs:44 (ensure_world_owned wrapper), timeline.rs:102, fork.rs:111, host_tool_handlers.rs:832, and prior cross-creator tests continue to pass.

- **R-V160P1-QC1-W001 (script.section_status.update capability scoping)**: CLOSED — properly guarded.
  - New capability (commit ab3ea216) registered in all three CapabilityRegistry factories (mod.rs).
  - Input guard: `validate_work_ref(&inp.work_ref)?` (same strict kebab-case, no .. / \ NUL control upper as novel scaffold) — rejects traversal before any FS op.
  - Path construction: `works_root.join(work_ref).join("Scripts").join(section_path)` — explicitly under `Scripts/`.
  - Only mutates `section_status` frontmatter via shared `extract_frontmatter_field` / `replace_frontmatter_field` / `atomic_write` (temp+rename) from game_bible helpers. No other fields, no DB writes, no access outside the validated work_ref/Scripts tree.
  - Transition rules enforced (draft→reviewed allowed; draft→accepted, accepted→draft, self-transition rejected).
  - Tests: happy draft→reviewed, reject skip, reject missing section.
  - No privilege escalation surface: capability is local orchestration only; work_ref validation + Scripts/ hard scope + atomic FS write under temp+rename.
  - Evidence: script_section_status.rs:110 (validate), 119-121 (Scripts/ path), 148-158 (frontmatter + atomic), 198-273 (tests); mod.rs:194,263,487 (registration); novel_scaffold_sanitize.rs:32 (validate_work_ref).

- **R-V160P0-QC3-W001 (N+1 batch pre-fetch in world.delta.apply)**: CLOSED — semantics preserved.
  - Prior: per-change SELECT for current body_json inside the apply loop (N+1).
  - Fix (commit 386169bd): collect all kb_key_block targets first, build dynamic IN-list, single `SELECT key_block_id, body_json FROM kb_key_blocks WHERE key_block_id IN (...)`, populate HashMap; lost-update guard now reads from map.
  - Same guard logic (old_value vs live) and same UPDATE paths; only the read is batched. No behavior change for single or multi-update packages.
  - Regression test: `world_delta_apply_batch_kb_updates_prefetch` — two body_json updates + one stale conflict in one package; asserts both apply and conflict detection work through bulk path.
  - Evidence: world.rs:486-533 (collect + single SELECT + HashMap), 640 (map lookup), 1129-1174 (test).

- **No wire change**: Confirmed. Diff touches only internal local-db (narrative_write, work_chapters, migration) + orchestration capability impls + registry counts. No schemas/, no JSON Schema, no contracts package, no wire DTOs. `script.section_status.update` is explicitly local-only per compass §5 #6.

### 🟢 Suggestion
- The batch pre-fetch uses a dynamic IN-list with runtime placeholders. This is acceptable for the small cardinality (delta packages) and matches existing patterns elsewhere; consider a future comment or small helper if the pattern grows.
- `resolve_script_work_dir` and the script capability both rely on `validate_work_ref` + canonicalize. If a common `resolve_work_dir(workspace, work_ref, subdir)` helper is added later, both sites could share it (non-blocking hygiene).

## Source Trace
- R-V160P0-QC2-W002 tx: commit 1e46085d, narrative_write.rs:354 (`append_event_in_tx`), timeline.rs:130 (`begin`/`commit` + rename inside tx), test `timeline_event_append_explicit_id_persists_atomically`.
- R-V160P1-QC2-W001 path: commit 14b8dcc8, work_chapters.rs:1602 (`resolve_script_work_dir`), 1680 (`is_script_complete` call), test `test_is_script_complete_rejects_path_traversal_in_work_ref`.
- R-V160P0-QC2-W001 ownership: narrative_write.rs:265 (`is_world_owned`), world.rs:44 (`ensure_world_owned` now thin wrapper), grep across timeline/fork/host_tool_handlers.
- R-V160P1-QC1-W001 script cap: commit ab3ea216, script_section_status.rs:110-121 (validate + Scripts/ path), 151 (validate_transition), mod.rs registration, tests.
- R-V160P0-QC3-W001 N+1: commit 386169bd, world.rs:486 ("batch pre-fetch"), 512 (IN-list SELECT), 1129 (regression test).
- Baseline prior warnings: .mstar/plans/reports/2026-06-22-v1.60-df46-local-parity/qc2.md (W-001 dedup, W-002 rename outside tx).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

(All five assigned security/correctness items are directly addressed with atomicity guarantees, boundary checks, unchanged authorization semantics, strict scoping, and batch-equivalent semantics. Regression tests exist for the behavior-changing fixes. No wire impact. Prior df46 Warnings W-001 and W-002 are closed by this wave. Ready for QA.)
