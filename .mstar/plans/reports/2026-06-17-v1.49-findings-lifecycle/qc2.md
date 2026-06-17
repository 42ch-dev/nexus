---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-17-v1.49-findings-lifecycle
verdict: Approve
generated_at: 2026-06-16T20:20:00Z
review_range: bc8efc8d..c4b4500f
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-17T14:30:00Z

## Scope
- plan_id: 2026-06-17-v1.49-findings-lifecycle
- Review range / Diff basis: 1fd3a9c4..04608722
- Working branch (verified): iteration/v1.49
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 11 (migration, DAO, lib re-exports, API errors, API handler, 3 test files, orchestration consumer + docstrings, 1 .sqlx cache rename, completion report)
- Commit range (feature commits): 237eec20..4356bf1f (T1 + T2/T3 + T4); merge commit 04608722 is the integration point
- Tools run: `git rev-parse --show-toplevel/branch/HEAD`, `git diff 1fd3a9c4...04608722 --stat`, `git log`, `Read`/`Grep` on all in-scope files + baseline pre-P0 handler via `git show 1fd3a9c4:...`, `SQLX_OFFLINE=true cargo check -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration` (clean), cross-check against qc1.md / qc3.md (de-duplication), `grep` for ConstraintViolation emission sites and prompt-flow sites.

## Security / Correctness Assessment

### State machine & transition enforcement (core correctness)
The 6-state lifecycle and `is_valid_transition` table in `findings.rs` (157–174) plus the pre-write `enforce_status_transition` guard (645–678) are the single SSOT. `update_finding` calls the guard before building the dynamic UPDATE. All terminal states have no outbound edges; `duplicate` / `in_review` are reachable only from the documented predecessors. This is correct by construction for single-writer paths.

### Creator-scope authorization (unchanged from V1.48 baseline)
`update_finding_handler` (and siblings) still perform:
1. `read_active_creator_id(...)` (AuthRequired otherwise)
2. `works::get_work(creator_id, work_id)` ownership check (NotFound on mismatch)
3. DAO call with the same `creator_id` (DAO WHERE clauses are creator-scoped)

Pre-P0 baseline (`git show 1fd3a9c4:.../handlers/findings.rs`) had the identical guard. P0 only added the transition check inside the DAO and the `ConstraintViolation` → 422 remap. No new cross-creator write surface, no relaxation of scope for the new states (`duplicate`, `in_review`). Owner-controlled terminal transitions (including "hiding" via `duplicate` or `wont_fix`) are consistent with prior `resolved`/`wont_fix` semantics and are not a privilege-escalation vector.

### SQL filter widening (no injection, no unintended consumer drift in scope)
`list_open_findings_for_chapter` widened to `status IN ('open', 'triaged')` (findings.rs:552). The IN clause is a **static literal tuple** inside a `sqlx::query!` macro with bound parameters for `creator_id`/`work_id`/`chapter`. No user-supplied value is interpolated into the SQL text. The `ACTIONABLE_FINDING_STATUSES` constant is the single source of truth and is re-exported cross-crate without duplication.

The only production consumer inside the P0 diff is `auto_chain::compute_open_findings_block_for_produce` (which intentionally benefits). The CLI consumer gap (`?status=open` only) is pre-registered as R-V149P0-01 and is out of scope for fresh findings here.

### TOCTOU in `enforce_status_transition` (correctness under concurrency)
The guard does a single-column `SELECT status ... WHERE creator_id=? AND finding_id=?` followed by a separate `UPDATE ... SET status=? WHERE creator_id=? AND finding_id=?`. Under concurrent writers both could observe the same `from` value and both could pass `is_valid_transition`, with the second writer winning. SQLite serialises writes and the WHERE is PK-scoped, so a second writer cannot "invent" a state outside the machine; the worst observable outcome is that two legal transitions race and the later one is applied (still a legal edge from the original `from`).

However, the **check-then-act** is not atomic. A stronger invariant-preserving form would be a single-statement compare-and-swap:
```sql
UPDATE findings SET status = ? WHERE creator_id = ? AND finding_id = ? AND status = ?
```
(or a CASE expression encoding the allowed predecessors). The current design relies on SQLite's serialisation + application-level best-effort. This is acceptable for a local-first SQLite product but is a correctness gap versus a true atomic state-machine transition. (See also qc1 S-3 and qc3 S-3 for the perf/observability framing; the security/correctness concern is whether a racy or malicious local caller can force the row into a state that the lifecycle diagram forbids.)

### Error classification & information disclosure (public contract)
The PATCH handler (handlers/findings.rs:328–333) maps **every** `LocalDbError::ConstraintViolation` to:
```rust
NexusApiError::BadRequest { code: "INVALID_TRANSITION", message: constraint }
```
(See errors.rs:172 for the 422 mapping.)

`ConstraintViolation` is emitted for at least four distinct conditions on this path (enumerated via grep on the DAO):
1. Illegal lifecycle transition (the intended case).
2. Invalid `severity` value (pre-transition enum check).
3. Invalid `target_executor` value (pre-transition enum check).
4. Unknown `status` membership (pre-transition enum check; also the "closed" test case).

All four surface as HTTP 422 with the **stable public code `INVALID_TRANSITION`**. The `message` field contains the DAO-generated constraint text, which includes the table name ("findings"), the rejected value, and the phrase "invalid status transition ..." or "invalid severity ...".

From a security/correctness lens:
- Clients that key off `error.code == "INVALID_TRANSITION"` to decide "this was a lifecycle policy violation" will also fire for simple enum typos (bad severity, bad executor, unknown status word). This makes client-side error handling and logging harder to reason about securely.
- The `message` leaks internal implementation phrasing (table name, exact constraint wording) into the public error surface. While the values themselves are attacker-controlled (the client sent them), the surrounding text is server-chosen and not part of any documented public error taxonomy.
- The same 422 code is used for semantically different failures, which can mask distinct attack classes (e.g., probing for valid enum members vs. probing for allowed transitions).

This is the security/correctness sibling of qc1 W-1 (which framed it as API-contract / maintainability drift). The root cause is the single `ConstraintViolation` variant being used for both "enum membership" and "transition legality", with a uniform remap in the handler. No new information is disclosed beyond what the DAO already produced in V1.48, but the P0 change makes the overloading visible on the new PATCH transition path and on the widened status enum.

### LLM / prompt-consumer surface (no new unsanitized flow in delta)
`triaged` rows now reach `compute_open_findings_block_for_produce` → `build_open_findings_block` → `preset.input.open_findings_block` (and thus into outline/draft prompts). The builder applies `truncate_chars` (MAX_BODY_CHARS=400, MAX_TOTAL_BLOCK_CHARS=3200) and renders a controlled Markdown shape. `title`, `description`, `kind`, `rule_suggestion`, and `severity` are taken verbatim from the DB row.

This is the same trust boundary as the V1.48 consumer path (R-V148P0-W1 was on the review-report *parser* side). The V1.49 delta widens the set of rows that flow in, but adds no new sanitization, escaping, or provenance tracking. The truncation is a size bound, not a content sanitizer. This is noted for completeness under the "LLM/Agent boundary" item in the assignment; it does not rise to a new Critical/Warning because the prior surface already carried the same fields and the truncation pattern is unchanged.

### Negative-path / adversarial test coverage
Existing tests (DAO + handler) cover:
- Happy-path transitions (including new states `triaged`/`in_review`/`duplicate`).
- Three classes of illegal transitions (terminal-locked, reverse-edge, self-loop) → 422 INVALID_TRANSITION.
- Unknown enum value ("closed") → 422 INVALID_TRANSITION (explicitly documents the uniform remap).

Gaps (from the 10-item checklist):
- No explicit test for oversized strings, non-UTF-8 bytes, or very long `rule_suggestion`/`description` on the PATCH path (the DAO `normalize_rule_suggestion` cap is tested elsewhere, but not end-to-end through the handler for the new lifecycle states).
- No concurrent-transition test (would require a multi-connection harness to exercise the TOCTOU window).
- No fuzz-style injection strings in the `status` field of a PATCH (the membership check happens before any SQL, so a literal `'; DROP ...` would be rejected as "invalid status '...' " with ConstraintViolation — functionally safe, but not demonstrated in the test corpus for this feature).

The widened `IN` clause itself has no injection surface (static literals + bound params).

### Migration hygiene
File `202606170001_extend_findings_status.sql` is a pure documentation marker + `ANALYZE findings`. No `ALTER TABLE` that touches data, no `CHECK` constraint (impossible on SQLite for existing tables per R-V139P1-W-1), no destructive rewrite of existing rows. Naming and timestamp follow the repo convention. `ANALYZE` is idempotent and cheap on realistic workspace sizes. Acceptable.

## Findings

### 🟡 Warning

#### W-1: Uniform `ConstraintViolation → INVALID_TRANSITION` remap leaks implementation detail and collapses distinct failure classes (security / client-reasoning impact)
**Location**: `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:328–333` (and the matching DAO emission sites in `findings.rs:714–743` for enum checks + `670` for transition).

**Issue**: Every `LocalDbError::ConstraintViolation` (whether "invalid severity 'extreme'", "invalid target_executor 'foo'", "invalid status 'closed'", or "invalid status transition 'resolved' → 'open'") is surfaced to callers as HTTP 422 with the stable public code `"INVALID_TRANSITION"` and the raw DAO constraint text in `message`.

See qc1 W-1 for the maintainability / API-contract framing (the test `findings_lifecycle_rejects_unknown_status_value` explicitly documents the uniform remap). From the security/correctness lens:

- A client that treats `error.code == "INVALID_TRANSITION"` as "lifecycle policy violation; show the user a friendly 'you cannot move a resolved finding'" will also fire for a simple bad `severity` or `target_executor` value. This makes it harder for clients to implement safe, least-surprise error handling.
- The `message` field contains server-chosen phrasing that includes the internal table name ("findings") and the exact constraint wording. While the rejected value is user-supplied, the surrounding diagnostic text is an implementation detail now visible on the public error surface.
- The same 422 code is used for enum-membership failures and transition-legality failures. This collapses two distinct classes (bad data vs. policy violation) and can mask probing attempts (an attacker sending crafted enum values vs. crafted transitions receives identical top-level signals).

**Impact**: Pre-1.0 the contract is allowed to change, and the `message` is still accurate. However, the overloading was introduced (or at least made newly visible) by the V1.49 lifecycle work, and it affects the public error taxonomy that CLI and future UI clients will key off. No guardrail prevents the same pattern from being applied to future validated fields on the PATCH path.

**Fix**: Same options as qc1 W-1, with security emphasis:
- Handler-level: inspect the `constraint` prefix and map only true transition strings to `INVALID_TRANSITION`; map pure enum violations to a distinct code (e.g. `INVALID_INPUT` or `INVALID_ENUM_VALUE`) with a stable public taxonomy.
- DAO-level (preferred): split `ConstraintViolation` into `IllegalTransition { from, to }` vs. `InvalidEnum { field, value, allowed }`. This gives the handler (and logs) precise classification without string prefix heuristics, and allows the public error surface to carry structured `details` for the rejected field.

**Severity rationale**: Warning (not Critical) because there is no direct privilege escalation, the values are attacker-controlled anyway, and the `message` remains human-readable. It is a classification / information-disclosure / client-reasoning correctness issue on the public contract.

### 🟢 Suggestion

#### S-1: TOCTOU window in `enforce_status_transition` — single-statement CAS would give a stronger state-machine invariant
**Location**: `crates/nexus-local-db/src/findings.rs:645–678` (`enforce_status_transition`) and 752–754 (call site in `update_finding`).

The read-before-write is documented as "best-effort single-statement" with the SQLite serialisation caveat. From a correctness lens, a concurrent local writer (or a compromised in-process caller) could observe a legal `from` and both attempt a legal successor; the later write wins. The row cannot end up in a state the machine forbids from the *original* `from`, but the observed history can differ from the sequential intent.

A single-statement form:
```sql
UPDATE findings SET status = ?, updated_at = ?
WHERE creator_id = ? AND finding_id = ? AND status = ?
```
(or a CASE encoding the allowed predecessors) would make the check atomic with the write. Current design is defensible for local SQLite; stronger form would be a one-line improvement for any future multi-writer scenario or when the DAO is used from multiple processes.

Do not duplicate qc3 S-3 (perf angle) or qc1 S-3 (documentation note); this is the invariant-preservation angle.

#### S-2: Negative-path / adversarial coverage for PATCH surface is positive-only plus explicit illegal edges
**Location**: `crates/nexus-daemon-runtime/tests/findings_api.rs` (the V1.49 lifecycle tests 437–578) and DAO tests in `findings.rs`.

Existing coverage is strong for the state-machine edges (happy paths + three rejection classes + unknown enum value). Missing from the 10-item checklist:
- Explicit oversized / non-UTF-8 / boundary payloads on `status`/`severity`/`rule_suggestion` through the handler (the DAO caps are tested in isolation).
- Concurrent PATCH races exercising the TOCTOU window (requires multi-connection harness).
- Fuzz-style injection strings in the `status` field (functionally rejected early by membership check, but not demonstrated for this feature).

Recommendation: add at least one test that sends a clearly malformed status string containing SQL-meta characters and asserts it is rejected as `INVALID_TRANSITION` (or a future `INVALID_INPUT`) *before* any SQL is built. This documents the "no injection surface" claim for auditors.

#### S-3: `duplicate` and `in_review` reachability and "hiding" semantics should be called out in API docs / error messages
**Location**: `is_valid_transition` (findings.rs:157–174) and handler docstring.

Current machine:
- `open` / `triaged` / `in_review` can all reach `duplicate`.
- Once in any terminal state (`resolved`, `wont_fix`, `duplicate`), no further outbound transitions (including back to `open`).
- `in_review` is a "held" state that can only advance to terminal.

An owner can move their own finding to `duplicate` or `in_review` and thereby exclude it from the actionable set (`open` | `triaged`) and from most CLI/consumer surfaces. This is by design (duplicate resolution, master-review hand-off), but it is a powerful "hide from prompt" lever that any creator can apply to their own data. No cross-creator abuse is possible (creator scope is enforced), but the semantics should be documented so that callers understand that `duplicate` is a terminal sink, not a "parked for later" state, and that `in_review` is the intended holding pen for master review.

Cross-reference the lifecycle diagram in the spec; a one-line note in the PATCH handler rustdoc or in the public error message for terminal-locked transitions would help.

#### S-4: R-V149P0-01 (CLI `?status=open` gap) remains the only pre-registered residual; no new high-impact security residual required from this review
The implementer already recorded the CLI consumer not picking up `triaged` rows. qc3 S-6 cross-references it. From the security lens this is a **completeness** gap (the daemon-supervised path sees the full actionable set; the human-driven CLI path does not), not a new injection or authorization hole. It is correctly left as a tracked residual rather than a fresh finding.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-1 | git-diff + manual-reasoning + grep on ConstraintViolation sites | `handlers/findings.rs:328-333` (uniform remap); `findings.rs:670,714-743` (four emission sites); `errors.rs:172` (422 mapping); pre-P0 baseline via `git show 1fd3a9c4` | High |
| S-1 | docstring + code structure | `findings.rs:696-698` (TOCTOU note) + 645-678 (SELECT then UPDATE) + 752-754 (call under status patch) | High |
| S-2 | test inspection | `findings_api.rs:437-578` (lifecycle tests) + DAO tests (transition table, unknown value); absence of concurrent / oversized / injection-string cases | High |
| S-3 | state-machine inspection + docstring | `findings.rs:157-174` (`is_valid_transition`); 166 (`duplicate` reachable from open/triaged/in_review); terminal arms have no outbound edges | High |
| S-4 | completion report + cross-check | R-V149P0-01 already registered; no new security residual needed | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

The core state machine, creator-scoped authorization, and widened SQL filter are sound. No new injection surface, no cross-creator write, and no relaxation of the pre-P0 permission model. The single Warning (W-1) is the security/correctness sibling of qc1 W-1: the uniform remapping of every `ConstraintViolation` (enum membership *and* transition legality) to the stable public code `INVALID_TRANSITION`, with raw DAO constraint text in the message. This collapses distinct failure classes for clients and leaks a small amount of implementation phrasing on the public error surface. Fixing the classification (handler inspection or DAO variant split) would resolve both the maintainability concern (qc1) and the security/client-reasoning concern (this report).

The four Suggestions are documentation, test-coverage, and future-hardening items. None are blocking on their own.

All CI-equivalent checks performed in-scope (`cargo check` on the three crates) were clean. No implementation files were modified during this review.

## Revalidation

**Re-review scope**: Targeted W-1 fix wave (DAO-level error split) per consolidated qc-consolidated.md §W-1 and fix-w1-completion.md. Diff range `bc8efc8d..c4b4500f` (single fix commit `7da35dd5` + completion report `c9f10af6` + merge `c4b4500f`; equivalent to `git diff bc8efc8d...c4b4500f`). Files in scope: `crates/nexus-local-db/src/error.rs`, `crates/nexus-local-db/src/findings.rs`, `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`, `crates/nexus-daemon-runtime/src/api/errors.rs`, `crates/nexus-daemon-runtime/tests/findings_api.rs`.

**Re-review date**: 2026-06-16

**Re-review focus (qc2 security/correctness lens, per assignment)**:
- Information disclosure: `message` no longer contains internal table name "findings" or raw DAO constraint phrasing. Messages are now structured: `"invalid status transition '{from}' → '{to}'"` for `IllegalTransition`; `"invalid {field} value '{value}'; allowed: ..."` for `InvalidEnum`.
- Error classification: distinct stable codes — `INVALID_TRANSITION` (422) for lifecycle violations vs. `INVALID_INPUT` (422) for enum-membership failures. Verified via `NexusApiError::error_code()` and `status_code()` (both → 422). Clients can now key off `error.code`.
- Structured details: the new variants carry typed `from`/`to`/`field`/`value`/`allowed`. These are formatted into the public `message` and emitted via `tracing::warn!` with structured fields (`creator_id`, `finding_id`, the error payload). No top-level `error.details` object (consistent with V1.48 P1 baseline error contract). For this local-first pre-1.0 surface, the formatted message + structured logs are acceptable; a future structured `details` field would be a non-breaking enhancement.
- `tracing::warn!` fields: include `creator_id`, `finding_id`, `from`/`to` or `field`/`value`. Sufficient for investigating a malicious or buggy client without leaking secrets (no PII or credentials in scope).
- Permission model: unchanged. `update_finding_handler` still performs `read_active_creator_id` + `works::get_work` ownership check + creator-scoped DAO WHERE. The W-1 fix only touched error emission paths inside the same guard.
- `InvalidEnum` allowed list: `allowed` is `&'static [&'static str]` listing valid members (e.g. status values, severity levels). For findings `status`/`severity`/`target_executor` this is intentional public contract (helps clients) and not a secret. No security concern.
- Negative-path test (qc2 S-2): `findings_lifecycle_rejects_sql_injection_style_status` sends `status: "'; DROP TABLE findings; --"`, asserts exactly 422 + `INVALID_INPUT` (not 500), then re-queries via `get_finding_handler` to prove the row and table are intact post-rejection. Passes.
- New variants reachability: `IllegalTransition` is emitted only by `enforce_status_transition` (called exclusively from `update_finding` on the PATCH path). `InvalidEnum` is emitted only by the three inline enum checks inside `update_finding` (severity / status membership / target_executor). Other DAO paths (create, shared validators, works.rs, etc.) continue to use the retained `ConstraintViolation`. The match arms in `update_finding_handler` are the only consumer of the new variants. No surprising cross-path behaviour.
- Pre-existing clippy `--all` failure: explicitly out of scope per assignment and fix-w1-completion.md (R-V149P0-03 defer to V1.50). Verified clippy-neutral on the changed crates.

**Evidence**:
- Code inspection of the 5 changed files (error.rs, findings.rs, handlers/findings.rs, errors.rs, findings_api.rs).
- Test execution: `cargo test -p nexus-daemon-runtime --test findings_api -- findings_lifecycle_distinguishes_invalid_transition_from_invalid_enum findings_lifecycle_rejects_sql_injection_style_status --exact` → both pass (2/2).
- Original W-1 (uniform `ConstraintViolation → INVALID_TRANSITION`) is resolved: no string-prefix sniffing remains; distinct codes and clean messages.
- All 9 security/correctness checklist items from the assignment are satisfied with no new Critical or Warning in the re-review scope.

**New findings in re-review scope**: 0 Critical, 0 Warning.

**Updated verdict**: Approve (0 Critical + 0 Warning in the W-1 fix re-review scope; R-V149P0-02 is addressed). The original review's 1 Warning and 4 Suggestions remain as historical record in the sections above; they are either fixed by this wave (W-1) or tracked as non-blocking follow-ups / pre-existing residuals (R-V149P0-01). No new residuals raised by this re-review.
