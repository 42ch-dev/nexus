---
plan_id: "2026-04-21-v1.7-ws-e-multi-agent-worker"
verdict: "FAIL"
acceptance_criteria_total: 24
acceptance_criteria_passed: 23
acceptance_criteria_failed: 1
generated_at: "2026-04-22T15:00:00Z"
qa_engineer: "qa-engineer"
---

# QA Verification Report — Multi-Agent Worker (WS-E)

**Plan**: `2026-04-21-v1.7-ws-e-multi-agent-worker`  
**Working branch**: `feature/v1.7-ws-e`  
**Diff basis**: `git diff feature/v1.7...HEAD` (40 files, +9196/-675 lines)  
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`  
**QC Reports Reviewed**: `qc-reviewer-1.md` (Request Changes → fixed), `qc-reviewer-3.md` (Approve)

---

## Verdict: **FAIL**

23 of 24 acceptance criteria pass. One CI gate failure remains: the integration test `config_path_shows_location` expects `config.json` output but the T8/T9 migration changed the default to `config.toml`. This test was not updated as part of the WS-E plan and causes `cargo test --workspace` to fail.

---

## Acceptance Criteria Verification

### Core Functionality (T0-T2, T3, T3b, T4)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | `WorkerRegistry<S>` supports register/lookup/remove/shutdown_all for multiple creators | **PASS** | `registry.rs:100-179` — `get_or_spawn`, `get`, `remove`, `shutdown_all` all implemented with `HashMap<String, WorkerHandle>`. Capacity enforcement at line 128. Tested in `worker_registry.rs` (276 lines). |
| 2 | `IpcClient` supports persistent multiplexed request routing (≥10 concurrent) | **PASS** | `ipc.rs` — background reader + `PendingMap` pattern. Tested in `ipc_client_multiplex.rs` with 3 and 10 concurrent requests. `Send + Sync` assertion at line 205. |
| 3 | `AgentSlot` state machine works (Initializing→Ready→Prompting→Ready→Stopping→Stopped) | **PASS** | `agent_slot.rs:66-79` — `AgentSlotState` enum with 6 variants. Transitions documented in ASCII diagram. `mark_ready_from_prompt` guard at line 296-301 preserves `Error` state. |
| 4 | `handle_initialize` supports both `agents` array and `agent_ref` (backward compat) | **PASS** | `mod.rs:310-363` — checks `agents` array first (multi-agent), falls back to `agent_ref` (single-agent), then creates default session. |
| 5 | `worker/agent_start` creates new AgentSlot; `worker/agent_stop` removes it | **PASS** | `handle_agent_start` at `mod.rs:528` — validates slot doesn't exist, creates `AgentSlot`, inserts into sessions map. `handle_agent_stop` at `mod.rs:620` — marks slot for shutdown, removes from sessions map. |
| 6 | `worker/acp_prompt` routes to correct AgentSlot by session_id | **PASS** | `handle_acp_prompt` at `mod.rs:393` — extracts `session_id` from JSON-RPC params, looks up slot via `sessions.get()`, routes prompt to correct slot. |
| 7 | `worker/agent_list` returns all active session summaries | **PASS** | `handle_agent_list` at `mod.rs:675` — iterates `sessions.values()`, builds JSON summaries via `session_summary_json`. |
| 8 | Crash isolation: one slot crash doesn't affect other slots | **PASS** | `multi_agent_worker.rs:639` — `crash_isolation_error_slot_no_impact` test verifies that when one slot enters `Error` state, other slots remain `Ready` and can still receive prompts. |
| 9 | `WorkerMgrSubsystem` uses real WorkerRegistry | **PASS** | `worker_mgr.rs:30-70` — `WorkerMgrSubsystem` wraps `WorkerRegistry<WorkerManagerSpawner>`. `registry()` method returns `Arc<Mutex<WorkerRegistry<...>>>`. |

### DTOs and Config (T6, T7, T8, T9)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 10 | `PresetRoleDefinition` has `system_prompt_file`, `recommended_models`, `agent` field on GraphNode | **PASS** | `preset.rs:441-453` — `PresetRoleDefinition` has `system_prompt_file: String` and `recommended_models: Vec<String>`. `GraphNode` at `preset.rs:277-294` has `agent: Option<String>`. |
| 11 | `UserAgentsConfig` loads from `~/.nexus42/agents.toml`; missing file returns empty config | **PASS** | `config.rs:1035` — `const FILENAME: &str = "agents.toml"`. `load()` at line 1037 tries to read, returns defaults if file not found (line 1047) or parse error (line 1055). |
| 12 | `--agent-ref reviewer:codex-acp:o3` parses correctly | **PASS** | `config.rs:1096-1136` — `parse_agent_ref` splits on `:` with `splitn(3, ':')`, returns `(role_id, acp_agent_id, Option<model>)`. Tests at lines 836, 928 confirm 3-segment parsing. |
| 13 | Resolution priority: CLI > user config > preset recommended_models | **PASS** | `resolve_agent_model` at `config.rs:1154-1181` — checks CLI overrides first (line 1162), then user config (line 1167), then preset recommended_models (line 1172). |
| 14 | config.json → config.toml migration works (one-time, idempotent) | **PASS** | `config.rs:80-144` — loads `config.toml` first (line 85), only falls back to `config.json` migration if `.toml` doesn't exist (line 108). Migration renames JSON to `.json.migrated` (line 124), making it idempotent. |
| 15 | `system_prompt` field flows from config to AgentSlot | **PASS** | `AgentConfig` at `agent_slot.rs:27` has `system_prompt: Option<String>`. `handle_initialize` at `mod.rs:331-334` extracts `system_prompt` from IPC params. `AgentSlot::new()` at `agent_slot.rs` stores it in config. |

### Engine Integration (T5)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 16 | `AcpPromptTask` has `session_id` field with default "default" | **PASS** | `tasks/mod.rs:850` — `session_id: String` field. Constructor at line 868-875 defaults to `"default"`. `with_session_id` constructor at line 899. |
| 17 | `InnerGraphNodeTask` resolves session_id from agent_ref or explicit field | **PASS** | `resolve_session_id` at `tasks/mod.rs:708-729` — 3-level priority: (1) explicit `session_id`, (2) `agent_ref` lookup in `_session_routes`, (3) fallback to `"default"`. |

### CI Gates

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 18 | `cargo build --workspace` clean | **PASS** | Build completed successfully in 4.87s. |
| 19 | `cargo clippy --all -- -D warnings` clean | **PASS** | No warnings or errors. |
| 20 | `cargo +nightly fmt --all -- --check` clean | **PASS** | No formatting differences. |
| 21 | `cargo test --workspace` passes (flaky test excluded) | **FAIL** | `config_path_shows_location` integration test fails — expects `config.json` in output but receives `config.toml`. See details below. |

### QC Fixes

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 22 | W1 fixed: `handle_initialize` clears sessions before re-init | **PASS** | `mod.rs:301-307` — `sessions.clear()` called before agent insertion. Warning logged with count. |
| 23 | W2 fixed: `resolve_agent_model` splits "agent:model" format | **PASS** | `config.rs:1172-1178` — `preset.split_once(':')` extracts `(agent, model)` tuple. Falls back to `(None, Some(preset))` if no colon present. |

---

## Failure Detail

### F1: Integration test `config_path_shows_location` fails

**File**: `crates/nexus42/tests/integration.rs:575-585`  
**Severity**: High (blocks CI test gate)

**Root Cause**: The test asserts that `nexus42 config path` outputs a path containing `config.json`:

```rust
.stdout(predicate::str::contains("config.json"));
```

But the WS-E T8/T9 config migration changed the default config file from `config.json` to `config.toml`. The `CliConfig::config_path()` method now returns `~/.nexus42/config.toml`.

**Actual output**: `/var/folders/.../.nexus42/config.toml`  
**Expected by test**: string containing `config.json`

**Scope**: This test was **not modified** in the WS-E diff (`git diff feature/v1.7...HEAD -- crates/nexus42/tests/integration.rs` returns empty). The implementation change (config.json → config.toml) IS in the WS-E diff (36 references in `config.rs`). This is a regression introduced by the WS-E plan — the test should have been updated alongside the implementation.

**Fix Required**: Update line 584 of `integration.rs` from:
```rust
.stdout(predicate::str::contains("config.json"))
```
to:
```rust
.stdout(predicate::str::contains("config.toml"))
```

---

## QC Report Cross-Reference

### QC Reviewer #1 (`qc-reviewer-1.md`) — Verdict: Request Changes → Fixed

| Finding | Status in QA Review | Notes |
|---------|-------------------|-------|
| **W1**: `handle_initialize` re-init accumulates sessions | **FIXED** | `sessions.clear()` added at `mod.rs:306` |
| **W2**: `resolve_agent_model` drops agent info from preset | **FIXED** | `split_once(':')` at `config.rs:1173-1176` |
| **S1**: Duplicated doc comment on `from_ipc_for_test` | **Not verified fixed** | Still present at `manager.rs:323-336` — cosmetic, non-blocking |
| **S2**: `build_inner_graphs` converts `None` agent to `Some("")` | **Not verified fixed** | Still `unwrap_or_default()` at `loader.rs:641` — functional but wasteful |
| **S3**: `handle_agent_crash` dead_code with no test | **Not verified fixed** | Still `#[allow(dead_code)]` at `mod.rs:776` — acceptable for WS-E |
| **S4**: Handle take/return pattern — potential loss on panic | **Not verified fixed** | Still present at `tasks/mod.rs:956-985` — theoretical concern only |

### QC Reviewer #3 (`qc-reviewer-3.md`) — Verdict: Approve

All 9 suggestions (S1-S9) are non-blocking and align with QA observations above. No new critical findings.

---

## Phase Gate Checklist

- [x] Plan exists at `.mstar/plans/2026-04-21-v1.7-ws-e-multi-agent-worker.md`
- [x] Plan contains `tasks` section with task IDs T0-T9
- [x] QC reports exist: `qc-reviewer-1.md` and `qc-reviewer-3.md`
- [x] Implementation scope matches plan tasks (no plan-out-of-scope code detected)
- [ ] All CI gates pass — **BLOCKED** by `config_path_shows_location` test failure

---

## Tested Scope

### What was tested
- Source code review of all 40 changed files
- CI gate execution: build, clippy, fmt, test
- Verification of QC findings W1 and W2 fixes
- Acceptance criteria mapping to specific code locations and test files

### What was NOT tested
- E2E testing with real ACP agents (requires external API keys)
- Daemon integration testing (requires running containers: Postgres, Neo4j, Redis)
- Performance testing under load beyond the 10-concurrent IPC test
- Config migration on a real user's `config.json` with edge-case data

---

## Recommended Owners

| Issue | Owner | Action |
|-------|-------|--------|
| F1: `config_path_shows_location` test assertion | @fullstack-dev | Update assertion from `config.json` to `config.toml` in `integration.rs:584` |
| S1: Duplicated doc comment | @fullstack-dev | Remove duplicate lines at `manager.rs:328-336` |

---

## Completion Report v2

**Agent**: @qa-engineer  
**Task**: QA Verification — Multi-Agent Worker (WS-E)  
**Status**: Partial — 23/24 criteria pass, 1 test failure blocks merge  
**Scope Delivered**: Full verification of all 24 acceptance criteria across core functionality, DTOs/config, engine integration, CI gates, and QC fixes  
**Artifacts**: QA report at `.mstar/plans/reports/2026-04-21-v1.7-ws-e-multi-agent-worker/qa-report.md`  
**Validation**: 
- `cargo build --workspace` — clean
- `cargo clippy --all -- -D warnings` — clean  
- `cargo +nightly fmt --all -- --check` — clean
- `cargo test --workspace -- --skip auth::tests::get_returns_none_for_unknown_creator` — 1 failure (`config_path_shows_location`)
**Issues/Risks**: F1 — integration test needs 1-line fix; S1 — cosmetic doc duplication remains  
**Plan Update**: PM to track F1 as residual or request quick fix before merge  
**Handoff**: @project-manager
