---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-18-v1.50-kb-editor-cli"
working_branch: "feature/v1.50-kb-editor-cli"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-editor-cli"
review_range: "c38fbe1f..3132f80e"
verdict: "Approve"
generated_at: "2026-06-17T06:20:00Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (Reviewer #2)
- Report Timestamp: 2026-06-17T06:20:00Z

## Scope
- plan_id: `2026-06-18-v1.50-kb-editor-cli`
- Review range / Diff basis: `c38fbe1f..3132f80e` (equivalent to merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..3132f80e740297ef6f79009f1c804fb68dcb95ea)
- Working branch (verified): `feature/v1.50-kb-editor-cli`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-editor-cli` (from `git rev-parse --show-toplevel`)
- Files reviewed: 5 (new `kb.rs` 393 lines, `world/mod.rs` rename, 2 new test files, plan doc)
- Commit range: `c38fbe1f..3132f80e` — 4 commits:
  - `ec201b40` feat(nexus42): creator world kb list/show author surface (T1, T2)
  - `de0c9d29` feat(nexus42): creator world kb edit/delete with author gate (T3, T4, T5)
  - `429b7101` test(nexus42): hermetic world_kb_cli + world_kb_authz (T6)
  - `3132f80e` docs(plan): mark V1.50 kb-editor-cli T1-T7 done + Completion Report v2
- Tools run: `git log`/`git diff`/`git show`, `cargo build -p nexus42 -p nexus-kb`, `cargo clippy -p nexus42 -p nexus-kb -- -D warnings`, `cargo test -p nexus42 --test world_kb_cli`, `cargo test -p nexus42 --test world_kb_authz`

## Reviewer Perspective Notes (security + correctness)

### Author identity gate — `narrative_worlds.owner_creator_id` (verified)
`KeyBlock`s are World-scoped per entity-scope-model §1.2/§5.1. There is no `works.creator_id` FK on `kb_key_blocks`. The implementation correctly resolves ownership via `SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?` in `require_world_owner` (kb.rs:327-352).

- Cross-author `edit`/`delete` return `CliError::Api { status: 403, message: "WORLD_KB_FORBIDDEN: ..." }`.
- The stable code `WORLD_KB_FORBIDDEN` is defined as `pub const WORLD_KB_FORBIDDEN_CODE` (kb.rs:34) and embedded in the message.
- Tests (`world_kb_authz.rs`): `cross_author_edit_returns_403`, `cross_author_delete_returns_403` assert 403 + code presence + no mutation on intruder path. `owner_can_edit_and_delete` is the positive control. `edit_on_missing_world_is_not_a_403` confirms missing world yields not-found, not 403.
- `kb_list` / `kb_show` are intentionally read-only (no owner gate), matching plan AC and V1.40 read surface precedent.

### Soft-delete vs hard-delete (verified)
`delete_key_block` (nexus-local-db/src/kb_store.rs:546-564) performs:
```sql
UPDATE kb_key_blocks SET status = 'deleted', updated_at = ? WHERE key_block_id = ?
```
No `DELETE FROM` path exists for KeyBlocks. The CLI `kb_delete` calls this after the owner gate and a pre-check. Hermetic test `delete_soft_deletes_block` (world_kb_cli.rs) asserts the block disappears from `list_by_world` post-delete but the row remains with `status='deleted'`.

Note: qc3 already surfaced S-2 (soft-deleted blocks can be re-edited to active via the edit path because `update_key_block` lacks a `status NOT IN ('deleted',...)` guard). This is pre-existing store behavior inherited by the new surface; not introduced here.

### Edit re-runs `ValidationMode::Novel` (V1.40 P1) (verified)
`kb_edit` (kb.rs:232) constructs:
```rust
let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
```
then calls `store.update_key_block(...)`. The test `edit_rejects_body_missing_novel_category` (world_kb_cli.rs) seeds a valid block, then attempts an edit body without `novel_category` and asserts `ValidationError` is surfaced. This re-uses the exact V1.40 P1 validation machinery.

### `canonical_name` validation on edit (N/A for this surface)
`kb_edit` only mutates `body` + `updated_at`. `canonical_name` is set at insert time and is not offered as an edit target in the CLI (`WorldKbCommand::Edit` takes `--body` only). The store `update_key_block` statement (kb_store.rs:520-541) includes `canonical_name` in the SET list, but since the in-memory `block` carries the original value, no change occurs and no re-validation of the name itself is exercised on the edit path. This matches the plan scope (name is immutable post-create for the author surface). If a future edit-name path is added, the store validation hook would apply.

### Stable error code `WORLD_KB_FORBIDDEN` — no conflict with `CliError` enum (verified)
- The code is a string constant (`WORLD_KB_FORBIDDEN`), carried inside `CliError::Api { status: 403, message }`.
- `CliError` (errors.rs) has no `WorldKbForbidden` variant; `Api` is the general carrier for HTTP-style errors (used by daemon client, auth, etc.).
- No enum conflict. The pattern (stable string code inside a generic error) is consistent with how other stable codes are surfaced in this codebase (e.g., via message text for CLI consumers).
- Tests explicitly assert the string appears in the 403 message.

### SQL prepared statements throughout (verified)
- `require_world_owner` (kb.rs:330): `sqlx::query_scalar("SELECT ... WHERE world_id = ?").bind(world_id)` — runtime query with parameter bind.
- Store layer (`nexus-local-db/src/kb_store.rs`): `sqlx::query!` compile-time checked macros for `insert`, `update`, `delete_key_block`, `list_by_world`, etc.
- No string concatenation into SQL in the new code paths.
- The runtime query in `require_world_owner` carries a `// SAFETY:` comment and follows the exact precedent in `world/mod.rs:212-219` (`run_event_add`). This is acceptable per project convention (see `nexus-local-db/AGENTS.md` waived residual on compile-time cache sharing).

### `--json` output leakage check (verified)
- `kb_list --json`: emits a curated array via `block_summary_json` — only `key_block_id`, `canonical_name`, `block_type` (wire serde form), `status`. No owner, no internal timestamps beyond what's public, no body.
- `kb_show --json` / `kb_edit --json`: `serde_json::to_value(&block)` on the full `KeyBlock`. This is the documented behavior ("full body + provenance + status"). The type is the shared `nexus_kb::KeyBlock` (no extra secrets injected by the CLI layer).
- No credential, token, or cross-creator data is present in `KeyBlock` for World KB rows. The world_id is echoed (expected, since the command is world-scoped).
- No leakage of the owner check result or other internal state.

### Hermetic tests + isolation (verified)
Both new test files (`world_kb_cli.rs`, `world_kb_authz.rs`) use `tempfile::tempdir()` + `Schema::init` per test. No shared state, no `$HOME`, no daemon. All 9 + 4 tests pass deterministically. Existing `integration` suite (47 tests) unaffected.

## Findings
### 🔴 Critical
- _(none)_

### 🟡 Warning
- _(none)_

### 🟢 Suggestion
- **S-V150KBED-QC2-01** (low) — `WORLD_KB_FORBIDDEN` is a string constant embedded in `CliError::Api` message rather than a distinct typed variant.  
  This is functionally correct and test-covered, but if the CLI ever grows a typed error catalog or machine-readable error code field (separate from the human message), the stable code should be promoted to a first-class carrier so consumers can match without string inspection.  
  → Track as hygiene; non-blocking for T-B P0. Pre-1.0 allows the current string-in-message pattern.

- **S-V150KBED-QC2-02** (nit) — `map_kb_store_error` still uses verb `"show"` for the load step inside `kb_edit` and `kb_delete` (kb.rs:237, 290).  
  Same observation as qc1 S-V150KBED-QC1-03. Cosmetic only (error still surfaces correctly as "not found" or "failed to ...").  
  → Pass `"load"` (or similar) for the pre-mutation `get_key_block` calls.

- **S-V150KBED-QC2-03** (low) — No explicit test that a soft-deleted block cannot be edited without an explicit "un-delete" step (qc3 S-2).  
  The current behavior is inherited from `SqliteKbStore::update_key_block` (no status guard). The new CLI surface does not add this guard.  
  → If a follow-up decides to close S-2, add a negative test `edit_rejects_deleted_block` (or equivalent) under the authz or cli test file. Non-blocking here.

## Source Trace
- Finding ID: S-V150KBED-QC2-01
  - Source Type: manual-reasoning / code pattern
  - Source Reference: `kb.rs:34` (const), `kb.rs:343-349` (CliError::Api construction), `errors.rs:46-49` (Api variant), `world_kb_authz.rs:69-71` (string assert)
  - Confidence: High

- Finding ID: S-V150KBED-QC2-02
  - Source Type: git-diff / manual-reasoning
  - Source Reference: `kb.rs:237` (edit load), `kb.rs:290` (delete pre-check), same pattern noted in qc1
  - Confidence: High

- Finding ID: S-V150KBED-QC2-03
  - Source Type: code-trace + cross-report
  - Source Reference: `kb_store.rs:520-541` (UPDATE without status filter), `kb.rs:212-265` (edit flow), qc3 S-2
  - Confidence: High

## Validation Evidence
```
# Build (scoped)
$ cargo build -p nexus42 -p nexus-kb
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s

# Clippy (scoped, -D warnings)
$ cargo clippy -p nexus42 -p nexus-kb -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.24s   # 0 warnings

# Security + correctness tests (AC2 + AC4 + AC5)
$ cargo test -p nexus42 --test world_kb_cli
test result: ok. 9 passed; 0 failed; 0 ignored

$ cargo test -p nexus42 --test world_kb_authz
test result: ok. 4 passed; 0 failed; 0 ignored
```

Author gate + stable code (excerpt from test):
```rust
// world_kb_authz.rs:66-78
match err {
    CliError::Api { status, message } => {
        assert_eq!(status, 403);
        assert!(message.contains(WORLD_KB_FORBIDDEN_CODE));
        ...
    }
}
```

Parameterized SQL (kb.rs:330):
```rust
sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
    .bind(world_id)
    .fetch_optional(pool)
    ...
```

Novel validation on edit (kb.rs:232 + test):
```rust
let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
...
// test: edit_rejects_body_missing_novel_category
```

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (2 low, 1 nit) |

**Verdict**: **Approve**

No Critical or Warning findings under the security + correctness lens. All mandatory verification commands are green. The author identity gate correctly resolves `narrative_worlds.owner_creator_id`, returns 403 with the documented stable code on cross-author attempts, and the hermetic tests prove no mutation occurs. Soft-delete is explicit (UPDATE status='deleted'), edit re-runs `ValidationMode::Novel` via the V1.40 store path, SQL uses parameterized forms (runtime query with bind + compile-time macros in the store), and `--json` surfaces only the intended public fields. The three Suggestions are low/nit hygiene items (typed error code carrier, cosmetic verb in error mapping, and a follow-up test for the pre-existing soft-delete editability behavior already noted by qc3). Consistent with qc1 and qc3 verdicts.

Pre-existing residuals from the plan (R-V150KBED-01, R-V150KBED-02) remain appropriate and are non-overlapping with the new suggestions above.
