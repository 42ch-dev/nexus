---
plan_id: 2026-06-22-v1.58-workspace-occ-hardening
reviewer: qc-specialist-2
reviewer_index: 2
focus: security-correctness
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: d443e855..af82ad39
reviewed_at: 2026-06-22T14:15:00Z
verdict: Approve
---

# QC2 — V1.58 P0 Workspace OCC Hardening — Security/Correctness Review

## Summary

Reviewer #2 (security-correctness) reviewed the merged changes for path canonicalization (T2), OCC transaction guard (T5), async I/O migration (T4), reqwest client reuse (T9), force/body-size/retry/metrics behavior (T7/T11/T13/T15/T16), and the .sqlx cache hygiene protocol (T18). Three high-severity issues block approval: (1) blocking `std::fs::canonicalize` remains in async session paths, (2) TOCTOU between exists+canonicalize and subsequent file hash read in `validate_changes_manifest` allows symlink escape for per-file OCC checks, and (3) the .sqlx query cache artifacts are deleted in the working tree and `SQLX_OFFLINE=true cargo check --workspace --tests` fails, directly regressing the R-V156P1-CACHE-01 residual the plan claims to close. Additional medium issues around misleading `force` semantics and incomplete async I/O coverage were noted.

## Findings

### High severity (blocking — security or correctness impact)

- **T4 incomplete — blocking I/O on async runtime (session.rs:167,332,443)**: `canonicalize_workspace_root` and the per-file canonicalize sites in `open_session` and `validate_changes_manifest` call `std::fs::canonicalize`. These are invoked from `async fn` contexts (`open_session`, `validate_changes_manifest`). `tokio::fs::canonicalize` exists and should have been used per the T4 acceptance criteria ("replace with tokio::fs equivalents"). A slow or blocked filesystem operation on the canonicalize path stalls the tokio worker thread. Evidence: `grep std::fs::canonicalize crates/nexus-daemon-runtime/src/workspace/session.rs` shows three call sites inside async functions; plan T4 and daemon-runtime.md §V1.58 overlay claim async I/O migration.

- **T2 TOCTOU — path boundary check does not cover the hash read path (session.rs:442-447,471)**: In `validate_changes_manifest`, the boundary enforcement is:
  ```rust
  if file_path.exists() {
      let canonical_file = std::fs::canonicalize(&file_path)?;
      let canonical_root = ...;
      enforce_path_boundary(&canonical_file, &canonical_root)?;
  }
  ...
  let current_hash = compute_single_file_hash(&file_path).await?;  // opens original path
  ```
  `compute_single_file_hash` does `tokio::fs::File::open(path)` with no re-validation. Between the conditional `exists()`/`canonicalize` and the open, a symlink can be introduced at `file_path` that points outside the workspace root. The hash of outside content is then used for OCC comparison. Contrast with `compute_content_hashes_inner` (lines 242-248) which correctly uses `symlink_metadata` and skips symlinks for every entry. The per-Modify path at commit time lacks this defense. This is a symlink escape vector for content-hash validation.

- **T18 process regression live — .sqlx cache deleted and verification fails (R-V156P1-CACHE-01)**: `git status --short .sqlx/` shows multiple `D .sqlx/query-*.json` entries. `SQLX_OFFLINE=true cargo check --workspace --tests` fails with 83+ errors ("no cached statement" class, plus Sized trait errors on query macros). The protocol documented in `daemon-runtime.md` §V1.58 (run with `--tests`, commit query json, verify with `SQLX_OFFLINE=true ... --tests`) is not satisfied in this tree. HEAD commit message claims "restore .sqlx/ cache", but the query artifacts are absent. This means the exact regression that triggered R-V156P1-CACHE-01 (prepare without `--tests` omitting test `query!` macros) is still reproducible. T18 claims to resolve both R-V156-PROCESS-01 and R-V156P1-CACHE-01; the state contradicts the claim.

### Medium severity (suggest blocking)

- **T7/T15 — `force` is a documented no-op but advertised as bypassing cache (registry.rs:383-390, 89-90)**: `force` is parsed and logged, but the implementation comment states "in the current design there is no cache layer (synthetic is always fresh; CDN always fetches), so the param is honored by construction". The help text (`registry_refresh_help_text`) still says "Set `force: true` to bypass cache freshness and re-fetch unconditionally." While `force` does not bypass any security gate (scheme/IP/body-size checks are in `fetch_from_cdn` regardless), the mismatch between advertised behavior and actual effect is a contract/documentation defect that will confuse callers and future maintainers when a real cache is added.

- **T2/T4 — canonicalize of target in open_session uses std::fs even for non-existent paths (session.rs:331-337)**: When `target_path` does not exist (Create case), code falls back to the logical join `target_path.clone()` and calls `enforce_path_boundary` on it. This is acceptable for Create, but the `canonicalize_workspace_root` wrapper itself remains blocking. More importantly, `validate_changes_manifest` still does per-file `std::fs::canonicalize` on existing files under Modify, which is the path that feeds the OCC hash check.

### Low severity (advisory)

- **T13 — jitter source documented but not under a named constant**: `retry_jitter_ms` uses `SystemTime` subsec nanos. The comment correctly states "sufficient for jitter; not cryptographic." Consider extracting `100u64..=500` into a named range constant for clarity and testability; current literal duplication between fn and test is minor.

- **T16 — metrics counters use Relaxed which is correct for this use case**: `AtomicU64` increments with `Relaxed` are appropriate (no cross-thread data dependency that requires Acquire/Release for correctness of the counter itself). All four counters are incremented on every relevant path (entry, success, failure-on-fallback, cache-hit). No missing increments observed.

- **T9 — SHARED_CDN_CLIENT safety**: `LazyLock<reqwest::Client>` with `Policy::limited(0)` and per-request `.timeout()` is the right pattern. `clone()` is cheap (Arc). No caller-visible mutable state. No evidence of corruption vectors.

- **T11 — body size enforcement is streaming and non-bypassable**: `read_body_with_limit` checks `buf.len() + chunk.len() > max_size` before extending and returns `BodyTooLarge` early. The limit is taken from `CdnConfig::max_body_bytes` (caller-injected). No streaming bypass.

- **T15 — generated_at captured once per invocation**: `let now = Utc::now().to_rfc3339();` is before the retry loop and used uniformly for success/fallback/synthetic outputs. This is the correct security property (deterministic value for a single logical refresh).

- **T5 OCC atomicity (session level)**: `commit_session` binds `validate_changes_manifest` + `consume_session`. The single-consumer CAS is the `UPDATE ... WHERE consumed = 0` inside `db::consume_session`. If two concurrent `commit_session` calls race on the same session_id, exactly one wins (test `concurrent_commit_session_single_winner` asserts this). The content-hash validation happens before the CAS; a concurrent file change on disk between open and commit is detected as hash mismatch, not as a session-consume race. This is the intended OCC model. No window was found where a session could be read after validate but before the consume CAS succeeds.

- **T6 metrics/tracing at conflict points**: `incr_occ_conflict()` (AtomicU64) and structured `tracing::warn!` with `conflict_type` are present in both `HashConflict` and `AlreadyCommitted` paths. Counter is observable via `occ_conflict_total()`.

## Security Properties Verified

- **Path traversal prevented at open time (T2 happy path)**: `open_session` canonicalizes root, joins, canonicalizes target (when exists), then calls `enforce_path_boundary`. Symlinks are skipped during full-tree hash computation via `symlink_metadata`. `validate_workspace_path_safe` in the HTTP handler layer rejects `..` and absolute paths before session creation.
- **OCC single-consumer for session ticket (T5)**: `consume_session` atomic UPDATE WHERE guarantees exactly one consumer per session ID. Concurrent test passes.
- **HTTPS + private-IP block + no-redirect + body cap (T9/T11)**: All enforced in `fetch_from_cdn` before any body accumulation or JSON parse. Shared client does not weaken these.
- **Metrics are monotonic and thread-safe (T16)**: AtomicU64 Relaxed counters incremented on all exit paths; tests assert increment behavior.
- **generated_at determinism (T15)**: Captured once before retry loop.

## Verdict Reasoning

**Request Changes**.

High-severity findings are blocking:

1. Blocking `std::fs::canonicalize` in async session paths (T4) violates the explicit acceptance criterion and creates a runtime-stall vector.
2. The TOCTOU in `validate_changes_manifest` (T2) means the per-file hash read at commit time is not protected by the same symlink defense that protects the open-time tree walk. This is a correctness and potential confidentiality issue for OCC.
3. The .sqlx cache state (T18) is a direct process regression. The plan asserts resolution of R-V156P1-CACHE-01 and R-V156-PROCESS-01; the working tree falsifies that assertion. Until `SQLX_OFFLINE=true cargo check --workspace --tests` is clean and the query-*.json artifacts are present and committed, T18 cannot be considered delivered.

Medium issues (misleading `force` contract, remaining std::fs sites) should be cleaned in the same fix wave.

No Critical findings were waived. The security surface that was intended to be hardened (path boundary + OCC atomicity + CDN safety) is mostly correct on the happy paths, but the gaps above are material.

## Cross-Plan Concerns

- **P1 (DF-44 reference refresh) and sqlx cache**: The P1 merge introduced new `sqlx::query!` macros (reference_source.rs, migration 06220003). The "restore" commit in this range claims to have run the full `--tests` prepare, yet the query json files are deleted in the tree under review. Any future plan that adds test queries must treat the cache as a first-class deliverable with a CI gate, not a post-merge manual step. Recommend adding an explicit CI job `SQLX_OFFLINE=true cargo check --workspace --tests` as a required check for any PR touching migrations or `query!` sites.
- **Daemon-runtime async boundary**: Multiple crates (nexus-agent-host, preset loader, work_chapters) already use `std::fs::canonicalize` + `spawn_blocking` or `tokio::fs`. The workspace session layer should converge on the same pattern to avoid a second class of "blocking in async" bugs. Consider a small internal helper `async fn canonicalize(path: &Path) -> Result<PathBuf, SessionError>` that does the right thing.
- **OCC model documentation**: The distinction between "session ticket single-consumer" (T5) and "content hash at open time" (T2) is subtle. Future work on concurrent writers editing the same files should make the threat model explicit in `daemon-runtime.md` (who is the adversary? malicious concurrent session holder, or compromised host process, or on-disk attacker?).

**Reviewed files (primary)**:
- `crates/nexus-daemon-runtime/src/workspace/session.rs`
- `crates/nexus-orchestration/src/capability/builtins/registry.rs`
- `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs`
- `crates/nexus-local-db/src/workspace_session.rs`
- `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs`
- `.mstar/knowledge/specs/daemon-runtime.md` (V1.58 overlay sections)
- `.mstar/plans/2026-06-22-v1.58-workspace-occ-hardening.md`
- Working tree `.sqlx/` state and `git status`

## Revalidation

**Revalidated by**: qc-specialist-2
**Revalidated at**: 2026-06-22T15:42:00Z
**Diff basis**: 43bf69e2..20c8ae0f (P0 fix-wave)

### Findings Status

| Original Finding | Severity | Status | Evidence |
| --- | --- | --- | --- |
| H-1 std::fs::canonicalize async | HIGH | Closed | `canonicalize_workspace_root` (session.rs:171-177) wraps `std::fs::canonicalize` in `tokio::task::spawn_blocking`; all 3 call sites (open_session:340/343, validate:453) route through wrapper. T4 acceptance criterion fully met. |
| H-2 validate_changes_manifest TOCTOU | HIGH | Closed | `symlink_metadata` re-validation immediately before hash read (session.rs:511-519); regression test `validate_changes_manifest_rejects_symlink_in_modify_path` (workspace_occ_concurrent.rs:192-251) exercises symlink escape and asserts `PathEscape`. |
| H-3 .sqlx/ cache hygiene | HIGH | Closed | `SQLX_OFFLINE=true cargo check --workspace --tests` → clean (exit 0); 141 `query-*.json` files present (≥50); `crates/nexus-local-db/tests/sqlx_cache_intact.rs` exists and passes; protocol in daemon-runtime.md §V1.58 explicitly requires `--tests` flag. |
| M-1 force param removed | Medium | Closed | Input schema (registry.rs:380) is `{}` with `additionalProperties:false`; `RegistryRefreshInput` struct (contracts) has no `force` field; help text (registry.rs:89-96) omits force; run() parses to empty struct; backward compat test `registry_refresh_tolerates_historical_force_field` (registry.rs:999-1005) confirms serde ignores unknown fields. |
| M-2 per-file canonicalize sites | Medium | Closed | Closed by H-1 (all canonicalize now via spawn_blocking wrapper). |

### New Findings (if any)

None. No new HIGH or MEDIUM findings discovered during targeted re-review.

### Verdict

**Verdict**: Approve
**Rationale**: All 3 original HIGH findings (H-1, H-2, H-3) are closed with concrete evidence (spawn_blocking wrapper, pre-open symlink_metadata + regression test, cache hygiene verification + guard test + documented protocol). Both Medium findings are also closed (M-1 by schema/help/run changes + compat test; M-2 subsumed by H-1). `SQLX_OFFLINE=true cargo check --workspace --tests` is clean; `cargo test -p nexus-local-db --test sqlx_cache_intact` passes. No new blocking issues found. Security/correctness surface for the P0 scope is now acceptable.
