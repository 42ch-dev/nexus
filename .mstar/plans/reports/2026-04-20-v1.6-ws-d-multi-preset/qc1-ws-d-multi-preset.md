---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-20-v1.6-ws-d-multi-preset"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# QC Review #1 — V1.6 WS-D: System-Managed Multi-Preset Scheduler

**Reviewer**: @qc-specialist (#1)
**Plan ID**: `2026-04-20-v1.6-ws-d-multi-preset`
**Working branch**: `feature/v1.6`
**Review range / Diff basis**: `git diff ccd6aee..HEAD` (WS-C Done → WS-D HEAD)
**Review cwd / Worktree path**: `/Users/bibi/workspace/organizations/42ch/nexus`

---

## Scope

### Files changed (11 files, +801 / -34 lines)

| File | Change Type | Summary |
|------|-------------|---------|
| `crates/nexus-orchestration/src/system_preset_dir.rs` | **NEW** | System preset directory scanner (578 lines, new module) |
| `crates/nexus42d/src/main.rs` | Modified | Replaces hardcoded `system_preset::build()` with directory scan + auto-create |
| `crates/nexus42/src/commands/system.rs` | **NEW** | `nexus42 system preset list` CLI command (95 lines) |
| `crates/nexus42/src/commands/mod.rs` | Modified | Adds `system` module export |
| `crates/nexus42/src/main.rs` | Modified | Wires `SystemPresetCommand` into CLI enum and dispatch |
| `crates/nexus42d/src/api/handlers/orchestration/presets.rs` | Modified | `list_presets` handler scans `_system/` dir instead of hardcoded push |
| `crates/nexus42d/tests/orchestration_http_smoke.rs` | Modified | Smoke test creates `_system/maintenance/` dir for preset listing test |
| `crates/nexus-orchestration/src/lib.rs` | Modified | Exports new `system_preset_dir` module |
| `crates/nexus-orchestration/src/engine.rs` | Modified | Adds `Clone` impl for `GraphFlowEngine` |
| `crates/nexus-orchestration/src/preset/loader.rs` | Modified | Adds `Clone` derive for `LoadedPreset` |
| `crates/nexus-contracts/src/local/orchestration/http.rs` | Modified | Adds `Deserialize` to `ListPresetsResponse` |

### Old code retained (not removed in this diff)

- `crates/nexus-orchestration/src/system_preset.rs` — **still present and exported** in `lib.rs` but **no longer called** from `nexus42d/src/main.rs`. This is dead code.

---

## Verification Performed

| Check | Result | Evidence |
|-------|--------|----------|
| `cargo +nightly fmt --all -- --check` | **PASS** | No formatting issues |
| `cargo clippy --all -- -D warnings` | **PASS** | Zero warnings/errors |
| `cargo test --workspace` | **BLOCKED** | Permission restrictions prevented test execution |
| Manual code review (all 11 files) | **DONE** | See findings below |

---

## Findings

### F1: Dead code — `system_preset.rs` not removed (Warning)

**Severity**: Warning
**File**: `crates/nexus-orchestration/src/system_preset.rs`, `crates/nexus-orchestration/src/lib.rs`

The old `system_preset.rs` module remains in the crate and is still exported via `pub mod system_preset;` in `lib.rs`. It is no longer referenced from `nexus42d/src/main.rs` (the only external caller was replaced by `system_preset_dir`). The module's own doc comment says "WS3 will replace this with a preset file loader" — this plan (WS-D) has done exactly that, but the old module was not cleaned up.

**Impact**: Carries dead code into the release; adds compilation overhead and potential confusion for future maintainers who may wonder which module is authoritative.

**Recommendation**: Remove `pub mod system_preset;` from `lib.rs` and delete `system_preset.rs`. If the `PresetCapabilityTask` / `EndTask` patterns are useful reference, move them to a test-only module or a design doc.

---

### F2: `list_presets` handler rescans directory on every request (Warning)

**Severity**: Warning
**File**: `crates/nexus42d/src/api/handlers/orchestration/presets.rs` (line 18-21)

```rust
let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
let scan_result = system_preset_dir::scan_system_presets(state.nexus_home(), &caps);
```

Every `GET /v1/local/orchestration/presets` call performs a full filesystem scan + YAML parse + validation of all `_system/` presets. This includes:
- `read_dir()` on the filesystem
- `read_to_string()` for each `preset.yaml`
- `serde_yaml::from_str()` + full `validate_manifest()` per preset
- Capability registry lookups

While this is a local API and unlikely to be high-traffic, it's inconsistent with the pattern used for embedded presets (which use an `include_dir!`-based in-memory cache). For consistency and performance, system presets should be scanned once at boot and the results cached (e.g., in `WorkspaceState`).

**Impact**: Each API call performs unnecessary I/O and CPU work. Under rapid polling (e.g., a UI refreshing preset list), this could cause measurable overhead.

**Recommendation**: Store the `SystemPresetScanResult` (or just the list of IDs) in `WorkspaceState` at boot, similar to how the engine and capability registry are stored. The handler can then read from memory instead of rescanning.

---

### F3: `ensure_maintenance_preset` error is logged but silently ignored (Note)

**Severity**: Note (Informational)
**File**: `crates/nexus42d/src/main.rs` (lines 169-173)

```rust
match system_preset_dir::ensure_maintenance_preset(&system_presets_dir) {
    Ok(true) => tracing::info!("auto-created _system.maintenance preset directory"),
    Ok(false) => {} // Already existed
    Err(e) => tracing::warn!("failed to auto-create _system.maintenance: {}", e),
}
```

If `ensure_maintenance_preset` fails (e.g., disk full, permission denied, read-only filesystem), the warning is logged but execution continues. The subsequent `scan_system_presets` call will find no `_system/maintenance/` directory and return an empty result. This means on a fresh install where auto-creation fails, the user gets **no** `_system.maintenance` preset and **no** error — the daemon starts but the maintenance preset is silently absent.

**Assessment**: This matches the plan's T4 requirement ("graceful degradation"), but the plan's evidence criteria state "`nexus42 schedule start _system.maintenance` identical to pre-V1.6 behavior." If auto-creation fails, this evidence criterion cannot be met.

**Recommendation**: Acceptable as-is for pre-1.0, but document this behavior in user-facing docs. Consider adding a startup diagnostic (e.g., `nexus42 doctor`) that checks for the presence of the maintenance preset.

---

### F4: `Clone` impl on `GraphFlowEngine` added without comment in diff (Note)

**Severity**: Note (Informational)
**File**: `crates/nexus-orchestration/src/engine.rs` (lines 508-515)

```rust
impl Clone for GraphFlowEngine {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            caps: self.caps.clone(),
        }
    }
}
```

This is required because `main.rs` now does `concrete_engine.clone()` before wrapping it in `Arc<dyn OrchestrationEngine>`. The impl is correct — it clones `Arc`s, so it's cheap. However, `GraphFlowEngine` previously did not derive `Clone` and this change was necessary for the WS-D boot sequence. It's a reasonable addition with no negative side effects.

---

### F5: `LoadedPreset` derives `Clone` (Note)

**Severity**: Note (Informational)
**File**: `crates/nexus-orchestration/src/preset/loader.rs` (line 22)

`LoadedPreset` now has `#[derive(Clone)]`. This is needed because `system_preset_dir.rs` stores `LoadedPreset` inside `SystemPresetEntry` and the entry is cloned/used during the boot scan. The `Arc<Graph>` fields inside make this cheap.

---

### F6: CLI `system preset list` uses `print_literal` allow (Note)

**Severity**: Note (Informational)
**File**: `crates/nexus42/src/commands/system.rs` (line 5)

```rust
#![allow(clippy::print_literal)]
```

This is consistent with other CLI commands that use `println!` for user output. The module-level `#[deny(clippy::unwrap_used)]` from `mod.rs` is inherited correctly.

---

### F7: `list_presets` test no longer asserts `_system.maintenance` presence (Note)

**Severity**: Note (Informational)
**File**: `crates/nexus42d/src/api/handlers/orchestration/presets.rs` (test at lines 73-92)

The previous test asserted both `novel-writing` and `_system.maintenance` were present. The new test only asserts `novel-writing`. The comment explains that `_system.maintenance` is auto-created by `ensure_maintenance_preset` "if the scan runs" but the test environment may not have the directory set up.

**Assessment**: This is a reasonable adaptation since the handler now does a filesystem scan (which depends on the test workspace having the directory). The smoke test (`orchestration_http_smoke.rs`) covers the full `_system.maintenance` presence scenario separately. However, the handler unit test would be stronger if it also set up a minimal `_system/` directory to assert the merge behavior.

---

### F8: Architecture — `_system.` prefix convention is correct (Positive)

The `_system.` prefix is applied consistently:
- `system_preset_dir.rs`: `SYSTEM_PREFIX = "_system."`
- `list_system_presets` in CLI: filters by `starts_with("_system.")`
- Daemon boot: uses `entry.qualified_id` which already has the prefix

This creates a clean namespace separation between system and embedded presets.

---

### F9: Test coverage is thorough for `system_preset_dir.rs` (Positive)

The new module includes 13 unit tests covering:
- Directory layout path construction
- Missing directory (empty result)
- Single valid preset loading
- Multiple preset loading
- Corrupted YAML handling
- Empty directory (no preset.yaml)
- Hidden directory skipping
- ID listing and lookup
- `ensure_maintenance_preset` first-start creation
- `ensure_maintenance_preset` non-overwrite of existing
- Embedded YAML validity

This is excellent test coverage for the core module.

---

## Task-by-Task Verification Against Plan

| Plan Task | Status | Assessment |
|-----------|--------|------------|
| **T1**: Define system preset directory convention (`~/.nexus42/presets/_system/<name>/`) | ✅ Met | `SYSTEM_PRESET_DIR_NAME = "_system"`, `system_preset_base_dir()` constructs `<nexus_home>/presets/_system/` |
| **T2**: Engine startup scans `_system.*` directories and registers all discovered presets | ✅ Met | `scan_system_presets()` in `main.rs` boot, loops over results, calls `build_wired_outer_graph()` + `start_session()` |
| **T3**: Extract `_system.maintenance` from hardcoded Rust into directory bundle. First-start fallback auto-creates from embedded content | ✅ Met | `EMBEDDED_MAINTENANCE_YAML` constant + `ensure_maintenance_preset()` auto-creates on first start |
| **T4**: Graceful handling: missing directory = no system presets; corrupted = log warning + skip | ✅ Met | `scan_system_presets()` returns empty for missing dir; per-preset errors produce warnings, not failures |
| **T5**: Implement `nexus42 system preset list` command | ✅ Met | `system.rs` with `preset list` subcommand, calls daemon API, filters by `_system.` prefix |

---

## Shared Baseline Checks

| Check | Status | Notes |
|-------|--------|-------|
| **Functional regression / behavior change** | ⚠️ Minor | `_system.maintenance` is no longer hardcoded in Rust but loaded from YAML. Behavior should be identical per `EMBEDDED_MAINTENANCE_YAML` definition. Old `system_preset.rs` module is dead code (F1). |
| **Blocking security issues** | ✅ None | No hardcoded secrets, no injection vectors, no permission bypasses. Directory scanning is limited to `_system/` subdirectory. |
| **Data consistency issues** | ✅ None | No data migration or schema changes. Backward compatible — existing users get auto-created `_system/maintenance/`. |
| **Test coverage** | ⚠️ Adequate but gaps | `system_preset_dir.rs` has excellent coverage. CLI command has parse tests but no integration test. Smoke test covers API. Workspace test execution was blocked by permissions. |

---

## Severity Summary

| Severity | Count | Finding IDs |
|----------|-------|-------------|
| Critical | 0 | — |
| Warning | 2 | F1, F2 |
| Note | 5 | F3, F4, F5, F6, F7 |
| Positive | 2 | F8, F9 |

---

## Verdict: **Request Changes**

**Rationale**: Two Warning-level findings must be addressed before approval:

1. **F1 (Dead code)**: The old `system_preset.rs` module should be removed. It is no longer called, still exported, and its doc comment references a future replacement that has already happened. Leaving it creates maintenance confusion.

2. **F2 (Rescan on every API call)**: The `list_presets` handler performs filesystem I/O and YAML parsing on every request. This is a performance and consistency concern — the scan should happen once at boot and the result cached in `WorkspaceState`.

Both are fixable without architectural changes. After these are resolved, a re-review should yield `Approve`.

---

## Cross-Reviewer Ready Notes

- **Integration risk**: LOW. The changes are well-contained to system preset discovery and the CLI command. No wire contract changes, no schema changes.
- **Migration cost**: LOW. Users with existing `_system/maintenance/` directories see no change. Fresh installs get auto-created maintenance preset. The only action required is removing the dead `system_preset.rs` module.
- **Dependency direction**: Correct. `nexus42d` → `nexus_orchestration::system_preset_dir` → `nexus_orchestration::preset::loader`. No circular dependencies introduced.
- **Reviewer #1 unique findings**: F1 (dead code removal), F2 (rescan on every request)
- **Cross-reviewer overlap expected**: F3 (graceful error handling — other reviewers may note this), F7 (test coverage gaps — QA reviewer likely to flag)
