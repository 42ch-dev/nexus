---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-04-20-v1.6-ws-d-multi-preset"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# QC Review #2 — WS-D Multi-Preset Scheduler

## Scope

- **Plan ID**: `2026-04-20-v1.6-ws-d-multi-preset`
- **Working branch**: `feature/v1.6`
- **Review range**: `git diff ccd6aee..HEAD` (WS-C Done → WS-D HEAD)
- **Primary accent**: Security & correctness (input validation, auth boundaries, sensitive data, error paths, state consistency)
- **Secondary accent**: Maintainability & interface contract clarity

## Files Reviewed

| File | Lines | Nature |
|------|-------|--------|
| `crates/nexus-orchestration/src/system_preset_dir.rs` | +578 (new) | System preset directory scanner |
| `crates/nexus-orchestration/src/engine.rs` | +9 | `Clone` impl for `GraphFlowEngine` |
| `crates/nexus-orchestration/src/lib.rs` | +1 | Export `system_preset_dir` |
| `crates/nexus-orchestration/src/preset/loader.rs` | +1 | `#[derive(Clone)]` on `LoadedPreset` |
| `crates/nexus42d/src/main.rs` | ~+35/-12 | Boot-time system preset discovery |
| `crates/nexus42d/src/api/handlers/orchestration/presets.rs` | ~+20/-12 | `list_presets` handler (now stateful) |
| `crates/nexus42d/tests/orchestration_http_smoke.rs` | ~+30/-5 | Smoke test updates |
| `crates/nexus42/src/commands/system.rs` | +95 (new) | CLI `system preset list` command |
| `crates/nexus42/src/commands/mod.rs` | +2 | Register `system` module |
| `crates/nexus42/src/main.rs` | +1 | Route `System` command |
| `crates/nexus-contracts/src/local/orchestration/http.rs` | +1/-1 | `Deserialize` on `ListPresetsResponse` |

## Automated Checks

| Check | Result | Notes |
|-------|--------|-------|
| `cargo clippy --all -- -D warnings` | ✅ Pass | Clean, no warnings |
| `cargo fmt --check --all` | ⚠️ Pre-existing diffs | Diffs are **entirely** in `crates/nexus-contracts/src/generated/` (generated files ignored by `.rustfmt.toml` but stable fmt cannot honor `ignore`). WS-D source files are clean. |
| `cargo test --workspace` | 🔴 Not executed | Tool permission restrictions prevented execution. Dev team should verify before merge. |
| `cargo audit` | Not executed | Tool permission restrictions. |

## Findings

### W-1: `list_presets` handler creates fresh `CapabilityRegistry` instead of using runtime registry

**Severity**: Warning
**File**: `crates/nexus42d/src/api/handlers/orchestration/presets.rs:18`
**Code**:
```rust
let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
let scan_result = system_preset_dir::scan_system_presets(state.nexus_home(), &caps);
```

**Problem**: The handler instantiates a fresh `CapabilityRegistry::with_builtins()` rather than using `state.capability_registry()`, which is the canonical runtime registry that may include dynamically registered capabilities. This creates a mismatch:
- `list_capabilities` (same module family) correctly uses `state.capability_registry()`
- A system preset that requires a dynamically registered capability would fail validation here but succeed at daemon boot (where the real registry is used)
- Unnecessary allocation on every `GET /presets` request

**Impact**: System presets requiring non-built-in capabilities would be silently omitted from the API response even though the daemon successfully loaded them at boot.

**Recommended fix**:
```rust
let caps = match state.capability_registry() {
    Some(r) => r,
    None => {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(ListPresetsResponse { presets }));
    }
};
```

### W-2: `#![allow(clippy::print_literal)]` is unnecessary

**Severity**: Warning (code smell)
**File**: `crates/nexus42/src/commands/system.rs:5`

**Problem**: The module-level `#![allow(clippy::print_literal)]` suppresses a lint that is not triggered anywhere in the file. There are no literal string arguments to `println!` that would trigger this lint (all prints use variables).

**Impact**: Minor noise; sets a bad precedent for copy-paste module scaffolding.

**Recommended fix**: Remove the attribute.

### W-3: `scan_system_presets` silently swallows `read_dir` entry errors

**Severity**: Warning (observability gap)
**File**: `crates/nexus-orchestration/src/system_preset_dir.rs:98`
**Code**:
```rust
for entry in entries.flatten() {
```

**Problem**: `DirEntry` errors (e.g., permission denied on a subdirectory) are silently discarded by `flatten()`. The function correctly handles the top-level directory read failure with a warning log, but individual entry failures are invisible. In a local daemon context this is unlikely to matter, but if a preset directory contains a symlink to an inaccessible location, the failure is silent.

**Impact**: Operational blind spot; operators cannot distinguish "no presets" from "permission denied on presets".

**Recommended fix**: Log `tracing::warn!` for each `Err` item before skipping:
```rust
for entry in entries {
    let entry = match entry {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read system preset directory entry");
            continue;
        }
    };
    // ...
}
```

### I-1: `system_preset.rs` hardcoded graph is now orphaned from production path

**Severity**: Info (tech debt)
**File**: `crates/nexus-orchestration/src/system_preset.rs`

**Observation**: `main.rs` no longer imports `system_preset` (replaced by `system_preset_dir`), but `system_preset.rs` remains in the crate and is still referenced by:
- `crates/nexus42d/tests/engine_started_at_boot.rs:34`
- `crates/nexus-orchestration/tests/system_preset_e2e.rs`

These tests exercise deprecated code paths. No functional regression, but this is accumulating tech debt. A follow-up should migrate these tests to use the directory-based loader or delete them if coverage is duplicated by `system_preset_dir.rs` tests.

### I-2: `ListPresetsResponse` wire contract change

**Severity**: Info
**File**: `crates/nexus-contracts/src/local/orchestration/http.rs:126`

**Observation**: Added `Deserialize` derive to a previously `Serialize`-only response type. This is required for the CLI client to parse the response and is backward-compatible for the API (serialization shape unchanged). No action needed, but worth noting as a wire-contract touch.

### I-3: `GraphFlowEngine::Clone` safety verified

**Severity**: Info
**File**: `crates/nexus-orchestration/src/engine.rs:508-515`

**Observation**: The new `Clone` impl performs shallow clones of `Arc<EngineSharedState>` and `Arc<CapabilityRegistry>`. This is safe and correct for sharing the engine between the daemon boot sequence and `WorkspaceState`. The `start_session` method takes `&self`, so shared access is intentional.

### I-4: `ensure_maintenance_preset` edge case with directory named `preset.yaml`

**Severity**: Info (extremely low probability)
**File**: `crates/nexus-orchestration/src/system_preset_dir.rs:286-292`

**Observation**: `preset_yaml.exists()` returns `true` for both files and directories. If a directory exists at `.../maintenance/preset.yaml/`, `ensure_maintenance_preset` would return `Ok(false)` and skip creation. The subsequent `fs::write` would then fail with `IsADirectory` if anyone tried to write. In practice this path is attacker-controlled only by the local user, so the security impact is negligible. Consider using `is_file()` instead of `exists()` for precision.

## Cross-Reviewer Ready Notes

### For Reviewer #1 (@qc-specialist-1)
- Verify **backward compatibility**: `nexus42 schedule start _system.maintenance` should behave identically. The embedded YAML graph matches the old hardcoded graph (same states, same capabilities, same transitions).
- Check **test coverage**: `system_preset_dir.rs` has 12 unit tests covering happy path, missing dir, corrupted YAML, hidden dirs, and idempotency. The smoke test was updated to create the directory first. No integration test covers the full daemon boot → preset start → CLI list end-to-end flow.

### For Reviewer #3 (@qc-specialist-3)
- Verify **performance**: `scan_system_presets` is called at daemon boot (once) and on every `GET /presets` API call. The latter performs a filesystem walk + YAML parse. For a local API with typically <10 system presets, this is acceptable, but if the API is called frequently it could become a bottleneck.
- Check **naming consistency**: The CLI command is `nexus42 system preset list` but the handler is `SystemPresetCommand::Preset { SystemPresetSubcommand::List }`. The nesting (`system preset list`) is consistent with the plan.

## Security Assessment

| Vector | Assessment |
|--------|------------|
| Path traversal | **Low risk**: `nexus_home` comes from `WorkspaceState` (trusted). Subdirectory names come from `read_dir` and are used only as string identifiers, not filesystem paths. No user-controlled path concatenation. |
| Arbitrary code execution | **None**: Presets are YAML manifests loaded into a validated state machine. No code execution from file content. |
| Information disclosure | **None**: Hidden directories (`.git`, etc.) are explicitly skipped. Error messages in warnings include only directory names and generic parse errors, not file contents. |
| Denial of service | **Low risk**: A very large `preset.yaml` could cause memory pressure during parse. The loader does not enforce file size limits. However, this is local filesystem only. |
| Privilege escalation | **None**: System presets run with the same daemon privileges as before. No new capabilities are added. |

## Backward Compatibility

- **Daemon boot**: `_system.maintenance` is auto-created on first start from embedded YAML, preserving pre-V1.6 behavior.
- **CLI**: `nexus42 schedule start _system.maintenance` continues to work because the preset ID is unchanged.
- **API**: `GET /v1/local/orchestration/presets` now returns dynamically discovered system presets instead of a hardcoded list. This is an expansion, not a breaking change.
- **Data migration**: None required. Pre-1.0 local data can be wiped/replaced per AGENTS.md policy.

## Test Coverage Assessment

| Component | Coverage | Gap |
|-----------|----------|-----|
| `system_preset_dir.rs` | 12 unit tests | No test for `read_dir` entry error path; no test for non-UTF-8 directory names |
| `system.rs` (CLI) | 2 parser tests | No integration test against a running daemon; no test for empty list output formatting |
| `main.rs` (daemon boot) | Smoke tests updated | No test asserting that `ensure_maintenance_preset` is called at boot |
| `presets.rs` handler | 1 updated test | No test asserting dynamic discovery vs. hardcoded list |

## CI Gate

- `cargo clippy` passes cleanly.
- `cargo fmt` shows only pre-existing generated-file diffs; WS-D source files are clean.
- **Blocked**: `cargo test --workspace` could not be executed due to tool permission restrictions. **Recommendation**: Dev must run and confirm green before merge.

## Residual / Tech Debt

1. **W-1 fix**: `list_presets` should use `state.capability_registry()`
2. **W-2 fix**: Remove unnecessary `#![allow(clippy::print_literal)]`
3. **W-3 fix**: Log warnings for `read_dir` entry errors
4. **Orphaned `system_preset.rs`**: Migrate or remove tests that reference the deprecated hardcoded graph builder
5. **Size limit on preset.yaml**: Consider adding a file-size guard in `load_system_preset_from_dir` to prevent DoS from accidentally enormous files

## Verdict

**Request Changes**

Rationale:
- W-1 (`CapabilityRegistry` mismatch) is a real interface-contract inconsistency that could cause system presets to be missing from API responses under non-default capability configurations.
- W-2 and W-3 are minor but should be fixed before merge to maintain code quality.
- Tests could not be run due to tool restrictions, so the CI gate is partially unverified.

Once W-1, W-2, W-3 are addressed and `cargo test --workspace` is confirmed green, this is safe to approve.
