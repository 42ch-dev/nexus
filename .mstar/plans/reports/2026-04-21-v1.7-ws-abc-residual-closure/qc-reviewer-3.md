---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-04-21-v1.7-ws-abc-residual-closure"
verdict: "Approve"
generated_at: "2026-04-21"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Review Perspective: Performance & Reliability (complexity, hot paths, resource lifecycle, concurrency risks, degradation risks); Test sufficiency & observability
- Report Timestamp: 2026-04-21

## Scope
- plan_id: `2026-04-21-v1.7-ws-abc-residual-closure`
- Review range / Diff basis: `git diff feature/v1.7...fix/v1.7-ws-abc`
- Working branch (verified): `fix/v1.7-ws-abc`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 7
- Commit range: `76551c2` (chore: start WS-A/B/C) → `3af735f` (docs: mark WS-C tasks T7/T8 complete)
- Tools run: git diff, git log, manual code review

## Findings

### 🟢 Suggestion

- **R3-1: `policy_path` visibility escalation** — The `policy_path` method was `fn` (implicitly private) in the original implementation but is now declared `pub fn`. While this enables CLI commands to use `PermissionPolicy::policy_path` directly, it expands the public API surface of `policy.rs` beyond what the plan documented. The method is a trivial path constructor with no internal state access beyond workspace_root. No behavior change risk, but the broader visibility should be noted.

  *Source*: `crates/nexus-acp-host/src/policy.rs` lines 250-252 (`pub fn policy_path`)

- **R3-2: `VALID_TOP_LEVEL_KEYS` hardcodes schema contract** — `VALID_TOP_LEVEL_KEYS` is a `&'static [&str]` slice listing accepted TOML top-level keys. Future schema additions (e.g., a new `roles` section) will require code changes to suppress false-positive warnings. The plan addressed this in PERM-W3's edge cases. This is acceptable for V1.7 but should be tracked as a schema evolution consideration.

  *Source*: `crates/nexus-acp-host/src/policy.rs` line 62

- **R3-3: `load()` still uses `toml::from_str` which discards comments** — The existing `PermissionPolicy::load()` method continues using `toml::from_str` for deserialization, which cannot preserve comments. Only `load_toml_edit()` preserves comments. The plan correctly uses `load_toml_edit` for writes via the CLI, and the `list` command uses it only for validation. The asymmetry is intentional and documented. No user-facing behavior regression.

  *Source*: `crates/nexus-acp-host/src/policy.rs` lines 147-156

## Source Trace

- Finding R3-1: Source Type: git-diff | Source Reference: `policy.rs` visibility change
- Finding R3-2: Source Type: manual-reasoning | Source Reference: `policy.rs` line 62 + plan §2 PERM-W3 edge cases
- Finding R3-3: Source Type: git-diff | Source Reference: `policy.rs` load method vs save_toml_edit

## Verification Summary

### WS-A (DTO-C1, DTO-C2, DTO-W1) ✅
- **DTO-C1**: `mcp_servers` propagation implemented via `nexus_mcp_server_to_sdk()` with variant mapping (Http/Sse/Stdio). Tests verify all 3 server types and empty vec.
- **DTO-C2**: `sdk_protocol_version_from_nexus()` replaces `unwrap_or(1)` with explicit `tracing::warn` on parse failure, defaults to `LATEST`. Tests verify valid, invalid, and empty strings.
- **DTO-W1**: `Eq` derive added to `NexusContentBlock`. All inner types already implement `Eq`.

### WS-B (PERM-W1, PERM-W2, PERM-W3) ✅
- **PERM-W1**: `toml_edit` refactor complete — `save_toml_edit()`, `load_toml_edit()`, `save_toml_edit_doc()`, and mutation helpers (`ensure_agents_table_doc`, `ensure_agent_action_table_doc`, `set_agent_capability_doc`, `remove_agent_capability_doc`, `clean_empty_agent_tables_doc`) moved to `policy.rs`. CLI `permission.rs` delegates to these. Comment and unknown key preservation tests added.
- **PERM-W2**: JSON output now includes `"global"` key with `grant`/`deny` when global rules exist. Tests verify both presence when rules exist and absence when they don't.
- **PERM-W3**: `validate_toml_keys()` checks top-level and agent sub-keys against known schema. Warnings printed to stderr in `run_list`. Tests verify known keys produce no warnings and unknown keys produce warnings.

### WS-C (R4, R7) ✅
- **R4**: Module doc updated to accurately describe UTC-only DST safety and documented limitation on wall-clock recurrence rules. Matches plan's recommended approach exactly.
- **R7**: `cleanup_guard()` method added with proper safety docs (Arc-sharing semantics explained). Tests verify: entry removed and new guard created; non-existent schedule is no-op.

## Reviewer #3独视角总结

**Performance & Reliability observations:**

1. **Resource lifecycle (PERM-W1)**: The `toml_edit` approach reads the file into memory as `DocumentMut`, modifies it, and writes it back. For small permission files this is O(n) where n = file size — negligible. No unbounded growth risks identified.

2. **Concurrency (R7)**: The `cleanup_guard` uses `Arc<Mutex<()>>` correctly — active writers hold a clone and complete safely. The `schedule_guard` entry creation is atomic via `HashMap::entry().or_insert_with()`. The `await` lock acquisition pattern is consistent with the rest of the async codebase.

3. **Observability (DTO-C2)**: Protocol version parse failures now emit `tracing::warn` with version string and error detail. This enables monitoring/alerting on malformed version strings from clients.

4. **Test coverage**: Each residual has explicit unit tests. The `test_json_output_includes_global_when_present` and `test_json_output_omits_global_when_absent` tests cover the PERM-W2 JSON shape change. The `test_validate_toml_keys_*` tests cover PERM-W3 validation. The `cleanup_guard_*` tests cover R7. All are new tests added in this branch.

5. **No hot-path changes**: None of the residual fixes touch hot paths. The `toml_edit` save is triggered only on explicit CLI commands (grant/deny/ask/revoke/reset). MCP server propagation is on session initiation (cold path).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

All 8 residuals (DTO-C1, DTO-C2, DTO-W1, PERM-W1, PERM-W2, PERM-W3, R4, R7) are implemented per the plan specification. No blocking issues identified. The 3 suggestions are minor observations that do not prevent approval.

---

## Completion Report v2

**Agent**: @qc-specialist-3
**Task**: QC code review of `2026-04-21-v1.7-ws-abc-residual-closure` — 8 residuals across 3 workstreams
**Status**: Done
**Scope Delivered**: All 7 files in diff; all 8 residuals reviewed
**Artifacts**: QC review report at `.agents/plans/reports/2026-04-21-v1.7-ws-abc-residual-closure/qc-reviewer-3.md`
**Validation**: Manual code review + git diff analysis; cargo clippy/fmt blocked by permission
**Source Attribution**:
- Primary Evidence: git diff `feature/v1.7...fix/v1.7-ws-abc`
- Evidence Quality: High
- Traceability: R3-1 → policy.rs visibility; R3-2 → VALID_TOP_LEVEL_KEYS; R3-3 → load() vs load_toml_edit()
**Issues/Risks**: No Critical or Warning findings; 3 Suggestions (visibility escalation, hardcoded schema keys, load/save asymmetry — all acceptable for V1.7)
**Plan Update**: PM to update `status.json` residuals as closed after all QC reviewers complete
**Handoff**: @project-manager
**Git**: (to be added after commit)
