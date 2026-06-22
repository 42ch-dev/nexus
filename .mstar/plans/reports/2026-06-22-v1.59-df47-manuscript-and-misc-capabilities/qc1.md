---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.59-df47-manuscript-and-misc-capabilities"
verdict: "Approve with residuals"
generated_at: "2026-06-22"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-22

## Scope
- plan_id: 2026-06-22-v1.59-df47-manuscript-and-misc-capabilities
- Review range / Diff basis: `merge-base: 578be523 + tip: 95d3595c` (equivalent: `git diff 578be523...95d3595c`)
- Working branch (verified): iteration/v1.59
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 7 in-scope (4 Rust + 2 markdown + 1 integration test)
- Commit range (if not identical to Review range line, explain): 578be523..95d3595c (10 commits; P0 scope is 2 commits — `9c2f78a9` for T1-T9 code/tests, `03b4f884` for T10-T11 docs)
- Tools run: git diff, git show, git log, grep, manual read

## Findings

### Critical
- *(none)*

### Warning
- *(none unresolved)*

### Suggestion
- W-001 (carry-over residual; pre-existing) → see Residual section. New code
  reinforces a pre-existing pattern; not introduced by V1.59 P0.
- S-001 (test determinism): `workspace_paths_rejects_without_workspace` uses a
  conditional `if let Err(e) = result { assert_eq!(e.error_code(), "INVALID_INPUT"); }`
  which accepts either success OR a specific error. Since `create_test_workspace`
  DOES seed `active_creator_id` (test_utils.rs:69-72) and `WorkspaceState::new(..., None)`
  defaults `workspace_path` to `None` (workspace/mod.rs:150), the test state is
  fully deterministic: admission `ActiveCreator` passes, then the handler
  deterministically returns `InvalidInput`. Tighten to a strict assertion to
  prevent silent regression if admission order ever changes.

## Source Trace
- Finding ID: W-001
- Source Type: doc-rule + manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:2167-2172` (new); `crates/nexus-local-db/src/works.rs:1178-1189`, `:1210-1218` (pre-existing). AGENTS.md rules: `crates/nexus-daemon-runtime/AGENTS.md` ("sqlx Compile-Time Macros (Mandatory)") and `crates/nexus-local-db/AGENTS.md` ("Compile-time checked queries only").
- Confidence: High
- Notes: Pre-existing pattern (waived per `R-V140P0-S3`); new handler `execute_manuscript_write` adds one more site to the same pattern with a `// SAFETY: ... — runtime query` comment that is technically misleading because the SQL string is static (no runtime concatenation). Recommend `sqlx::query!("UPDATE work_chapters SET actual_word_count = ?, updated_at = ? WHERE work_id = ? AND chapter = ? AND volume = ?", ...)` for compile-time schema validation, but this is a codebase-wide convention rather than a V1.59 P0 regression.

- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor_tests.rs:2645-2665`; `crates/nexus-daemon-runtime/src/test_utils.rs:43-89`; `crates/nexus-daemon-runtime/src/workspace/mod.rs:150`.
- Confidence: High

## Architecture & Maintainability Assessment

### Pattern conformance (good)

The 9 new host tools follow the established `execute_* + registry_*` two-layer
pattern exactly:
- 5 manuscript tools use `async fn execute_*` + `pub(crate) fn registry_*` that
  returns `Pin<Box<dyn Future + Send + 'a>>` (handlers.rs:1877-2542).
- 3 sync tools (`workspace.paths`, `runtime.health`, `trace.correlation`) use
  the `Box::pin(async move { result })` sync→async wrapper idiom, matching
  `registry_workspace_info` / `registry_daemon_health` (handlers.rs:1053, 1157).
- 9 re-exports in `host_tool_executor.rs:362-370` mirror the existing 21.

Naming is consistent (`nexus.<group>.<verb>`). Failure modes align with the
existing 21 (`FailureMode::Forbidden` for read tools without workspace
ownership; `FailureMode::InvalidInput` for parameter validation). Handler
docstrings follow the same docstring template as V1.53/V1.54 siblings.

### Responsibility separation (good)

- `MANUSCRIPT_PHASES` constant + `phase_index` helper are module-private to
  `host_tool_handlers.rs` and only used by `execute_manuscript_phase_set`. Not
  promoted to a public type because it's a UI-facing canonical set, not a
  domain entity.
- `MANUSCRIPT_WRITE_MAX_BYTES = 1024 * 1024` (1 MiB) is a single constant
  referenced once. Acceptable inline; would justify a `pub const` only if
  shared across crates.
- The `nexus.runtime.health` vs `nexus.observability.daemon.health` distinction
  is real and documented in the handler docstring (handlers.rs:2401-2404): the
  former returns agent-facing `{runtime_mode, registry_reachable,
  registry_size, sync_state, cloud_enabled, pool_healthy}`; the latter returns
  daemon-lifecycle `{uptime_seconds, started_at, lifecycle_state, registry_size,
  registry_ids, pool_healthy}`. Different response shape, different audience.
  Test `runtime_health_returns_agent_visible_status` validates the agent-facing
  shape; `daemon_health_returns_registry_status` validates the daemon-lifecycle
  shape and was correctly updated for `registry_size: 21 → 30`.

### Error handling (good)

- Input validation uses `NexusApiError::InvalidInput { field, reason }` with
  `field` set to the JSON path (`parameters.work_id`, `parameters.chapter`,
  etc.), matching V1.53/V1.54 patterns.
- Cross-creator access denial returns `NexusApiError::Forbidden { resource,
  reason }` (fail-closed), matching `execute_manuscript_chapter_get` etc.
- `NotFound` is used for missing chapter/body (handlers.rs:1997, 2067).
- `Internal { code, message }` is used for `tokio::fs::*` failures and DB
  errors. Error code vocabulary is consistent with existing handlers
  (`FILE_READ_FAILED`, `FILE_WRITE_FAILED`, `FILE_RENAME_FAILED`,
  `DIR_CREATE_FAILED`, `WORKSPACE_PATH_ERROR`, `CHAPTER_BODY_MISSING`,
  `DATABASE_ERROR`).

### Test coverage (good; minor weakness)

- 18 new test vectors = 9 success + 9 failure, matching the plan's
  acceptance criterion "Each capability has ≥ 1 success-path test vector +
  ≥ 1 failure-path test vector" (1:1, not ≥1).
- Each test dispatches through `HostToolExecutor::execute()` (full admission +
  registry path), not by calling handlers directly. This validates the
  registry wiring, not just the handler.
- The failure-mode tests cover 3 distinct categories: missing active creator
  (4 tests), missing input field (2 tests), bad value (2 tests),
  cross-creator (1 test), missing row (1 test). Solid diversity.
- The helper `create_test_manuscript_work` is well-structured and reusable.
- **Weakness:** `workspace_paths_rejects_without_workspace` uses a conditional
  accept pattern (S-001) — see Suggestion section.

### Spec/code alignment (good)

- `acp-capability-set.md` §4 (V1.59 roster) flips exactly the 9 expected rows
  from `catalog-only` to `shipped` with `Shipped in = V1.59 P0` and
  `Registry row ref = host_tool` (lines 109, 127-131, 138-140). Section
  header updated to "V1.59".
- `capability-registry.md` adds a dedicated "V1.59 P0: DF-47 manuscript &
  misc capability parity batch" section (lines 287-405) with per-ID runtime
  contract (id, access, admission, handler, ACP wire, failure mode) and
  test vector documentation (success + failure paths for each).
- The 9 new `id:` strings in `capability_registry.rs:715-866` are
  byte-identical to the §4 roster rows and the capability-registry.md
  per-ID entries.
- `NEXUS_TOOL_IDS` in `tests/cross_caller_e2e.rs:32-60` matches the
  registry's 28 nexus.* entries (30 - 2 fs/* baseline = 28).
- The renamed test `all_19_nexus_tool_ids_registered_in_capability_registry`
  → `all_nexus_tool_ids_registered_in_capability_registry` is correctly
  version-agnostic (good hygiene).

### Bidirectional `catalog_registry_invariant_all_ids_present` change (appropriate)

- **Previous behavior:** Soft check (logging-only via `eprintln!`); would not
  fail tests if a catalog-shipped ID was missing from the registry.
- **New behavior:** Hard bidirectional check via the explicit
  `is_shipped_host_tool` match list (28 entries). Fails the test if any
  catalog id marked as shipped is not in the registry.
- **List integrity:** All 28 entries in the new `is_shipped_host_tool`
  match list match the 28 `nexus.*` IDs in `NEXUS_TOOL_IDS`
  (cross_caller_e2e.rs:32-60) and the 28 `nexus.*` registrations in
  `build_registry()` (capability_registry.rs:366-877). The 2 `fs/*` tools
  are explicitly excluded per the documented "known gap" (line 1120-1122 in
  the pre-P0 file). Manually verified.
- **Justification:** With 21→30 host tools and the plan §Acceptance stating
  "All 9 capabilities have a host_tool binding", a soft check no longer
  matches the spec's quality bar. The hard check is appropriate.
- **Caveat:** The match list is now a manual maintenance burden — if a future
  plan adds a 31st host tool, the developer must remember to add it to
  BOTH the registry AND the match list. A more robust approach would
  auto-derive the match list from the registry's IDs (e.g., by introspecting
  `host_tool_registry().ids()` and intersecting with catalog IDs). Recommend
  this as a future cleanup; not a V1.59 P0 blocker.

### Hotfix-rule compliance (not applicable / good)

- No new code acquires `RuntimeLockGuard`. The V1.42.1 hotfix rule
  (existence check before acquire; explicit `lock.release().await` on every
  exit path) is therefore not triggered by this change.
- `execute_manuscript_phase_set` does call `works::update_work_stage(...)`
  which performs `UPDATE works SET current_stage = ?, ...`. This UPDATE
  races with the FL-E auto-advance scheduler that may also call
  `update_work_stage` concurrently. Not introduced by V1.59 P0 (the
  pre-existing `execute_work_patch` already invokes the same DAO), but
  worth noting as a follow-up if multi-writer contention surfaces.

## Summary

| Severity | Count |
|----------|-------|
| Critical | 0 |
| Warning  | 0 |
| Suggestion | 2 |

**Verdict**: Approve with residuals

The 9 new host tools follow the existing `execute_* + registry_*` pattern
with consistent naming, failure modes, and error vocabulary. The 18 test
vectors (9 success + 9 failure) cover the registry→admission→handler path
end-to-end, with one minor test-determinism weakness (S-001). The spec/code
alignment is exact (acp §4 + capability-registry.md + capability_registry.rs
+ NEXUS_TOOL_IDS all consistent at 28 nexus.* + 2 fs/* = 30). The strengthened
`catalog_registry_invariant_all_ids_present` bidirectional hard check is
appropriate for the "30 shipped host tools" quality bar; the manual match
list is a maintainability cost (S-002) that should be auto-derived in a
follow-up plan.

The single carry-over residual (W-001, sqlx runtime query pattern) is
pre-existing and waived per `R-V140P0-S3`; the new code reinforces the
pattern but does not introduce a regression.

No blocking findings. Plan is mergeable from an architecture/maintainability
perspective.

---

## Residual Findings (for PM/QA tracking)

The following items are not blocking but should be tracked. PM/QA may
register them in `.mstar/status.json` `residual_findings[plan_id]`.

### R1 (carry-over, severity `low`, source: pre-existing AGENTS.md waiver)
- **id**: R1
- **title**: `execute_manuscript_write` uses runtime `sqlx::query()` for static SQL
- **severity**: `low` (pre-existing pattern, waived per R-V140P0-S3)
- **source**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:2167-2183`
- **scope**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:execute_manuscript_write`; also affects 7+ pre-existing sites in same file + 3 sites in `crates/nexus-local-db/src/works.rs`.
- **decision**: defer (codebase-wide cleanup, not V1.59 P0 scope)
- **owner**: `@fullstack-dev` (Track A) — but should be addressed at P-last or P0 of next iteration that targets the local-db layer
- **target milestone**: post-1.0 (per R-V140P0-S3 waiver) — when CI enforces single shared sqlx cache across crates
- **tracking link**: `.mstar/archived/residuals/R-V140P0-S3.json` (waiver) + this report
- **closure_note**: code follows the existing convention. SAFETY comment is technically misleading (SQL is static) but consistent with the file's pattern. Recommend updating SAFETY comments to read "STATIC SQL — runtime query used per codebase convention (waived R-V140P0-S3)" so future readers know it's intentional.

### R2 (severity `low`, source: this report)
- **id**: R2
- **title**: `catalog_registry_invariant_all_ids_present` match list requires manual sync
- **severity**: `low`
- **source**: `crates/nexus-daemon-runtime/src/capability_registry.rs:1188-1207` (`is_shipped_host_tool` match list)
- **scope**: hard-coded list of 28 IDs that must be kept in sync with the registry
- **decision**: accept for V1.59 P0; recommend cleanup in a follow-up plan
- **owner**: `@fullstack-dev` (Track A) — P-last or V1.60 P0 candidate
- **target milestone**: V1.60
- **tracking link**: this report
- **closure_note**: replace the match list with `host_tool_registry().ids().filter(|id| id.starts_with("nexus."))` and intersect against catalog IDs. Eliminates the 28-element manual list.
