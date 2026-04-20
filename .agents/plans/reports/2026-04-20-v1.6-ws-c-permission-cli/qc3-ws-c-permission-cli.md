---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-04-20-v1.6-ws-c-permission-cli"
verdict: "Approve"
generated_at: "2026-04-20"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Review Perspective: Performance & reliability (complexity, hot paths, resource lifecycle, concurrency risk, degradation behavior); test sufficiency and observability
- Report Timestamp: 2026-04-20

## Scope
- plan_id: `2026-04-20-v1.6-ws-c-permission-cli`
- Review range / Diff basis: `git diff a9381e8..HEAD` (WS-B Done → WS-C HEAD)
- Working branch (verified): `feature/v1.6`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 2 key files (`crates/nexus42/src/commands/permission.rs`, `crates/nexus-acp-host/src/policy.rs`) + 8 supporting files in diff stat
- Tools attempted: `cargo fmt --check`, `cargo clippy --all -- -D warnings` (blocked by environment permission system — evidence gap noted)
- Tools run: git diff, manual code inspection, grep

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

- **[S-001] `run_revoke` double-loads `list_agent_rules` — redundant I/O**: `run_revoke` calls `policy.list_agent_rules(agent)` twice in succession (lines 267 and 270-276). The second call is only to get `asked` (which was already destructured on line 267) and then rebuilds `has_rule` by iterating the same list again. Consolidating to a single call would halve the policy load cost on revoke.

  ```rust
  // Current: loads policy twice
  let (_, _, asked) = policy.list_agent_rules(agent);
  let has_rule = policy.list_agent_rules(agent).0.iter()...
  ```
  
  Fix: use the already-loaded `asked` in the chain directly, and only call `list_agent_rules(agent)` once.

- **[S-002] `run_list` for filtered agent does three `list_agent_rules` calls**: In `print_list_text` (line 138-148), when an `agent_filter` is provided, the code calls `policy.list_agent_rules(filter)` three times to check if all three vectors are empty. Each call re-iterates the HashMap. Replace with a single call and store the result.

  ```rust
  // Current: 3 lookups
  if policy.list_agent_rules(filter).0.is_empty()
      && policy.list_agent_rules(filter).1.is_empty()
      && policy.list_agent_rules(filter).2.is_empty()
  // Fix: single lookup
  let (g, d, a) = policy.list_agent_rules(filter);
  if g.is_empty() && d.is_empty() && a.is_empty()
  ```

- **[S-003] `run_reset` error message for empty agents is imprecise**: When `reset --agent <id>` finds no rules for the agent, the error says "No rules found for agent 'X'." When `reset` (no `--agent`) finds no agents at all, the error is "No agent rules configured." These are fine but could be improved to distinguish from each other more clearly — however this is cosmetic and not a regression.

- **[S-004] `permission.rs` module-level doc claims TOML preservation**: The module doc says "Uses `toml_edit` to preserve unknown fields and comments." The test `test_preserves_comments_via_toml_edit` confirms this for the `# My custom comment` case. However, the test only writes `# My custom comment\ndefault = "ask"\n` and then a grant. The comment is preserved after the first grant, but it's unclear whether the comment survives across multiple operations (e.g., grant → deny → revoke). This is a partial coverage observation — not a bug, but worth extending the test to cover multi-operation roundtrips.

## Source Trace

| Finding | Type | Reference | Confidence |
|---------|------|-----------|------------|
| S-001: double list_agent_rules | manual-reasoning | `permission.rs:265-276` | High |
| S-002: triple list_agent_rules | manual-reasoning | `permission.rs:138-148` | High |
| S-003: imprecise error msg | manual-reasoning | `permission.rs:307-311` | Medium |
| S-004: comment preservation test gap | manual-reasoning | `permission.rs:577-595` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: `Approve`

### Quality Assessment

**Correctness**: The `PermissionPolicy` engine additions in `policy.rs` are sound — agent-scoped rule precedence (agent > global > default) is correctly implemented. The `grant/deny/ask` mutual-exclusion logic (removing from other categories before inserting) is consistent across all three operations. Revoke and reset cleanup of empty tables is thorough. The `evaluate_for_agent` correctly falls through to global rules when no agent rule matches.

**TOML Preservation**: The `toml_edit` approach is correctly chosen for comment preservation. The test verifies a basic case. The `clean_empty_agent_tables` helper handles cascading cleanup (capability → action table → agent table → agents table) correctly.

**CLI UX**: All 6 commands (list/grant/deny/ask/revoke/reset) are wired with proper clap arguments. Error messages are informative. The `--agent` filter and `--output json` options on `list` are correctly implemented.

**Test Coverage**: Policy engine has 17 new tests covering grant/deny/ask overwrite, revoke, reset, TOML roundtrip, and agent precedence. CLI layer has 11 tests covering create, list, overwrite, revoke, reset, ask, global rule preservation, and comment preservation. Combined: **28 new tests** in the diff. No obvious happy-path gaps.

**Code Quality**: No `unsafe`, no SQL, no file handle leaks (all file ops are short-lived in helper functions). `tempfile::TempDir` in tests ensures cleanup. `tokio` is available but this code is synchronous (appropriate for TOML file I/O).

**Performance**: File I/O is per-command (not per-request). No unbounded collections. Agent map lookup is O(1) HashMap. No N+1 concerns.

**No Regressions**: Global rules and default policy are unaffected. Agent rules are additive and orthogonal. `PermissionPolicy::load`/`save` remain backward-compatible (new `agents` key is optional in serde).

**Lint/CI Gate**: `cargo fmt --check` and `cargo clippy --all -- -D warnings` were **not executable** in this environment due to bash permission restrictions (see `permission.edit` block in system prompt). This is an evidence gap. Both commands are listed in AGENTS.md as mandatory pre-merge checks. The reviewer was unable to obtain lint/format evidence from the current HEAD. **This should be resolved by CI or a subsequent local verification pass before merge.**

## Cross-Reviewer Ready Notes (for QC-1 / QC-2 peers)

- The `policy.rs` engine layer and `permission.rs` CLI layer are cleanly separated — engine tests are in `policy.rs`, CLI integration tests are in `permission.rs`.
- No shared mutable state between commands; each run function loads/saves independently.
- The `nexus-acp-host` crate's public API (`PermissionPolicy`, `PermissionDecision`, `DefaultPolicy`) is unchanged except for new methods — no breaking changes to existing callers.
- The `ask` decision value (`PermissionDecision::Ask`) was already present in the enum from V1.1; this WS-C exposes it via CLI.

## Completion Report v2

**Agent**: @qc-specialist-3
**Task**: QC review #3 of V1.6 WS-C — Permission Policy CLI. Independent third assessment.
**Status**: Partial (lint/clippy evidence unavailable due to bash permission restrictions in this environment)
**Scope Delivered**: Full manual code review of 2 key files + diff stat analysis, TOML roundtrip verification, test coverage assessment
**Artifacts**: `2026-04-20-v1.6-ws-c-permission-cli-qc3.md`
**Validation**: Manual code inspection + git diff analysis. Lint/clippy evidence NOT obtained due to environment bash restriction.
**Source Attribution**:
- Primary Evidence: `git diff a9381e8..HEAD` (captured in full), manual code inspection
- Evidence Quality: High (for code logic); Low (for CI-equivalent lint confirmation)
- Traceability: S-001 → `permission.rs:265-276`; S-002 → `permission.rs:138-148`; S-004 → `permission.rs:577-595`
**Issues/Risks**: 4 suggestions (all low severity). No Critical/Warning findings. Lint gate not verified locally.
**Plan Update**: PM to note lint evidence gap — recommend CI run confirm clippy/fmt clean before merge.
**Handoff**: @project-manager
**Git**: Not committed — bash tool was blocked for `git add` / `git commit` operations in this session. Report written to `.agents/plans/reports/2026-04-20-v1.6-ws-c-permission-cli/`. PM should run `git add .agents/plans/reports/2026-04-20-v1.6-ws-c-permission-cli/qc3-ws-c-permission-cli.md && git commit -m "docs(qc): 2026-04-20-v1.6-ws-c-permission-cli qc3 report"` in the repository root.
