---
plan_id: 2026-06-21-v1.53-df46-read-capability-slice
working_branch: feature/v1.53-df46-read-capability-slice
review_cwd: main worktree
review_range: e7b369d4..4507e58e
reviewer_index: 1
focus: architecture/maintainability
date: 2026-06-20
verdict: Approve with Notes
---

# QC #1 Review — V1.53 P1 DF-46 Read Slice (architecture/maintainability)

## Summary

Reviewed the single assigned commit `4507e58e feat(v1.53-p1): add 5 read-heavy nexus.* tools + close 3 P0 residuals` over `e7b369d4..4507e58e`. Scope was confined to `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs` and `crates/nexus-daemon-runtime/src/capability_registry.rs`, with cross-checks against the V1.53 compass, the P1 plan, `capability-registry.md`, and `acp-capability-set.md`.

The registry seam is substantially improved: all five P1 IDs are in `TOOL_ALLOWLIST`, `host_tool_registry()` has populated rows, `ACCEPTED_TEST_FN_NAMES` is now bidirectional against registry rows, and the allowlist/registry drift test is bidirectional. P0 residual closure is also mostly successful: work.get, work.patch, schedule_status, and context.assemble backfills are present, catalog parsing works for current Markdown table rows, and the `DaemonToolDispatchAdapter` comment accurately describes the execute → registry dispatch chain.

However, the new world/timeline/KB read handlers introduce an architectural authorization gap: they accept `world_id` and query world-scoped data without proving that the active creator owns or may access that world. This differs from existing `works` patterns and from the capability catalog's world-policy note. I also found incomplete per-tool failure/admission coverage relative to P1 acceptance. Verdict is **Request Changes**.

## Verification evidence

- Alignment verified: `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`; `git branch --show-current` → `feature/v1.53-df46-read-capability-slice`. Pre-existing working tree note: `.gitignore` was already modified and was not touched by this review.
- `git log --oneline e7b369d4..4507e58e` → one commit: `4507e58e feat(v1.53-p1): add 5 read-heavy nexus.* tools + close 3 P0 residuals`.
- `git diff --stat e7b369d4..4507e58e` → 2 files, 900 insertions, 2 deletions.
- GitNexus `detect_changes(scope=compare, base_ref=e7b369d4)` → 8 changed symbols, 14 affected flows, risk `high` (index appears not to include the newly added P1 symbols yet, but confirms host-tool executor/registry impact area).
- `cargo test -p nexus-daemon-runtime --lib capability_registry` → 10 passed, 0 failed.
- `cargo test -p nexus-daemon-runtime --lib host_tool_executor` → 26 passed, 0 failed.
- `cargo check -p nexus-daemon-runtime` → passed.
- `cargo clippy -p nexus-daemon-runtime -- -D warnings` → passed.
- `cargo +nightly fmt --all -- --check` → passed (no output after successful chained command).

## Findings

### Blocking / High severity

- R-V153P1QC1-001: World/timeline/KB read handlers bypass creator/world ownership checks before returning world-scoped data.

  Evidence: the P1 handlers parse `world_id` and call world-scoped readers directly, while ignoring `creator_id`:

  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1170-1185`
    ```rust
    async fn execute_world_snapshot_get(... _creator_id: &str, ...) ...
    gw.get_world_state(world_id)
    ```
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1202-1222`
    ```rust
    async fn execute_timeline_recent_get(... _creator_id: &str, ...) ...
    gw.get_timeline(world_id, None)
    ```
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1238-1254`
    ```rust
    async fn execute_kb_snapshot_read(... _creator_id: &str, ...) ...
    .list_by_world(world_id)
    ```

  The underlying queries also key only by `world_id`, e.g. `nexus-local-db/src/narrative_gateway.rs:201-203` uses `WHERE world_id = ?`, and `nexus-local-db/src/kb_store.rs:447-448` uses `WHERE world_id = ?`. This contrasts with existing Work creation validation, which explicitly checks world ownership: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:429-435` queries `narrative_worlds WHERE world_id = ? AND owner_creator_id = ?`. It also conflicts with `acp-capability-set.md:129-131`, which states world access is determined by policy/membership and capability presence alone does not grant private access.

  Recommendation: before dispatching these reads, add a shared helper such as `ensure_world_accessible_for_creator(pool, creator_id, world_id)` (owner now; policy/membership later), include `WorkspaceBounds`/entity-bound semantics in the rows, and add cross-creator denial tests for all three tools.

### Medium severity

- R-V153P1QC1-002: P1 failure/admission test coverage is incomplete for the five new rows.

  Plan acceptance requires each tool to have at least one success and one failure/admission vector (`.mstar/plans/2026-06-21-v1.53-df46-read-capability-slice.md:28-29`). The new test block has success tests for all five, but only one P1-specific negative test: `world_snapshot_get_rejects_missing_world_id` at `host_tool_executor.rs:1919-1935`. There are no negative/admission tests for `timeline.recent.get`, `kb_snapshot.read`, `manuscript.chapter.get`, or `observability.daemon.health`; nor are there cross-creator/admission tests for the world-scoped tools.

  Recommendation: add at least one failure/admission test per P1 capability. For the world-scoped reads, make the negative vector cross-creator/world access denial. For daemon health, a missing active creator or policy denial would cover admission behavior.

### Low severity

(none)

### Nit / observation

- R-V153P1QC1-003: A few comments still describe the surface as V1.34-only after P1 expansion. Examples: `host_tool_executor.rs:47-48` says “Allowlist of all V1.34 tool IDs,” and `capability_registry.rs:244` says “Create a registry pre-populated with all 8 V1.34 host tools,” even though the runtime surface is now 13 host tools. This is not behaviorally risky, but it makes future registry maintenance slightly harder.

- Architectural note on `manuscript.chapter.get`: verifying Work ownership through `works::get_work(state.pool(), creator_id, work_id)` at `host_tool_executor.rs:1294-1304` is sufficient for current cross-creator isolation if `work_id` remains globally unique and `work_chapters` remains keyed by that Work. For defense-in-depth and future maintainability, I would still prefer a `get_chapter_for_creator` helper (join `work_chapters` to `works`) once more manuscript read surfaces are added.

## Verdict

**Request Changes**

The registry synchronization and P0 residual backfills are directionally sound and the targeted Rust gates pass, but the new world/timeline/KB handlers need an explicit creator/world access gate before this read slice should be accepted. The missing P1 negative/admission vectors also leave the acceptance criteria under-proven.

---

## Targeted re-review (fix-wave, commit 4507e58e..4d8fb458)

**Date**: 2026-06-20
**Reviewer**: qc-specialist (Reviewer #1)
**Verdict**: Approve with Notes

### Fix verification

#### R-V153P1QC1-001 (cross-creator/world isolation)

The blocking isolation gap is resolved. `host_tool_executor.rs:1174-1198` now adds `ensure_world_accessible_for_creator(pool, creator_id, world_id)`, using the expected `narrative_worlds WHERE world_id = ? AND owner_creator_id = ?` pattern and returning `Forbidden` for missing or cross-creator worlds. The three world-scoped handlers call this helper immediately after parsing `world_id` and before any world-state, timeline, or KB fetch (`world_snapshot.get` at line 1214, `timeline.recent.get` at line 1248, `kb_snapshot.read` at line 1284). I found no alternate handler or worker/schedule path that bypasses the registry wrapper and helper; all routes still converge through `HostToolExecutor::execute()` → `registry_dispatch()` → `CapabilityRegistry::dispatch()`.

`AdmissionGate::RequireWorldOwnership` is present and the three P1 registry rows include it. The gate is declarative rather than centrally enforced in the registry, but that is consistent with the current architecture where handler-level entity predicates implement `WorkspaceBounds`. Keeping the helper in `host_tool_executor.rs` is acceptable for this P1 slice because all call sites are local and the policy is owner-only; if future world-scoped tools are added, this should move to a shared world-access helper module to avoid drift.

#### R-V153P1QC1-002 (failure/admission test coverage)

The coverage gap is resolved for P1 acceptance. I verified the new cross-creator denial tests for all three world-scoped tools, missing-parameter/admission tests for the remaining P1 tools, and the two timeline limit tests. The cross-creator tests would fail if the helper were removed: without the pre-fetch ownership check, `world_snapshot.get` would return the other creator's world, while timeline/KB reads would return successful empty/visible results instead of `FORBIDDEN`.

#### R-V153P1QC1-003 (stale V1.34-only comments)

The two stale comments from the initial report are updated: `host_tool_executor.rs:47` now says “V1.34 + V1.53 P1 tool IDs,” and `capability_registry.rs:246` says “V1.34 + V1.53 P1 host tools.” A repo search for `V1.34-only`, `Allowlist of all V1.34 tool IDs`, and `all 8 V1.34 host tools` under Rust sources found no remaining matches.

#### R-V153P1QC3-001 (bonus, qc3 timeline LIMIT)

The handler now parses `limit`, defaults to 100, clamps to 500, and passes `Some(limit)` into `get_timeline`. Trait and call sites were updated (`moment.rs`, `narrative_write.rs`, local-db context assembly, and tests pass `None` where unlimited behavior is intended). The SQLite implementation applies a server-side `LIMIT` when `Some(n)` is provided and orders by descending recency, with the handler reversing for ASC display.

One note: the implementation uses a sanitized dynamic clause `LIMIT {limit_i64}` rather than the assignment-described bound `LIMIT ?`. Because the value comes from `usize` → `i64` conversion and not user-provided SQL text, I do not consider this a blocker, but a bound parameter would better match the stated fix shape and reduce future maintenance risk.

### Verification evidence

- Alignment: `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`; `git branch --show-current` → `feature/v1.53-df46-read-capability-slice`.
- `git log --oneline 4507e58e..4d8fb458` reviewed; fix-wave includes `4d8fb458` plus qc2/qc3/.gitignore commits.
- Targeted tests passed: `world_snapshot_get_cross_creator_denied`, `timeline_recent_get_cross_creator_denied`, `kb_snapshot_read_cross_creator_denied`, `_rejects_` (31 passed), `_respects_server_limit`, `_clamps_limit_to_500`.
- Build hygiene passed: `cargo check -p nexus-daemon-runtime`; `cargo check -p nexus-local-db`; `cargo clippy -p nexus-daemon-runtime -- -D warnings`; `cargo clippy -p nexus-local-db -- -D warnings`; `cargo +nightly fmt --all -- --check`.
- Full daemon-runtime test suite passed: `cargo test -p nexus-daemon-runtime` (lib 223 passed; integration/doc tests passed; non-fatal pre-existing test warnings emitted during test compilation only).

### New findings (if any)

#### Medium severity

(none)

#### Low severity

- R-V153P1QC1R-001: `SqliteNarrativeGateway::get_timeline` implements server-side limiting with a dynamic `LIMIT {limit_i64}` clause instead of a bound `LIMIT ?`. The value is numeric and sanitized, so this is not a security blocker, but a bound parameter would better match the intended implementation and future-proof the query.

### Architectural judgment

P1 is now ready to merge from an architecture/maintainability perspective. The ownership helper closes the data-isolation hole at the correct seam, the registry rows now document the world-ownership admission requirement, and the new tests make the critical behavior regression-resistant. The remaining LIMIT binding note is small and can be handled opportunistically.

### Verdict

**Approve with Notes**

The previous blocking and medium findings are resolved; only one low-risk maintainability note remains.
