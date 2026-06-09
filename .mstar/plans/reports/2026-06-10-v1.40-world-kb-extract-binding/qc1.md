---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-10-v1.40-world-kb-extract-binding"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-10T20:00:00Z

## Scope
- plan_id: 2026-06-10-v1.40-world-kb-extract-binding
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-kb-extract-binding (b172dfa5..5c3b4c01)
- Working branch (verified): feature/v1.40-world-kb-extract-binding
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 17
- Commit range: b172dfa5..5c3b4c01 (11 commits)
- Tools run: git diff, git log, git grep, manual code review

## Findings

### 🔴 Critical
(None)

### 🟡 Warning

#### W-001: Dead code — `build_child_kb_extract_schedule` never called
- **File**: `crates/nexus-orchestration/src/stage_gates.rs:276`
- **Evidence**: `git grep -rn "build_child_kb_extract_schedule"` returns only the definition site. No callers exist in any `.rs` or `.yaml` file. The plan (T8) specifies a `schedule.enqueue_child` capability, but the implementation uses a different approach: the `sync_world_kb` state in `novel-review-master/preset.yaml` calls `kb.extract_work` directly via `enter` action, not via `build_child_kb_extract_schedule` or any `schedule.enqueue_child` capability.
- **Impact**: The function (76 lines) is dead code — it will be flagged by `cargo clippy` as `dead_code` once warnings are promoted. It also creates confusion about the intended architecture: the plan says "schedule.enqueue_child capability" but no such capability exists in the registry or preset YAML.
- **-> Fix**: Either (a) wire `build_child_kb_extract_schedule` into the `sync_world_kb` state execution (replacing the direct `kb.extract_work` call), or (b) remove the dead function and update the plan to reflect that `sync_world_kb` uses direct capability invocation. If (a), register a `schedule.enqueue_child` capability in `CapabilityRegistry::with_builtins()`.

#### W-002: `sync_world_kb` state fails silently for worldless Works
- **File**: `crates/nexus-orchestration/embedded-presets/novel-review-master/preset.yaml:88-103`
- **Evidence**: The `sync_world_kb` state calls `kb.extract_work` unconditionally in its `enter` action. For worldless Works (V1.39 legacy), `{{preset.input.world_id}}` renders to empty string `""`. The `kb.extract_work` capability will then fail because `world_id` is empty/invalid. The state uses `exit_when: kind: rule` (always-true immediate transition), but the task executor processes `enter` actions sequentially — if the capability call fails, the task returns an error before reaching the exit check, potentially blocking the state machine.
- **Impact**: Worldless V1.39 Works using `novel-review-master` would encounter a capability failure in `sync_world_kb`, blocking the review flow from reaching `done`. The preset description says "No-op for worldless" but the implementation is not a true no-op.
- **-> Fix**: Add a conditional guard so `sync_world_kb` skips the `kb.extract_work` call when `world_id` is absent. Options: (a) use a Handlebars `{{#if preset.input.world_id}}` conditional in the preset YAML (if the template engine supports it in `enter` actions), (b) add a `Conditional` enter action type that checks `world_id` presence, or (c) make `kb.extract_work` return a success no-op when `world_id` is empty (least preferred — masks errors).

#### W-003: Runtime `sqlx::query_as` replaces compile-time checked queries
- **File**: `crates/nexus-local-db/src/kb_extract_job.rs:62-65` (JOB_COLUMNS constant) and all query sites (lines 71-89, 161-172, 219-230, 267-277, 291-301, 341-351)
- **Evidence**: The refactored `kb_extract_job.rs` replaces compile-time checked `sqlx::query_as!()` macros with runtime `sqlx::query_as::<_, KbExtractJob>(&format!(...))` using a `JOB_COLUMNS` constant. The `nexus-local-db/AGENTS.md` rule states: "Compile-time checked queries only — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL." While `SAFETY` comments document the rationale (column list shared across queries), this pattern loses compile-time verification of column-name/type alignment between the SQL string and the Rust struct.
- **Impact**: A future schema migration adding/renaming columns would not be caught at compile time — it would fail at runtime instead. The previous `query_as!()` pattern provided stronger guarantees.
- **-> Fix**: Either (a) revert to `sqlx::query_as!()` with explicit column lists per query (accepting the slight duplication), or (b) document this as an explicit exception in `nexus-local-db/AGENTS.md` with justification and a note that integration tests must cover all query paths.

### 🟢 Suggestion

#### S-001: `enqueue_with_artifact` uses unusual `Box::pin(async move {...})` pattern
- **File**: `crates/nexus-local-db/src/kb_extract_job.rs:210-248`
- **Evidence**: `enqueue_with_artifact` returns `Pin<Box<dyn Future<Output = ...> + 'a>>` via `Box::pin(async move {...})`. The existing `enqueue` function uses a standard `async fn` pattern. The `Box::pin` pattern adds indirection without clear benefit — the function is not stored or spawned, just `.await`ed directly at the call site.
- **-> Fix**: Consider converting to a standard `async fn` for consistency with `enqueue` and reduced cognitive overhead. If the `Box::pin` pattern is needed for a specific reason (e.g., type erasure for dynamic dispatch), document it in a comment.

#### S-002: `world_refs_validate` exported but not registered in capability registry
- **File**: `crates/nexus-orchestration/src/capability/builtins/mod.rs:41`
- **Evidence**: `world_refs_validate` module and its public types are exported from `builtins/mod.rs` but the `WorldRefsValidate` capability struct is NOT registered in `CapabilityRegistry::with_builtins()` or `with_builtins_and_pool()`. This is pre-existing (not in this diff), but worth noting since this plan touches World KB infrastructure.
- **-> Fix**: Register `WorldRefsValidate` in the capability registry or remove the unused export. (Out of scope for this plan — track as residual.)

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|-----------------|------------|
| W-001 | git-grep + manual-reasoning | `git grep -rn "build_child_kb_extract_schedule"` — only definition at `stage_gates.rs:276`, no callers | High |
| W-002 | manual-reasoning | `novel-review-master/preset.yaml:88-103` + `tasks/mod.rs` enter-action processing | High |
| W-003 | doc-rule + git-diff | `nexus-local-db/AGENTS.md` "Compile-time checked queries only" vs `kb_extract_job.rs` refactor | High |
| S-001 | manual-reasoning | `kb_extract_job.rs:210-248` — `Box::pin` pattern vs `enqueue` async fn | Medium |
| S-002 | git-grep | `builtins/mod.rs:41` exports `world_refs_validate` but not in `CapabilityRegistry::with_builtins()` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

**Rationale**: Three warnings require resolution before approval:
1. **W-001** (dead code): `build_child_kb_extract_schedule` is defined but never called — either wire it in or remove it.
2. **W-002** (worldless breakage): `sync_world_kb` state will fail for V1.39 worldless Works, contradicting the stated "no-op for worldless" design.
3. **W-003** (compile-time safety regression): Runtime `sqlx::query_as` replaces compile-time checked queries, violating `nexus-local-db` AGENTS.md rules.

**Positive findings**: Schema migration is additive (ALTER TABLE ADD COLUMN, no drop+recreate). `nexus-kb::extract_finalize` module follows existing patterns (KbStore trait, validation module reuse). `kb.extract_work` capability name preserved per grill-me #13. `WorkFields.world_id` is additive (Option<String>, doesn't break V1.39 callers). CLI `--chapter N` sugar uses standard clap arg pattern. DF-63 tracker correctly marks W5 Shipped. E2E tests cover the happy path (persist → extract → chapter block) and edge cases (worldless skip, idempotency, novel_category validation).
