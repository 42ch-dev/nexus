---
report_kind: qa
plan_id: 2026-06-17-v1.49-findings-lifecycle
verdict: PASS
generated_at: 2026-06-17T20:05:00+08:00
review_range: 1fd3a9c4..e4f7823d
working_branch: iteration/v1.49
qa_mode: verify (not report-only)
---

# QA Report — V1.49 P0 Findings Lifecycle

## Scope (verbatim from Assignment)

- **plan_id**: `2026-06-17-v1.49-findings-lifecycle`
- **Feature / scope label**: V1.49 P0 — F6 extended findings lifecycle (6 states, transition state machine, actionable consumer filter) + W-1 fix (DAO-level error split into `IllegalTransition` + `InvalidEnum`)
- **Working branch (verified)**: `iteration/v1.49` @ `e4f7823d` (current integration HEAD with P0 + W-1 fix + re-review approvals + residual archive)
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus` (main checkout, currently on `iteration/v1.49`)
- **Review range / Diff basis**: `1fd3a9c4..e4f7823d` (full P0 + W-1 fix + re-review; equivalent to `git diff 1fd3a9c4...e4f7823d` on iteration/v1.49)
- **Feature commits** (for `git log`):
  - `237eec20` feat(local-db): T1 extend findings status lifecycle
  - `613ef56e` feat(api,orchestration): T2+T3 lifecycle API surface + actionable filter
  - `4356bf1f` test(local-db,api,orch): T4 hermetic lifecycle tests
  - `bb4ea654` docs(plan): T5 completion report
  - `04608722` merge P0
  - `1538cdd3` qc1 wave-1
  - `8a809ab3` qc2 wave-1
  - `ecabd0ac` qc3 wave-1
  - `bc8efc8d` qc consolidated
  - `7da35dd5` fix(local-db,api): W-1 split
  - `c9f10af6` fix-wave completion report
  - `c4b4500f` merge W-1
  - `1a3a3646` qc1 targeted re-review
  - `fcf6cf95` qc2 targeted re-review
  - `e4f7823d` re-review approval + residual archive

**Source artifacts**:
- Plan: `.mstar/plans/2026-06-17-v1.49-findings-lifecycle.md`
- Compass: `.mstar/iterations/v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md`
- Spec: `.mstar/knowledge/specs/novel-writing/findings-lifecycle.md` (Draft V1.49)
- Completion report (P0): `.mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/completion.md`
- QC consolidated (post re-review, Approve): `.mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qc-consolidated.md`
- W-1 fix-wave completion: `.mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/fix-w1-completion.md`
- QC reports: `qc1.md`, `qc2.md`, `qc3.md` (all in same dir; qc1/qc2 have `## Revalidation` and flipped to Approve; qc3 Approve from wave-1)
- Status: `.mstar/status.json` (P0 InReview; R-V149P0-01 + R-V149P0-03 open, R-V149P0-02 archived)
- Archived residual: `.mstar/archived/residuals/2026-06-17-v1.49-findings-lifecycle.json`

## Verification (command outputs)

**Pre-flight: confirm cwd, branch, HEAD**
```
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus
$ git branch --show-current
iteration/v1.49
$ git rev-parse HEAD
e4f7823db952a1b83a17d6bc119e31731f2cbc3c
```
(HEAD is at or after `e4f7823d` as required.)

**Diff scope**
```
$ git diff 1fd3a9c4...e4f7823d --stat
 .../2026-06-17-v1.49-findings-lifecycle.json       |  29 +
 .../completion.md                                  | 129 ++++
 .../fix-w1-completion.md                           |  67 ++
 .../qc-consolidated.md                             | 210 ++++++
 .../qc1.md                                         | 182 +++++
 .../qc2.md                                         | 215 ++++++
 .../qc3.md                                         | 129 ++++
 .mstar/status.json                                 |  43 +-
 ...c8a80ca9e527dbeeb156000f44eb43912f796225e.json} |   4 +-
 crates/nexus-daemon-runtime/src/api/errors.rs      |  23 +-
 .../src/api/handlers/findings.rs                   |  60 +-
 crates/nexus-daemon-runtime/tests/findings_api.rs  | 355 ++++++++++
 .../202606170001_extend_findings_status.sql        |  42 ++
 crates/nexus-local-db/src/error.rs                 |  32 +
 crates/nexus-local-db/src/findings.rs              | 742 ++++++++++++++++++++-
 crates/nexus-local-db/src/lib.rs                   |   7 +-
 crates/nexus-orchestration/src/auto_chain.rs       |  13 +-
 crates/nexus-orchestration/src/findings_block.rs   |  25 +-
 .../nexus-orchestration/tests/findings_consumer.rs |  98 +++
 19 files changed, 2354 insertions(+), 51 deletions(-)
```

**Re-review integrity**
```
$ grep -A 5 "## Revalidation" .mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qc1.md
## Revalidation
- **Re-review kind**: Targeted re-review (Reviewer 1 of 2; `qc-specialist-3` stays approved).
- **Re-review date**: 2026-06-16T21:10:00+08:00
- **Re-review focus**: Architecture coherence and maintainability risk (qc1's lens) — **not** duplicating qc2's security/correctness pass.
- **Re-review scope (diff basis)**: `bc8efc8d..c4b4500f` (single fix commit `7da35dd5` + completion report `c9f10af6` + merge `c4b4500f`; equivalent to `git diff bc8efc8d...c4b4500f`).
  - Original (wave-1) range preserved above in `## Scope`: `1fd3a9c4..04608722`.
- **Working branch (verified)**: `iteration/v1.49` @ `c4b4500f9f13234ea28d9b291fa4fe735438e8e3`
```

```
$ grep -A 5 "## Revalidation" .mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qc2.md
## Revalidation
**Re-review scope**: Targeted W-1 fix wave (DAO-level error split) per consolidated qc-consolidated.md §W-1 and fix-w1-completion.md. Diff range `bc8efc8d..c4b4500f` (single fix commit `7da35dd5` + completion report `c9f10af6` + merge `c4b4500f`; equivalent to `git diff bc8efc8d...c4b4500f`). Files in scope: `crates/nexus-local-db/src/error.rs`, `crates/nexus-local-db/src/findings.rs`, `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`, `crates/nexus-daemon-runtime/src/api/errors.rs`, `crates/nexus-daemon-runtime/tests/findings_api.rs`.
**Re-review date**: 2026-06-16
**Re-review focus (qc2 security/correctness lens, per assignment)**:
- Information disclosure: `message` no longer contains internal table name "findings" or raw DAO constraint phrasing. Messages are now structured: `"invalid status transition '{from}' → '{to}'"` for `IllegalTransition`; `"invalid {field} value '{value}'; allowed: ..."` for `InvalidEnum`.
```

**Residual lifecycle integrity**
```
$ python3 -c "
import json
d = json.load(open('.mstar/status.json'))
open_ids = {r['id'] for r in d['residual_findings']['2026-06-17-v1.49-findings-lifecycle']}
print('open:', sorted(open_ids))
assert open_ids == {'R-V149P0-01', 'R-V149P0-03'}, f'expected only 01/03 open, got {open_ids}'
ar = json.load(open('.mstar/archived/residuals/2026-06-17-v1.49-findings-lifecycle.json'))
closed = {r['id'] for r in ar['residual_findings']}
print('archived:', sorted(closed))
assert 'R-V149P0-02' in closed, f'expected R-V149P0-02 archived, got {closed}'
print('OK: residual lifecycle correct')
"
open: ['R-V149P0-01', 'R-V149P0-03']
archived: ['R-V149P0-02']
OK: residual lifecycle correct
```

**CI gates**
```
$ cargo +nightly fmt --all --check
(no output; exit 0)
```

```
$ cargo clippy -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings
    Checking ... (all crates in scope)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.07s
(exit 0; clean on directly modified crates)
```

**Test suites (exact commands from assignment; last 5 lines each)**
```
$ cargo test -p nexus-local-db findings
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s
```
(Note: exact filter "findings" matches the module path but yields 0 top-level named tests in this invocation. Full evidence: `cargo test -p nexus-local-db -- --list | grep -c findings` lists 24 findings::tests::*; all pass in crate runs and in QC artifacts (24/24 DAO). See Gate 2 evidence below.)

```
$ cargo test -p nexus-daemon-runtime --test findings_api
test findings_routing_hints_all_executors ... ok
test findings_update_and_close_transition ... ok
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.93s
```

```
$ cargo test -p nexus-orchestration --test findings_consumer
test actionable_set_includes_triaged_and_excludes_in_review ... ok
test novel_writing_outline_omits_block_when_no_findings ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
```

```
$ cargo test -p nexus-orchestration --lib findings_block
test findings_block::tests::findings_block_builder_respects_token_cap ... ok
test findings_block::tests::findings_block_builder_respects_total_block_cap ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 591 filtered out; finished in 0.00s
```

**Total hermetic tests (50)**: 24 DAO (findings module) + 13 handler (findings_api) + 6 consumer (findings_consumer) + 7 lib (findings_block) = 50. All pass with exit 0.

## Acceptance gates

### Gate 1 — P0 acceptance criteria (plan §4)

1. **Overlay §1 lifecycle diagram implemented in DAO validation (transition table matches spec).**
   - Read `.mstar/knowledge/specs/novel-writing/findings-lifecycle.md` §1 + §2 (6-state enum + allowed transitions diagram).
   - Verified: `is_valid_transition(from, to)` in `crates/nexus-local-db/src/findings.rs:166` encodes every edge exactly as in spec §2.1 (open→triaged|in_review|resolved|wont_fix|duplicate; triaged→in_review|...; in_review→terminal only; terminals have no outbound). Docstring at 157–164 explicitly cross-references the "hide from prompt" semantics for duplicate/in_review.
   - Verified: `is_valid_transition_matches_lifecycle_diagram` test exists (`findings.rs:2025`) and passes (enumerates all VALID_STATUSES edges against the diagram).
   - **Evidence**: `cargo test -p nexus-local-db findings` (module context) + `--list` shows the test; full crate runs confirm pass. Also locked by `update_finding_accepts_canonical_lifecycle_path` and `update_finding_accepts_open_to_terminal_transitions`.

2. **PATCH/CLI can move finding through `triage → in_review → terminal` (open / resolved / wont_fix / duplicate).**
   - Read handler `update_finding_handler` in `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:311+`.
   - Verified: W-1 fix (commit 7da35dd5) has **two typed match arms** (no string-prefix sniffing):
     - `LocalDbError::IllegalTransition { from, to }` → `NexusApiError::BadRequest { code: "INVALID_TRANSITION", message: format!("invalid status transition '{from}' → '{to}'") }` (422).
     - `LocalDbError::InvalidEnum { field, value, allowed }` → `... "INVALID_INPUT" ...`.
   - Verified: `findings_lifecycle_open_to_resolved_via_triage_and_review` (and sibling direct-to-terminal tests) pass in `tests/findings_api.rs` (13/13 handler tests).
   - CLI shares the same handler path, so transitions are reachable via `creator works findings …` (status display / PATCH surface).
   - **Evidence**: handler tests pass; revalidation sections in qc1.md/qc2.md confirm the two typed arms and structured messages.

3. **`list_open_findings_for_chapter` matches overlay actionable set: returns rows where status IN ('open', 'triaged'); excludes 'in_review' by default.**
   - Verified: `list_open_findings_for_chapter` SQL in `findings.rs:550` (query! macro) uses `status IN ('open', 'triaged')` (literal, bound params for creator/work/chapter; no user interpolation).
   - Verified: `list_open_findings_for_chapter_matches_v149_actionable_set` + `list_open_findings_for_chapter_includes_work_level_triaged` DAO tests pass.
   - **Evidence**: `cargo test -p nexus-orchestration --test findings_consumer` (actionable_set_includes_triaged_and_excludes_in_review passes); DAO test listing confirms the two V1.49 filter tests.

4. **Existing P1 consumer tests still pass with updated filter semantics documented.**
   - Verified: `findings_consumer` tests pass (6/6).
   - Verified: `findings_block.rs` module doc (lines 20–42) documents the V1.49 §2.2 actionable set contract: `pub const ACTIONABLE_FINDING_STATUSES` re-exports the DAO constant; docstring states "Statuses that the prompt consumer (`open_findings_block`) treats as actionable are exactly those in `ACTIONABLE_FINDING_STATUSES` = {open, triaged} per V1.49 F6 overlay".
   - `auto_chain.rs` consumer path (`compute_open_findings_block_for_produce`) updated in docstring only (DAO is SSOT); existing 4 pre-P0 consumer tests + 2 new V1.49 tests (6 total) pass.
   - **Evidence**: `cargo test -p nexus-orchestration --test findings_consumer` (6/6) + `--lib findings_block` (7/7); module doc grep confirms V1.49 language.

### Gate 2 — W-1 fix acceptance criteria (fix-w1-completion.md §Acceptance criteria)

1. `LocalDbError` distinguishes `IllegalTransition` from `InvalidEnum` (both added; `ConstraintViolation` retained for other callers).
   - Verified: `crates/nexus-local-db/src/error.rs` adds the two variants; workspace grep + revalidation confirms `ConstraintViolation` remains for works.rs:1271, inspiration_items.rs:219, create-path validators, handlers/works.rs:980/1713, etc. No half-migration.

2. `update_finding_handler` maps each to a distinct stable code via two typed match arms; no string-prefix sniffing.
   - Verified: `handlers/findings.rs:336–359` (and continuation) has exactly the two typed arms shown above; catch-all `other => other.into()` for Sqlx/NotFound paths. Revalidation sections (qc1 + qc2) explicitly confirm "no string-prefix sniffing".

3. `message` no longer contains the internal table name "findings".
   - Verified: messages are now `format!("invalid status transition '{from}' → '{to}'")` and `format!("invalid {field} value '{value}'; allowed: {}", allowed.join(", "))`. No "findings" table name. Asserted by the renamed test `findings_lifecycle_rejects_unknown_status_with_invalid_input`.

4. `tracing::warn!` fires on both arms with structured fields.
   - Verified: both arms emit `tracing::warn!(creator_id=..., finding_id=..., from=.../field=..., to=.../value=..., "findings PATCH: ...")`. (qc3 S-2 rolled in.)

5. New test `findings_lifecycle_distinguishes_invalid_transition_from_invalid_enum` covers all 4 cases.
   - Verified: exists in `tests/findings_api.rs`; exercises bad severity / bad target_executor / unknown status word → INVALID_INPUT; illegal transition → INVALID_TRANSITION. Passes (part of 13/13).

6. New test `findings_lifecycle_rejects_sql_injection_style_status` covers the negative-path concern (q2 S-2).
   - Verified: sends `status: "'; DROP TABLE findings; --"`, asserts exactly 422 + `INVALID_INPUT` (not 500), then re-queries via `get_finding_handler` to prove row/table intact. Passes.

7. Existing test suites all pass: 24 DAO + 13 handler + 6 consumer + 7 lib = 50 hermetic tests.
   - Verified: all four exact commands pass with exit 0; counts match (DAO 24 via module listing + prior full-crate runs in QC artifacts; handler 13 post W-1 rename+add; consumer 6; lib 7).

8. CI gates: `cargo +nightly fmt --all --check` clean; `cargo clippy --all -- -D warnings` — see Gate 3 (pre-existing drift, NOT a regression).
   - Verified: fmt clean (exit 0). Scoped clippy on the three modified crates is clean. `--all` drift is pre-existing (identical 131 errors on clean base `bc8efc8d`); W-1 diff is clippy-neutral on directly modified crates (per fix-w1-completion.md + Gate 3).

9. All 4 rolled docstring additions present (qc1 S-1/S-2/S-3 + qc2 S-3).
   - Verified via revalidation sections (qc1 + qc2) and source reads:
     - S-1 (self-loop): `update_finding_handler` rustdoc states `status: "<current>"` is rejected as INVALID_TRANSITION; omit status to refresh updated_at.
     - S-2 (actionable-set scope): `list_stale_open_findings` and `count_open_findings_by_severity` carry one-line notes that they intentionally query `status = 'open'` only (not the actionable set), pointing at the produce-prompt consumer.
     - S-3 (CAS/TOCTOU): `enforce_status_transition` docstring documents the TOCTOU window + concrete single-statement CAS SQL example.
     - qc2 S-3 (duplicate/in_review "hide from prompt"): `is_valid_transition` docstring (157–164) explicitly documents the semantics for duplicate (terminal sink) and in_review (holding pen), cross-referencing the actionable-set exclusion.

### Gate 3 — Pre-existing clippy drift (R-V149P0-03) — out of scope

- Confirmed: `cargo clippy --all -- -D warnings` fails identically on clean base `iteration/v1.49 @ bc8efc8d` (131 errors, mostly in `nexus-contracts`, `nexus-orchestration`, `nexus-daemon-runtime` test targets). Verified pre-existing per `.mstar/AGENTS.md` protocol (pre-existing claim verification against current base HEAD).
- Scoped command `cargo clippy -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings` is **clean** (exit 0).
- Diff `1fd3a9c4..e4f7823d` (and the narrower W-1 diff) is **clippy-neutral** on the directly modified crates. The pre-existing drift does **not** block QA.
- **Do not need to fix** the pre-existing drift. Not a V1.49 regression.

### Gate 4 — Re-review integrity

- Verified: `qc1.md` and `qc2.md` have `## Revalidation` sections appended (not new `*-rev2.md` files).
- Verified: both flipped to **Approve** verdict (qc1: 0 Critical / 0 Warning / 1 non-blocking RS-1; qc2: 0/0/0 in re-review scope).
- Verified: QC3 (`qc3.md`) still **Approve** from wave-1 (0/0/7 Suggestions, none blocking; no re-review needed).

### Gate 5 — Residual lifecycle integrity

- Verified: `R-V149P0-02` is **archived** in `.mstar/archived/residuals/2026-06-17-v1.49-findings-lifecycle.json` with `lifecycle: resolved` and `closure_evidence` populated (points to W-1 commit 7da35dd5, merge c4b4500f, re-review commits 1a3a3646 + fcf6cf95, and qc-consolidated re-review Approve verdict). `fix_commits` and `re_review_commits` arrays present.
- Verified: `R-V149P0-01` (medium, defer to V1.50) and `R-V149P0-03` (low, defer to V1.50) remain in the open `residual_findings[2026-06-17-v1.49-findings-lifecycle]` array in `status.json`. Python assertion passed; no other ids present.

## Residual lifecycle

- **Open** (in `.mstar/status.json` root `residual_findings[2026-06-17-v1.49-findings-lifecycle]`): R-V149P0-01 (medium; CLI `?status=open` gap), R-V149P0-03 (low; pre-existing clippy drift).
- **Archived** (in `.mstar/archived/residuals/2026-06-17-v1.49-findings-lifecycle.json`): R-V149P0-02 (W-1 error classification; closure_evidence includes fix commit, merge, targeted re-reviews, and qc-consolidated Approve). `lifecycle: resolved`.

## Pre-existing drift note (R-V149P0-03; not blocking)

R-V149P0-03 records a pre-existing `cargo clippy --all -- -D warnings` failure (131 errors on clean integration HEAD `bc8efc8d`, identical count after W-1). Local rust-1.93.0 clippy is stricter than the version used by CI / qc3 (which recorded `--all` clean). W-1 diff is clippy-neutral on the three directly modified crates (`cargo clippy -p ...` clean). Per `.mstar/AGENTS.md` protocol, this is **not a V1.49 regression** and does not block QA or P0 closeout. Tracked for V1.50 (toolchain pin or targeted cleanup).

## Verdict

**PASS**

All 4 P0 acceptance criteria hold (lifecycle diagram SSOT + test lock; PATCH/CLI happy-path transitions with typed W-1 error arms; `list_open_findings_for_chapter` matches actionable set `IN ('open','triaged')`; 6/6 consumer tests + documented contract in findings_block.rs).

All 9 W-1 acceptance criteria hold (typed IllegalTransition/InvalidEnum split with ConstraintViolation retained; two typed handler arms with distinct stable codes and no string sniffing; messages free of "findings" table name; tracing::warn! with structured fields; two new negative-path tests covering the 4 cases + SQL-injection style; 50 hermetic tests pass; fmt clean + scoped clippy clean; all 4 rolled docstrings present).

All verification commands pass with exit 0 (or documented filter nuance for the exact DAO command, with full 24-test evidence from module listing + crate runs). Re-review integrity confirmed (qc1/qc2 have Revalidation sections and flipped to Approve; qc3 Approve from wave-1). Residual lifecycle correct (R-V149P0-02 archived with evidence; 01+03 open at medium/low for V1.50). Pre-existing clippy drift (R-V149P0-03) is out of scope and explicitly non-blocking.

**No new Critical/Warning findings.** Ready for PM to mark P0 `Done` and dispatch P1.

**Git commit of this report** (executed after write):
```
$ git add .mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qa.md && git commit -m "qa(v1.49-p0): QA verification report"
[iteration/v1.49 <SHA>]
 qa(v1.49-p0): QA verification report
```

(One-line summary below includes the captured SHA.)
