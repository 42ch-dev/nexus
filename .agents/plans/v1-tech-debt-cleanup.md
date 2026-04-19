# V1 Tech Debt Cleanup (Long-term)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Address residuals from V1.1 development and architecture alignment gaps in bounded batches, improving production readiness and code quality without blocking feature work.

**Source:** Consolidated from all plans' residual findings (high, medium, low severity) + architecture alignment review (`.agents/knowledge/architecture-alignment-review-v1.md`).

**Scope:** **Long-lived plan** covering tech-debt and architecture-alignment work across **V1.x** (V1.1 through V1.2 and later V1 releases until the program explicitly retires this plan). **Milestones:** A–D are **delivered** (C+D completed 2026-04-10). **Batch E** (TD-11, TD-13) is scheduled on the **V1.2 track** but remains **owned by this same plan id** — V1.2 is still V1, not a separate product line for plan ownership.

**Priority:** **Lowest** — backfill only; pick up when higher-priority delivery allows. **Do not** mark this plan **Done** merely because Batch D closed; keep **InProgress** until Batch E (and any agreed V1.x follow-on) is done or the team moves remaining items to another canonical plan.

---

## Batch Progress


| Batch       | Status                   | Residuals                                                | Effort             | Target                             |
| ----------- | ------------------------ | -------------------------------------------------------- | ------------------ | ---------------------------------- |
| **Batch A** | ✅ Completed (2026-04-09) | 3 critical (CTX-R4, SYNC-R10, SYNC-R13)                  | S (~2 sessions)    | V1.0-phase2 blocker                |
| **Batch B** | ✅ Completed (2026-04-08) | 4 low (QC-W2, QC-W4, QC-W3, QC-W7)                       | S (~2 sessions)    | V1.1 milestone                     |
| **Batch C** | ✅ Completed (2026-04-10) | QC-W1–W11 + **DM-R3** (pairing / fork_branch edge tests) | M (~4 sessions)    | V1.1+ milestone                    |
| **Batch D** | ✅ Completed (2026-04-10) | TD-7 resolve; TD-8/10 waive + docs; TD-9 minimal API + doc | L (~5–9 sessions)  | V1.1+ milestone                    |
| **Batch E** | 🔶 Next (V1.2 schedule)    | 2 low (TD-11, TD-13)                                     | L (~6–16 sessions) | **V1.2 track — same plan**         |


**Total Residuals**: 18 tracked historically; **open under this plan**: Batch E + any V1.x follow-up from waived/partial Batch D items (see knowledge docs).

**Source Breakdown**:

- QC reviews: critical/low batches A/B done; remaining low work in **Batch C** (in scope)
- Architecture alignment: **Batch D** (4 medium) **completed** (2026-04-10); **Batch E** (2 low) **next on V1.2 schedule, still this plan**

**Categories**:

- **MEDIUM (Batch D, in scope):** ForkBranch naming, dual outbox, daemon state machine, OAuth (TD-7–TD-10)
- **LOW (Batch C, in scope):** QC-driven docs, tests, type alignment
- **LOW (Batch E, V1.2 schedule):** TD-11, TD-13 — same plan id; staff when V1.2 capacity opens
  - 4 运行时观测性改进（Pool + HTTP error handling）— completed in Batch B
  - 8 长期改进（文档 + 测试基础设施 + 类型对齐）— Batch C（本里程碑）
  - 4 架构对齐改进（命名一致性 + 双 outbox + 状态机 + OAuth）— Batch D（本里程碑）
  - 2 完整性改进（缺失 CLI 命令 + 测试覆盖）— Batch E（**V1.2 排期，仍属本计划**）

---

## Task Groups

### Batch A: Critical Residuals (✅ Completed)

**Completed**: 2026-04-09

#### Task 1: CTX-R4 — WorldId format validation ✅

**Status**: Completed (already implemented in prior batch)

**Files**:

- `crates/nexus42/src/commands/context.rs`

**Acceptance Criteria**:

- Add clap validator for `wld`_ prefix pattern
- Add unit tests for validation
- Update error messages to be user-friendly

**Evidence**: Tests passing; residual archived to `.agents/archived/residuals/2025-04-05-context-assembly.json`

---

#### Task 7: SYNC-R10 — Outbox schema migration documentation ✅

**Status**: Completed

**Files**:

- `crates/nexus-sync/src/outbox.rs`

**Acceptance Criteria**:

- Add schema_version column design
- Document migration strategy
- Add migration examples

**Evidence**: Documentation added to `outbox.rs` comments; residual archived.

---

#### Task 8: SYNC-R13 — AutoReject confirmation prompt ✅

**Status**: Completed

**Files**:

- `crates/nexus-sync/src/conflict.rs`
- `crates/nexus42/src/commands/sync.rs` (new Resolve subcommand)

**Acceptance Criteria**:

- Add confirmation prompt for auto-reject
- Add `--force` flag to bypass prompt
- Add tests

**Evidence**: 4 new tests passing; residual archived.

---

### Batch B: Runtime Observability & Pool Configuration (✅ Completed)

**Estimated Effort**: S (~2 sessions)
**Target**: V1.1 milestone
**Focus**: Pool configuration + HTTP error handling improvements
**Completed**: 2026-04-08

#### Task 1: QC-W2 — HTTP body size error variant ✅

**Priority**: Low (severity: warning → low per plan-convention.md §209-211)
**Source**: QC#1, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus-sync/src/sync_client.rs:258-284`

**Context**: HTTP body size errors currently use `Serialization` error variant, which is semantically incorrect.

**Acceptance Criteria**:

- Add dedicated `HttpBodySizeExceeded` error variant to appropriate error enum
- Update sync_client.rs to use new variant for body size errors
- Add unit tests for new error variant
- Document error handling behavior

**Evidence**: Added HttpBodySizeExceeded variant to SyncError, updated sync_client.rs, added tests

**Target**: V1.1+

---

#### Task 2: QC-W4 — InvalidParameterName misuse ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#2, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus42d/src/db/pool.rs:163-171`

**Context**: `InvalidParameterName` error is used for pool-related errors, which is not semantically appropriate.

**Acceptance Criteria**:

- Review pool.rs error handling
- Replace `InvalidParameterName` with domain-specific error variant if applicable
- Update error messages to be more descriptive
- Add tests for error scenarios

**Evidence**: Replaced InvalidParameterName with SqliteFailure in interact_to_rusqlite_err, added documentation

**Target**: Next batch

---

#### Task 3: QC-W3 — Pool status monitoring ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#1, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus42d/src/db/pool.rs:59-61`

**Context**: Pool status monitoring is not exposed, limiting observability.

**Acceptance Criteria**:

- Add pool status monitoring endpoint or API
- Expose pool metrics (active connections, idle connections, pool size)
- Add tracing for pool status changes
- Document monitoring capabilities

**Evidence**: Added GET /v1/local/monitoring/pool endpoint, added tracing, updated documentation

**Target**: V1.1+

---

#### Task 4: QC-W7 — Pool configuration ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#3, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus42d/src/db/pool.rs:39-46`

**Context**: Pool configuration (timeout, max size) is not tunable via config.

**Acceptance Criteria**:

- Add configurable pool timeout to DbPool::new()
- Add configurable max pool size
- Document default values and configuration options
- Add builder pattern or config struct for pool configuration
- Add tests for configuration variations

**Evidence**: PoolConfig already implemented with builder pattern, from_env(), and tests

**Target**: V1.1+

---

### Batch C: Documentation, Testing Infrastructure & Type Alignment (🔶 In progress)

**Estimated Effort**: M (~4 sessions)
**Target**: V1.1+ milestone (long-term tracking)
**Focus**: Documentation, test infrastructure improvements, type alignment

#### Task 5: QC-W1 — Migration documentation placement ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#1, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus-sync/src/outbox.rs:28-44`

**Context**: Migration documentation should be in a dedicated doc file, not inline comments.

**Acceptance Criteria**:

- Extract migration documentation to `.agents/archived/knowledge/outbox-schema-v2.md`
- Keep inline summary in outbox.rs comments
- Add migration guide for developers

**Target**: Before V1.1 release

---

#### Task 6: QC-W9 — CI script extraction ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#1 (Wave 3 Batch B), status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `.github/workflows/ci.yml:119-177`

**Context**: CI shell script embedded in YAML workflow is hard to read and maintain.

**Acceptance Criteria**:

- Extract CI shell script to `tooling/check-schema-drift.sh`
- Update CI workflow to call standalone script
- Add script documentation and usage examples
- Test extracted script locally

**Target**: V1.1+

---

#### Task 7: QC-W5 — Streaming body reader ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#2, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus-sync/src/sync_client.rs`

**Context**: Body size check previously used `Response::text()`, buffering the full body before enforcing limits.

**Acceptance Criteria**:

- Implement streaming body reader with size limit check
- Use chunked reading to avoid full buffer
- Add tests for large body handling
- Document streaming approach

**Evidence**: `SyncClient::read_response_body_limited` uses `Response::chunk()`; wiremock integration test `crates/nexus-sync/tests/body_size_limit.rs`.

**Target**: V1.1+

---

#### Task 8: QC-W6 — Test helper cleanup ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#2, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus-sync/src/outbox.rs` (`Outbox::new_in_memory`)

**Context**: `new_in_memory()` leaked `TempDir` via `mem::forget`, leaving temp directories on disk after test runs.

**Acceptance Criteria**:

- Review test helper cleanup logic
- Ensure temp dirs are cleaned up in all scenarios (success, failure, panic)
- Add cleanup verification tests
- Document test helper behavior

**Evidence**: `Outbox` holds `#[cfg(test)] Option<Arc<TempDir>>` so the temp directory drops with the last `Outbox` clone; `init_pool_with_schema` shared with production `with_pool_size`.

**Target**: CI cleanup

---

#### Task 9: QC-W8 — Test count fix ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#3, status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: Historical plan/QC prose (`status.json:614` reference — obsolete after hot compaction)

**Context**: Stale “7 vs 8” test count referred to pre-compaction `status.json` lines, not a failing suite.

**Acceptance Criteria**:

- Verify actual test count in affected module
- Update plan/task documentation with correct count
- Ensure test count tracking is consistent

**Evidence**: This task row + residual archived; canonical counts come from `cargo test` / CI, not static plan line numbers.

**Target**: Before merge

---

#### Task 10: QC-W10 — TempDir type hint ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#1 (Wave 3 Batch B), status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `crates/nexus42d/src/test_utils.rs` (QC originally cited nexus-sync; daemon helpers were the real site)

**Context**: TempDir ownership semantics could benefit from type hint wrapper.

**Acceptance Criteria**:

- Add newtype wrapper or compile-time hint for TempDir ownership
- Document ownership semantics
- Add tests for wrapper behavior
- Update all TempDir usages to use new wrapper

**Evidence**: `TestTempRoot` newtype with `#[must_use]` + `Deref`; `create_test_workspace` / `create_initialized_test_workspace` return types updated.

**Target**: V1.1+

---

#### Task 11: QC-W11 — Schema version type alignment ✅

**Priority**: Low (severity: warning → low)
**Source**: QC#1 (Wave 3 Batch B), status.json residual_findings["2026-04-08-v1.1-tech-debt-mitigation"]
**Scope**: `tooling/check-schema-drift.sh` + generated contracts

**Context**: Ensure Rust `LATEST_SCHEMA_VERSION` (u32) and TypeScript export stay numerically aligned.

**Acceptance Criteria**:

- Standardize schema version type across CLI, daemon, and contracts
- Update CI check to use consistent type
- Add tests for schema version comparison
- Document schema version type decision

**Evidence**: `check-schema-drift.sh` parses `pub const LATEST_SCHEMA_VERSION: u32` and `export const LATEST_SCHEMA_VERSION` and fails CI on mismatch.

**Target**: V1.1+

---

#### Task 12: DM-R3 — Edge-case test coverage ✅

**Priority**: Low
**Source**: QC-#2, status.json residual_findings["2025-04-05-domain-models"]
**Scope**: `crates/nexus-domain/src/pairing.rs`, `crates/nexus-domain/src/fork_branch.rs`

**Context**: Edge-case test coverage for Pairing and ForkBranch lifecycle missing.

**Acceptance Criteria**:

- Add boundary/edge-case tests for concurrent pairing conflicts
- Add tests for fork cycle detection
- Verify existing tests cover key scenarios
- Document test coverage

**Evidence**: Unit tests: `authorizes_false_after_revoke_even_when_ids_match`, `two_distinct_active_pairings_same_creator_user_both_authorize`, `revoke_rejects_non_active_status`; `fork_from_allows_child_world_equal_to_parent_world_id`, `request_verification_fails_when_archived`, `request_verification_second_call_invalid_transition`, `verify_after_reject_is_invalid_transition`, `validate_write_scope_fails_when_archived`. Residual archived to `archived/residuals/2025-04-05-domain-models.json`.

**Target**: V1.1

---

### Batch D: Architecture Alignment — Medium Priority ✅ (closed 2026-04-10)

**Estimated Effort**: L (~5–9 sessions) — **met via resolved + waived + minimal slice per task**
**Target**: V1.1+ milestone (**milestone delivered**; plan continues for E + follow-ons)
**Focus**: Architecture alignment gaps identified in architecture alignment review (TD-7–TD-10)
**Source**: `.agents/knowledge/architecture-alignment-review-v1.md` §2.6

**Closure**: TD-7 resolved (tests + knowledge). TD-8 / TD-10 waived with knowledge + code/doc pointers. TD-9: minimal `GET /v1/local/daemon/status` + tests + knowledge; full §10.1 state machine **out of scope** until V1.2+. Archive: `.agents/archived/residuals/v1-tech-debt-cleanup-batch-d.json`.

#### Task 13: TD-7 — ForkBranch field naming consistency verification ✅

**Priority**: Medium
**Source**: Architecture alignment review TD-7, `.agents/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus-domain/src/fork_branch.rs`, `crates/nexus-contracts/src/generated/fork_branch.rs`, spec `data-model-v1.md`

**Context**: Spec uses `parent_branch_id` and `forked_from_event_id`; contracts previously had only `forked_from_event_id`. The review found contracts now add `parent_branch_id` matching spec, but field naming may differ slightly from the domain model. TD-12 noted "no actual issue" after closer inspection, but TD-7 remains as a verification action item to confirm consistency across all three layers.

**Acceptance Criteria**:

- Verify `parent_branch_id` and `forked_from_event_id` naming in domain model, contracts, and spec are consistent
- Document verification results and any discrepancies
- Add tests for ForkBranch field serialization/deserialization if gaps found
- Update domain or contracts if naming inconsistency is confirmed

**Target**: V1.1+

**Evidence**: [fork-branch-contract-alignment-v1.md](archived/knowledge/fork-branch-contract-alignment-v1.md) (archived 2026-04-17); `test_fork_branch_parent_branch_and_event_ids_roundtrip` in `crates/nexus-domain/src/contract_assertions.rs`.

---

#### Task 14: TD-8 — Consolidate dual outbox implementations ⏸️ waived (documented)

**Priority**: Medium
**Source**: Architecture alignment review TD-8, `.agents/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus-local-db/src/schema.rs` (daemon `outbox` table), `crates/nexus-sync/src/outbox.rs` (`outbox_entries` table)

**Context**: The daemon has its own `outbox` table in `nexus-local-db`, while `nexus-sync` has a richer `outbox_entries` table with delivery state tracking, retry logic, and precheck hooks. This dual implementation creates data inconsistency risk and duplicated schema management. The consolidation should migrate the daemon's outbox usage to delegation to `nexus-sync::Outbox` or unify the schema.

**Acceptance Criteria**:

- Analyze both outbox table schemas and identify differences
- Design unified outbox schema (or delegation strategy) that preserves all functionality
- Migrate daemon code to use `nexus-sync::Outbox` or unified schema
- Update database migration scripts
- Add tests for unified outbox behavior
- Document the consolidation approach and migration path

**Target**: V1.1+

**Evidence / decision**: Full merge deferred; rationale and follow-up in [dual-outbox-architecture-v1.md](knowledge/dual-outbox-architecture-v1.md). Archived as waived in `archived/residuals/v1-tech-debt-cleanup-batch-d.json`.

---

#### Task 15: TD-9 — Implement daemon lifecycle state machine 🔶 → partial (endpoint + doc)

**Priority**: Medium
**Source**: Architecture alignment review TD-9, `.agents/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus42d/src/main.rs`, spec `cli-spec-v1.md` §10.1

**Context**: Spec §10.1 defines 6 daemon lifecycle states (Stopped, Starting, Running, Degraded, Stopping, Failed) with explicit transition rules. Current implementation has a simple start-and-serve model with no state machine. This limits graceful degradation, status tracking, and recovery capabilities.

**Acceptance Criteria**:

- Define `DaemonState` enum with 6 states per spec §10.1
- Implement state transitions with proper guards (Stopped→Starting, Starting→Running/Degraded/Failed, etc.)
- Add `/v1/local/daemon/status` endpoint exposing current state
- Integrate health checks for Degraded state detection
- Handle graceful shutdown with Stopping→Stopped transitions
- Add unit tests for all state transitions and edge cases
- Document state machine diagram and transition rules

**Target**: V1.1+

**Evidence**: `GET /v1/local/daemon/status` in `crates/nexus42d` (handler `daemon_status`, tests in `api/middleware.rs`, `tests/integration.rs`). Gap vs full FSM: [daemon-lifecycle-api-v1.md](archived/knowledge/daemon-lifecycle-api-v1.md) (archived 2026-04-17; full 6-state HSM design now in [knowledge/daemon-lifecycle-api-v2.md](knowledge/daemon-lifecycle-api-v2.md)).

---

#### Task 16: TD-10 — Implement real device flow OAuth ⏸️ waived (platform-dependent)

**Priority**: Medium
**Source**: Architecture alignment review TD-10, `.agents/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus42d/src/auth/device_flow.rs`, platform auth endpoints

**Context**: Current auth flow generates mock tokens (`at_<uuid>`, `usr_mock_<uuid>`) with `verify_device_code` returning `Ok(false)`. Production requires real OAuth device flow with the platform. This is a medium-priority architecture gap since it's explicitly a V1.1+ scope item but critical for production deployment.

**Acceptance Criteria**:

- Define platform OAuth endpoints (token exchange, device code verification, refresh)
- Replace mock token generation with real platform OAuth flow
- Implement token refresh logic with proper expiry handling
- Add token storage encryption at rest
- Add error handling for network failures and invalid tokens
- Add integration tests (with platform auth stub for CI)
- Document OAuth flow and configuration

**Target**: V1.1+ (platform dependency)

**Evidence / decision**: Stub retained; module doc + [device-flow-oauth-scope-v1.md](knowledge/device-flow-oauth-scope-v1.md). Archived as waived in `archived/residuals/v1-tech-debt-cleanup-batch-d.json`.

---

### Batch E: CLI Completeness & Test Coverage — 🔶 Next (V1.2 schedule, same plan)

**Scheduling:** Execute when **V1.2** work is staffed — still under **`v1-tech-debt-cleanup`** (long-lived V1.x plan). Does not retroactively change C/D completion dates.

**Estimated Effort**: L (~6–16 sessions)
**Target**: **V1.2 milestone** (within V1 product line)
**Focus**: Missing CLI commands and integration test coverage from architecture alignment review (TD-11, TD-13)
**Source**: `.agents/knowledge/architecture-alignment-review-v1.md` §2.3, §2.6

#### Task 17: TD-11 — Complete missing CLI commands 🔶

**Priority**: Low
**Source**: Architecture alignment review TD-11, `.agents/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus42/src/commands/`

**Context**: CLI command coverage is approximately 65% functional. Missing commands: `clone <world-ref>`, `link/unlink`, `config` command group, `publish chapter/story`, `debug dump-workspace/replay-delta`, and `doctor`. Some existing commands have partial implementations (`auth profiles`, `daemon restart/logs`).

**Acceptance Criteria**:

- Implement `nexus42 clone <world-ref>` command
- Implement `nexus42 link` / `nexus42 unlink` commands
- Implement `nexus42 config get/set/unset/path` command group
- Implement `nexus42 publish chapter/story` commands (V1.2 per spec)
- Implement `nexus42 debug dump-workspace/replay-delta` commands
- Implement `nexus42 doctor` diagnostic command
- Add unit tests for each new command
- Update CLI help text and shell completions

**Target**: V1.2+

---

#### Task 18: TD-13 — Improve integration test coverage 🔶

**Priority**: Low
**Source**: Architecture alignment review TD-13, `.agents/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus42/tests/`, `crates/nexus42d/tests/`, `crates/nexus-sync/`

**Context**: No `cargo test` results were available for the architecture review. Domain logic has unit tests in `#[cfg(test)]` modules, but integration tests for sync workflows, CLI commands, and daemon handlers appear incomplete. This affects regression safety.

**Acceptance Criteria**:

- Audit current test coverage across nexus42, nexus42d, and nexus-sync
- Add integration tests for sync push/pull/reject workflows
- Add integration tests for CLI command execution paths
- Add integration tests for daemon HTTP API endpoints
- Add integration tests for ACP client session lifecycle
- Set up CI coverage reporting (optional: codecov or similar)
- Document coverage baseline and improvement targets

**Target**: V1.2+

---

## Execution Order

**Multi-batch Execution**:

1. **Batch A (Completed)**: 3 critical residuals (CTX-R4, SYNC-R10, SYNC-R13) — 2026-04-09
2. **Batch B (Completed)**: 4 low residuals (QC-W2, QC-W4, QC-W3, QC-W7) — 2026-04-08
3. **Batch C (Completed)**: QC-W5–W11 + DM-R3 — **2026-04-10**
4. **Batch D (Completed)**: TD-7 resolved; TD-8/10 waived + documented; TD-9 minimal API + doc — **2026-04-10**
5. **Batch E (Next)**: TD-11, TD-13 — **V1.2 schedule; same plan id**

**Parallelization**: Batch B tasks (Pool + HTTP error) completed in single session. **C and D** are lowest-priority backfill. **E** starts when V1.2 capacity allows. Deferred Batch D follow-ups (full outbox merge, §10.1 FSM, production OAuth) may run in parallel with E as separate tasks under this plan or linked plans.

**Scheduling note**: TD-7 (ForkBranch naming verification) is XS effort and may be completed quickly as a confidence check. TD-10 (OAuth) depends on platform auth endpoints being available.

**Plan lifecycle**: **Milestone C+D** is **complete** (2026-04-10). The **overall plan** stays **InProgress** until Batch E is delivered (or remaining work is **explicitly** moved to another plan id). Do not use “Batch D done” as a trigger to mark the long-lived plan Done.

---

## Validation Commands

**After each batch**:

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo +nightly fmt --all -- --check
```

---

## Plan Completion Criteria

**Milestone C+D (met 2026-04-10):**

- Batch A–D: delivered per batch sections above (D includes waived/partial items with knowledge).

**Required before marking this long-lived plan Done** (or handing off remaining scope to another canonical plan):

- Batch E: TD-11 and TD-13 meet their acceptance criteria **or** are formally waived/archived with PM decision
- Any **explicit** program decision on deferred D follow-ups (outbox unification, full daemon FSM, production OAuth) — tracked via knowledge docs + `status.json` / residuals as applicable
- Tests / clippy / docs: maintained per batch (see Validation Commands)

**Current status**: **InProgress** — **lowest priority**; milestone **A–D complete**; **Batch E** next on **V1.2** schedule under **this plan id**  
**Residuals SSOT**: `status.json` → `metadata.residual_findings` (program-wide items may live on sibling plan ids)

**Audit snapshot** (C+D milestone row, historical only): `archived/plans/v1-tech-debt-cleanup-milestone-cd-2026-04-10.json`

---

## Dependencies

- **External**: None
- **Internal**: V1.1 GA complete (confirmed)

---

## Notes

This plan is a **long-term tech-debt cleanup** with **lowest scheduling priority**, spanning **V1.x** (V1.1 through V1.2+ until retired). **C+D milestone completed 2026-04-10**; the plan remains **InProgress** for **Batch E** and V1.x follow-on work. Point-in-time row after C+D is preserved as `archived/plans/v1-tech-debt-cleanup-milestone-cd-2026-04-10.json` (audit only).

**Batch A** (3 critical residuals) was prioritized as V1.0-phase2 blocker and completed on 2026-04-09.

**Batch B** (4 low residuals) completed on 2026-04-08, targeting runtime observability (Pool configuration + HTTP error handling). Implemented:

- QC-W2: Added HttpBodySizeExceeded error variant
- QC-W4: Fixed InvalidParameterName misuse in pool error handling
- QC-W3: Added pool status monitoring endpoint (/v1/local/monitoring/pool)
- QC-W7: Verified PoolConfig with builder pattern and environment variable support

**Batch C** completed 2026-04-10 (QC-W1/W9/W5/W6/W8/W10/W11 + DM-R3 + schema drift check + related tests).

**Batch D** closed 2026-04-10 per C+D cap:

- TD-7: Resolved (alignment doc + roundtrip test).
- TD-8: Waived consolidation; documented dual outbox and V1.2+ follow-up.
- TD-9: Minimal `GET /v1/local/daemon/status`; full §10.1 FSM deferred.
- TD-10: Waived real OAuth in OSS milestone; stub documented with platform dependency.

**Batch E** (2 low) — **next** on **V1.2 schedule**, **same plan** (`v1-tech-debt-cleanup`):

- TD-11: Complete missing CLI commands (~65% coverage → full coverage)
- TD-13: Improve integration test coverage for sync, CLI, and daemon

**Reorganization**: Plan reorganized on 2026-04-08 based on actual status.json residuals (12 open). Batch B/C restructured per technical domain analysis (方案 A). Batch D/E added on 2026-04-09 from architecture alignment review TD-7–TD-13 (TD-12 excluded as "no actual issue — naming is consistent").

**Created**: 2026-04-09
**Status**: **InProgress** — **priority: lowest**; milestones **A–D** delivered; **Batch E** (V1.2 track) outstanding  
**Updated**: 2026-04-11 (restore long-lived plan after C+D milestone; Batch E in-scope)