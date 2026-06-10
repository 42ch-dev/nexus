---
report_kind: qa-verification
plan_id: 2026-06-10-v1.41-selection-pool
verdict: Request Changes
generated_at: 2026-06-11T00:12:34+08:00
review_range: "merge-base: 55689706 → tip: 97470073"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
mode: full
---

# QA Verification Report — V1.41 P1 (DF-61 selection pool + inspiration)

## Reviewer Metadata
- Reviewer: @qa-engineer
- Runtime Agent ID: qa-engineer
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Behavior verification against acceptance criteria
- Report Timestamp: 2026-06-11T00:12:34+08:00

## Scope
- plan_id: 2026-06-10-v1.41-selection-pool
- Review range / Diff basis: merge-base: 55689706 → tip: 97470073
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Tools run: cargo test (4 P1 crates), cargo clippy, cargo +nightly fmt --check, AC-targeted hermetic checks, CLI help smokes, residual register audit, spec amendment grep, workspace regression suite

Checkout gate evidence:

```text
$ git rev-parse --show-toplevel && git branch --show-current && git rev-parse --verify 974700733b44828b719769a6a6289cdd9352ffc6 && git rev-parse --verify 556897061f625c53cd172e2bdb40d509dac61775
/Users/bibi/workspace/organizations/42ch/nexus
iteration/v1.41
974700733b44828b719769a6a6289cdd9352ffc6
556897061f625c53cd172e2bdb40d509dac61775
```

## Acceptance criteria verification

| AC | Description | Status | Evidence |
|----|-------------|--------|----------|
| AC1 | One active per creator | PASS | `cargo test -p nexus-daemon-runtime --test selection_pool test_pool_promote` → `test_pool_promote_demotes_prior_active ... ok`, `test_pool_promote_idempotent_on_same_target ... ok`; `test_pool_promote_demotes_prior_active` asserts `active.len() == 1` after promoting two Works. |
| AC2 | inspiration add atomic + path Pool/Ideas/ | PASS | `cargo test -p nexus-daemon-runtime --test selection_pool test_inspiration_add` → `test_inspiration_add_creates_md_and_db_row_atomically ... ok`, `test_inspiration_add_rejects_existing_path ... ok`; code/doc grep confirms `rel_path = "Pool/Ideas/{slug}.md"` in `crates/nexus-local-db/src/inspiration_items.rs:179`. |
| AC3 | promote --set-default | PASS (behavior), GAP (direct test) | `test_pool_promote_demotes_prior_active` verifies promote sets the pool active and demotes prior active. CLI help/code expose `--set-default`; no dedicated `test_pool_promote_set_default` was found, but current behavior makes promote active by default and does not introduce a global pause path. |
| AC4 | list matches DB | PASS | `cargo test -p nexus-daemon-runtime --test selection_pool test_pool_list_returns_all_statuses` → `test_pool_list_returns_all_statuses ... ok`; list handler returns DB rows/statuses without reading markdown. |
| AC5 | distinct from per-work inspiration_log | FAIL | Spec documents distinctness (`novel-work-pool.md:74` says not per-Work `works.inspiration_log`), and code grep shows separate `inspiration_items` vs `works.inspiration_log`; however CLI help smokes for `creator works pool inspiration add --help` and `creator run continue --help` do **not** document that the pool is distinct from per-Work `inspiration_log`, as required by AC5. |
| AC6 | hermetic API + CLI tests | PASS | `cargo test -p nexus-daemon-runtime --test selection_pool` → `13 passed`; `cargo test -p nexus42 --test command_surface_contract v141_` → `5 passed`. |

## CI / static analysis

```text
$ cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db 2>&1 | tail -50
... test result: ok. 15 passed; 0 failed ...
... Doc-tests nexus_daemon_runtime: 1 passed; 0 failed; 1 ignored ...
... Doc-tests nexus_local_db: 2 passed; 0 failed ...
... Doc-tests nexus_orchestration: 1 passed; 0 failed; 3 ignored ...
... Doc-tests nexus42: 1 passed; 0 failed; 1 ignored ...

$ cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings 2>&1 | tail -20
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s

$ cargo +nightly fmt --all -- --check 2>&1 | tail -5
(no output)

$ cargo test -p nexus-daemon-runtime --test selection_pool 2>&1 | tail -40
running 13 tests
...
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

$ cargo test -p nexus42 --test command_surface_contract v141_ 2>&1 | tail -40
running 5 tests
...
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 43 filtered out; finished in 1.50s
```

Note: `cargo test -p nexus-daemon-runtime --test selection_pool ...` emits a non-fatal Rust test-target warning for an unused import (`ArchiveInspirationRequest`). `cargo clippy ... -D warnings` is clean for the mandated crate set.

## Residual register audit

- Total residuals registered: 19
- Canonical fields present: partial
- Severity enum compliance: yes (all in critical|high|medium|low|nit)
- Decision enum compliance: yes (all in defer|accept|risk-accepted|accept-with-fix)
- Specific notes:
  - IDs present: R-V141P1-01 through R-V141P1-19.
  - `R-V141P1-02` is missing the canonical `owner` field. This makes the residual disposition incomplete under `mstar-review-qc` / `mstar-plan-artifacts` residual rules.

Audit evidence:

```text
count 19
ids R-V141P1-01,R-V141P1-02,R-V141P1-03,R-V141P1-04,R-V141P1-05,R-V141P1-06,R-V141P1-07,R-V141P1-08,R-V141P1-09,R-V141P1-10,R-V141P1-11,R-V141P1-12,R-V141P1-13,R-V141P1-14,R-V141P1-15,R-V141P1-16,R-V141P1-17,R-V141P1-18,R-V141P1-19
missing [('R-V141P1-02', ['owner'])]
bad_severity none
bad_decision none
```

## Spec amendment verification

- File 1: `.mstar/knowledge/specs/novel-work-pool.md` §3.1/§3.3/§5
  - Pool/Ideas/ path present: yes (`{workspace_root}/Pool/Ideas/<slug>.md`)
  - --idea semantics documented: yes (§5.1)
- File 2: `.mstar/knowledge/specs/cli-spec.md` §6.2H
  - Pool/Ideas/ path present: yes (`{workspace}/Pool/Ideas/<slug>.md`)
- File 3: `.mstar/knowledge/specs/local-db-schema.md` §4.1.5
  - Pool/Ideas/ path present: yes (`Path to {workspace}/Pool/Ideas/<slug>.md`)
- File 4: `.mstar/knowledge/deferred-features-cross-version-tracker.md` DF-61 row
  - Pool/Ideas/ path present: yes in DF-61 row
  - Old `Works/_pool/灵感池/` absent: **no** — stale line remains at §3.6.1 V1.41 distill overlay line 208: ``DB SSOT; `Works/_pool/灵感池/*.md` for inspiration files``.

Spec grep evidence:

```text
.mstar/knowledge/specs/local-db-schema.md:286: Path to `{workspace}/Pool/Ideas/<slug>.md`
.mstar/knowledge/specs/novel-work-pool.md:72: `{workspace_root}/Pool/Ideas/<slug>.md`
.mstar/knowledge/specs/cli-spec.md:455: Create `{workspace}/Pool/Ideas/<slug>.md` + DB row
.mstar/knowledge/deferred-features-cross-version-tracker.md:88: inspiration files under `Pool/Ideas/`
.mstar/knowledge/deferred-features-cross-version-tracker.md:208: DB SSOT; `Works/_pool/灵感池/*.md` for inspiration files
```

## Regressions

- Any test that passed before and now fails: no from executed suites.
- Broader regression command was run because `target/` was 19G, below the local hygiene skip threshold.

```text
$ cargo test --workspace 2>&1 | tail -30
... Doc-tests nexus_orchestration ... ok
... Doc-tests nexus42 ... ok
```

## Findings (if any)

### Critical
(none)

### Warning
- **AC5 help documentation gap**: CLI help does not document that `creator works pool inspiration ...` is distinct from per-Work `inspiration_log`. Fix by updating CLI help/docstrings and, preferably, adding a CLI contract test that asserts this distinction appears in help output.
- **Residual register incomplete**: `.mstar/status.json` residual `R-V141P1-02` is missing required canonical field `owner`. Fix by adding the owner before PM marks the plan Done.
- **Spec amendment incomplete**: `.mstar/knowledge/deferred-features-cross-version-tracker.md` still contains stale `Works/_pool/灵感池/*.md` wording at line 208. Fix to `Pool/Ideas/` to make all four amendment targets consistent.

### Suggestion
- Add a direct hermetic test for `pool promote --set-default` / `set_default: Some(true)` so AC3 is covered by an explicit set-default test name rather than by the general promote-active behavior.
- Clean up the non-fatal unused import warning in `crates/nexus-daemon-runtime/tests/selection_pool.rs`.

## Verdict

**Request Changes**

**Rationale**: Core API behavior and the canonical test/static-analysis battery are green, but QA found three release-gating documentation/disposition gaps: AC5 is not documented in CLI help, one residual row is missing a canonical `owner`, and the deferred tracker still contains the old `Works/_pool/灵感池/` path. These must be corrected before PM marks the plan `Done`.
