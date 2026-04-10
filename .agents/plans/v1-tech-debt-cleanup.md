# V1 Tech Debt Cleanup (Long-term)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Address residuals from V1.1 development and architecture alignment gaps in bounded batches, improving production readiness and code quality without blocking feature work.

**Source:** Consolidated from all plans' residual findings (high, medium, low severity) + architecture alignment review (`.agents/plans/knowledge/architecture-alignment-review-v1.md`).

**Scope ceiling (program intent):** For the current V1.1-era program, implement **at most Batch C and Batch D**. **Batch E is deferred to V1.2** — do not schedule or staff it under this milestone; track TD-11 / TD-13 only as a future V1.2 workstream.

**Priority:** **Lowest** — backfill only; pick up when higher-priority delivery allows. After C+D, this plan may be marked **Done** even though Batch E items remain intentionally out of scope until V1.2.

---

## Batch Progress


| Batch       | Status                   | Residuals                                                                                 | Effort             | Target                             |
| ----------- | ------------------------ | ----------------------------------------------------------------------------------------- | ------------------ | ---------------------------------- |
| **Batch A** | ✅ Completed (2026-04-09) | 3 critical (CTX-R4, SYNC-R10, SYNC-R13)                                                   | S (~2 sessions)    | V1.0-phase2 blocker                |
| **Batch B** | ✅ Completed (2026-04-08) | 4 low (QC-W2, QC-W4, QC-W3, QC-W7)                                                        | S (~2 sessions)    | V1.1 milestone                     |
| **Batch C** | 🔶 In progress           | QC-W5–W11 + QC-W8 doc done 2026-04-10; **DM-R3** still open (plan `2025-04-05-domain-models`) | M (~4 sessions)    | V1.1+ milestone                    |
| **Batch D** | 🔶 Pending               | 4 medium (TD-7, TD-8, TD-9, TD-10)                                                        | L (~5–9 sessions)  | V1.1+ (in scope after C)           |
| **Batch E** | ⏸️ Deferred              | 2 low (TD-11, TD-13)                                                                      | L (~6–16 sessions) | **V1.2 only** — not this milestone |


**Total Residuals**: 18 tracked historically; **in-scope for closing this plan**: Batch C + D only (Batch E deferred to V1.2).

**Source Breakdown**:

- QC reviews: critical/low batches A/B done; remaining low work in **Batch C** (in scope)
- Architecture alignment: **Batch D** (4 medium) in scope; **Batch E** (2 low) **deferred to V1.2**

**Categories**:

- **MEDIUM (Batch D, in scope):** ForkBranch naming, dual outbox, daemon state machine, OAuth (TD-7–TD-10)
- **LOW (Batch C, in scope):** QC-driven docs, tests, type alignment
- **LOW (Batch E, V1.2 deferral):** TD-11, TD-13 — do not schedule under this plan until V1.2
  - 4 运行时观测性改进（Pool + HTTP error handling）— completed in Batch B
  - 8 长期改进（文档 + 测试基础设施 + 类型对齐）— Batch C（本里程碑）
  - 4 架构对齐改进（命名一致性 + 双 outbox + 状态机 + OAuth）— Batch D（本里程碑）
  - 2 完整性改进（缺失 CLI 命令 + 测试覆盖）— Batch E（**仅 V1.2**）

---

## Task Groups

### Batch A: Critical Residuals (✅ Completed)

**Completed**: 2026-04-09

#### Task 1: CTX-R4 — WorldId format validation ✅

**Status**: Completed (already implemented in prior batch)

**Files**:

- `crates/nexus42/src/commands/context.rs`

**Acceptance Criteria**:

- Add clap validator for `wld_` prefix pattern
- Add unit tests for validation
- Update error messages to be user-friendly

**Evidence**: Tests passing; residual archived to `.agents/plans/archived/residuals/2025-04-05-context-assembly.json`

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

- Extract migration documentation to `docs/migrations/outbox-schema-v2.md`
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

#### Task 12: DM-R3 — Edge-case test coverage 🔶

**Priority**: Low
**Source**: QC-#2, status.json residual_findings["2025-04-05-domain-models"]
**Scope**: `crates/nexus-domain/src/pairing.rs`, `crates/nexus-domain/src/fork_branch.rs`

**Context**: Edge-case test coverage for Pairing and ForkBranch lifecycle missing.

**Acceptance Criteria**:

- Add boundary/edge-case tests for concurrent pairing conflicts
- Add tests for fork cycle detection
- Verify existing tests cover key scenarios
- Document test coverage

**Target**: V1.1

---

### Batch D: Architecture Alignment — Medium Priority (🔶 Pending)

**Estimated Effort**: L (~5–9 sessions)
**Target**: V1.1+ milestone
**Focus**: Architecture alignment gaps identified in architecture alignment review (TD-7–TD-10)
**Source**: `.agents/plans/knowledge/architecture-alignment-review-v1.md` §2.6

#### Task 13: TD-7 — ForkBranch field naming consistency verification 🔶

**Priority**: Medium
**Source**: Architecture alignment review TD-7, `.agents/plans/knowledge/architecture-alignment-review-v1.md`
**Scope**: `crates/nexus-domain/src/fork_branch.rs`, `crates/nexus-contracts/src/generated/fork_branch.rs`, spec `data-model-v1.md`

**Context**: Spec uses `parent_branch_id` and `forked_from_event_id`; contracts previously had only `forked_from_event_id`. The review found contracts now add `parent_branch_id` matching spec, but field naming may differ slightly from the domain model. TD-12 noted "no actual issue" after closer inspection, but TD-7 remains as a verification action item to confirm consistency across all three layers.

**Acceptance Criteria**:

- Verify `parent_branch_id` and `forked_from_event_id` naming in domain model, contracts, and spec are consistent
- Document verification results and any discrepancies
- Add tests for ForkBranch field serialization/deserialization if gaps found
- Update domain or contracts if naming inconsistency is confirmed

**Target**: V1.1+

---

#### Task 14: TD-8 — Consolidate dual outbox implementations 🔶

**Priority**: Medium
**Source**: Architecture alignment review TD-8, `.agents/plans/knowledge/architecture-alignment-review-v1.md`
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

---

#### Task 15: TD-9 — Implement daemon lifecycle state machine 🔶

**Priority**: Medium
**Source**: Architecture alignment review TD-9, `.agents/plans/knowledge/architecture-alignment-review-v1.md`
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

---

#### Task 16: TD-10 — Implement real device flow OAuth 🔶

**Priority**: Medium
**Source**: Architecture alignment review TD-10, `.agents/plans/knowledge/architecture-alignment-review-v1.md`
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

---

### Batch E: CLI Completeness & Test Coverage — ⏸️ Deferred to V1.2 (out of current scope)

**Scheduling:** **Not part of this plan’s milestone.** Execute only after V1.2 planning opens; do not count toward Batch C/D completion or `status.json` progress for this plan until then.

**Estimated Effort**: L (~6–16 sessions)
**Target**: **V1.2+ milestone only**
**Focus**: Missing CLI commands and integration test coverage from architecture alignment review (TD-11, TD-13)
**Source**: `.agents/plans/knowledge/architecture-alignment-review-v1.md` §2.3, §2.6

#### Task 17: TD-11 — Complete missing CLI commands 🔶

**Priority**: Low
**Source**: Architecture alignment review TD-11, `.agents/plans/knowledge/architecture-alignment-review-v1.md`
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
**Source**: Architecture alignment review TD-13, `.agents/plans/knowledge/architecture-alignment-review-v1.md`
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
3. **Batch C (In progress)**: Remaining low (QC-W5, QC-W6, QC-W8, QC-W10, QC-W11, DM-R3); QC-W1 + QC-W9 done — **in scope**
4. **Batch D (Pending)**: 4 medium (TD-7, TD-8, TD-9, TD-10) — **in scope after C**
5. **Batch E (Deferred)**: TD-11, TD-13 — **V1.2 only; excluded from this plan’s schedule**

**Parallelization**: Batch B tasks (Pool + HTTP error) completed in single session. **C and D** are lowest-priority backfill; run only when capacity allows. **E** is not parallelized under this plan before V1.2. Batch D Tasks 14 (dual outbox) and 15 (daemon state machine) have no hard dependency on Batch C.

**Scheduling note**: TD-7 (ForkBranch naming verification) is XS effort and may be completed quickly as a confidence check. TD-10 (OAuth) depends on platform auth endpoints being available.

**Milestone close**: This plan may transition to **Done** once **Batch C and Batch D** meet their completion criteria (or accepted waivers). **Batch E does not block Done** — it is explicitly deferred to V1.2. Each in-scope batch completion updates `status.json` progress and `residual_findings` where applicable.

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

**Required to mark this plan Done (V1.1-era milestone, C+D cap):**

- Batch A: 3 critical residuals resolved (CTX-R4, SYNC-R10, SYNC-R13)
- Batch B: 4 low residuals resolved (QC-W2, QC-W4, QC-W3, QC-W7)
- Batch C: remaining low resolved or waived (QC-W5, QC-W6, QC-W8, QC-W10, QC-W11, DM-R3; QC-W1 + QC-W9 done)
- Batch D: 4 medium gaps resolved or waived (TD-7, TD-8, TD-9, TD-10)
- Tests / clippy / docs: maintained per batch (see Validation Commands)

**Out of scope for this plan’s Done gate:**

- Batch E (TD-11, TD-13) — **deferred to V1.2**; **does not block** marking this plan Done

**Separate product line:**

- V1.2 release readiness (includes Batch E and other V1.2-only work)

**Current status**: InProgress — **lowest priority**; milestone scope **C+D max**; Batch E ⏸️ V1.2  
**Residuals SSOT**: `status.json` → `metadata.residual_findings`

---

## Dependencies

- **External**: None
- **Internal**: V1.1 GA complete (confirmed)

---

## Notes

This plan is a **long-term tech-debt cleanup** with **lowest scheduling priority**. It covers V1.1-era residuals and architecture gaps **up to Batch C + D**. Batch E is **explicitly deferred to V1.2** and must not consume capacity until V1.2 planning. It remains `InProgress` until C+D are finished, without blocking other development.

**Batch A** (3 critical residuals) was prioritized as V1.0-phase2 blocker and completed on 2026-04-09.

**Batch B** (4 low residuals) completed on 2026-04-08, targeting runtime observability (Pool configuration + HTTP error handling). Implemented:

- QC-W2: Added HttpBodySizeExceeded error variant
- QC-W4: Fixed InvalidParameterName misuse in pool error handling
- QC-W3: Added pool status monitoring endpoint (/v1/local/monitoring/pool)
- QC-W7: Verified PoolConfig with builder pattern and environment variable support

**Batch C** tracks long-term quality improvements (documentation, testing infrastructure, type alignment); **in scope** for this plan’s C+D milestone (QC-W1 + QC-W9 done; remainder pending).

**Batch D** (4 medium residuals) tracks architecture alignment gaps from the architecture alignment review (`.agents/plans/knowledge/architecture-alignment-review-v1.md`):

- TD-7: ForkBranch field naming consistency verification
- TD-8: Consolidate dual outbox implementations (daemon vs nexus-sync)
- TD-9: Implement daemon lifecycle state machine (6 states per spec §10.1)
- TD-10: Replace mock OAuth with real device flow

**Batch E** (2 low) — **deferred to V1.2**; traceability only until a V1.2 plan owns TD-11 / TD-13:

- TD-11: Complete missing CLI commands (~65% coverage → full coverage)
- TD-13: Improve integration test coverage for sync, CLI, and daemon

**Reorganization**: Plan reorganized on 2026-04-08 based on actual status.json residuals (12 open). Batch B/C restructured per technical domain analysis (方案 A). Batch D/E added on 2026-04-09 from architecture alignment review TD-7–TD-13 (TD-12 excluded as "no actual issue — naming is consistent").

**Created**: 2026-04-09
**Status**: InProgress — **priority: lowest**; **C+D scope cap**; Batch E → V1.2
**Updated**: 2026-04-10 (scope ceiling; Batch E deferral; priority lowered)