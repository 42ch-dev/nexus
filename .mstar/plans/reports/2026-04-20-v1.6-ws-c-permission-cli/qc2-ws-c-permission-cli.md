---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-04-20-v1.6-ws-c-permission-cli"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# Code Review Report — V1.6 WS-C Permission Policy CLI

## Reviewer Metadata
- **Reviewer**: @qc-specialist-2
- **Review Perspective**: Security & correctness (primary), maintainability & interface contracts (secondary)
- **Report Timestamp**: 2026-04-20T00:00:00Z

## Scope
- **plan_id**: `2026-04-20-v1.6-ws-c-permission-cli`
- **Review range / Diff basis**: `git diff a9381e8..HEAD` (WS-B Done → WS-C HEAD)
- **Working branch (verified)**: `feature/v1.6`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 7
  - `crates/nexus42/src/commands/permission.rs` (new, 638 lines)
  - `crates/nexus-acp-host/src/policy.rs` (modified, +389/-0 lines)
  - `crates/nexus42/src/commands/mod.rs` (modified)
  - `crates/nexus42/src/main.rs` (modified)
  - `crates/nexus42/Cargo.toml` (modified)
  - `crates/nexus-acp-host/Cargo.toml` (modified)
  - `crates/nexus-acp-host/src/lib.rs` (modified)
- **Commit range**: `a9381e8..303b518` (plan metadata) + `649cb1c` (dev commit)
- **Tools run**: `cargo clippy -p nexus42 -p nexus-acp-host -- -D warnings` (clean pass)

> **Note**: `cargo test` could not be executed due to bash permission constraints (no `cargo test*` pattern in allowlist). The test coverage assessment below is based on static code analysis.

## Findings

### 🔴 Critical
_None._

### 🟡 Warning

#### W-001 — Unused `toml_edit` dependency in `nexus-acp-host`
- **Location**: `crates/nexus-acp-host/Cargo.toml:25` (+`toml_edit = "0.22"`)
- **Issue**: `toml_edit` is added to `nexus-acp-host`'s dependencies but is **never imported or used** in that crate. The `policy.rs` module in `nexus-acp-host` only uses the standard `toml` crate for serialization/deserialization. `toml_edit` is only consumed by the CLI in `crates/nexus42/src/commands/permission.rs`.
- **Impact**: Unnecessary compile-time overhead, binary bloat, and dependency surface expansion in a library crate.
- **Fix**: Remove `toml_edit` from `crates/nexus-acp-host/Cargo.toml`.
- **Cross-reviewer note**: @qc-specialist / @qc-specialist-3 should verify dependency hygiene across the workspace.

#### W-002 — `print_list_json` omits global grant/deny rules
- **Location**: `crates/nexus42/src/commands/permission.rs`, `print_list_json` (~lines 232–253)
- **Issue**: The JSON output only includes `default` (string) and `agents` (object) fields. Global `grant` and `deny` rules, which are prominently displayed in the text output (`print_list_text`), are completely absent from the JSON representation.
- **Impact**: API/scripting consumers using `--output json` cannot see global rules, creating a **format inconsistency** and incomplete data exposure. This breaks the principle that different output formats of the same command should present equivalent information.
- **Fix**: Add `global_grant` and `global_deny` arrays (or equivalent structure) to the JSON output object, mirroring the text output's global rules section.

#### W-003 — No input validation/sanitization for TOML keys
- **Location**: `crates/nexus42/src/commands/permission.rs`, `ensure_agent_action_table`, `set_agent_capability`, `remove_agent_capability`
- **Issue**: `agent` and `capability` strings are used directly as TOML table keys without validation. TOML table keys with special characters (dots `.`, brackets `[]`, quotes `"`) create nested table structures rather than literal quoted keys.
- **Impact**: An agent ID like `foo.bar` creates `[agents.foo.bar.grant]` (nested tables) instead of `[agents."foo.bar".grant]` (single agent named `foo.bar`). This is a **data integrity bug** — the serialized structure no longer matches the intended semantic model, and deserialization may fail or produce incorrect results.
- **Exploitability / Impact scope**: Low direct security impact (local CLI, no privilege escalation), but high for data integrity. A user could accidentally or intentionally create unmaintainable TOML structures that break subsequent operations.
- **Fix**: Validate agent IDs and capability names against an allowlist (e.g., `^[a-zA-Z0-9._-]+$`), or explicitly escape/quote keys when inserting into `toml_edit`.
- **Cross-reviewer note**: Verify if similar unvalidated key insertion exists in other TOML-mutating commands.

#### W-004 — Fragile nested mutable borrow pattern in `clean_empty_agent_tables`
- **Location**: `crates/nexus42/src/commands/permission.rs`, `clean_empty_agent_tables` (~lines 480–510)
- **Issue**: The function holds simultaneous mutable borrows of `doc → agents → agent_table` at multiple nesting levels, then attempts to call `agents.as_table_like_mut()` while `agent_table` (borrowed from `agents`) is still in scope. While the current borrow checker accepts this (the code compiles), the pattern is **difficult to reason about**, prone to breakage during refactoring, and may fail with future compiler changes or different `toml_edit` versions.
- **Impact**: Maintenance risk. A future refactor (e.g., extracting a helper) could easily introduce a borrow checker error or, worse, undefined behavior if `unsafe` were introduced to "fix" it.
- **Fix**: Restructure to reduce borrow depth — e.g., check emptiness first, then drop intermediate borrows before mutating the parent, or use a two-pass approach (collect keys to remove, then remove them).

### 🟢 Suggestion

#### S-001 — Inefficient repeated `list_agent_rules` calls
- **Location**: `permission.rs`, `print_list_text` and `run_revoke`
- **Issue**: `print_list_text` calls `policy.list_agent_rules(filter)` three times to check emptiness. `run_revoke` calls it three times (once for `asked`, then chains `granted` + `denied` + `asked`).
- **Improvement**: Cache the tuple result in a local binding.

#### S-002 — Tests assert execution success but not output content
- **Location**: `permission.rs` test module
- **Issue**: Tests like `test_grant_then_list_shows_rule`, `test_list_json_output`, `test_list_filtered_by_agent` contain comments like "Verified by successful execution" or test policy state directly rather than capturing and asserting CLI stdout/stderr.
- **Improvement**: For `test_list_json_output`, parse the JSON and assert structure. For `test_grant_then_list_shows_rule`, capture stdout and assert it contains the expected rule text.

#### S-003 — `output_format` uses unvalidated `String`
- **Location**: `PermissionCommand::List`, `output_format: String`
- **Issue**: Any value other than `"json"` silently falls through to text mode. A typo like `"jsob"` or `"JSON"` (case-sensitive) produces text output unexpectedly.
- **Improvement**: Use a clap `ValueEnum` (`text`, `json`) for type-safe parsing and automatic help text.

#### S-004 — `HashMap<String, bool>` semantically equivalent to `HashSet<String>`
- **Location**: `AgentRules` struct in `policy.rs`
- **Issue**: The `bool` value in `grant`/`deny`/`ask` maps is always `true` and never read (only `contains_key` is used). A `HashSet<String>` would express the intent more clearly and save a boolean's worth of memory per entry.
- **Improvement**: Consider migrating to `HashSet<String>` in a future refactor, keeping serializer compatibility.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-001 | manual-reasoning | `crates/nexus-acp-host/Cargo.toml` vs. `rg toml_edit crates/nexus-acp-host/src/` | High |
| W-002 | manual-reasoning | `permission.rs:232-253`, `permission.rs:170-228` (text output comparison) | High |
| W-003 | manual-reasoning | `permission.rs:418-434` (`ensure_agent_action_table`) | High |
| W-004 | manual-reasoning | `permission.rs:480-510` (`clean_empty_agent_tables`) | Medium |
| S-001 | manual-reasoning | `permission.rs:170-185`, `permission.rs:310-325` | High |
| S-002 | manual-reasoning | `permission.rs:560-565`, `permission.rs:610-615` | High |
| S-003 | manual-reasoning | `permission.rs:36-39` | High |
| S-004 | manual-reasoning | `policy.rs:85-105` (`AgentRules`) | High |

## Cross-Reviewer Ready Notes

- **W-003 (input validation)**: The TOML key injection risk is the most security-relevant finding in this review. While the CLI is local and unprivileged, unvalidated keys breaking the TOML structure is a correctness issue that could affect the daemon's policy parser. Recommend all reviewers verify the proposed fix.
- **W-002 (JSON output parity)**: A functional regression in API contract. Other reviewers should confirm whether JSON consumers exist in the test suite or documentation that would break.
- **W-001 (unused dep)**: Pure dependency hygiene; easy fix.
- **TOML preservation**: The `toml_edit` approach for preserving comments and unknown fields is correctly implemented (tested by `test_preserves_comments_via_toml_edit`). This satisfies the plan's TOML preservation requirement.
- **Test coverage**: The module has 16 unit tests covering all 6 CLI subcommands and the core policy engine extensions. However, stdout-asserting tests are missing (S-002).

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: **Request Changes**

Rationale: W-002 (JSON output omits global rules) is a functional regression in output parity, and W-003 (unvalidated TOML keys) is a data integrity risk. Both are blocking for approval under the review harness severity gate. W-001 and W-004 should also be addressed before merge.

## CI Gate

- `cargo clippy -p nexus42 -p nexus-acp-host -- -D warnings`: **PASS** (exit 0, no warnings)
- `cargo fmt --check`: Generated-code diffs only (outside review scope; `.rustfmt.toml` ignores generated dir on nightly)
- `cargo test`: Could not verify (bash permission constraint). Tests were reviewed statically and appear comprehensive for happy-path coverage, but stdout-asserting tests are missing (S-002).
