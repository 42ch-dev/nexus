## Completion Report v2 — P0 fix wave (W-1)

- plan_id: 2026-06-17-v1.49-findings-lifecycle
- owner: @fullstack-dev
- Working branch used: fix/v1.49-p0-w1-error-classification
- Worktree path: .worktrees/v1.49-p0-w1-fix
- Base: iteration/v1.49 @ bc8efc8d16fcda231dd2e3979f73fe08c010c737 ("harness(v1.49-p0): QC consolidated — verdict Request Changes, R-V149P0-02 registered")
- Commits:
  - `7da35dd5` — fix(local-db,api): W-1 split PATCH ConstraintViolation into typed IllegalTransition/InvalidEnum

### Summary of change

DAO-level error split (the preferred fix per qc1 + qc2 consensus). `LocalDbError::ConstraintViolation { table, constraint }` is **retained** because it has many other callers (`works.rs`, `inspiration_items.rs`, `create_finding`/`validate_finding_enums`, `normalize_rule_suggestion`, `FindingKind::validate`, `handlers/works.rs`). Two typed variants were **added alongside** it and emitted only on the findings PATCH path:

- `IllegalTransition { from, to }` — emitted by `enforce_status_transition`.
- `InvalidEnum { field: &'static str, value: String, allowed: &'static [&'static str] }` — emitted by the three inline enum checks inside `update_finding` (severity / status membership / target_executor).

The PATCH handler maps them to distinct stable public codes with structured messages, plus a `tracing::warn!` on each arm. No string-prefix sniffing. No table name "findings" leaks on the public surface.

### Cargo verification

Last lines of each gate command (run in the worktree with `SQLX_OFFLINE=true`):

```
# cargo +nightly fmt --all --check   →  rc=0 (no diff)
# cargo test -p nexus-local-db findings  (lib unittests binary)
test result: ok. 24 passed; 0 failed; 0 ignored; 193 filtered out; finished in 0.94s
# cargo test -p nexus-daemon-runtime --test findings_api
test result: ok. 13 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.79s
# cargo test -p nexus-orchestration --test findings_consumer
test result: ok. 6 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.13s
# cargo test -p nexus-orchestration --lib findings_block
test result: ok. 7 passed; 0 failed; 0 ignored; 591 filtered out; finished in 0.00s
```

Clippy note (see Risks): `cargo clippy --all -- -D warnings` fails **pre-existing** on the clean base `iteration/v1.49 @ bc8efc8d` — in `nexus-contracts/src/local/schedule/http.rs:236` (`redundant_clone`) and in `nexus-orchestration`/`nexus-daemon-runtime` test targets (`doc_markdown`, `unwrap_used`). Verified identical on the clean base (no `rust-toolchain.toml`; local clippy `rust-1.93.0` is stricter than the clippy CI/qc3 used when qc3 recorded `--all` clean). My diff is **clippy-neutral**: `cargo clippy -p nexus-daemon-runtime --all-targets -- -D warnings` reports **131 errors in both the worktree and the clean base** (identical). The two directly-modified crates introduce zero new lints.

### Acceptance criteria

1. `LocalDbError` distinguishes `IllegalTransition` from `InvalidEnum` — both added alongside the retained `ConstraintViolation` (other-callers check via workspace grep: works.rs, inspiration_items.rs, create-path validators, handlers/works.rs) — `crates/nexus-local-db/src/error.rs` @ `7da35dd5`.
2. `update_finding_handler` maps each to a distinct stable code (`INVALID_TRANSITION` vs `INVALID_INPUT`) via two typed match arms; no string sniffing — `crates/nexus-daemon-runtime/src/api/handlers/findings.rs::update_finding_handler` @ `7da35dd5`.
3. `message` no longer contains the internal table name "findings"; structured as `invalid status transition '{from}' → '{to}'` / `invalid {field} value '{value}'; allowed: {…}` — handler @ `7da35dd5`; asserted by `findings_lifecycle_rejects_unknown_status_with_invalid_input`.
4. `tracing::warn!` fires on both arms with structured fields (`creator_id`, `finding_id`, `from`/`to` or `field`/`value`) — handler @ `7da35dd5` (qc3 S-2).
5. New test `findings_lifecycle_distinguishes_invalid_transition_from_invalid_enum` covers all 4 cases (bad severity, bad target_executor, unknown status word → `INVALID_INPUT`; illegal transition → `INVALID_TRANSITION`) — `tests/findings_api.rs` @ `7da35dd5` (passes).
6. New test `findings_lifecycle_rejects_sql_injection_style_status` sends `status: "'; DROP TABLE findings; --"`, asserts `422 INVALID_INPUT`, and re-queries the row to prove the table is intact — `tests/findings_api.rs` @ `7da35dd5` (passes; qc2 S-2).
7. Existing suites pass: 11→13 handler tests (rename + 2 new), 24 DAO tests (2 updated in place, count unchanged), 6 consumer tests, 7 lib tests = **50 hermetic tests, all green**.
8. CI gates: `cargo +nightly fmt --all --check` clean (rc=0); `cargo clippy --all -- -D warnings` is **pre-existing-broken on the clean base** (toolchain drift, not W-1) — my diff is clippy-neutral (131 == 131). See Risks.
9. All four rolled docstring additions present: qc1 S-1 (self-loop note on `update_finding_handler`), qc1 S-2 (`count_open_findings_by_severity` + `list_stale_open_findings` actionable-set scope), qc1 S-3 + qc3 S-3 (`enforce_status_transition` CAS alternative), qc2 S-3 (`is_valid_transition` duplicate/in_review "hide from prompt" semantics) — `crates/nexus-local-db/src/findings.rs` + handler @ `7da35dd5`.

### Residual closure

- **R-V149P0-02** fixed in this wave. **DO NOT archive** — PM owns closure after targeted re-review (qc-specialist + qc-specialist-2) passes.

### Risks / follow-ups

- **Pre-existing clippy `--all` failure (NOT W-1):** `cargo clippy --all -- -D warnings` fails identically on the clean base `iteration/v1.49 @ bc8efc8d` due to local clippy `rust-1.93.0` being stricter than the clippy used by CI / qc3 (which recorded `--all` clean). Reproduced in `crates/nexus-contracts/src/local/schedule/http.rs:236` (`redundant_clone`), plus `nexus-orchestration` (`doc_markdown`) and `nexus-daemon-runtime` test targets. Verified pre-existing per `.mstar/AGENTS.md` "pre-existing claim verification protocol" (fails on current base HEAD). **Out of W-1 scope (no piggyback).** Recommend a separate small cleanup task (or pinning a `rust-toolchain.toml`) tracked by PM — not blocking the W-1 targeted re-review, which should evaluate the 5 changed files against the W-1 acceptance criteria.
- **`gitnexus` index is stale** relative to V1.49; `gitnexus_detect_changes(compare → iteration/v1.49)` returned null. Blast radius was established manually via workspace-wide `ConstraintViolation` grep (all callers enumerated) + clippy-neutrality check.
- R-V149P0-01 (CLI `?status=open` gap) remains untouched and deferred to V1.50, as registered.

### Ready for targeted QC re-review

**yes** — reviewers: @qc-specialist + @qc-specialist-2 (N=2, one message). @qc-specialist-3 stays approved (raised no blocking finding). Each reviewer updates the **same** `qc1.md` / `qc2.md` (add a `## Revalidation` section, update verdict) — no `*-rev2.md`.

- Review cwd / Worktree path: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.49-p0-w1-fix`
- Working branch: `fix/v1.49-p0-w1-error-classification`
- plan_id: `2026-06-17-v1.49-findings-lifecycle`
- Review range / Diff basis: `iteration/v1.49..fix/v1.49-p0-w1-error-classification` (single commit `7da35dd5`; equivalent to `git diff bc8efc8d...7da35dd5`).
