---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-05-21-v1.22-cli-deprecation-cleanup"
verdict: "Approve"
generated_at: "2026-05-21"
---

# Code Review Report — QC Specialist #2 (Security & Correctness Focus)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: glm-5
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-05-21T17:00:00Z

## Scope
- plan_id: 2026-05-21-v1.22-cli-deprecation-cleanup
- Review range / Diff basis: main (49778a5) → HEAD (2528f10)
- Working branch (verified): feature/v1.22-cli-deprecation-cleanup
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 45
- Commit range: 9397641..2528f10 (4 commits)
- Tools run: git diff, cargo test, cargo clippy, rg, read

## Findings

### 🔴 Critical
(None)

### 🟡 Warning
(None)

### 🟢 Suggestion
(None)

## Checklist Results

### Security (All PASS)
| Item | Status | Evidence |
|------|--------|----------|
| No credential/secret handling affected by cleanup | ✅ PASS | Auth module unchanged — only CLI path string updates (`nexus42 auth login` → `nexus42 platform auth login`) |
| No new `unsafe` blocks introduced | ✅ PASS | rg scan of commands/ directory returned no matches |
| Auth/authorization flow unchanged | ✅ PASS | Auth logic intact; only user-facing message paths updated |
| File permission handling unchanged | ✅ PASS | Daemon PID file handling not modified |
| No path traversal/injection risks in moved utilities | ✅ PASS | F001 path validation present in creator/mod.rs (`validate_creator_id_safe`, `validate_entry_id_safe`) |

### Correctness — Function Extraction (All PASS)
| Item | Status | Evidence |
|------|--------|----------|
| `default_creative_root` extracted | ✅ PASS | creator/mod.rs:81-86 |
| `validate_slug` extracted | ✅ PASS | creator/mod.rs:89-101 |
| `materialize_adr014_workspace` extracted | ✅ PASS | creator/mod.rs:104-151 |
| `persist_cli_workspace_selection` extracted | ✅ PASS | creator/mod.rs:154-167 |
| `InitCommand` extracted | ✅ PASS | creator/mod.rs:49-66 |
| `WorkspaceMeta` extracted | ✅ PASS | creator/mod.rs:69-78 |
| `init_workspace` extracted | ✅ PASS | creator/mod.rs:183-298 |
| `validate_world_id` extracted | ✅ PASS | creator/mod.rs:352-366 |
| `CloneArgs` extracted | ✅ PASS | creator/mod.rs:317-330 |
| `CloneSourceArg` extracted | ✅ PASS | creator/mod.rs:333-339 |
| `WorldCloneResponse` extracted | ✅ PASS | creator/mod.rs:342-349 |
| `validate_world_ref` extracted | ✅ PASS | creator/mod.rs:369-377 |
| `confirm_clone` extracted | ✅ PASS | creator/mod.rs:380-396 |
| `schedule.rs` moved correctly | ✅ PASS | daemon/schedule.rs contains `ScheduleCommand`, `run()`, `SCHEDULE_BASE`, all HTTP helpers |
| No functions dropped during extraction | ✅ PASS | Verified presence of all expected symbols in target files |

### Correctness — Module Restructuring (All PASS)
| Item | Status | Evidence |
|------|--------|----------|
| `acp/mod.rs` + sub-modules complete | ✅ PASS | Sub-modules: permission, policy, session — all present |
| `platform/mod.rs` + sub-modules complete | ✅ PASS | Sub-modules: auth, context, explore — all present |
| `system/mod.rs` + sub-modules complete | ✅ PASS | Sub-modules: config, db, debug, identity, runtime_mode — all present |
| `sync/mod.rs` + world.rs complete | ✅ PASS | Sub-module: world — present |

### Import Integrity (All PASS)
| Item | Status | Evidence |
|------|--------|----------|
| No circular dependencies | ✅ PASS | cargo check succeeded |
| All `pub use` re-exports correct | ✅ PASS | commands/mod.rs has 10 clean module declarations |
| No orphaned use statements | ✅ PASS | rg scan for deleted module paths returned no matches |
| All sub-modules use `crate::` paths | ✅ PASS | Verified in acp/mod.rs, platform/mod.rs, system/mod.rs, sync/mod.rs |

### Test Integrity (All PASS)
| Item | Status | Evidence |
|------|--------|----------|
| Integration tests updated | ✅ PASS | tests/integration.rs updated in diff |
| Regression tests updated | ✅ PASS | tests/regression.rs updated in diff |
| No tests removed | ✅ PASS | Test counts: 592+ passed, 0 failed |
| All `#[cfg(test)]` modules work | ✅ PASS | cargo test --workspace: all pass |

### Verification (All PASS)
| Item | Status | Evidence |
|------|--------|----------|
| `cargo test --workspace` passes | ✅ PASS | 592+ tests passed, 0 failed |
| `cargo clippy --all -- -D warnings` passes | ✅ PASS | 0 warnings, 0 errors |
| `rg '#\[deprecated\]' crates/nexus42/src/` returns 0 | ✅ PASS | No deprecated markers in source |
| `rg 'deprecated' cli.rs main.rs` returns 0 | ✅ PASS | Clean entry files |

## Source Trace

### Key Files Reviewed
| File | Lines | Review Notes |
|------|-------|--------------|
| crates/nexus42/src/commands/creator/mod.rs | 1-464 | Verified all extracted functions/types present |
| crates/nexus42/src/commands/daemon/schedule.rs | 1-1027 | Verified ScheduleCommand, run(), SCHEDULE_BASE, all helpers |
| crates/nexus42/src/commands/acp/mod.rs | 1-1362 | Verified sub-modules: permission, policy, session |
| crates/nexus42/src/commands/platform/mod.rs | 1-57 | Verified sub-modules: auth, context, explore |
| crates/nexus42/src/commands/system/mod.rs | 1-295 | Verified sub-modules: config, db, debug, identity, runtime_mode |
| crates/nexus42/src/commands/sync/mod.rs | 1-433 | Verified sub-module: world |
| crates/nexus42/src/cli.rs | 1-110 | Verified clean Commands enum (8 active) |
| crates/nexus42/src/main.rs | 1-82 | Verified clean command routing |
| crates/nexus42/src/commands/mod.rs | 1-26 | Verified 10 module declarations |
| crates/nexus42/src/auth/creator_auth.rs | diff only | Only CLI path string updates |
| crates/nexus42/src/auth/user_auth.rs | diff only | Only CLI path string updates |

### Diff Summary
- 45 files changed
- 1,102 insertions
- 2,366 deletions (net: -1,264 lines)
- 20 deprecated command modules deleted
- 6 modules restructured from flat files to directory modules

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

This PR removes ~2,300 lines of deprecated CLI code with complete extraction of shared utilities to appropriate locations. All security-sensitive paths (auth, credential handling) are untouched except for user-facing message string updates. Path traversal validation is preserved. Module restructuring maintains complete functionality across all sub-modules. Tests pass (592+), clippy passes (0 warnings), and no deprecated markers remain in the codebase.