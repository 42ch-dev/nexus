---
report_kind: qa-report
plan_id: "2026-06-22-v1.56-df31-df42-full-redesign"
qa_agent: qa-engineer
mode: report-only
generated_at: "2026-06-22"
working_branch: "iteration/v1.56"
reviewed_head: "f4920e86"
review_range: "7552e97a..a264c383"
---

# QA Report — V1.56 P0 (DF-31 Full + DF-42 Local API Redesign)

## Scope
- **Plan**: 2026-06-22-v1.56-df31-df42-full-redesign (P0)
- **Working branch**: iteration/v1.56 (HEAD f4920e86)
- **P0 implementation range**: 7552e97a..a264c383 (feature `325220fc` + merge `a264c383`)
- **Review context**: Post tri-review (qc-consolidated: Approve with comments). 6 medium residuals registered. P1 fix-wave work has also landed on the same branch.
- **Reference**: `.mstar/plans/2026-06-22-v1.56-df31-df42-full-redesign.md` §Acceptance Criteria (8 items); compass §9 success criteria; qc-consolidated.md.

## 7-Key Acceptance Gate Verification

### Gate 1 — All 8 AC items demonstrably met

| # | Acceptance Criterion | Status | Evidence |
|---|----------------------|--------|----------|
| 1 | `workspace.open` returns session with content hashes for all tracked files in workspace scope | **Pass** | `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs:184-192`: `open_workspace` calls `session_mgr.open_session(...)` then reads back `file_hashes_json` into `OpenSnapshot.file_hashes: HashMap<String,String>`. Handler returns `WorkspaceOpenResponse { session_id, snapshot: { workspace_root, path, existed, file_hashes } }`. `session.rs:227-231`: `compute_content_hashes` on `target_path` when `existed && is_dir`. |
| 2 | `workspace.commit` validates `changes[]` manifest against session snapshot; rejects on hash mismatch (OCC conflict) | **Pass** | `workspace.rs:270-279`: calls `session_mgr.validate_changes_manifest(&session_id, &req.changes, &workspace_root)`. `session.rs:407+`: on mismatch emits `SessionError::HashConflict { path, expected_hash, actual_hash }`. Handler maps to `NexusApiError::Conflict` with `HASH_CONFLICT` (HTTP 409). Typed `ChangeEntry { path, content_hash, op: create/modify/delete }`. |
| 3 | Sessions persisted in SQLite (DB-backed), survive daemon restart, expire per TTL | **Pass** | Migration `crates/nexus-local-db/migrations/202606220002_workspace_sessions.sql`: table `workspace_sessions (session_id PK, workspace_root, relative_path, existed, file_hashes_json, created_at, expires_at, consumed)`. Indexes on `expires_at` and `(consumed, expires_at)`. `nexus-local-db/src/workspace_session.rs`: `create_session`, `get_session`, `consume_session`, `cleanup_expired_sessions`. `WorkspaceSessionManager` (session.rs:194) is DB-backed via `SqlitePool`; replaces V1.55 in-memory. TTL default 300s. `consumed` column provides single-consumer semantics. |
| 4 | `changes[]` payload includes manifest path, content hash, operation type; invalid manifests rejected with typed errors | **Pass** | `ChangeEntry` struct (session.rs) with `path: String`, `content_hash: String`, `op: ChangeOp`. `WorkspaceCommitRequest.changes: Vec<ChangeEntry>`. Validation rejects with `SessionError::HashConflict` (typed), `NotFound`, `AlreadyCommitted`, `Expired`. Handler surfaces as 400/404/409 with explicit codes (`SESSION_NOT_FOUND`, `STALE_SESSION`, `SESSION_EXPIRED`, `HASH_CONFLICT`). |
| 5 | Local API `/v1/local/*` scope redesigned + documented (coherent naming, unified error model, no ad-hoc endpoints) | **Pass** | Routes under `/v1/local/workspace/open` + `/commit` (handlers/workspace.rs). No ad-hoc singletons added. Unified error model via `NexusApiError` (InvalidInput 400, NotFound 404, Conflict 409, Internal 500). Spec `local-runtime-boundary.md:97` explicitly documents the new endpoints and OCC behaviour. |
| 6 | V1.55 P1 skeleton fully replaced (no dual in-memory/DB session path) | **Pass** | Commit `325220fc` message: "Replace V1.55 in-memory `WorkspaceSessionManager` with DB-backed async version using sqlx". `session.rs:181`: "Replaces the V1.55 in-memory `WorkspaceSessionManager`". `workspace/mod.rs:88,145`: constructs `WorkspaceSessionManager::new(Arc::new(db.pool().clone()))` (DB-backed). No remaining in-memory fallback path for sessions. |
| 7 | All amended specs reflect new OCC + session + API scope behaviour | **Pass** | Four specs amended in P0 range (`git diff --stat 7552e97a..a264c383 -- '*.md'`):<br/>- `.mstar/knowledge/specs/local-runtime-boundary.md:97` — documents `workspace.open`/`commit` with OCC, `workspace_sessions` table, 409 `HASH_CONFLICT`.<br/>- `.mstar/knowledge/specs/daemon-runtime.md:49` — "workspace session persistence (`workspace_sessions` DB table, V1.56 P0)".<br/>- `.mstar/knowledge/specs/local-db-schema.md:304-339` — full `workspace_sessions` table DDL + column table.<br/>- `.mstar/knowledge/specs/concurrency.md` — §9 workspace session OCC (content hash snapshot on open, manifest validation on commit).<br/>Implementation (handlers + session manager + migration) matches spec text exactly. |
| 8 | P0 topic branch merged to `iteration/v1.56` before tri-review | **Pass** | Merge commit `a264c383` ("merge(v1.56): P0 DF-31 full + DF-42 Local API redesign from topic branch") is on `iteration/v1.56`. Current HEAD `f4920e86` is descendant. Tri-review occurred after this merge (per qc-consolidated and status.json `qc_completed_at`). |

**Gate 1 overall**: **Pass** — all 8 AC items are demonstrably met by code + spec + Git history.

### Gate 2 — cargo test passes for all touched crates

**Commands executed**:
- `cargo test -p nexus-local-db --lib`
- `cargo test -p nexus-daemon-runtime --lib`
- `cargo test --workspace --lib` (scoped where possible)

**Result**: **Blocked / Cannot verify in current environment**

**Evidence**:
- Both `cargo test -p nexus-local-db --lib` and `cargo test -p nexus-daemon-runtime --lib` fail at compile time with:
  ```
  error: set `DATABASE_URL` to use query macros online, or run `cargo sqlx prepare` to update the query cache
  ```
  Examples: `nexus-local-db/src/kb_store.rs:894`, `reference_source.rs:583`, `nexus-daemon-runtime/src/db/pool.rs:252`.
- `SQLX_OFFLINE=true` also fails: "no cached data for this query".
- The failing queries are **not** in the P0 diff (they pre-exist in `kb_store`, `reference_source`, `pool`).
- P0 added only the `workspace_sessions` table + its queries (which were prepared).
- This is exactly the pre-existing condition registered as **R-V156P0-CACHE-01** (medium, resolved at `8809f0b5` by PM pre-QC fix-wave regenerating consumer caches for `nexus42` + workspace).
- qc-consolidated states: "All 263 daemon-runtime + 26 local-db tests pass" (implementer self-report at feature time).
- No evidence that P0 logic itself introduced test breakage. The harness cannot re-execute the test binaries without a full `cargo sqlx prepare --workspace --all -- --all-targets` against a freshly migrated DB.

**Assessment**: Gate is **not green** in the strict command-output sense on this checkout. The root cause is a known, already-registered residual (R-V156P0-CACHE-01) that was marked resolved before tri-review. P0 code changes are isolated to workspace session paths that do not affect the stale queries.

**Recommendation to PM**: Re-run the exact test commands after a workspace-wide `cargo sqlx prepare` (or accept the prior green run + residual closure as sufficient for this gate). Do not treat this as a P0 correctness failure.

### Gate 3 — cargo clippy clean (CI gate)

**Command**: `cargo clippy --workspace -- -D warnings`

**Result**: **Pass**

**Evidence**:
- Ran to completion: "Finished `dev` profile [unoptimized + debuginfo] target(s) in 22.21s".
- No `-D warnings` violations emitted for any crate.
- Clippy completed without error on the full workspace (including touched crates `nexus-local-db` and `nexus-daemon-runtime`).

### Gate 4 — cargo +nightly fmt clean (CI gate)

**Command**: `cargo +nightly fmt --all -- --check`

**Result**: **Fail**

**Evidence**:
- Diffs reported in `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs`:
  - Line wrapping / formatting on `map_session_error` signature and match arms.
  - `let session_id = r.session_id.unwrap_or_else(...)` reformatted across lines.
  - Several `NexusApiError::Conflict(format!(...))` calls reformatted.
- Current branch HEAD `f4920e86` is **not** fmt-clean.
- Note: P0 feature commit `325220fc` and merge `a264c383` pre-date the current HEAD. Subsequent work on `iteration/v1.56` (P1 fix-wave merges `27bc1b09`, qc revalidation, harness commits) has introduced fmt drift.

**Assessment**: CI gate **red** on the branch as presented for mid-QA. This is a process hygiene failure on the integration branch, not necessarily a P0 semantic defect.

### Gate 5 — No scope creep beyond §Scope In

**Result**: **Pass**

**Evidence**:
- `git log --oneline 7552e97a..a264c383 -- crates/ | grep -E 'df29|df56|registry|conditional'` → **no matches**.
- Only migration added in P0 range: `202606220002_workspace_sessions.sql`.
- `202606220001_work_profile_game_bible.sql` exists in the migrations directory but is **not** part of the P0 diff range (pre-dates or belongs to parallel work).
- `git diff --stat 7552e97a..a264c383` shows only the workspace_sessions migration + source changes under `nexus-local-db`, `nexus-daemon-runtime`, `nexus-contracts`, and the 4 spec files.
- No DF-29 (`registry.refresh`), DF-56 (conditional routing), or R-V155P2-F002 scope touched.

### Gate 6 — Residuals registered

**Result**: **Pass**

**Evidence** (from `.mstar/status.json` under `residual_findings["2026-06-22-v1.56-df31-df42-full-redesign"]`):

1. `R-V156P0-CACHE-01` (medium, **resolved**):
   - title, severity, source ("PM pre-QC verification"), scope (nexus42 consumer query! macros), decision, owner, target, tracking_link, note (full root-cause + architectural lesson), lifecycle: "resolved", closed_at, closure_note, closure_evidence (`8809f0b5`), resolution object all present.

2–7. `R-V156P0-M001` through `M006` (all medium, **deferred**):
   - Each has: id, title (exact match to qc-consolidated M-001..M-006), severity: "medium", source (qc1/qc2/qc3 W-xxx), scope (workspace/session files), decision: "accept (defer to post-V1.56 hardening)", owner, target: "V1.57+", tracking_link, note, lifecycle: "deferred".
   - All 6 are present with complete fields.

No missing entries. No malformed records. Matches qc-consolidated §Combined Findings exactly.

### Gate 7 — Docs updated

**Result**: **Pass**

**Evidence**:
- Exactly 4 spec files amended in P0 range (confirmed by `git diff --stat`).
- **local-runtime-boundary.md**: §3.2.1 now documents `/v1/local/workspace/open` + `/commit` with full OCC description, `workspace_sessions` table, 409 `HASH_CONFLICT`, TTL, DB-backed semantics. Matches implementation.
- **daemon-runtime.md**: Subsystem table now lists "workspace session persistence (`workspace_sessions` DB table, V1.56 P0)".
- **local-db-schema.md**: §4.2.1 full table DDL + column breakdown for `workspace_sessions` (V1.56 P0). Matches the actual migration.
- **concurrency.md**: Header updated "V1.56 P0 amendment (§9 workspace session OCC)"; new section describes content-hash snapshot on open + manifest validation on commit (TOCTOU relaxation noted as known per spec).
- Cross-check: handler response shapes, error codes, session lifecycle (create → validate → consume → cleanup), and file_hashes_json usage are consistent between code and the four amended specs. No contradictions found.

## Findings Summary

| Gate | Verdict | Notes |
|------|---------|-------|
| 1. All 8 AC items | **Pass** | Fully evidenced by code, contracts, migration, and spec text. |
| 2. cargo test | **Blocked / Not re-runnable** | Test binaries fail to build due to stale `.sqlx/` cache (R-V156P0-CACHE-01, already registered + resolved pre-QC). No P0-introduced query! macros are missing. Prior green run (263+26 tests) reported by implementer. |
| 3. cargo clippy | **Pass** | `cargo clippy --workspace -- -D warnings` completed cleanly. |
| 4. cargo +nightly fmt | **Fail** | Diffs in `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs` on current HEAD `f4920e86`. Post-P0 work on the branch introduced drift. |
| 5. Scope | **Pass** | No DF-29/DF-56/R-V155P2-F002 leakage. Only 1 new migration (`workspace_sessions`). |
| 6. Residuals | **Pass** | `R-V156P0-CACHE-01` (resolved) + `M001..M006` (deferred) all present with full required fields in `status.json`. |
| 7. Docs | **Pass** | 4 specs amended; content internally consistent with implementation. |

## Overall Verdict

**Fail**

**Rationale**:
- Gate 4 (fmt) is red on the branch HEAD presented for mid-QA.
- Gate 2 (tests) cannot be executed in the current environment; while the cause is a known residual rather than P0 logic, the gate as written ("cargo test passes") is not green.

All functional ACs (Gate 1), scope discipline (Gate 5), residual registration (Gate 6), doc updates (Gate 7), and clippy (Gate 3) are green. The failures are process/CI hygiene and test-harness availability on the integration branch after subsequent P1 work.

## Issues / Risks

1. **Fmt drift on integration branch** (Gate 4): `cargo +nightly fmt --all -- --check` fails on `f4920e86`. Recommend a hygiene pass (or pre-commit enforcement) before any further waves claim "CI gate clean".
2. **Test re-execution blocked by sqlx cache** (Gate 2): Any future mid-QA or P-last verification on this branch will hit the same wall unless `cargo sqlx prepare --workspace --all` is run and committed after migrations. R-V156P0-CACHE-01 closure note already captured the architectural lesson.
3. **Branch has post-P0 changes**: P1 fix-wave + revalidation commits are present. Mid-QA for P0 is effectively validating a mixed state. Future iterations should consider per-plan integration branches or explicit "P0-only" tags for QA if strict isolation is required.

## Not Tested / Out of Scope for this mid-QA

- Full workspace-wide `cargo test --all` (blocked by sqlx cache).
- Concurrent load / race tests for OCC (explicitly listed as M-003 residual; deferred).
- E2E via `nexus42` CLI (no daemon start + curl in this report-only pass).
- Metrics/tracing spans at conflict paths (M-006 residual).

## Recommended Next Steps (for PM)

1. Address Gate 4: run `cargo +nightly fmt --all` and commit the style fixes (or confirm they belong to a later plan and accept the hygiene debt).
2. For Gate 2: either (a) perform a workspace sqlx prepare + re-run tests and record green output, or (b) explicitly note in plan status that the gate relies on the pre-QC green run + residual closure.
3. If the above two are cleared, re-dispatch targeted QA or mark P0 `Done` (PM-only permission per harness).
4. Consider whether subsequent waves on `iteration/v1.56` should be rebased or use separate worktrees to keep fmt/CI state clean per plan.

## Git
- Working branch: `iteration/v1.56`
- Reviewed HEAD: `f4920e86`
- P0 merge commit: `a264c383`
- No commits made by this QA session (report-only)

---

**QA Agent**: qa-engineer (report-only mode)  
**Status**: Complete  
**Verdict**: Fail (fmt red + tests not re-runnable on presented branch HEAD)  
**Handoff**: PM to decide hygiene fix vs. residual acceptance before marking P0 `Done`.
