---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.56-df31-df42-full-redesign"
verdict: "Approve with comments"
generated_at: "2026-06-21"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (Local API input validation, path traversal, content-hash OCC correctness, session model unguessability/expiry/races, changes[] manifest validation, migration safety, spec contract fidelity, typed error handling without information disclosure)
- Report Timestamp: 2026-06-21

## Scope
- plan_id: 2026-06-22-v1.56-df31-df42-full-redesign
- Review range / Diff basis: 7552e97a..a264c383 (pre-P0 base to P0 merge commit; only 325220fc feature + merge)
- Working branch (verified): iteration/v1.56
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 10 (core: session.rs, workspace_session.rs, workspace.rs handler, migration, 4 spec amendments + route registration, home-layout validator)
- Commit range: 7552e97a..a264c383 (exactly; post-a264c383 commits including pre-QC cache regen and retro docs excluded per assignment)
- Tools run: git diff --stat/range, git show @merge:paths, read of plan/compass, cargo check -p nexus-local-db (lib), cargo check -p nexus-daemon-runtime (lib), manual static analysis of OCC paths, path validator, error mapping, and DB consume logic

## Findings

### 🔴 Critical
None.

### 🟡 Warning

- **W-001 (security / path boundary incompleteness — inherited, not regressed)**: `nexus_home_layout::validate_workspace_path_safe` (used by `open_workspace`) remains a purely syntactic guard. It rejects `..` (substring), absolute paths, control chars, and empty strings. It performs **no** `canonicalize()`, **no** symlink resolution, and **no** prefix check that the resolved target remains under `workspace_root`.
  - In `open_workspace`:
    ```rust
    let target_path = std::path::PathBuf::from(&workspace_root).join(&req.path);
    ...
    let session_id = session_mgr.open_session(&workspace_root, &req.path, existed).await?;
    ```
  - Inside `open_session` / `compute_content_hashes`, the directory is walked and SHA-256 is computed over `is_file()` entries. On Unix, `is_file()` on a symlink target follows the link; a symlink inside the workspace pointing outside will cause hashing (and later potential commit effects) of external content.
  - Same limitation was flagged in V1.55 P1 qc2 (W-002). P0 delivers "production-grade" DF-31 full OCC + DB sessions but does not strengthen the filesystem boundary.
  - Risk: local symlink escape for content disclosure or (via crafted changes[] on commit) indirect effects on paths outside the declared workspace scope.
  - Evidence: `crates/nexus-home-layout/src/lib.rs:378-399` (the four checks), handler at `a264c383:crates/nexus-daemon-runtime/src/api/handlers/workspace.rs:139`, hash walk at `325220fc:.../session.rs:compute_content_hashes`.
  - Fix suggestion (for implementer or future): after `join`, call `canonicalize()` (best-effort) and verify `resolved.starts_with(workspace_root)`. Document symlink policy in `local-runtime-boundary.md` or `concurrency.md` §9 (e.g., "symlinks are followed; workspace owners must not place escaping symlinks").

- **W-002 (correctness / insufficient coverage of core OCC primitive under contention)**: The new DB-backed OCC path (atomic `consume_session` via `UPDATE ... WHERE consumed = 0 AND expires_at > ...`, `validate_changes_manifest`, two-phase open-then-consume in handler) has no integration test in the P0 delta that exercises concurrent `open` + `commit` (or two concurrent commits) against the real `workspace_sessions` table.
  - Unit tests exist for `compute_content_hashes`, `SessionId` uniqueness, error display, and `ChangeEntry` deserialization.
  - Handler tests are single-threaded (stale, missing, empty, basic lifecycle).
  - The DAO `consume_session` correctly uses a read + conditional UPDATE + re-read on zero rows affected, but the "single-consumer semantics" claim for production OCC has no parallel load evidence in this change set.
  - The V1.55 skeleton had an explicit concurrent test (1 success / N conflicts). It was not carried forward or re-expressed for the DB implementation in P0.
  - Evidence: `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs:363-582` (all `#[tokio::test]` are sequential), `crates/nexus-local-db/src/workspace_session.rs:140-180` (consume logic), session.rs tests at end of `325220fc`.
  - Impact: lowers confidence that the OCC + session expiry + consumed flag invariants hold under real concurrent load from CLI + agents.
  - Fix suggestion: add at least one `#[tokio::test]` (or integration test) that spawns N tasks, opens a shared session scope, and asserts exactly one `commit` succeeds while others receive `AlreadyCommitted` / `HashConflict` / `Expired` as appropriate. Use `handler_state()` pattern or fresh DB per the crate's existing test utilities.

### 🟢 Suggestion

- **S-001 (correctness / Create semantics underspecified)**: In `validate_changes_manifest`, `ChangeOp::Create` only checks that the path is absent from the *open-time snapshot*. It does not inspect current on-disk state. If a file appears on disk between `open` and `commit` (by another actor), a `Create` entry will be accepted. This may be intentional ("last writer creates"), but it is not documented in `concurrency.md` §9 or the handler error paths.
  - Related: `Modify` and `Delete` do re-check disk at commit time for some conditions; `Create` does not.
  - Suggestion: add a short paragraph in `concurrency.md` §9.2 or §9.5 clarifying Create-vs-existing-file behavior, or add an explicit disk-existence check + typed outcome if a stricter "must not exist at commit time" rule is desired.

- **S-002 (maintainability / session_id DB constraint is prefix-only)**: Migration enforces only `CHECK (session_id LIKE 'ws_%')`. The generation path (`uuid::Uuid::new_v4()` → `ws_<id>`) is strong, but a future bug that emits a colliding or malformed ID would only be caught at the application layer. Consider a comment in the migration or a length CHECK (`length(session_id) = 39` or similar) as defense-in-depth. Not a current vulnerability.

- **S-003 (scope vs. claim)**: The plan title, compass Q5, and AC-5 speak of a "full Local API redesign" for `/v1/local/{world,work,kb,schedule,workspace,findings}` with "coherent resource naming". The P0 delta matures the *workspace* surface (`/v1/local/workspace/open`, `/commit`, `changes[]`, OCC, DB sessions, typed 409/404 errors) and updates the four normative specs. However, the router registration (`api/mod.rs:182-189`) and other resource routes remain under the existing flat `/v1/local/*` structure; no new world/work/kb-scoped prefixes appear in the changed files for this review range.
  - If the redesign intent was primarily the session/OCC model + error unification for the workspace capability (rather than URL restructuring), the implementation matches. If broader prefix scoping was expected in P0, that portion is not visible in the diff.
  - No action required for this wave; clarify in P-last or next plan whether URL-level scoping is in or out.

- **S-004 (spec fidelity — good)**: `concurrency.md` §9 correctly documents SHA-256, the open→validate→atomic-consume flow, and the client retry obligation on `HASH_CONFLICT`. `local-db-schema.md` accurately transcribes the migration. Implementation and spec are in sync on the core contract.

## Source Trace
- Finding ID: W-001
- Source Type: manual-reasoning + git-diff
- Source Reference: `git diff 7552e97a..a264c383 -- crates/nexus-home-layout/src/lib.rs` (no change to validator); handler path join at `a264c383:.../workspace.rs:148`; compute walk at `325220fc:.../session.rs:58-100`; V1.55 qc2 precedent
- Confidence: High

- Finding ID: W-002
- Source Type: manual-reasoning + test inspection
- Source Reference: `git show a264c383:crates/nexus-daemon-runtime/src/api/handlers/workspace.rs:363-582` (test module); absence of `tokio::spawn` + concurrent commit assertions; DAO consume at `325220fc:.../workspace_session.rs:140`
- Confidence: High

- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: `validate_changes_manifest` arms for `Create` vs `Modify` at `325220fc:.../session.rs:190-240`
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve with comments

## Additional Notes (qc-specialist-2 focus)
- No SQL injection surface: migration is pure DDL; all queries use `sqlx::query!` compile-time checked macros.
- No panic paths from untrusted input: all error paths in `open_workspace`/`commit_workspace` return `NexusApiError` variants (InvalidInput, NotFound, Conflict, Uninitialized, Internal). `map_session_error` is exhaustive.
- Session ID generation: `ws_` + uuid v4 (effectively 128-bit random, unguessable for local threat model). Good.
- Atomic single-consumer: `UPDATE ... WHERE consumed = 0 AND expires_at > ...` + re-read on zero rows gives correct AlreadyConsumed/Expired. Correct.
- Content hash algorithm: SHA-256 over raw file bytes, documented, collision resistance appropriate for local workspace OCC. Matches spec.
- changes[] validation: manifest is required to match snapshot for Modify; Create/Delete are snapshot-only checks. Consistent with the OCC model described.
- Pre-QC cache residual (R-V156P0-CACHE-01) and retro docs are after `a264c383` and were not re-reviewed.
- No out-of-scope creep into DF-29, DF-56, cloud, or git-backed sessions was observed in the review range.

The two Warnings are medium-impact and do not block the core OCC + session contract. They are suitable for PM residual registration or targeted follow-up. The implementation is safe to merge mid-QA under "Approve with comments".
