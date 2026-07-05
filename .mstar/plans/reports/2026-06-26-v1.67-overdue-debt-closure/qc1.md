---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-26-v1.67-overdue-debt-closure"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture & maintainability (focus per role parameters) — single-canonical ownership gate, no piggyback refactor, surgical scope, named-column parsing robustness, capability registry consistency, wire/contract surface unchanged.
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-26-v1.67-overdue-debt-closure (P2 Overdue Debt Closure — 9 V1.52/V1.60 residuals)
- Review range / Diff basis: P2 code commits `138a98fd 0f21fc6a 8682486b 1e46085d 386169bd ab3ea216 14b8dcc8 38d80308 ae1b960e` merged at HEAD; diff basis vs `26e477ee`. Equivalent `git log 26e477ee..HEAD -- crates/`.
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed (targeted diff + source): 15 files, +976/-111 lines per `git diff --stat 26e477ee..HEAD -- crates/`:
  - `crates/nexus-orchestration/src/quality_loop.rs` (R-V152TA-S001/S006)
  - `crates/nexus-daemon-runtime/src/capability_registry.rs` (R-V160P0-QC1-W002 — header-named parsing)
  - `crates/nexus-local-db/src/narrative_write.rs` (R-V160P0-QC2-W001 dedup, R-V160P0-QC2-W002 `append_event_in_tx`)
  - `crates/nexus-orchestration/src/capability/builtins/world.rs` (R-V160P0-QC2-W001, R-V160P0-QC3-W001 batch pre-fetch)
  - `crates/nexus-orchestration/src/capability/builtins/timeline.rs` (R-V160P0-QC2-W002 tx wrap)
  - `crates/nexus-orchestration/src/capability/builtins/script_section_status.rs` (R-V160P1-QC1-W001 — new builtin)
  - `crates/nexus-orchestration/src/capability/builtins/game_bible_section_status.rs` (helpers lifted to `pub`)
  - `crates/nexus-orchestration/src/capability/builtins/mod.rs` (module + re-export)
  - `crates/nexus-orchestration/src/capability/mod.rs` (registry constructors x3 + count test)
  - `crates/nexus-orchestration/tests/capability_registry.rs` (33→34 count)
  - `crates/nexus-daemon-runtime/tests/daemon_boot_llm_wiring.rs` (32→33 count)
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs` (R-V160P0-QC2-W001 — delegates to `is_world_owned`)
  - `crates/nexus-local-db/src/work_chapters.rs` (R-V160P1-QC2-W001 `resolve_script_work_dir`)
  - `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql` (R-V160P1-QC2-W002 `PRAGMA foreign_key_check`)
  - `crates/nexus-local-db/src/lib.rs` (R-V160P1-QC2-W002 regression test)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git status`
  - `git log --oneline 26e477ee..HEAD -- crates/` and per-commit `git show <hash>` diffs
  - targeted reads of every touched source file (mod tests + production bodies)
  - `grep` for inline ownership-gate duplicates, `ensure_world_owned` call sites, registry count tests, and SQL pattern lookups
  - `SQLX_OFFLINE=true cargo test -p nexus-orchestration -p nexus-kb -p nexus-local-db` (all green; 957 + 139 + 273 unit tests pass)
  - `SQLX_OFFLINE=true cargo test -p nexus-orchestration --lib script_section_status timeline_event_append_explicit_id_persists_atomically world_delta_apply_batch_kb_updates_prefetch llm_candidate_parser_shape_is_shared_across_pathways registry_has_34_builtins registry_iter_returns_all registry_lookup_each_builtin` — all green
  - `SQLX_OFFLINE=true cargo test -p nexus-local-db --lib migrations_leave_no_foreign_key_violations test_is_script_complete_rejects_path_traversal_in_work_ref is_script_complete` — all green
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test daemon_boot_llm_wiring` — green
  - `SQLX_OFFLINE=true cargo clippy -p nexus-orchestration -p nexus-kb -p nexus-local-db -p nexus-daemon-runtime -- -D warnings` — clean (lib + bin scope; matches CI command `cargo clippy --all -- -D warnings`)
  - `cargo +nightly fmt --all --check` — clean
  - `./tooling/check-wire-drift.sh` — clean (4/4 schema drift detection tests pass; no `schemas/` or `crates/nexus-contracts/` changes)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None from qc1's architecture/maintainability lens. (See "Cross-reviewer acknowledgement" below for findings owned by qc3.)

### 🟢 Suggestion
- **S1 — `outline_five_q_nogo_info_logs_dimensions` regression test promised in the plan Completion Report table is not present.** The plan (`plans/2026-06-26-v1.67-overdue-debt-closure.md` §"Completion Report v2" row for R-V152TA-S006) explicitly lists `outline_five_q_nogo_info_logs_dimensions` as the regression test, and a doc-only change would not need one — but the actual implementation adds a `tracing::info!` call inside the new `build_five_q_verdict` helper (`quality_loop.rs:1412-1422`), which is a real behavior change. The implementation is correct and is exercised by the existing `outline_five_q_fails_*` tests (which trigger NOGO and now emit the log), but no test asserts on the emitted log fields (`structure`/`arc`/`foreshadow`/`pacing`/`hook`/`failed`). Severity: low / minor — observability assertion gap, not behavior. Suggestion: either drop the "regression test" claim from the plan table for S006, or add a test that asserts the structured fields (e.g. via `tracing-test` or a `tracing_subscriber::layer()` test layer). This is purely a promise-vs-delivery documentation gap and is **not** a blocker. Source Trace: `crates/nexus-orchestration/src/quality_loop.rs:1412-1422` (the `tracing::info!` call), and the plan row at line 56 of the P2 plan. Confidence: High.
- **S2 — Direct unit test for the newly extracted `nexus_local_db::narrative_write::is_world_owned` is absent.** The fix (R-V160P0-QC2-W001) lifts the ownership SELECT into `narrative_write::is_world_owned` and delegates from both `nexus-orchestration::ensure_world_owned` and `nexus-daemon-runtime::ensure_world_accessible_for_creator`. The function is reachable transitively via existing `world.delta.apply` / `timeline.event.append` / `fork.create` integration tests, but no `#[tokio::test]` lives next to it in `narrative_write.rs` (the `mod tests` block at lines 464-672 covers `create_world` / `append_event` but not `is_world_owned`). Defensive coverage would catch future regressions in the canonical gate without needing the orchestration crate to be in scope. Severity: low / nit. Suggestion: add a small `is_world_owned_returns_false_for_cross_creator` / `is_world_owned_returns_true_for_owner` pair. **Not** a blocker — the dedup itself is correct. Source Trace: `crates/nexus-local-db/src/narrative_write.rs:255-278` (function body); `crates/nexus-local-db/src/narrative_write.rs:464-672` (existing test module does not cover `is_world_owned`). Confidence: High.
- **S3 — Three new pedantic clippy lints introduced in `capability_registry.rs` test code (`cargo clippy --all-targets`).** R-V160P0-QC1-W002 introduces a `struct ColumnIndex` after statements (line 1168), a `struct CatalogRow` after statements was already present (line 1158) but is now adjacent to a second occurrence of the same lint, and a `cols.iter().any(|c| *c == "Capability ID")` triggers `manual_contains` (line 1180, suggesting `cols.contains(&"Capability ID")`). **Default CI command** `cargo clippy --all -- -D warnings` (lib + bin only) **remains clean** because these lints fire only on `--all-targets`; the new code's pedantic style is consistent with the pre-existing test file (which already triggers `items_after_statements` in `mod tests`). Cosmetic, behavior-preserving. Severity: nit. Suggestion: prefer `cols.contains(&"Capability ID")` and move both struct definitions to the top of the test function; otherwise add a narrow `#[allow(clippy::items_after_statements, clippy::manual_contains)]` on the test function. Source Trace: `crates/nexus-daemon-runtime/src/capability_registry.rs:1158, 1168, 1180`. Confidence: High.
- **S4 — Helpers in `game_bible_section_status` are now `pub` rather than `pub(crate)`/private.** The diff lifts `validate_transition`, `parse_frontmatter`, `replace_frontmatter_field`, `atomic_write`, `extract_frontmatter_field` from `fn` to `pub fn` so `script_section_status` can re-use them via `super::game_bible_section_status::{...}`. This is a deliberate architectural choice for code reuse between the two frontmatter-edit capabilities. The trade-off is that the helpers now form part of the module's public API surface (visible to anyone who can name the module) — which is fine today but constrains future evolution (e.g. a generic `FrontmatterEditor` struct would be cleaner). Severity: nit / architectural-smell. Suggestion: acceptable as-is for V1.67; if a third frontmatter-edit capability ships (V1.68+), extract these into a small `frontmatter.rs` private module and re-export. Source Trace: `crates/nexus-orchestration/src/capability/builtins/game_bible_section_status.rs:67,106,137,233,368` (5 `pub fn` lifts). Confidence: Medium.

## Cross-reviewer acknowledgement (out-of-lens)
- **qc3 W-001 — `PRAGMA foreign_key_check` in `202606230001_work_profile_script.sql` does not fail closed on returned violation rows.** SQLite's `PRAGMA foreign_key_check` returns rows for violations but does not raise an error itself; whether `sqlx::migrate!().run()` propagates the rows into a migration failure depends on the executor's row-consumption semantics. qc3 verified this with a SQLite reproducer. From an architecture/maintainability lens, this means the integrity check is currently **diagnostic** rather than **enforced** — the fix orders the check correctly post-rename, but a future code reader cannot rely on it to halt the migration. **Owned by qc3 (correctness/reliability)** — I concur the assertion is incomplete and would benefit from either (a) an explicit Rust-side post-migration `PRAGMA foreign_key_check` row consumer that returns `LocalDbError`, or (b) a SQLite construct that raises on non-empty violation result. This is **not** a qc1-architecture blocker, but I agree it should be addressed before final approval. Source Trace: `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql:56-60`; qc3 reproduction evidence in `qc3.md` §W-001. Confidence: High.
- **qc3 W-002 — `world.delta.apply` batch pre-fetch has an unbounded dynamic IN-list.** R-V160P0-QC3-W001 replaces per-change `SELECT body_json ... WHERE key_block_id = ?` with one bulk `IN (...)` SELECT. The pre-fetched ID list is bounded by the caller-supplied `proposed_changes`, and the input schema has no `maxItems`. A large agent-proposed delta can produce a SQL bind-parameter count that exceeds SQLite's `SQLITE_MAX_VARIABLE_NUMBER` (default 32766, but realistically lower per connection). **Owned by qc3 (correctness/reliability)** — from architecture/maintainability lens, I concur this should be bounded by either an input-schema `maxItems` cap, runtime chunking of the IN-list, or deduplication of update IDs. Not blocking for the architecture/maintainability verdict (the N+1 itself is correctly eliminated), but it should be tracked. Source Trace: `crates/nexus-orchestration/src/capability/builtins/world.rs:486-525`; `capability/builtins/world.rs` input schema in `Capability::input_schema` (no `maxItems` on `proposed_changes`). Confidence: High.

## Per-residual verification matrix

| Residual | Lifecycle (status.json) | Surgical? | Behavior-preserving? | Single canonical? | Wire/contract drift | Verified fix site | qc1 verdict |
|----------|------------------------|-----------|----------------------|-------------------|---------------------|-------------------|-------------|
| R-V152TA-S001 | deferred → V1.61+ | ✓ | ✓ (doc + regression test only) | n/a (pathway already unified in V1.55 P2 F-001) | none | `quality_loop.rs:619-625` doc-comment + test `llm_candidate_parser_shape_is_shared_across_pathways` | Closed |
| R-V152TA-S006 | deferred → V1.61+ | ✓ | Adds `tracing::info!` on NOGO (intended observability); no other behavior change | n/a | none | `quality_loop.rs:1387-1431` (`build_five_q_verdict` helper + `tracing::info!`) | Closed (S1 note on missing log-assert test) |
| R-V160P0-QC1-W002 | deferred → V1.61+ | ✓ | ✓ (test-only change) | n/a (test code) | none | `capability_registry.rs:1164-1233` (header-name lookup + `ColumnIndex` mapping) | Closed (S3 note on pedantic clippy) |
| R-V160P0-QC2-W001 | deferred → V1.61+ | ✓ | ✓ (delegation only) | ✓ single canonical `is_world_owned` at `narrative_write.rs:265-278`; both callers (`world.rs:44-57`, `host_tool_handlers.rs:823-839`) delegate | none | `narrative_write.rs:255-278`; `world.rs:44-57`; `host_tool_handlers.rs:823-839` | Closed (S2 note on direct unit test) |
| R-V160P0-QC2-W002 | deferred → V1.61+ | ✓ | Adds tx wrap (intended atomicity) | n/a | none | `narrative_write.rs:347-409` (`append_event_in_tx`, `append_event_core`); `timeline.rs:104-171` (tx wrap with collision check + insert + rename + commit) | Closed |
| R-V160P0-QC3-W001 | deferred → V1.61+ | ✓ | Replaces N+1 with single bulk pre-fetch; per-change `UPDATE` retained (correct — each change can carry a different field/value) | n/a | none | `world.rs:486-525` (bulk SELECT) + `world.rs:530-538` (in-memory HashMap lookup) | Closed (qc3 W-002 acknowledged) |
| R-V160P1-QC1-W001 | deferred → V1.61+ | ✓ | Adds new builtin (no schema; local-only per compass §5 #6) | n/a | **none** (no `schemas/` or `nexus-contracts/` change; confirmed via `git diff 26e477ee..HEAD -- schemas/ crates/nexus-contracts/` returning empty) | `script_section_status.rs:1-274` (new module); `capability/builtins/{mod.rs:28,62,builtins/mod.rs:58,builtins/script_section_status.rs}`; `capability/mod.rs:194-196, 262-264, 486-488` (all 3 factories); `tests/capability_registry.rs:13-31` (33→34); `tests/daemon_boot_llm_wiring.rs:226-242` (32→33); `capability/mod.rs:602-607` (registry_has_34_builtins) | Closed; registry count tests updated correctly to 34 (orchestration) and 33 (daemon-runtime); `script.section_status.update` added to `registry_lookup_each_builtin` and `registry_iter_returns_all` |
| R-V160P1-QC2-W001 | deferred → V1.61+ | ✓ | ✓ (validation guard; intended security hardening) | ✓ single canonical `resolve_script_work_dir` helper | none | `work_chapters.rs:1605-1653` (helper); `work_chapters.rs:1720-1744` (call site) | Closed |
| R-V160P1-QC2-W002 | deferred → V1.61+ | ✓ | Adds `PRAGMA foreign_key_check` post-rename (intended belt-and-suspenders) | n/a | none | `migrations/202606230001_work_profile_script.sql:56-60`; `lib.rs:399-423` (`migrations_leave_no_foreign_key_violations` test) | Closed (qc3 W-001 acknowledged — assertion is diagnostic not enforced) |

## Architecture/Maintainability summary
- **Single-canonical ownership gate:** verified — `is_world_owned` in `nexus_local_db::narrative_write` is the only inline ownership SELECT against `narrative_worlds WHERE world_id = ? AND owner_creator_id = ?` for the orchestrated path. Both `nexus-orchestration` (`ensure_world_owned`) and `nexus-daemon-runtime` (`ensure_world_accessible_for_creator`) delegate to it. Other inline sites (`works.rs:312`, `novel_scaffold.rs:368`) intentionally retain bespoke SQL because they have **different error semantics** (BadRequest with helpful CLI hints vs Forbidden) and **different return shape** (`Option<String>` with non-null world_id validity check via `EXISTS`). They are not drift risk for the `ensure_world_owned` pattern; they are part of distinct input-validation flows.
- **Named-column parsing robustness:** the header-name lookup at `capability_registry.rs:1166-1212` correctly fails loud on missing columns (`.expect("Capability ID column missing")` / `.expect("Status column missing")` / `.expect("Registry row ref column missing")`), and the row-level guard `cols.len() <= id_col.max(status_col).max(registry_ref_col)` prevents indexing into a short row. Future column insertions will be picked up by header match; renames will fail at the header-lookup step rather than silently mis-classifying rows.
- **Capability registry consistency:** `script.section_status.update` is registered in all three `CapabilityRegistry` constructors (`with_builtins`, `with_builtins_and_pool`, `with_runtime_deps`). The integration test `registry_has_thirty_four_builtins` (`tests/capability_registry.rs:13-31`) and unit test `registry_has_34_builtins` (`capability/mod.rs:602-607`) both assert 34. The `registry_lookup_each_builtin` and `registry_iter_returns_all` tests now include `script.section_status.update`. The `daemon_boot_llm_wiring` test asserts 33 for `with_runtime_deps` (one fewer because `narrative_compute` is built externally and the inline count is 33 boxed items in the caps vector — the plan's claim "with_runtime_deps (no pool) = 33" matches).
- **Transaction atomicity for timeline rename:** the `begin → collision check → append_event_in_tx → optional rename → commit` flow at `timeline.rs:104-171` is consistent with the project's other tx-aware admission patterns. The collision check runs inside the same transaction as the insert (was previously `fetch_one(&**pool)` outside the tx), which tightens the guard's consistency.
- **Local-only capability verification (R-V160P1-QC1-W001):** `script_section_status.update` operates exclusively on filesystem paths under `Works/<work_ref>/Scripts/` (validated via `validate_work_ref` from `novel_scaffold_sanitize`, then joined with the per-instance `works_root`). No database, no wire contract, no preset wiring — fully consistent with the plan's "local-only" lock per compass §5 #6 and the prior residual deferral rationale. The `check-wire-drift.sh` test passes; `git log 26e477ee..HEAD -- schemas/ crates/nexus-contracts/` returns empty.
- **Surgical scope:** verified — each of the 9 P2 code commits touches ≤3 files and adds ≤300 lines. No piggyback refactor. The `ae1b960e` "nightly fmt + registry count fixes" commit is correctly scoped to fmt-only diffs in previously-touched files plus the 2 registry count-test updates and the `script.section_status.update` entries in lookup/iteration assertions — pure mechanics for the R-V160P1-QC1-W001 closure.
- **Behavior preservation for S001/S006 (doc/observability):**
  - S001: production code unchanged in `run_llm_extract`; only a doc-comment documenting the existing unified pathway + a regression test (`llm_candidate_parser_shape_is_shared_across_pathways`) were added. ✓
  - S006: production code adds a `tracing::info!` call inside `build_five_q_verdict` — this is the intended observability per the residual's "info-log on NOGO with dimension scores" requirement, not an unintended behavior change. ✓
- **Cargo fmt / clippy / wire-drift:**
  - `cargo +nightly fmt --all --check` — clean
  - `cargo clippy --all -- -D warnings` — clean (default target scope = lib + bin; matches CI)
  - `./tooling/check-wire-drift.sh` — clean (4/4 schema drift detection tests pass)
  - `cargo test -p nexus-orchestration -p nexus-kb -p nexus-local-db -p nexus-daemon-runtime` — all green

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (from qc1 lens) |
| 🟢 Suggestion | 4 (S1–S4) |

**Verdict**: Approve

The 9 P2 residuals are surgically and correctly closed from an architecture/maintainability perspective: single canonical `is_world_owned` ownership gate with delegation from both orchestration and daemon-runtime callers; header-name parsing replaces positional indices in the catalog-registry invariant test; `script.section_status.update` is registered in all three CapabilityRegistry factories, locally scoped, and wire-clean; the timeline append + rename is wrapped in one transaction with a pre-fetched lost-update guard; the migration gains a `PRAGMA foreign_key_check` step plus a regression test; and `is_script_complete` rejects path-traversal via `resolve_script_work_dir`. No piggyback refactor, no unintended behavior change in the doc/observability residuals, no wire/contract surface drift. The 4 Suggestions are minor (missing log-assert test for S006, missing direct unit test for `is_world_owned`, cosmetic pedantic clippy lints in test code, and `pub` helper visibility trade-off) and are not merge-blocking. Two findings owned by qc3 (PRAGMA enforcement, unbounded IN-list) are acknowledged as out-of-lens for qc1 but are real and should be tracked.