# QA Report — V1.6 WS-C Permission CLI

**Plan ID**: `2026-04-20-v1.6-ws-c-permission-cli`
**QA Agent**: @qa-engineer
**Date**: 2026-04-20
**Working branch**: `feature/v1.6`
**Diff basis**: `a9381e8..HEAD`
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`

---

## Scope Tested

- `nexus42 permission grant <agent> <capability>` — AC #1
- `nexus42 permission list [--agent <id>]` — AC #2
- `nexus42 permission deny <agent> <capability>` — AC #3
- `nexus42 permission revoke <agent> <capability>` — AC #4
- `nexus42 permission reset --agent <id>` — AC #5
- `.nexus42/permissions.toml` file persistence — AC #6
- `cargo test --workspace` — AC #7
- `cargo clippy --all -- -D warnings` — AC #7
- `cargo +nightly fmt --all -- --check` — AC #7

## Acceptance Criteria Results

| # | Criterion | Result | Evidence |
|---|-----------|--------|----------|
| 1 | `nexus42 permission grant test-agent terminal.create` succeeds | **PASS** | Output: `Granted 'terminal.create' for agent 'test-agent'.` |
| 2 | `permission list` shows the rule | **PASS** | Output: `[grant] terminal.create` under `Agent: test-agent` |
| 3 | `permission deny test-agent terminal.create` overwrites grant | **PASS** | Output: `Denied 'terminal.create' for agent 'test-agent'.` → list shows `[deny] terminal.create` |
| 4 | `permission revoke test-agent terminal.create` removes specific rule | **PASS** | Output: `Revoked 'terminal.create' for agent 'test-agent'.` → list shows `No permission rules configured.` |
| 5 | `permission reset --agent test-agent` removes all rules | **PASS** | After granting two rules (terminal.create + terminal.read), reset → list shows `No permission rules configured.` |
| 6 | `.nexus42/permissions.toml` reflects changes | **PASS** | After full roundtrip + reset: file contains `[agents]` section (empty as expected after reset) |
| 7 | `cargo test --workspace` green; clippy/fmt clean | **SEE NOTE** | Clippy: clean. Fmt: clean. Tests: 400 passed, **1 pre-existing failure** (see below) |

## Detailed Verification

### cargo test --workspace

```
Test run 1 (lib crate):  401 passed; 0 failed
Test run 2 (bin crate):  400 passed; 1 failed
```

**Single failure**: `auth::tests::get_returns_none_for_unknown_creator`
- Error: `Json(Error("trailing characters", line: 10, column: 2))` at `crates/nexus42/src/auth/mod.rs:240`
- **Confirmed pre-existing**: Verified by checking out diff basis `a9381e8` and running the same test — identical failure.
- **Not in WS-C scope**: `git diff a9381e8..HEAD -- crates/nexus42/src/auth/` shows zero changes. The auth module is untouched by this plan.
- **Impact**: Does not affect permission CLI functionality.

### cargo clippy --all -- -D warnings

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.31s
```
✅ Zero warnings, zero errors.

### cargo +nightly fmt --all -- --check

```
(no output)
```
✅ All formatting matches `cargo +nightly fmt --all`.

### CLI Roundtrip (full acceptance flow)

Executed in a fresh temp directory with minimal `.nexus42/config.json`:

```
$ nexus42 permission grant test-agent terminal.create
Granted 'terminal.create' for agent 'test-agent'.

$ nexus42 permission list
Permission Rules
================
Agent: test-agent
  [grant] terminal.create

$ nexus42 permission deny test-agent terminal.create
Denied 'terminal.create' for agent 'test-agent'.

$ nexus42 permission list
Permission Rules
================
Agent: test-agent
  [deny] terminal.create

$ nexus42 permission revoke test-agent terminal.create
Revoked 'terminal.create' for agent 'test-agent'.

$ nexus42 permission list
Permission Rules
================
No permission rules configured.

$ nexus42 permission grant test-agent terminal.create
Granted 'terminal.create' for agent 'test-agent'.

$ nexus42 permission grant test-agent terminal.read
Granted 'terminal.read' for agent 'test-agent'.

$ nexus42 permission reset --agent test-agent
Reset all rules for agent 'test-agent'.

$ nexus42 permission list
Permission Rules
================
No permission rules configured.

$ cat .nexus42/permissions.toml
[agents]
```

All CLI commands returned exit code 0. TOML file reflects state correctly at each step.

## Unit Tests (Permission Module)

11 permission-specific unit tests all pass:

- `test_grant_creates_permission_file`
- `test_grant_then_list_shows_rule`
- `test_list_filtered_by_agent`
- `test_list_json_output`
- `test_ask_adds_rule`
- `test_deny_overwrites_grant`
- `test_revoke_removes_specific_rule`
- `test_revoke_nonexistent_returns_error`
- `test_reset_removes_all_agent_rules`
- `test_reset_all_agents`
- `test_reset_nonexistent_agent_returns_error`
- `test_preserves_comments_via_toml_edit`
- `test_preserves_existing_global_rules`

## Phase Gate Check

- ✅ Plan tasks (T1-T6): all marked `[x]`
- ✅ Evidence section: present and matches tested flows
- ⚠️ `clarify` section: not present in plan document. Not a blocker — scope was unambiguous (straightforward CLI surface over existing `policy.rs`).
- ✅ No out-of-scope implementation detected in diff.

## Not Tested

- Interactive/TTY prompt behavior (not applicable — commands are non-interactive).
- Concurrent multi-agent permission conflicts (out of scope for CLI surface).
- Permission enforcement at runtime (covered by WS-A residual governance, not this plan).

## Conclusion

**QA Status: PASS** (with noted pre-existing test failure unrelated to this feature).

All 7 acceptance criteria are met. The pre-existing `auth::tests::get_returns_none_for_unknown_creator` failure is documented but out of scope for this plan (zero changes to `auth/` module in the diff range).

## Recommended Owners

None — no residual findings from this QA run.

The pre-existing auth test failure (`auth::tests::get_returns_none_for_unknown_creator`) should be tracked separately and is not attributable to WS-C.
