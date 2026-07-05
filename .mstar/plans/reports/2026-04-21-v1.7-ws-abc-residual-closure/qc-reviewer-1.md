---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-21-v1.7-ws-abc-residual-closure"
verdict: "Approve"
generated_at: "2026-04-21"
---

# QC Review #1 — WS-A/B/C Residual Closure

**Reviewer**: @qc-specialist (#1)
**Working branch**: `fix/v1.7-ws-abc`
**Review range**: `git diff feature/v1.7...fix/v1.7-ws-abc`
**12 commits**, 9 files changed, +751 / -205 lines

---

## Scope

This review covers all 12 commits on `fix/v1.7-ws-abc` not on `feature/v1.7`, addressing 8 residuals across 3 workstreams:

| Workstream | Residuals | Description |
|---|---|---|
| WS-A (DTO) | DTO-C1, DTO-C2, DTO-W1 | MCP server propagation, version parse safety, Eq derive |
| WS-B (Permission) | PERM-W1, PERM-W2, PERM-W3 | toml_edit round-trip, JSON global rules, key validation |
| WS-C (Schedule) | R4, R7 | Module doc accuracy, cleanup_guard |

---

## Review Checklist

### 1. Correctness — Do changes correctly address the 8 residuals?

| Residual | Verdict | Notes |
|---|---|---|
| **DTO-C1** | ✅ PASS | `sdk_new_session_request_from_nexus` now maps `mcp_servers` via `nexus_mcp_server_to_sdk`, covering all 3 variants (Http, Sse, Stdio). Correct use of builder `.mcp_servers()` pattern. |
| **DTO-C2** | ✅ PASS | Extracted `sdk_protocol_version_from_nexus` replaces `unwrap_or(1)` silent fallback with explicit `match` + `tracing::warn!` + defaults to `LATEST`. Proper error surface. |
| **DTO-W1** | ✅ PASS | Added `Eq` derive to `NexusContentBlock`. Safe: all inner types (`NexusTextContent`, `NexusResourceLink`) already derive `Eq`. |
| **PERM-W1** | ✅ PASS | `save_toml_edit()` replaces `save()` using `toml_edit::DocumentMut` for round-trip. Comments and unknown keys preserved. Old `save()` fully removed. All 3 call sites in `policy.rs` updated. Tests confirm comment/unknown-key preservation. |
| **PERM-W2** | ✅ PASS | `build_list_json()` extracted from `print_list_json()` (testable). JSON output now includes `global.grant`/`global.deny` when present, omits when absent. Tests cover both branches. |
| **PERM-W3** | ✅ PASS | `validate_toml_keys()` checks `[grant, deny, agents, default]` at top level + `[grant, deny, ask]` under `[agents.<agent>]`. Warnings printed to `stderr` in `run_list`. Tests cover valid, unknown top-level, and unknown agent sub-keys. |
| **R4** | ✅ PASS | Module doc now accurately describes UTC-only DST safety and documents limitation (no wall-clock recurrence rule support). |
| **R7** | ✅ PASS | `cleanup_guard()` removes per-schedule guard from `HashMap`. Doc comments document safety properties (Arc clone semantics, no-op on missing key). Tests confirm cleanup + new guard creation, and noop on nonexistent schedule. |

### 2. Test Coverage

| Crate | New Tests | Coverage Quality |
|---|---|---|
| `nexus-acp-host` (client.rs) | 5 | All 3 MCP variants + empty + 3 protocol version cases |
| `nexus-acp-host` (policy.rs) | 3 | Comment preservation, unknown key preservation, key validation |
| `nexus42` (permission.rs) | 3 | JSON global present/absent, unknown key warning on list |
| `nexus-orchestration` (derivation.rs) | 2 | Cleanup + fresh guard creation, noop on nonexistent |
| **Total** | **13 new tests** | Edge cases well-covered |

Existing tests updated to use `save_toml_edit()` instead of `save()`: `test_save_and_load_policy`, `test_agent_rules_roundtrip` in `policy.rs`, and `test_preserves_comments_via_toml_edit` in `permission.rs`.

### 3. API Compatibility

- `nexus_mcp_server_to_sdk` — **new** private function, no API break
- `sdk_protocol_version_from_nexus` — **new** private function, no API break
- `PermissionPolicy::save_toml_edit()` — **new** public method; old `save()` **removed**
- `PermissionPolicy::policy_path()` — visibility changed from `fn` to `pub fn` (breaking only if external code was already using a workaround)
- `PermissionPolicy::validate_toml_keys()` — **new** public method
- `PermissionPolicy::load_toml_edit()`, `save_toml_edit_doc()`, `ensure_*_doc()`, `set_*_doc()`, `remove_*_doc()`, `clean_*_doc()` — **new** public methods
- `NexusContentBlock` — added `Eq` derive (additive, not breaking)
- `build_list_json()` — **new** private function in `permission.rs`

**Assessment**: `save()` removal is the only surface-level API change. All in-repo call sites updated. No external consumers known to depend on `PermissionPolicy::save()`.

### 4. Code Quality

- **Error handling**: All new code uses `anyhow::Result` with `.map_err()` for CLI error wrapping. No `unwrap()` in production paths across any changed file.
- **`#[deny(clippy::unwrap_used)]`**: The `permission` module is under this deny attribute — verified no `.unwrap()` in prod code.
- **Refactoring quality**: PERM-W1 correctly consolidates duplicated TOML editing logic from `permission.rs` (8 standalone functions, ~120 lines) into `PermissionPolicy` methods. Single source of truth.
- **`nexus_mcp_server_to_sdk`**: Exhaustive `match` on `NexusMcpServer` enum — compiler-enforced completeness.
- **Minor observation**: `sync_hashmap_to_table` and `sync_hashmap_to_table_nested` share ~70% structural similarity. A helper taking a path builder closure could reduce duplication, but the nesting complexity makes this a judgment call — not a defect.

### 5. Security

- **No credential leaks** — no secrets, API keys, or sensitive data in any changed file.
- **No `unsafe` code** — all changes are safe Rust.
- **`unwrap()` audit**: All `.unwrap()` and `.expect()` in changed files are either in `#[cfg(test)]` blocks or on values guaranteed by construction (e.g., `doc["agents"].as_table_mut().expect("agents table")` immediately after `ensure_agents_table_doc` creates it).
- **Key validation (PERM-W3)**: Warnings are informational only (printed to `stderr`), do not block `permission list` execution. Appropriate — unknown keys should be warned, not rejected, to allow forward-compatible future keys.

### 6. Generated Code

- ✅ **No files under `*/generated/` directories modified.** Diff touches only hand-written source files.

### 7. Documentation

- **R4 module doc** (`scheduler/mod.rs`): Accurately describes UTC-only DST safety, explicitly documents limitation for wall-clock recurrence. Well-structured with `# DST Safety` section heading.
- **`cleanup_guard` doc** (`derivation.rs`): `# Safety` section documents two safety properties (no-op on missing key, Arc clone semantics). Appropriate for a method with concurrency implications.
- **Method docs** on new `PermissionPolicy` methods: Clear, accurate, describe input/output and side effects.
- **Plan checklist**: All milestone checkboxes ticked in plan `.md` file.

---

## Findings Summary

### Critical: 0

No critical issues found.

### Warning: 0

No warnings found. The only potential concern (`.expect()` on `as_table_mut()` in helper methods) is on values guaranteed by construction — the table is created immediately before access in the same call chain.

### Suggestion: 3

| ID | Severity | Location | Description |
|---|---|---|---|
| S1 | Suggestion | `client.rs:148` | `serde_json::from_value(serde_json::json!(v)).unwrap_or(...)` in `sdk_protocol_version_from_nexus` is roundabout. A direct `acp::ProtocolVersion` constructor or `TryFrom<u16>` impl would be cleaner if the SDK supports it. Current code is functionally correct but incurs unnecessary JSON serialization/deserialization for a numeric cast. |
| S2 | Suggestion | `policy.rs` | `sync_hashmap_to_table` and `sync_hashmap_to_table_nested` are structurally similar (~30 lines each). Consider a shared helper taking a path-navigation closure if more table-sync methods are added in the future. Not urgent. |
| S3 | Suggestion | `status.json` | Plan `.md` file status is `InReview` but `status.json` still shows `"status": "InProgress"`, `"phase": "implement"`. Minor inconsistency — should be updated to `"status": "InReview"`, `"phase": "review"` before merge. |

---

## Cross-Reviewer Ready Notes

- **Integration risk**: LOW. Changes are well-scoped to 3 independent workstreams with minimal cross-workstream coupling.
- **Migration cost**: LOW. PERM-W1's `save()` → `save_toml_edit()` migration is complete with all call sites updated. No external API consumers identified.
- **This reviewer's unique findings**: S1 (JSON round-trip for protocol version), S3 (status.json inconsistency). S2 (code duplication in table sync) is a stylistic observation.
- **Expected overlap with other reviewers**: All 8 residual correctness verdicts, test coverage assessment, and security audit are shared baseline checks likely to be independently verified by reviewers #2 and #3.

---

## Verdict: **Approve**

All 8 residuals are correctly addressed with appropriate tests. No critical or warning-level issues. 3 minor suggestions do not block approval. CI gates (clippy, fmt) should be verified at merge time per the plan checklist.
