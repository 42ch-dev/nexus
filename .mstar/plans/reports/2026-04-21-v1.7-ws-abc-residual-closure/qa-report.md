# QA Report — WS-A/B/C Residual Closure

**Plan ID**: `2026-04-21-v1.7-ws-abc-residual-closure`
**Branch**: `fix/v1.7-ws-abc`
**Diff Basis**: `git diff feature/v1.7...fix/v1.7-ws-abc`
**Date**: 2026-04-21
**QA Engineer**: @qa-engineer

---

## Acceptance Criteria Verification

### WS-A (DTO residuals)

| Criterion | Status | Evidence |
|-----------|--------|----------|
| **DTO-C1**: `sdk_new_session_request_from_nexus` propagates `mcp_servers` — each NexusMcpServer variant maps correctly | ✅ PASS | `client.rs:160-177`: `nexus_mcp_server_to_sdk` maps Http/Sse/Stdio → SDK variants. Tests `new_session_request_propagates_mcp_servers` and `new_session_request_empty_mcp_servers` both pass (5/5 DTO tests green). |
| **DTO-C2**: Invalid protocol version → warning logged + `ProtocolVersion::LATEST` returned | ✅ PASS | `client.rs:144-158`: `sdk_protocol_version_from_nexus` uses `match` with `tracing::warn!` on parse error. Tests `protocol_version_valid_string`, `protocol_version_invalid_string_defaults_to_latest`, `protocol_version_empty_string_defaults_to_latest` all pass. |
| **DTO-W1**: `NexusContentBlock` derives `Eq` | ✅ PASS | `types.rs:179`: `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`. All 49 `nexus-contracts` tests pass. Compile-time check confirmed by successful `cargo test -p nexus-contracts`. |

### WS-B (Permission residuals)

| Criterion | Status | Evidence |
|-----------|--------|----------|
| **PERM-W1**: `policy.rs` uses `toml_edit` for save — round-trip preservation | ✅ PASS | `policy.rs:163-187`: `save_toml_edit` method uses `load_toml_edit` → `DocumentMut` → in-place update → `save_toml_edit_doc`. Tests `test_save_toml_edit_preserves_comments` and `test_save_toml_edit_preserves_unknown_key` both pass. |
| **PERM-W2**: `permission list -o json --agent X` includes `global` key when present, omits when absent | ✅ PASS | `permission.rs:232-243`: `build_list_json` checks `!global_granted.is_empty() || !global_denied.is_empty()` before inserting `global` key. Tests `test_json_output_includes_global_when_present` and `test_json_output_omits_global_when_absent` both pass. |
| **PERM-W3**: Unknown TOML keys trigger warnings in `permission list` output | ✅ PASS | `policy.rs:317-349`: `validate_toml_keys` checks top-level and agent sub-keys. `permission.rs:105-110`: `run_list` calls validation and prints to stderr. Tests `test_validate_toml_keys_*` (3 tests) and `test_list_warns_on_unknown_toml_keys` all pass. |

### WS-C (Schedule residuals)

| Criterion | Status | Evidence |
|-----------|--------|----------|
| **R4**: Module doc in `scheduler/mod.rs` accurately describes UTC-only safety | ✅ PASS | `scheduler/mod.rs:10-18`: Doc accurately states "operates entirely in UTC via Unix timestamps", "inherently safe against DST transitions", and documents the limitation about wall-clock recurrence. No code changes needed — doc-only fix as planned. |
| **R7**: `cleanup_guard` method exists, removes entries, no-op on missing | ✅ PASS | `derivation.rs:74-78`: `cleanup_guard` calls `guards.remove(&key)`. Tests `cleanup_guard_removes_entry_and_allows_new_guard` and `cleanup_guard_on_nonexistent_schedule_is_noop` both pass. |

### Cross-cutting

| Criterion | Status | Evidence |
|-----------|--------|----------|
| `cargo test --workspace` | ✅ PASS (1 known failure) | **405 passed; 1 failed** — the single failure is `auth::tests::get_returns_none_for_unknown_creator`, a pre-existing flaky test explicitly excluded in acceptance criteria. |
| `cargo clippy --all -- -D warnings` | ✅ PASS | Clean — no warnings or errors. Output: `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`. |
| `cargo +nightly fmt --all -- --check` | ✅ PASS | Clean — no output (no formatting issues). |
| No hand-edits to `*/generated/` | ✅ PASS | `git diff feature/v1.7...fix/v1.7-ws-abc -- '*/generated/*'` produces no output. |

---

## Phase Gate Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Phase Gate Checklist exists | ⚠️ Partial | Plan uses `§3 Task Decomposition` as tasks and residuals from prior QC rounds as `clarify` input. No explicit YAML `phase_gate` frontmatter, but residual closure plans inherit clarify/tasks context from source QC reports. |
| Delivery matches plan tasks | ✅ PASS | All 8 residuals (DTO-C1, DTO-C2, DTO-W1, PERM-W1, PERM-W2, PERM-W3, R4, R7) addressed. No plan-out-of-scope implementation detected in diff. |
| Hotfix RCA | N/A | Not a hotfix. |

---

## Commit Summary

```
4f8ccb0 chore(agents): close 8 residuals, archive, add 4 new QC residuals for WS-A/B/C
f7a9d7b docs(qc): 2026-04-21-v1.7-ws-abc-residual-closure qc2 report
c757cb3 docs(qc): 2026-04-21-v1.7-ws-abc-residual-closure qc1 report
d3947ae docs(qc): 2026-04-21-v1.7-ws-abc-residual-closure qc3 report
3af735f docs(plan): mark WS-C tasks T7/T8 complete
d864a28 fix(orchestration): add cleanup_guard method to CoreContextManager (R7)
61258de fix(scheduler): update module doc to accurately describe UTC-only DST safety (R4)
2cf01ef docs(plan): mark WS-B tasks T4/T5/T6 complete
0305edd fix(acp-host): validate TOML keys against known schema (PERM-W3)
d39ea91 fix(nexus42): include global rules in --agent filtered JSON output (PERM-W2)
1984f67 fix(acp-host): use toml_edit for round-trip TOML preservation in policy.rs (PERM-W1)
653a569 docs(plan): mark WS-A tasks complete, set plan status InReview
c9fda19 fix(acp-host): explicit error handling for protocol version parse (DTO-C2)
6389e2a fix(acp-host): propagate mcp_servers from NexusNewSessionRequest to SDK (DTO-C1)
3be92d6 fix(contracts): add Eq derive to NexusContentBlock (DTO-W1)
76551c2 chore(agents): start WS-A/B/C implementation — plan status InProgress
```

15 commits, 13 files changed, +1434/-338 lines.

---

## Verdict

**QA: PASS** — All 8 acceptance criteria verified with passing unit tests, clean CI gates, and code review of implementation. The single test failure (`auth::tests::get_returns_none_for_unknown_creator`) is a known pre-existing flaky test explicitly excluded from this plan's scope.
