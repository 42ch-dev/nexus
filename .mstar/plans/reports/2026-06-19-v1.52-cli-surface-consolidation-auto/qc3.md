---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-19-v1.52-cli-surface-consolidation-auto"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: opencode/minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Performance and reliability risk (alias overhead, resource lifecycle, log volume, error-message parity, deprecation frequency, test coverage of forwarding path)
- Report Timestamp: 2026-06-19T13:45:00Z

## Scope
- plan_id: 2026-06-19-v1.52-cli-surface-consolidation-auto
- Review range / Diff basis: b97ec0d9..771f89e7
- Working branch (verified): feature/v1.52-cli-surface-consolidation-auto
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p1/
- Files reviewed: 3 code-side (crates/nexus42/src/commands/creator/kb.rs [alias rewire + deprecation helper], crates/nexus42/src/commands/creator/world/kb.rs [hermetic forwarding target, pre-existing — read for parity check], crates/nexus42/tests/world_kb_alias.rs [new integration tests]) + spec overlay (.mstar/knowledge/specs/cli-spec.md §6.2G.2) + plan (.mstar/plans/2026-06-19-v1.52-cli-surface-consolidation-auto.md)
- Commit range: 771f89e7 (single implement commit in range for this plan)
- Tools run:
  - `git diff b97ec0d9..771f89e7 -- crates/nexus42/src/commands/creator/kb.rs`
  - `git diff b97ec0d9..771f89e7 -- crates/nexus42/tests/world_kb_alias.rs`
  - `git diff b97ec0d9..771f89e7 -- .mstar/knowledge/specs/cli-spec.md`
  - `cargo test -p nexus42 --test world_kb_alias -- --nocapture` (6/6 passed)
  - `cargo test -p nexus42` (1004 passed, 0 failed)
  - `cargo clippy --all -- -D warnings` (clean — CI gate)
  - `./target/debug/nexus42 creator kb list --help` and `creator kb list --scope world --world-id ...` for runtime emission check
  - `grep` of `super::world::kb::` forwarding sites, `deprecation_notice_legacy_world_kb` helper, and `open_world_pool` helper

## Findings

### 🔴 Critical
(none)

### 🟡 Warning

- **W-001: Alias forwarding path is NOT actually tested — plan T6 and T7 are not delivered.**
  The plan §5 lists:
  - **T6**: "Write integration test `legacy_kb_scope_world_emits_deprecation` that captures stderr and asserts the deprecation message."
  - **T7**: "Write integration test `legacy_kb_scope_world_list_forwards_to_canonical` that verifies output parity."
  Both are marked `[x]` (done) in the plan, but the new file `crates/nexus42/tests/world_kb_alias.rs` contains **neither** test. The actual tests in the file are:
  1. `creator_kb_list_help_documents_scope_world` — only asserts help text contains `--scope` and `world`.
  2. `creator_world_kb_adopt_help_is_reachable` — only asserts help text contains `adopt` (T-A P0 forward-compat).
  3. `canonical_kb_list_lists_seeded_block` — calls `kb_list(&pool, WORLD, false)` **directly**; does not exercise the alias path in `kb.rs`.
  4. `canonical_kb_show_shows_seeded_block` — same; direct call.
  5. `canonical_kb_delete_soft_deletes_block` — same; direct call.
  6. `canonical_kb_delete_cross_author_rejects` — same; direct call.

  None of the 6 tests:
  - Invokes the `nexus42 creator kb --scope world <subcmd>` binary path
  - Captures stderr / asserts the deprecation string format on stderr
  - Verifies the legacy alias forward to `super::world::kb::kb_list(&pool, &wid, false)` (or `kb_show` / `kb_delete`) is the function actually invoked
  - Verifies output parity between `creator kb --scope world list` and `creator world kb list`

  **Reliability impact**: the alias wiring — the actual feature being shipped (R-V150KBED-01 closure) — is **untested**. If a future refactor accidentally drops the `deprecation_notice_legacy_world_kb` call, changes the sub-command argument (e.g., `false` → `true` for `json`), or replaces `super::world::kb::kb_list(&pool, &wid, false)` with a `SqliteKbStore::list_by_world` direct call, **no test fails**. This is exactly the divergence class the plan was meant to prevent.
  **→ Fix**: Add at least one `assert_cmd`-based integration test that invokes `creator kb list --scope world --world-id <id>` against a temp `NEXUS42_HOME`, captures stdout/stderr, and asserts (a) the deprecation line on stderr and (b) the output equals `creator world kb list <id>`. Same for `show` and `remove`. A single `legacy_alias_emits_deprecation_and_forwards` test covering one of the three would already close the gap; the plan asked for two.

- **W-002: Error message format on pool-init failure diverges between legacy alias and canonical surface — observable behavioral difference, not just message text.**
  The new `open_world_pool` in `kb.rs:338-343` wraps `Schema::init` errors as `"Failed to open workspace pool: {e}"`. The canonical `super::world::open_workspace_pool` in `world/mod.rs:152-156` lets the raw `LocalDbError` bubble. Verified at runtime against a corrupted-migration DB:

  ```
  $ nexus42 creator kb list --scope world --world-id wld_x
  Error: Failed to open workspace pool: database migration failed: migration 202606070001 was previously applied but has been modified

  $ nexus42 creator world kb list wld_x
  Error: local database error: database migration failed: migration 202606070001 was previously applied but has been modified
  ```

  The two surfaces return **different `Display` strings** for the same underlying cause. Users scripting error handling (CI log parsers, support tickets, monitoring that greps for "Failed to open workspace pool" or "local database error") will see two different error signatures for the same failure. Plan §4 acceptance criteria #1 says "produces the same output as `creator world kb list`" — strictly speaking, this is on the success path, but the failure-path divergence is a real reliability concern for downstream tooling.

  **→ Fix**: Either (a) make `open_world_pool` return the raw error (matching the canonical surface), or (b) update the canonical `super::world::open_workspace_pool` to apply the same wrapping. Option (a) is the surgical change consistent with "alias to canonical" semantics. The current asymmetry is the opposite of the consolidation goal.

### 🟢 Suggestion

- **S-001: `open_world_pool` in `kb.rs:338-343` duplicates `super::world::open_workspace_pool` in `world/mod.rs:152-156` (modulo error wrapping).** Two near-identical helpers for the same purpose live in the same crate. The W-002 fix collapses this to a single call site; if a third call site emerges, extract a `crate::db::open_workspace_pool(&CliConfig) -> Result<SqlitePool>` helper in `db/mod.rs` and have both call sites use it. Low-priority refactor; do not block merge.

- **S-002: Deprecation notice has no rate-limit; emits on every legacy invocation (1× `tracing::warn!` + 1× `eprintln!`).** At 100 invocations per CI run (a script looping through 100 world blocks), the user gets 200 lines of deprecation noise in stderr/log. For a single human, the volume is fine. For automation, it is log spam. Two options to consider before V1.53 (current design is acceptable for a pre-removal deprecation cycle):
  - **Eager**: log a one-line summary on first invocation, then suppress.
  - **Lazy**: aggregate with a counter and emit one summary line at process exit.
  Current per-call emit is the conventional pattern (matches `rustc --deprecated`); the suggestion is to keep an eye on real-world usage before V1.53 removal. Not blocking.

- **S-003: `tracing::warn!` + `eprintln!` duplication doubles log volume per call.** When `tracing-subscriber` writes to stderr (the default in this CLI), the user sees **two** lines per deprecation — one colored `WARN` from tracing, one `nexus42:` prefixed from `eprintln!`. The `tracing` line alone is sufficient for both log-infrastructure consumers and human readers (with `RUST_LOG=warn`). Keeping the explicit `eprintln!` is defensible if a substantial user base runs without tracing-subscriber; current behavior is fine, but consider whether the duplication is needed. Not blocking.

- **S-004: Help text (`nexus42 creator kb --help` and `nexus42 creator kb list --help`) does not surface the deprecation notice.** A user reading the help output sees no hint that `--scope world` is deprecated or that they should use `creator world kb` instead. The deprecation only surfaces **after** they run the command and see the stderr line. This is a discoverability issue, not a performance/reliability regression, but it is the single biggest reason users will not migrate before V1.53 removal. **→ Fix**: add a single line to the `Long` help of `--scope` (e.g., `Note: --scope world is deprecated; use 'creator world kb ...' (removal V1.53).`) or to the subcommand `About` text. Low-risk one-line patch.

- **S-005: `kb_remove` (world scope) now enforces `require_world_owner` via `kb_delete` forwarding — this is a security improvement, but it is a behavior change from the legacy inline path.** The original legacy `kb_remove` world-scope path used `store.delete_key_block` directly, with **no** owner gate. The new path delegates to `kb_delete(&pool, &cid, &wid, entry_id, true)`, which calls `require_world_owner` and returns `403 WORLD_KB_FORBIDDEN` for cross-author attempts. The plan §4 acceptance criterion #5 says "produces a result equivalent to `creator world kb delete <id> <entry_id> --yes`" — i.e., explicitly accepts the new auth gate. **No fix required**, but flagging it because: (a) a user whose existing scripts relied on cross-author removal will now see `403` errors; (b) this is a real semantic change, not just a forwarding rewrite. qc2 already noted this as S-001; surfacing here for completeness.

- **S-006: Two `tracing::warn!` + `eprintln!` events per legacy invocation is `2N` for N calls; combined with S-002 means the worst case is `2N` log lines. A `tracing::warn!` per legacy invocation in a 10,000-call CI loop is 10,000 log lines.** Today's pre-v1.0 deprecation cycle with low adoption is unlikely to hit this; flag for monitoring.

- **S-007: `fresh_pool_with_block()` in the new test file ignores `key_block_id` for the `kb_list` test (`drop(key_block_id);`) — not a bug, but signals the test was authored against the canonical call signature even though the plan T7 was supposed to use the legacy alias path.** Aligns with W-001.

## Source Trace
- Finding ID: QC3-2026-06-19-R-V150KBED-01
- Source Type: git-diff + hermetic test execution + runtime smoke test + manual review
- Source Reference:
  - W-001: `crates/nexus42/tests/world_kb_alias.rs:1-171` (test inventory) vs `.mstar/plans/2026-06-19-v1.52-cli-surface-consolidation-auto.md:73-74` (T6/T7 claim) — gap between plan and delivery
  - W-002: `crates/nexus42/src/commands/creator/kb.rs:338-343` (open_world_pool wrapper) vs `crates/nexus42/src/commands/creator/world/mod.rs:152-156` (open_workspace_pool raw error)
  - S-001: same files as W-002
  - S-002/S-003/S-006: `crates/nexus42/src/commands/creator/kb.rs:325-332` (deprecation_notice_legacy_world_kb)
  - S-004: `crates/nexus42/src/commands/creator/kb.rs:54-110` (clap long-help, no deprecation note)
  - S-005: `crates/nexus42/src/commands/creator/kb.rs:789-797` (kb_remove forward) + `world/kb.rs:352-372` (kb_delete + require_world_owner)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (initial) → **0 (post-revalidation)** |
| 🟢 Suggestion | 7 |

**Verdict**: Request Changes (initial wave; **resolved in revalidation below**)

## Revalidation (2026-06-19, targeted re-review)

**Re-review scope**: Review range `771f89e7..fe3c5730` (1 fix commit: `fe3c5730`)

**Re-review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p1/`
**Working branch (verified)**: `feature/v1.52-cli-surface-consolidation-auto`
**Commit range (verified)**: `fe3c5730 fix(cli): V1.52 T-A P1 QC fix-wave — close R-V152TAP1-W001/W002/S001`

### Re-validation Method
- `git diff 771f89e7..fe3c5730 --stat` (3 files: kb.rs +255, world_kb_alias.rs +251, qc3.md +189)
- `git diff 771f89e7..fe3c5730 -- crates/nexus42/src/commands/creator/kb.rs` (full forward + error-wrapping change review)
- `git diff 771f89e7..fe3c5730 -- crates/nexus42/tests/world_kb_alias.rs` (full new integration tests review)
- `cargo test -p nexus42 --test world_kb_alias -- --nocapture` → **9/9 pass** (3 new: `legacy_kb_scope_world_{list,show,remove}_forwards_to_canonical` + 6 pre-existing)
- `cargo test -p nexus42 --lib commands::creator::kb` → **12/12 pass** (5 new: `open_world_pool_error_matches_canonical_format` + 3 `_exercises_forward_path` + improved `deprecation_notice_emits_stderr_message`; 7 pre-existing)
- `cargo test -p nexus42` → **760/760 unit tests pass** + all integration test suites green (no regressions)
- `cargo clippy --all -- -D warnings` → **clean** (CI gate green)
- `cargo +nightly fmt --all --check` → **clean**
- `target/debug/nexus42 creator kb list --help` → deprecation note now appears in help text (`S-001` fix verified at runtime)

### Fix Validation

| Initial Warning | Status | Evidence |
|----------------|--------|----------|
| W-001 (alias forward wiring untested at kb.rs:448-454, 610-615, 789-797) | **Resolved** | commit `fe3c5730` adds **3 hermetic assert_cmd integration tests** in `tests/world_kb_alias.rs:256-422` (`legacy_kb_scope_world_list_forwards_to_canonical`, `legacy_kb_scope_world_show_forwards_to_canonical`, `legacy_kb_scope_world_remove_forwards_to_canonical`) that drive the full `nexus42` binary against a seeded hermetic HOME and assert: (a) deprecation string on stderr (`"deprecated"`, canonical surface name, `"V1.53"`), (b) output parity (seeded block `char_alias_cmd` in stdout), (c) for remove — block is actually soft-deleted (verified by re-listing). Additionally **3 unit tests** in `kb.rs:1178-1348` mirror the exact forward call shape (deprecation + canonical `kb_list`/`kb_show`/`kb_delete` call with same argument signature), and the previously tautological `deprecation_notice_emits_stderr_message` test now actually invokes the function. All 9/9 world_kb_alias tests pass; all 5 new kb unit tests pass. The forward call sites at lines 469 (kb_list), 631 (kb_show), 813 (kb_delete) are now covered by hermetic tests that exercise the actual binary path — not just direct canonical calls. |
| W-002 (error message divergence: `"Failed to open workspace pool: …"` vs canonical `"local database error: …"`) | **Resolved** | commit `fe3c5730` `kb.rs:356-359`: `open_world_pool` switched from `.map_err(\|e\| CliError::Other(format!("Failed to open workspace pool: {e}")))` to `Ok(crate::db::Schema::init(&db_path).await?)` — the `?` operator now triggers `impl From<nexus_local_db::LocalDbError> for CliError` (`errors.rs:447-451`) which produces exactly `"local database error: {err}"`, matching canonical `world::open_workspace_pool` (`world/mod.rs:152-156`). New regression test `open_world_pool_error_matches_canonical_format` (`kb.rs:1138-1162`) explicitly constructs a `LocalDbError::VersionMismatch`, converts to `CliError`, and asserts (a) `msg.contains("local database error:")`, (b) `!msg.contains("Failed to open workspace pool")` (pre-fix format absence). Both legacy and canonical surfaces now emit identical error Display strings, so CI log parsers and monitoring rules keyed on `"local database error"` will work uniformly. |
| S-001 (deprecation discoverability: help text had no hint) — listed in qc1 as partial-blocking | **Resolved** | commit `fe3c5730` `kb.rs:54-128` adds deprecation note to `Long` help of `--scope` for all 5 subcommands (List, Search, Show, Add, Remove), e.g. `"Note: \`--scope world\` is deprecated; use \`creator world kb list\` instead (planned removal V1.53)."`. Verified at runtime: `nexus42 creator kb list --help` now prints the deprecation note in the `--scope` description. Integration test `creator_kb_list_help_documents_scope_world` extended (`world_kb_alias.rs:85-89`) to assert the help text contains `"deprecated"` or `"creator world kb"`. |

### Performance & Reliability Re-check
- **Forward overhead**: unchanged from initial analysis. The 3 new hermetic integration tests use `tempfile::tempdir()` + `Schema::init` once per test, so per-test cost is bounded. No new hot-path allocation or syscall on the alias path itself.
- **Error path**: `?` operator on `LocalDbError → CliError` is a zero-cost `From` impl (single `format!` call). No regression vs. prior `.map_err` wrapping; actually marginally cheaper (no double-format of the underlying error).
- **Resource lifecycle**: hermetic HOME tests use `tempfile::TempDir` (auto-cleanup on drop); no leaked handles, no DB files in the user's actual `~/.nexus42/`.
- **Deprecation frequency**: still per-call (1× `tracing::warn!` + 1× `eprintln!`); S-002/S-006 deferred to V1.53 monitoring per initial report — non-blocking.
- **Cross-author auth gate**: `legacy_kb_scope_world_remove_exercises_forward_path` (`kb.rs:1330-1347`) explicitly asserts the forwarded `kb_delete` returns 403/`WORLD_KB_FORBIDDEN` for cross-author attempts — preserving the security improvement noted in qc2.

### Updated Findings
- 🔴 Critical: **0**
- 🟡 Warning: **0** (both blocking Warnings resolved; S-001 elevated to blocking also resolved)
- 🟢 Suggestion: **7** (unchanged from initial; S-002..S-007 deferred to V1.52 P-last WL-A for migration-quality hygiene)

### Updated Verdict: **Approve**

Both blocking items from the initial wave are **conclusively resolved** with hermetic test evidence and observable behavior changes. S-001 (partial-blocking Suggestion from qc1) is also addressed. No new blocking findings introduced. CI gates clean (`cargo clippy --all -- -D warnings`, `cargo +nightly fmt --all --check`). 9/9 world_kb_alias integration tests + 760/760 unit tests pass.

**Handoff**: PM `@project-manager` — qc3 verdict flipped to `Approve` on revalidation. PM may now consolidate the re-review verdict (qc1 + qc3 both Approve → T-A P1 ready for `@qa-engineer` verification and merge to `iteration/v1.52`).

---

## Detailed Performance & Reliability Review (per assignment)

### 1. Alias call overhead
- Each legacy invocation now goes through **one extra Rust function call** (`deprecation_notice_legacy_world_kb`) and **one extra DB pool acquisition** (`open_world_pool`) before reaching the canonical hermetic function. The function call is trivially inlinable (no allocation, ~5 instructions). The DB pool acquisition is **not** trivial — `Schema::init` does:
  1. `tokio::fs::create_dir_all(parent)` (1× syscall, only first time)
  2. `local_db_open_pool(db_path)` (sqlx pool construction)
  3. `run_migrations(&pool)` (idempotent but executes `PRAGMA user_version` + early-return on match)
  4. `nexus_local_db::seed_versions(&pool)` (1× write to `meta` table, idempotent)
- **Net overhead per call**: ~1–5 ms on a warm pool (migration early-return), ~50–200 ms on a cold start. For a single user, inaudible. For a CI loop, the migration early-return is fast but still a network roundtrip to SQLite per call. The same overhead exists on the canonical path (`super::world::open_workspace_pool` does the same thing), so **no regression** introduced. The wrapper is **zero-added** modulo the deprecation log emission.
- **No action**: confirmed no avoidable overhead on the hot path beyond what canonical already pays.

### 2. Forwarding path complexity
- Verified at the call sites: `kb_list`, `kb_show`, `kb_remove` each call the canonical hermetic function with the **same argument shape** the canonical `world::kb` `run` function uses:
  - `super::world::kb::kb_list(&pool, &wid, false)` — `json: false` matches the legacy text-output contract.
  - `super::world::kb::kb_show(&pool, &wid, entry_id, false)` — `json: false` matches.
  - `super::world::kb::kb_delete(&pool, &cid, &wid, entry_id, true)` — `yes: true` matches the non-interactive contract of `kb remove` (no prompt for legacy path).
- No re-parsing of arguments; no extra `clap` invocation; no serialization roundtrip. Forwarding is a **direct function call** with the alias resolving `&CliConfig` → `&SqlitePool` once, then handing off. **Zero inlined overhead**, would optimize to identical machine code if the wrapper were removed.

### 3. Deprecation frequency
- `deprecation_notice_legacy_world_kb` is called **once per legacy invocation**, with no rate-limit. At 100 calls in a CI script: 100 `tracing::warn!` events + 100 `eprintln!` lines = 200 lines of deprecation noise. The plan explicitly accepts this design ("Planned removal V1.53"); not a defect, but S-002/S-006 flag the volume for monitoring.
- Tracing event volume at 1000 events/day (modest CI use): ~1 MB/day of structured log data; immaterial.

### 4. Tracing log volume + level
- `tracing::warn!` level is appropriate: deprecation is a **warning-class event** (the path still works, but is on the way out). Not `info!` (too quiet for migration urgency), not `error!` (path is functional, not broken).
- Volume is small in absolute terms. The double-emission (tracing + eprintln) is the real source of noise — see S-003.

### 5. Resource lifecycle
- `open_world_pool` opens a `SqlitePool`; the pool is consumed by the canonical hermetic function and dropped on return. **No leaked handles** in either the alias or canonical path. `SqlitePool::clone()` is an `Arc` clone (cheap).
- `SqliteKbStore::new(pool.clone())` in the canonical functions does **not** spawn background tasks or hold resources beyond the pool reference. Clean.
- No `unsafe`, no manual `close()` needed. **No resource-lifecycle concern.**

### 6. `open_world_pool()` helper — acquisition path optimization
- The helper calls `Schema::init` which runs migrations + seeds. This is **not** cached across calls; each invocation re-runs the idempotent migrations. For a single user, this is fine. For a high-frequency CI loop, this is **N migrations** instead of 1.
- However, the **canonical** path (`super::world::open_workspace_pool`) does the exact same thing — it is **not cached** either. So the alias wrapper introduces **zero added** migration overhead relative to canonical.
- The pre-existing pattern is "open pool per subcommand" — not optimal, but consistent. Any caching would be a cross-cutting refactor, out of scope for this plan. **No regression** from this PR.

### 7. Failure mode on canonical surface error — ERROR MESSAGE DIVERGENCE (W-002)
- The alias wraps the `Schema::init` error as `"Failed to open workspace pool: {e}"`. The canonical surface lets the raw `LocalDbError` bubble (`"local database error: {e}"`). Same root cause, different user-visible message. **Documented under W-002 above** with repro command and output.

### 8. CLI startup time + binary size
- Aliases add command definitions to clap: in this case, **zero** new commands — the change is purely in the handler logic of existing `creator kb` subcommands. No `clap` definition was added.
- Binary size impact: < 1 KB (the deprecation helper + open_world_pool wrapper). Startup time: 0 (no new init code at startup).
- **No measurable startup-time or size regression.**

### 9. Backward compat regression on canonical surface
- Verified: `cargo test -p nexus42` shows **1004/1004 tests passing** (including 6 new + the 3 creator_world_kb regression set). Canonical surface tests are unchanged and pass.
- No regression on `creator world kb list/show/edit/delete/pending/adopt/reject` paths. Confirmed via `cargo test -p nexus42` aggregate run.

### 10. Test coverage of the new alias feature
- **W-001 covers this in full**: the new test file does not exercise the alias path. The 6 tests are (a) help-text smoke tests and (b) direct-calls to canonical functions. The alias forward wiring in `kb.rs:448-454, 610-615, 789-797` is **untested**. This is the most important finding from a reliability perspective: the **feature being shipped is not tested**.

## Verification Evidence
- `cargo test -p nexus42 --test world_kb_alias -- --nocapture`: 6/6 passed (but does not cover the alias itself — see W-001).
- `cargo test -p nexus42`: 1004 passed, 0 failed (full nexus42 suite green).
- `cargo clippy --all -- -D warnings`: clean (CI gate green).
- Runtime smoke test:
  - `./target/debug/nexus42 creator kb list --help` — no deprecation mention in help text (S-004).
  - `./target/debug/nexus42 creator kb list --scope world --world-id wld_x` — emits both tracing WARN and eprintln line, then errors with `Failed to open workspace pool: ...` (W-002, S-003).
  - `./target/debug/nexus42 creator world kb list wld_x` — no deprecation, errors with `local database error: ...` (W-002 confirmed).
- Checkout alignment verified:
  ```
  /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p1
  feature/v1.52-cli-surface-consolidation-auto
  771f89e710d7a2d8c908d22e4fef252dc13a5d54
  ```
- Review range matches Assignment exactly: `b97ec0d9..771f89e7`.

## Residuals (for PM)
- **Initial blocking items W-001 and W-002 are now resolved** (see Revalidation above). PM should close residual entries R-V152TAP1-W001 and R-V152TAP1-W002 in `status.json` (and archive to `.mstar/archived/residuals/2026-06-19-v1.52-cli-surface-consolidation-auto.json` per `mstar-plan-artifacts` lifecycle).
- S-001 (help-text deprecation discoverability) was tracked as partial-blocking — now resolved by the same fix commit.
- Suggestions S-002..S-007 remain as migration-quality hygiene for V1.53 removal prep; non-blocking. PM may keep them as open `residual_findings` or fold into V1.52 P-last WL-A backlog.
- No new `Critical` findings to register.
