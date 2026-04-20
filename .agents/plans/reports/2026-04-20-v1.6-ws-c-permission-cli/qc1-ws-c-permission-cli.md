---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-20-v1.6-ws-c-permission-cli"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# QC Review #1 — V1.6 WS-C Permission Policy CLI

**Reviewer**: `@qc-specialist` (Reviewer #1 — Architecture & maintainability focus)
**Working branch**: `feature/v1.6`
**Review cwd / Worktree path**: `/Users/bibi/workspace/organizations/42ch/nexus`
**Review range / Diff basis**: `git diff a9381e8..HEAD`
**Scope**: New `nexus42 permission` command group (T1–T6), supporting changes in `nexus-acp-host/src/policy.rs`

## Files Changed (10 files, +1050 / −15 lines)

| File | Δ | Notes |
|------|---|-------|
| `crates/nexus42/src/commands/permission.rs` | +638 new | CLI command implementation |
| `crates/nexus-acp-host/src/policy.rs` | +339 / −9 | Agent-scoped rule engine |
| `crates/nexus42/src/main.rs` | +6 / −4 | Subcommand registration |
| `crates/nexus42/src/commands/mod.rs` | +2 | Module declaration |
| `crates/nexus-acp-host/Cargo.toml` | +1 | `toml_edit` dep |
| `crates/nexus42/Cargo.toml` | +1 | `toml_edit` dep |
| `Cargo.lock` | +2 | Lockfile update |
| `.agents/plans/2026-04-20-v1.6-ws-c-permission-cli.md` | plan updated | T1–T6 marked done |
| `.agents/status.json` | +1 / −1 | Status → InReview |

---

## Verification Performed

| Check | Command | Result |
|-------|---------|--------|
| Clippy | `cargo clippy --all -- -D warnings` | ✅ Pass (no warnings) |
| Format | `cargo fmt --check` | ✅ No diffs in changed files (only pre-existing diffs in `nexus-contracts/src/generated/`, unrelated to this change) |
| Diff coverage | `git diff a9381e8..HEAD` | ✅ All 10 changed files reviewed |
| Regression scan | grep for existing callers of modified symbols | ✅ No regression in existing callers |

---

## Task Coverage

| Task | Implementation | Status |
|------|---------------|--------|
| T1: `permission list [--agent]` | `run_list`, `print_list_text`, `print_list_json` | ✅ Implemented |
| T2: `permission grant <agent> <cap>` | `run_grant` | ✅ Implemented |
| T3: `permission deny <agent> <cap>` | `run_deny` | ✅ Implemented |
| T4: `permission ask <agent> <cap>` | `run_ask` | ✅ Implemented |
| T5: `permission revoke <agent> <cap>` | `run_revoke` | ✅ Implemented |
| T6: `permission reset [--agent]` | `run_reset` | ✅ Implemented |

---

## Findings

### W1 — TOML serialization inconsistency between `policy` and `permission` commands

**Severity**: Warning
**Category**: Architecture / data integrity
**Files**: `crates/nexus42/src/commands/policy.rs`, `crates/nexus42/src/commands/permission.rs`

**Issue**: The two CLI command groups operate on the same `permissions.toml` file but use different TOML libraries with different preservation guarantees:

- **`policy.rs`** uses `toml::from_str` → modify in-memory struct → `toml::to_string_pretty` to save. This **re-serializes** the entire file, **losing comments, unknown fields, and original formatting**.
- **`permission.rs`** uses `toml_edit::DocumentMut` for **round-trip preservation** (comments and unknown fields retained).

**Impact**: A user who adds comments to `permissions.toml` and then runs `nexus42 permission grant` will see their comments preserved. But if they subsequently run `nexus42 policy grant` (or `policy default`), those comments are **silently lost**. This undermines the explicit design goal stated in `permission.rs`:

```rust
//! Uses `toml_edit` to preserve unknown fields and comments.
```

This inconsistency exists because `policy.rs` predates this feature, but the new `permission` CLI makes it a user-visible problem. Users mixing the two commands will experience unpredictable comment loss.

**Recommended fix**: Migrate `policy.rs` `save` path to also use `toml_edit`-based round-trip editing (load as `DocumentMut`, modify in-place, save). Alternatively, consolidate both commands to share a single persistence layer.

**This reviewer's独有 finding.**

---

### W2 — Repeated `list_agent_rules` calls with same arguments (performance + correctness smell)

**Severity**: Warning
**Category**: Code quality
**Files**: `crates/nexus42/src/commands/permission.rs` lines 266–276, 303–307

**Issue**: `run_revoke` calls `policy.list_agent_rules(agent)` three separate times (lines 267, 271, 274) when a single call with tuple destructuring would suffice:

```rust
// Current (3 calls):
let (_, _, asked) = policy.list_agent_rules(agent);
let has_rule = policy
    .list_agent_rules(agent)  // 2nd call
    .0
    .iter()
    .chain(policy.list_agent_rules(agent).1.iter())  // 3rd call
    .chain(asked.iter())
    .any(|c| c == capability);

// Should be (1 call):
let (granted, denied, asked) = policy.list_agent_rules(agent);
let has_rule = granted.iter().chain(denied.iter()).chain(asked.iter()).any(|c| c == capability);
```

Same pattern in `run_reset` (lines 304–306).

**Impact**: Minor performance cost (3 file reads + TOML parses per call), but more importantly it's a correctness risk — if the file changes between calls (unlikely for CLI, but possible in tests), the checks would be inconsistent.

**Recommended fix**: Cache the result in a local variable.

**This reviewer's独有 finding.**

---

### I1 — Misleading `#[allow(dead_code)]` on functions used in tests

**Severity**: Info
**Category**: Code quality
**Files**: `crates/nexus-acp-host/src/policy.rs` lines 103, 136

**Issue**: `AgentRules::new()` and `PermissionPolicy::new()` carry `#[allow(dead_code)]` but are actually called in test code (`test_agent_rules_is_empty` and many unit tests). In Rust, `#[allow(dead_code)]` is unnecessary for `pub` functions and misleading when tests use them.

**Recommended fix**: Remove both `#[allow(dead_code)]` annotations.

**This reviewer's独有 finding.**

---

### I2 — `print_list_json` omits global rules and default policy display

**Severity**: Info
**Category**: UX consistency
**Files**: `crates/nexus42/src/commands/permission.rs` lines 186–218

**Issue**: `print_list_text` displays global rules and the default policy, but `print_list_json` only outputs `default` and `agents`. If the file has global `[grant]`/`[deny]` entries, they are absent from JSON output. The `--output json` consumer gets incomplete data compared to `--output text`.

**Recommended fix**: Include `global_grant` and `global_deny` arrays in the JSON output, mirroring the text output's information.

**Potential cross-reviewer finding** (other reviewers may also flag this UX gap).

---

### I3 — `.map()` used for side effects (non-idiomatic)

**Severity**: Info
**Category**: Code quality
**Files**: `crates/nexus42/src/commands/permission.rs` lines 430–432, 460

**Issue**: `remove_agent_capability` and `clean_empty_agent_tables` use `Option::map` for side effects:

```rust
action_table
    .as_table_like_mut()
    .map(|t| t.remove(capability));  // return value discarded

agents.as_table_like_mut().map(|t| t.remove(agent));  // return value discarded
```

This is unconventional — `map` is intended for transformations, not mutations. The side effect works because the closure receives a `&mut`, but it reads as if the developer expected `map` to do something with the result.

**Recommended fix**: Replace with `if let Some(t) = ... { t.remove(capability); }`.

**This reviewer's独有 finding.**

---

### I4 — `run_revoke` reads policy file twice

**Severity**: Info
**Category**: Code quality
**Files**: `crates/nexus42/src/commands/permission.rs` lines 265–285

**Issue**: `run_revoke` loads the policy via `PermissionPolicy::load` (TOML deserialize) to check if the rule exists, then loads the same file again via `load_toml_edit` (TOML edit parse) to perform the removal. Two parses of the same file for one logical operation.

**Recommended fix**: Consider using `load_toml_edit` as the primary read and check the document directly, or merge the check + modify into a single flow.

**Potential cross-reviewer finding.**

---

### I5 — Test coverage gaps

**Severity**: Info
**Category**: Test quality
**Files**: `crates/nexus42/src/commands/permission.rs` test module

**Issues**:
- `test_grant_then_list_shows_rule` (line 496): calls `run_list` but has no assertion on output — comment says "verified by I/O"
- `test_list_json_output` (line 620): same pattern, no output assertion
- `test_list_filtered_by_agent` (line 628): asserts on the `PermissionPolicy` data model, not on the actual CLI output filtering behavior
- No test for `--output` with an invalid format string (e.g., `--output xml`)

**Recommended fix**: Add output assertions to list tests. At minimum, capture stdout and verify expected content.

**Potential cross-reviewer finding.**

---

## No-Regression Verification

| Area | Check | Result |
|------|-------|--------|
| Existing `policy` CLI | `PolicyCommand` enum unchanged, behavior unmodified | ✅ No regression |
| ACP client | `client.rs:230` calls `PermissionPolicy::load().unwrap_or_default()` — unchanged | ✅ No regression |
| `PermissionPolicy::evaluate()` | Unchanged signature and behavior | ✅ No regression |
| `PermissionPolicy::load` / `save` | Unchanged, backwards-compatible with new `agents` field via `#[serde(default)]` | ✅ No regression |
| Generated types | No schema changes, no codegen impact | ✅ No regression |
| Existing tests | `policy.rs` pre-V1.6 tests (lines 316–466) unaffected by new code | ✅ No regression |

---

## Summary

| Severity | Count | Finding IDs |
|----------|-------|-------------|
| Critical | 0 | — |
| Warning | 2 | W1, W2 |
| Info | 5 | I1–I5 |

**Verdict**: **Request Changes** — W1 and W2 require resolution before approval.

- **W1** is the most architecturally significant: the TOML preservation promise of the new CLI is undermined by the pre-existing `policy` command. Fix by migrating `policy.rs` save path to `toml_edit`.
- **W2** is a straightforward code quality fix (cache repeated calls).
- **I1–I5** are recommended improvements; addressing them improves long-term maintainability.

### Residual (out of scope)

- **R-agent-eval-integration**: Agent-scoped rules (`evaluate_for_agent`) exist in `policy.rs` but are **not yet integrated** into the ACP client's runtime evaluation (`client.rs` still calls `policy.evaluate()` without agent context). This is a follow-up task — the CLI surface is correct per plan scope, but the engine needs a second pass to actually enforce per-agent rules at runtime.

### This reviewer's独有 findings

W1, W2, I1, I3 — architecture-level and code-quality observations focused on module boundaries, data flow consistency, and long-term maintainability.

### Potential cross-reviewer overlap

I2 (JSON output completeness), I4 (double-read), I5 (test assertions) — other QC reviewers may independently surface these correctness and test-quality observations.
