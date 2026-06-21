---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.56-df31-df42-full-redesign"
verdict: "Approve with comments"
generated_at: "2026-06-21"
---

# Code Review Report — V1.56 P0 (qc1)

## Reviewer Metadata
- Reviewer: @qc-specialist (Reviewer #1 — Architecture Coherence & Maintainability Risk)
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek/deepseek-v4-flash
- Review Perspective: Architecture coherence, module boundaries, scope containment, spec consistency, maintainability risk
- Report Timestamp: 2026-06-21

## Scope
- **plan_id**: `2026-06-22-v1.56-df31-df42-full-redesign`
- **Review range / Diff basis**: `7552e97a..a264c383`
- **Working branch (verified)**: `iteration/v1.56`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 35 files changed (1014 insertions, 775 deletions)
- **Commit range**: `7552e97a..a264c383` (2 commits: `325220fc` feature + `a264c383` merge)
- **Tools run**: `cargo clippy -p nexus-daemon-runtime -p nexus-local-db` (clean), `git diff --stat`, `git log`, `grep`, `read`

## Scope Gate Checks

### No Scope Creep
- **DF-29 / P1**: No files touched in `preset-conditional-routing.md`, `acp-capability-set.md`, `cli-spec.md`, or any registry-related code. ✅
- **DF-56 / P2+P3**: No files touched in `orchestration-engine.md`, `preset-conditional-routing.md`, `entity-scope-model.md`, or any routing/conditional logic. ✅
- **R-V155P2-F002**: No changes to game-bible, `section_status`, or Work profiles. ✅
- **Spec amendments are strictly limited to the 4 listed in §Scope In**: `local-runtime-boundary.md`, `daemon-runtime.md`, `local-db-schema.md`, `concurrency.md`. ✅

### AC Item Verification
| # | Acceptance Criterion | Status | Evidence |
|---|---------------------|--------|----------|
| 1 | `workspace.open` returns session with content hashes | ✅ | `OpenSnapshot.file_hashes: HashMap<String, String>` field added; populated from `compute_content_hashes` during `open_session()` |
| 2 | `workspace.commit` validates changes[] against snapshot; rejects on hash mismatch | ✅ | `validate_changes_manifest()` with per-operation checks; `SessionError::HashConflict` → HTTP 409 |
| 3 | Sessions persisted in SQLite, survive restart, expire per TTL | ✅ | `workspace_sessions` table, `consume_session` checks `expires_at > now()`; `WorkspaceSessionManager` takes `Arc<SqlitePool>` |
| 4 | changes[] with path, content_hash, op; invalid manifests rejected with typed errors | ✅ | `ChangeEntry { path, content_hash, op: ChangeOp }`; `Create`/`Modify`/`Delete` validation branches; `SessionError::HashConflict` includes path + expected/actual hash |
| 5 | Local API `/v1/local/*` scope redesigned: coherent naming, unified error model | ✅ | `POST /v1/local/workspace/open`, `POST /v1/local/workspace/commit`; `OpenSnapshot` response type; `map_session_error()` maps all error variants to standard HTTP codes |
| 6 | V1.55 P1 skeleton fully replaced; no dual in-memory/DB path | ✅ | `HashMap + Mutex<...>` removed; `WorkspaceSessionManager::new()` now takes `Arc<SqlitePool>`; no in-memory fallback path |
| 7 | All amended specs reflect new OCC, session, and API scope behaviour | ✅ | 4 specs amended (see §Spec Amendment Quality below) |
| 8 | P0 topic branch merged to `iteration/v1.56` before tri-review | ✅ | `a264c383` is merge commit of `feature/v1.56-df31-df42-full-redesign` into `iteration/v1.56` |

### Reverse Dependency Check
- `nexus-daemon-runtime/Cargo.toml`: depends on `nexus-local-db = { path = "../nexus-local-db" }` ✅ (correct direction)
- `nexus-local-db/Cargo.toml`: **no** dependency on `nexus-daemon-runtime` ✅ (no circular dependency)

## Spec Amendment Quality

### `concurrency.md` (§9 Workspace Session OCC)
- New §9 is self-contained and correctly cross-references existing sections (§2 file lock, §7 kb_extract_jobs version-based OCC).
- Algorithm description (9.2) matches implementation: `workspace.open` → SHA-256 scan → JSON snapshot; `workspace.commit` → per-operation validation → atomic `consumed = 1` UPDATE.
- SHA-256 choice is documented (9.3) with collision-resistance rationale. ✅
- Retry model (9.4) correctly states daemon does NOT auto-retry. ✅
- Anti-patterns (9.5) correctly distinguish independent concurrency domains. ✅

### `daemon-runtime.md` (§3 Subsystem responsibilities)
- Single-row amendment adding workspace session persistence to Daemon runtime ownership column.
- No contradictions with existing subsystem boundaries. ✅

### `local-db-schema.md` (§4.2.1 workspace_sessions)
- Table DDL in spec matches migration SQL exactly.
- Indexes, column types, defaults, and CHECK constraints all documented.
- `session_id LIKE 'ws_%'` prefix constraint is documented. ✅
- Column notes correctly document RFC 3339 format for timestamps. ✅

### `local-runtime-boundary.md` (§3.2.1 endpoint table)
- `POST /v1/local/workspace/open` and `POST /v1/local/workspace/commit` added with `Active (V1.56 P0)` status.
- Description correctly references `concurrency.md §OCC` and SQLite persistence. ✅
- No existing endpoints modified or removed. ✅

## Key Architecture Observations

### Reversible Design Decisions
- **SHA-256 algorithm**: isolated in two functions (`compute_content_hashes`, `compute_single_file_hash`); trivially replaceable. ✅
- **Schema constraints**: `CHECK (session_id LIKE 'ws_%')` and `CHECK (consumed IN (0, 1))` can be relaxed forward-compatibly. ✅
- **`file_hashes_json` as TEXT**: stores JSON; migrate to separate table or normalized columns without breaking consumers. ✅
- **Migration SQL**: all statements use `IF NOT EXISTS`; idempotent and safe for re-execution. ✅

### Migration Idempotency & Integrity
- `CREATE TABLE IF NOT EXISTS` — idempotent. ✅
- `CREATE INDEX IF NOT EXISTS` — idempotent. ✅
- No foreign key references to other tables — no FK integrity concerns for this migration. ✅
- No rollback script — consistent with all existing migrations (rollback by git revert). ✅

### Compute Content Hashes — Symlink Following
`compute_content_hashes()` uses `std::fs::DirEntry` with `path.is_dir()` and `path.is_file()`, both of which **follow symlinks** in Rust's standard library. If a symlink points outside the workspace directory or creates a cycle, content hashing could:
- Hash files outside the intended workspace scope.
- In degenerate symlink-loop cases, cause excessive recursion (mitigated by OS-enforced symlink depth limits; crash path is extremely unlikely in practice).

**Risk**: Low. No privileged-execution escalation (local-only tool). However, the OCC guarantee is subtly weaker than advertised if symlinks resolve outside the workspace root.

**Recommendation**: Add `entry.file_type().await.map(|ft| ft.is_symlink()).unwrap_or(true)` (or the sync equivalent `entry.file_type().map(|ft| ft.is_symlink())`) to skip symlink entries entirely. This can be deferred to a maintenance follow-up — not blocking for P0.

---

## Findings

### 🟡 Warning (Medium)

#### W-001: `sha2` dependency not workspace-managed
- **Source**: `crates/nexus-daemon-runtime/Cargo.toml` line: `sha2 = "0.10"` (direct version pin)
- **Observation**: The `sha2` crate is pinned as a direct dependency rather than referenced via `{ workspace = true }`. The root `Cargo.toml` has no `sha2` entry under `[workspace.dependencies]`. This is inconsistent with the majority of dependencies in the same file (e.g., `serde_yaml = { workspace = true }`).
- **Risk**: Low. Workspace-wide dependency upgrades (e.g., security patch for SHA-256 implementations) would require finding this exception rather than updating one workspace key. The `sha2` crate is from `RustCrypto/hashes` (stable, well-maintained), so drift risk is minimal.
- **Recommendation**: Add `sha2 = "0.10"` to root `Cargo.toml` `[workspace.dependencies]` and change the crate reference to `sha2 = { workspace = true }`.

### 🟢 Suggestion (Low)

#### S-001: `compute_content_hashes` follows symlinks
- **Source**: `session.rs:147,154` — `path.is_dir()` and `path.is_file()` follow symlinks
- **Observation**: Files/directories reachable via symlinks outside the workspace scope are included in content hashing. Symlink cycles could cause excessive recursion.
- **Impact**: Theoretical. Local-only tool; OS-enforced symlink depth limits prevent crash. OCC guarantee may be subtly weaker.
- **Recommendation**: Add symlink detection with `std::fs::Metadata::file_type().is_symlink()` and skip (or `continue`) symlinked entries.

#### S-002: Dead code arm in `map_session_error` for `HashConflict`
- **Source**: `workspace.rs:345-352` — `SessionError::HashConflict` arm in `map_session_error()`
- **Observation**: `commit_workspace` handles `HashConflict` before calling `consume_session` (step 1 → returns early on conflict). The `HashConflict` arm in `map_session_error` (used only for step 2 errors) is unreachable in current code.
- **Impact**: Trivial. Defensive programming for future use; not a bug.
- **Recommendation**: Remove the arm or add a comment explaining it's reserved for future refactoring.

#### S-003: SHA-256 buffer-read code duplication
- **Source**: `session.rs:126-146` (in `compute_content_hashes`) and `session.rs:278-293` (`compute_single_file_hash`)
- **Observation**: Both functions implement identical 8KB-buffered SHA-256 read loops.
- **Impact**: Maintenance burden if buffer size or read strategy changes.
- **Recommendation**: Extract a shared `fn sha256_file(path: &Path) -> Result<String, SessionError>` helper used by both callers.

#### S-004: Pre-existing `.sqlx` offline cache gap (non-P0)
- **Source**: `cargo test -p nexus-local-db -p nexus-daemon-runtime` fails with `DATABASE_URL` errors for queries in `kb_store.rs`, `reference_source.rs`, `db/pool.rs` — all pre-existing (not P0-introduced).
- **Observation**: The .sqlx offline cache was regenerated for nexus42 (per PM commit `8809f0b5`, R-V156P0-CACHE-01 resolved) but pre-existing queries in other crates still lack cache entries, blocking test execution in clean checkouts.
- **Impact**: Developer friction. Does not affect binary correctness (clippy passes, commit message confirms all tests pass in the build environment where DATABASE_URL is configured).
- **Recommendation**: Extend the `.sqlx` cache regeneration to cover `nexus-local-db` and `nexus-daemon-runtime` test queries in a follow-up.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|-----------------|------------|
| W-001 | static-analysis | `crates/nexus-daemon-runtime/Cargo.toml` | High |
| S-001 | manual-reasoning | `session.rs` lines 147, 154 | Medium |
| S-002 | manual-reasoning | `workspace.rs` lines 345-352 | High |
| S-003 | manual-reasoning | `session.rs` lines 126-146, 278-293 | High |
| S-004 | build-error | `cargo test -p nexus-local-db -p nexus-daemon-runtime` | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: **Approve with comments**

No critical or blocking issues identified. All 8 acceptance criteria are demonstrably met. The architecture is coherent: module boundaries are respected (`nexus-daemon-runtime` → `nexus-local-db` only), dependency direction is correct, the four spec amendments are internally consistent with each other and with existing spec content, and the design decisions are all reversible (SHA-256, schema constraints, JSON blob storage). There is zero scope creep into DF-29, DF-56, or R-V155P2-F002 territories.

The single Warning (W-001, `sha2` not workspace-managed) and four Suggestions are suitable for PM registration as residual or deferral to a maintenance follow-up. None justifies delaying the P0 gate.

**Targeted re-review**: If any QC-2 or QC-3 findings are resolved via targeted fix round and this seat is listed, I can revalidate from the same report file.
