## Completion Report v2 — P0 (findings-lifecycle)

- **plan_id**: `2026-06-17-v1.49-findings-lifecycle`
- **owner**: `@fullstack-dev`
- **Working branch used**: `feature/v1.49-findings-lifecycle`
- **Worktree path**: `.worktrees/v1.49-findings-lifecycle`
- **Base**: `iteration/v1.49` @ `1fd3a9c4`
- **Final HEAD on branch**: `4356bf1f`

### Commits

| SHA | Title | Task |
| --- | --- | --- |
| `237eec20` | `feat(local-db): T1 extend findings status lifecycle (V1.49 F6)` | T1 |
| `613ef56e` | `feat(api,orchestration): T2+T3 lifecycle API surface + actionable filter (V1.49 F6)` | T2 + T3 |
| `4356bf1f` | `test(local-db,api,orch): T4 hermetic lifecycle tests (V1.49 F6)` | T4 (+ clippy fixups) |

### Cargo verification (last meaningful lines)

```
=== 1. cargo +nightly fmt --all --check ===
exit: 0   (no diff)

=== 2. cargo clippy -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings ===
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s

=== 3. cargo test -p nexus-local-db findings ===
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 193 filtered out

=== 4. cargo test -p nexus-daemon-runtime --test findings_api ===
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured

=== 5. cargo test -p nexus-orchestration --test findings_consumer ===
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured

=== 6. cargo test -p nexus-orchestration open_findings_block ===
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 598 filtered out
```

> **Note on command 6**: the substring filter `open_findings_block` matches 0 test **names** in `nexus-orchestration` (no test is named with that exact substring). The two test groups that exercise the open-findings-block code path are:
> - `findings_consumer` integration tests (6 tests, command 5) — DAO + builder + preset-input wiring.
> - `findings_block::tests::*` lib tests (7 tests), run via `cargo test -p nexus-orchestration --lib findings_block`:
>   ```
>   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 591 filtered out
>   ```
> Both groups pass.

All commands run with `SQLX_OFFLINE=true` to use the committed `.sqlx/` cache (CI-equivalent mode).

### Acceptance criteria (4 lines, with evidence)

1. **Overlay §1 lifecycle diagram implemented in DAO validation (transition table matches spec).**
   `is_valid_transition` in `crates/nexus-local-db/src/findings.rs` (commit `237eec20`) encodes every edge of `findings-lifecycle.md` §2.1; `update_finding` rejects illegal transitions via the extracted `enforce_status_transition` helper (commit `4356bf1f`); locked by `is_valid_transition_matches_lifecycle_diagram` test.

2. **PATCH/CLI can move finding through `triage → in_review → terminal` (`open` / `resolved` / `wont_fix` / `duplicate`).**
   `findings_lifecycle_open_to_resolved_via_triage_and_review` and `findings_lifecycle_open_direct_to_terminal_states` handler tests (commit `4356bf1f`) walk the full happy path; the CLI shares the same handler so the same transitions are reachable via `creator works findings …`.

3. **`list_open_findings_for_chapter` (or successor) matches overlay actionable set: returns rows where status IN ('open', 'triaged'); excludes 'in_review' by default.**
   `list_open_findings_for_chapter` SQL widened to `status IN ('open', 'triaged')` (commit `237eec20`); `list_open_findings_for_chapter_matches_v149_actionable_set` + `list_open_findings_for_chapter_includes_work_level_triaged` DAO tests lock the filter; `actionable_set_includes_triaged_and_excludes_in_review` consumer test (commit `4356bf1f`) verifies end-to-end.

4. **Existing P1 consumer tests still pass with updated filter semantics documented.**
   The 4 pre-existing `findings_consumer` tests pass unchanged (commit `4356bf1f` — 6/6 in the file); the filter semantics are documented in `findings_block.rs` module docstring (V1.49 F6 actionable set section) and `auto_chain.rs::compute_open_findings_block_for_produce` docstring (commit `613ef56e`).

### T0 — Pre-flight findings

**`gitnexus_impact` on `VALID_STATUSES` and `update_finding`** returned `Target not found` for both symbols. The repo's GitNexus index (`11602 symbols`) does not include `nexus-local-db::findings` symbols — the index is stale relative to the V1.49 worktree (the main checkout on `iteration/v1.49` is what GitNexus indexes, but the symbols were not present even there). Per repo `AGENTS.md` fallback rules, call sites were enumerated manually via `grep`:

**`VALID_STATUSES` call sites (3 internal):**
- `validate_finding_enums` (create-time + review-hook validation)
- `update_finding` (patch-time membership check, now followed by transition check)

**`update_finding` call sites (2):**
- `nexus-local-db/src/lib.rs` (re-export)
- `nexus-daemon-runtime/src/api/handlers/findings.rs::update_finding_handler` (the API surface — modified in T2 to map `ConstraintViolation` → 422 `INVALID_TRANSITION`)

**`list_open_findings_for_chapter` call sites (3 production + tests):**
- `nexus-orchestration/src/auto_chain.rs::compute_open_findings_block_for_produce` (server-side consumer — benefits automatically from the widened SQL filter)
- `nexus42/src/commands/creator/run.rs::assemble_open_findings_block` (CLI client-side consumer — uses the Local API with `?status=open`; **NOT updated** — see Residual additions)
- tests in `nexus-local-db` and `nexus-orchestration/tests/findings_consumer.rs`

### Scope summary

**T1 — Migration + `VALID_STATUSES` expansion** (commit `237eec20`)
- New migration `202606170001_extend_findings_status.sql`: documentation marker + idempotent `ANALYZE findings`. SQLite `ALTER TABLE` cannot add `CHECK` to existing tables (R-V139P1-W-1); runtime validation remains sole enforcement.
- `VALID_STATUSES`: expanded to 6 states (`open`, `resolved`, `wont_fix`, `triaged`, `in_review`, `duplicate`).
- New `ACTIONABLE_FINDING_STATUSES = &["open", "triaged"]` (overlay §2.2).
- New `is_valid_status(s)` helper. Originally specified `const fn`; the stable Rust toolchain (1.93) does not yet support `matches!` on `&str` in `const` (rust-lang/rust#143874). Implemented as a regular `fn` with an inline comment documenting the upgrade path.
- New `is_valid_transition(from, to)` state machine per overlay §2.1.
- `update_finding`: now fetches current status and rejects illegal transitions before any write.
- `list_open_findings_for_chapter`: SQL widened from `status = 'open'` to `status IN ('open', 'triaged')`.
- `lib.rs` re-exports updated.
- `.sqlx/` offline cache regenerated (1 query renamed: old `status = 'open'` hash → new `status IN (...)` hash).

**T2 — Transition validation + API surface** (commit `613ef56e`, + `4356bf1f` clippy fixup)
- `errors.rs`: `BadRequest { code: "INVALID_TRANSITION" }` maps to HTTP `422` (per spec §2.1) and is surfaced verbatim in `error_code()`.
- `handlers/findings.rs::update_finding_handler`: catches the DAO's `ConstraintViolation` and remaps to `BadRequest { code: "INVALID_TRANSITION", message: <DAO constraint text> }` so callers see 422 instead of generic 500 `DATABASE_ERROR`. The DAO message describes the rejected `from → to` pair.

**T3 — Consumer filter alignment** (commit `613ef56e`)
- `findings_block.rs`: new `pub const ACTIONABLE_FINDING_STATUSES` re-export mirroring `nexus_local_db::findings::ACTIONABLE_FINDING_STATUSES`; module doc updated to document the V1.49 §2.2 actionable set contract.
- `auto_chain.rs::compute_open_findings_block_for_produce`: docstring updated; no call-site re-filter (DAO is SSOT).

**T4 — Hermetic tests** (commit `4356bf1f`)
- DAO lib tests: +9 (24 total). Covers enum membership, transition table, valid happy path, terminal rejections, unknown-value rejection, actionable-set SQL filter, work-level triaged inclusion.
- Handler tests (`findings_api.rs`): +4 (11 total). Covers happy-path triage→review→resolved, direct-to-terminal, 3 illegal-transition classes returning 422 with `INVALID_TRANSITION`, unknown-value rejection.
- Consumer tests (`findings_consumer.rs`): +2 (6 total). Covers actionable-set inclusion/exclusion through the DAO + builder, and the cross-crate constant mirror invariant.

### Residual additions

One new residual registered at **root** `residual_findings` severity (will be added to `.mstar/status.json` by PM during plan closeout — implementer does not write `status.json` per assignment rules):

- **`R-V149P0-01`** — `medium` — **CLI `creator run` client-side findings fetch still uses `?status=open`, missing the V1.49 `triaged` actionable set.**
  - **Where**: `crates/nexus42/src/commands/creator/run.rs::assemble_open_findings_block` line ~586 builds the path `/v1/local/works/{work_id}/findings?status=open&limit=200`.
  - **Impact**: the daemon-supervised auto-chain path (T3, `auto_chain.rs`) automatically picks up `triaged` findings via the widened DAO SQL. The CLI-driven `creator run stage advance --stage produce` path filters client-side to `status=open` only, so triaged findings do **not** reach the prompt when the human-driven CLI flow is used. This is a behaviour gap vs spec §2.2.
  - **Why deferred**: the CLI fetch path uses a single HTTP query param. Widening it to `{open, triaged}` requires either (a) two HTTP calls merged client-side, (b) extending the Local API to accept `?status=open,triaged`, or (c) removing the filter and filtering client-side. Each option is non-trivial scope beyond T1–T4; per the hard rule "scope strictly T1–T4", this is recorded as a follow-up.
  - **Suggested fix**: extend the Local API `ListFindingsQuery.status` to accept a comma-separated list (or add a new `status_in` param), update the CLI to pass `open,triaged`, and add a CLI integration test. Alternative: introduce a dedicated `/v1/local/works/{work_id}/findings:actionable` endpoint that always returns the actionable set per the DAO constant.

### Risks / follow-ups

- **`is_valid_status` is not `const fn`** — the plan specified `const fn`, but stable Rust 1.93 does not yet support `matches!`/`PartialEq` on `&str` in `const` contexts (rust-lang/rust#143874). Implemented as regular `fn`; promote to `const fn` once stable. No runtime impact.
- **TOCTOU in `update_finding`**: the read-before-write transition check is best-effort single-statement. Under concurrent writes a race is theoretically possible. SQLite serialises writes and the UPDATE scopes to `(creator_id, finding_id)` so practical risk is low; documented in the `update_finding` docstring.
- **Spec deviation on HTTP status**: the assignment T2 said "returns 400 on invalid transition"; the spec §2.1 mandates `422 with stable error code`. The assignment declares the spec as truth source, so **422** was implemented (with stable code `INVALID_TRANSITION`). No residual needed — the implementation matches the spec; the assignment wording appears to be a minor slip.
- **GitNexus index staleness**: `gitnexus_impact` on `VALID_STATUSES`/`update_finding` returned "not found"; the index predates these symbols. Call sites were enumerated via `grep` fallback (see T0 findings above). Recommend PM run `npx gitnexus analyze` to refresh before QC.

### Ready for QC tri-review: yes

- Branch: `feature/v1.49-findings-lifecycle`
- HEAD: `4356bf1f`
- Diff basis: `iteration/v1.49` @ `1fd3a9c4` (3 commits, 4 files changed in core code + 1 migration + `.sqlx` cache rename)
- Worktree intact for QC inspection at `.worktrees/v1.49-findings-lifecycle`
