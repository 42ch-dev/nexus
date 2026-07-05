---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.57-daemon-refactor-and-caller-adapters"
verdict: "Approve"
generated_at: "2026-06-22"
---

# QC1 Review — V1.57 P1 Daemon Refactor & 3-Caller Adapters

## Reviewer Metadata
- **Reviewer**: @qc-specialist (Reviewer #1, architecture/maintainability)
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: deepseek/deepseek-v4-flash
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-22

## Scope
- **plan_id**: `2026-06-22-v1.57-daemon-refactor-and-caller-adapters`
- **Review range / Diff basis**: `56d459ec..fe501b6b` (P0 merge parent → P1 merge commit)
- **Working branch (verified)**: `iteration/v1.57` @ `eae09e74`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (top-level of monorepo)
- **Files reviewed**: 16 (10 Rust source + 6 test/config)
- **Commit range**: `68989e03` (T1+T2 god-file split) → `6b08bb9e` (T3 host-call) → `64c0b245` (T4 CdnConfig) → `fc0e250a` (T5-T8 tests + spec + field drops) → `fe501b6b` (merge commit)
- **Tools run**: `wc -l`, `git diff --stat`, `grep` (global state, dispatch paths), `cargo check`, `cargo test -p nexus-daemon-runtime`, `cargo test -p nexus42`, `cargo clippy -p nexus-daemon-runtime -p nexus42 -- -D warnings`, `cargo +nightly fmt -p nexus-daemon-runtime -p nexus42 -- --check`

## Summary
- **AC met**: 18 / 18
- **Findings**: 2 (both 🟢 Suggestion)
- **Verdict**: Approve

All 18 acceptance criteria verified. Two suggestions for future maintainability improvements — neither is blocking.

---

## Acceptance Criteria Checklist

- [x] **AC1**: `host_tool_executor.rs` = 349 lines, well within 800-line limit (`wc -l` verified).
- [x] **AC2**: Three caller entry points exist: CLI (`host-call` via `DaemonClient::post` → HTTP → `HostToolExecutor::execute()`), worker (`HostToolExecutor::dispatch_from_worker()`), HTTP (`HostToolExecutor::execute()`). Also a 4th internal path `dispatch_for_schedule()` documented in specs.
- [x] **AC3**: All entry points dispatch through `CapabilityRegistry::dispatch(tool_id, input)` in `crate::capability_registry` — `execute()` → `registry_dispatch()` → `reg.dispatch(req, state, &creator_id)`. Worker and schedule paths construct a `ToolExecuteRequest` then call `execute()`.
- [x] **AC4**: The 7 previously-duplicated `execute_X` dispatch functions removed from `host_tool_executor.rs`. Handlers extracted to `host_tool_handlers.rs` and called exclusively through registry dispatch. No direct dispatch bypass paths remain.
- [x] **AC5**: `nexus42 host-call <tool_id> --args <json>` implemented (`crates/nexus42/src/commands/host_call.rs`). Posts to daemon `POST /v1/local/agent-host/internal/tool-executions` → `HostToolExecutor::execute()` → `CapabilityRegistry::dispatch()`.
- [x] **AC6**: `host-call --help` documents `--args` as JSON string format and debug-only intent. `Commands::HostCall` enum variant has verbose doc comment. `host_call.rs` module-level doc states exit codes explicitly.
- [x] **AC7**: `cli-spec.md` §6.2M documents `host-call` subcommand with type table, exit codes, debug-only intent, and wiring diagram.
- [x] **AC8**: `daemon-runtime.md` host_tool section updated with 3-caller entry point topology table and V1.57 P1 refactor bullet list (file sizes, handler split, CdnConfig).
- [x] **AC9**: `local-runtime-boundary.md` updated with detailed ASCII topology diagram showing 3-caller pattern → admission pipeline → `CapabilityRegistry::dispatch`.
- [x] **AC10**: `orchestration-engine.md` §6.4 updated: V1.57 P1 subsection explains `worker/agent_tool_request` as one of three caller entry points dispatching through `CapabilityRegistry::dispatch`. Schedule executor path also documented.
- [x] **AC11**: `CdnConfig` constructor-injected via `CapabilityRuntimeDeps.cdn_config` field. Global `RwLock<Option<CdnConfig>>` removed from `registry.rs`. Zero occurrences of `set_cdn_config`/`get_cdn_config`/`CDN_CONFIG` anywhere in codebase.
- [x] **AC12**: `R-V156P3-S003` field drops — commit `fc0e250a` message states "all fields actively used; no unused fields to drop." Conservative pass, no regression risk.
- [x] **AC13**: 3 caller integration tests in `host_tool_executor_tests.rs`:
  - `test_host_call_dispatches_through_registry_read`
  - `test_worker_agent_tool_request_dispatches_through_registry`
  - `test_http_tool_execute_dispatches_through_registry`
  - Plus `test_dispatch_equivalence_all_three_paths` (same input → same output)
- [x] **AC14**: `host-call` smoke test (`crates/nexus42/tests/host_call_smoke.rs`) with 3 tool IDs:
  - Read: `nexus.context.whoami`
  - Write: `nexus.pool.entry.manage`
  - Policy-gated: `nexus.context.assemble`
  - All tests `#[ignore]` (require running daemon) — expected for smoke tests.
- [x] **AC15**: `cargo test -p nexus-daemon-runtime` — **passed** (267+26+6+3+3+3+2+4+15+11+7+21+3+9+16+34 = ~431 tests across all modules, 0 failures).
- [x] **AC16**: `cargo test -p nexus42` — **passed** (762+4+10+18+3+5+4+37+8+3+5+11+4+3+9+1+47+6+3+11+8+15+2+9+4+9+11+1 = ~1,000+ tests, 0 failures).
- [x] **AC17**: `cargo clippy -p nexus-daemon-runtime -p nexus42 -- -D warnings` — clean (no warnings emitted).
- [x] **AC18**: `cargo +nightly fmt -p nexus-daemon-runtime -p nexus42 -- --check` — clean (no formatting changes needed).

---

## Findings

### 🟢 Suggestion F-001: `host_tool_handlers.rs` file size — god-file problem shifted

- **Scope**: Architecture / maintainability
- **Rationale**: The god-file refactor successfully reduced `host_tool_executor.rs` from 4,298→349 lines, but the extracted `host_tool_handlers.rs` is 1,839 lines. While this is a structural improvement (handlers are now separate from dispatch orchestration), a single file containing all 20+ handler implementations, the admission pipeline, permission checks, audit logging, and registry wrappers is approaching god-file territory again. The file mixes:
  - Admission pipeline logic (~lines 25–152)
  - Individual handler implementations (`execute_*` functions, ~lines 153–1839)
  - Permission policy helpers
  - File path validation helpers
- **Suggested action**: Consider splitting `host_tool_handlers.rs` by domain in a future plan:
  - `handlers/context.rs` (whoami, workspace.info, context.assemble)
  - `handlers/work.rs` (work.get, work.patch, schedule.set, pool.entry.manage, finding.resolve)
  - `handlers/world.rs` (world.snapshot.get, world.configure, timeline.recent.get, kb_snapshot.*)
  - `handlers/manuscript.rs` (manuscript.chapter.get, manuscript.chapter.update)
  - `handlers/admission.rs` (admission pipeline, permission checks, audit)
  - `handlers/filesystem.rs` (read_file, write_file)
  - `handlers/observability.rs` (daemon_health, registry_refresh)
- **Severity**: Suggestion (maintainability debt, not blocking)

### 🟢 Suggestion F-002: Re-export bridge between `host_tool_executor.rs` and `host_tool_handlers.rs`

- **Scope**: Architecture / maintainability
- **Rationale**: Lines 324–343 of `host_tool_executor.rs` re-export 17 `registry_*` functions from `host_tool_handlers.rs` to preserve backward compatibility with `capability_registry.rs`, which imports via `use crate::api::handlers::host_tool_executor as hte` and calls `hte::registry_*`. This creates a fragile bridge: if the handler module is restructured in the future, this re-export list must be manually kept in sync. The `capability_registry.rs::build_registry()` function references `hte::registry_*` in its handler registrations (e.g., `handler: hte::registry_context_whoami`).
- **Suggested action**: Consider having `capability_registry.rs` import directly from `host_tool_handlers` in a future plan:
  ```rust
  // Instead of: use crate::api::handlers::host_tool_executor as hte;
  // Consider:  use crate::api::handlers::host_tool_handlers as hth;
  ```
  This would eliminate the re-export bridge and reduce the maintenance surface. The change is mechanical and low-risk.
- **Severity**: Suggestion (bridge is correct today, risk is future maintenance)

---

## Detailed Notes

### Architecture assessment

**Entry point symmetry**: The three documented entry points (CLI, worker, HTTP) all converge to `HostToolExecutor::registry_dispatch()` → `CapabilityRegistry::dispatch()`. The symmetry is clean:

| Entry | Rust method | Normalization | Dispatch |
|-------|------------|---------------|----------|
| CLI `host-call` | `execute()` via HTTP POST | `DaemonClient::post` → `ToolExecuteRequest` | `registry_dispatch()` |
| Worker IPC | `dispatch_from_worker()` | `{tool_name, args, request_id}` → `ToolExecuteRequest` | calls `execute()` |
| HTTP POST | `execute()` (direct) | Deserialized `ToolExecuteRequest` from wire | `registry_dispatch()` |
| Schedule (4th, internal) | `dispatch_for_schedule()` | `{tool_name, args, request_id}` → `ToolExecuteRequest` with `HostToolCallerKind::Schedule` | calls `execute()` |

Note: CLI physically walks through the HTTP path (sends a POST to the daemon), so it shares `execute()` with the HTTP path. This is correct behavior — the CLI is a lightweight debug entry that reuses existing daemon HTTP infrastructure.

**CdnConfig injection**: The global `RwLock<Option<CdnConfig>>` is completely eliminated. The configuration flows:

1. `DaemonConfig.cdn_url: Option<String>` (parsed from `--cdn-url`)
2. → `boot.rs: run_daemon()` validates via `validate_cdn_url_static()` and constructs `Option<CdnConfig>`
3. → `CapabilityRuntimeDeps { cdn_config }` 
4. → `CapabilityRegistry::with_runtime_deps()` → `RegistryRefresh::with_cdn(cdn)` or `RegistryRefresh::new()`
5. → `RegistryRefresh.run()` reads `self.cdn_config` (no global access)

No remaining `set_cdn_config()`/`get_cdn_config()`/`static CDN_CONFIG: RwLock<>` anywhere.

**CapabilityRuntimeDeps `cdn_config` field**: Adding `cdn_config: None` to 9 test initializers (4 in `daemon_boot_llm_wiring.rs`, 4 in `tasks/mod.rs`, 1 in `novel_review_master.rs`) is the minimum necessary change. An alternative would be implementing `Default` for `CapabilityRuntimeDeps`, but that could hide real dependency requirements in production boot. The current explicit approach is correct — Rust's struct literal syntax forces every construction site to acknowledge the field.

**No scope creep**: All 16 files in the diff are within P1 scope:
- Core god-file split: `host_tool_executor.rs`, `host_tool_handlers.rs`, `mod.rs`
- CLI host-call: `host_call.rs`, `cli.rs`, `commands/mod.rs`, `main.rs`
- CdnConfig injection: `boot.rs`, `registry.rs`, `capability/mod.rs`, `builtins/mod.rs`
- Test boilerplate (`cdn_config: None`): `tasks/mod.rs`, `daemon_boot_llm_wiring.rs`, `novel_review_master.rs`
- Tests: `host_tool_executor_tests.rs`, `host_call_smoke.rs`

No changes to schema files, registry consolidation, worker IPC extension, or any out-of-scope area.

---

## Verdict

**Approve** — all 18 acceptance criteria met. Zero critical or warning findings. Two non-blocking suggestions for future maintainability improvements:

1. **F-001**: Split `host_tool_handlers.rs` (1,839 lines) by domain in a future plan.
2. **F-002**: Eliminate the re-export bridge between `host_tool_executor.rs` and `host_tool_handlers.rs` by having `capability_registry.rs` import directly.

The P1 refactor achieved its primary goals: god-file split from 4,298→349 lines, unified 3-caller dispatch through `CapabilityRegistry`, `CdnConfig` constructor injection (closing R-V156P1-M002), and comprehensive test coverage. No regression risk identified for the current structure.
