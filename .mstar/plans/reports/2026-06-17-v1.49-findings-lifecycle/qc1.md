---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-17-v1.49-findings-lifecycle
verdict: Request Changes
generated_at: 2026-06-17T12:00:00Z
review_range: 1fd3a9c4..04608722
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-17T12:00:00Z

## Scope
- plan_id: 2026-06-17-v1.49-findings-lifecycle
- Review range / Diff basis: 1fd3a9c4..04608722
- Working branch (verified): iteration/v1.49
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 11 (migration, DAO, lib re-exports, API errors, API handler, 3 test files, orchestration consumer + docstrings, 1 .sqlx cache rename, completion report)
- Commit range (feature commits): 237eec20..4356bf1f (T1 + T2/T3 + T4); merge commit 04608722 is the integration point
- Tools run: `git diff 1fd3a9c4...04608722 --stat`, `git log`, `Read`/`Grep` on all in-scope files, `SQLX_OFFLINE=true cargo check -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration` (clean, 0 errors/warnings), spec cross-check against `.mstar/knowledge/specs/novel-writing/findings-lifecycle.md`

## Architecture Assessment

### State machine clarity — single SSOT, well-localized

The transition table is a **single SSOT**: `is_valid_transition(from, to)` in `crates/nexus-local-db/src/findings.rs` (lines 157–174). The docstring embeds the lifecycle diagram from `findings-lifecycle.md` §2.1 verbatim, so a developer reading the function sees the spec without context-switching. Adding a new state requires touching exactly: `VALID_STATUSES`, `ACTIONABLE_FINDING_STATUSES` (if actionable), and one `match` arm in `is_valid_transition`. The `is_valid_transition_matches_lifecycle_diagram` test locks every edge against the spec. **This is clean and maintainable.**

### Actionable-set propagation — no drift risk

`ACTIONABLE_FINDING_STATUSES = &["open", "triaged"]` is defined once in the DAO crate (`nexus-local-db::findings`) and re-exported in `nexus-orchestration::findings_block` as a `pub const` alias pointing at the same slice (not a re-declaration). The cross-crate equality is locked by `actionable_finding_statuses_constant_is_mirrored_across_crates`. **Zero drift risk — this is the correct pattern.**

### `enforce_status_transition` helper — clear contract

Private `async fn` in `findings.rs` (lines 645–678), called only by `update_finding`. The contract is explicit: returns `Ok(())` for missing rows (deferring to the UPDATE's `rows_affected = 0` → NotFound path), and `Err(ConstraintViolation)` for illegal transitions. The extracted-helper pattern keeps `update_finding` readable. **Clean.**

### Migration — adequate runtime enforcement

SQLite `ALTER TABLE` cannot add `CHECK` to existing tables (R-V139P1-W-1). The migration is a well-documented no-op DDL marker (`ANALYZE findings`) that records the lifecycle expansion in the `schema_migrations` history. Runtime validation (`VALID_STATUSES` + `is_valid_transition` + `enforce_status_transition`) is the sole enforcement, consistent with the pre-existing pattern. **No Postgres-level enforcement gap concern for this overlay** — the spec targets a local-first SQLite product.

### Tests — no fragile string-matching anti-patterns

- Handler tests assert on `status_code()` and `error_code()` — **not** on full error message strings. This avoids the V1.46 P1 WL-A anti-pattern.
- DAO tests use `constraint.contains("resolved") && constraint.contains("open")` (substring match on status names, not message templates). Reasonably robust.
- No snapshot tests. **Good discipline.**

### `const fn` deviation — well-documented

The `is_valid_status` docstring (lines 130–134) explains the `const fn` → `fn` deviation with the exact upstream issue (`rust-lang/rust#143874`) and the upgrade path. **No action needed.**

### Re-exports — correct and minimal

`lib.rs` adds `is_valid_status`, `is_valid_transition`, `ACTIONABLE_FINDING_STATUSES`, `VALID_STATUSES` to the existing findings re-export block. No surprise surface area. **Clean.**

## Findings

### 🟡 Warning

#### W-1: `INVALID_TRANSITION` error code overloaded for all DAO `ConstraintViolation` subtypes in PATCH handler

**Location**: `crates/nexus-daemon-runtime/src/api/handlers/findings.rs` lines 319–335

**Issue**: The `update_finding_handler` maps **every** `LocalDbError::ConstraintViolation` to `BadRequest { code: "INVALID_TRANSITION", message: constraint }`, regardless of the underlying cause. The DAO emits `ConstraintViolation` for at least four distinct cases on the PATCH path:

1. Illegal lifecycle transition (e.g. `resolved → open`) — the intended target for `INVALID_TRANSITION`
2. Invalid severity value (e.g. `severity: "critical"`) — **not** a transition
3. Invalid `target_executor` value — **not** a transition
4. Unknown status membership failure (e.g. `status: "closed"`) — **not** a transition

The test `findings_lifecycle_rejects_unknown_status_value` (line 565) explicitly documents this uniform remapping: it passes `status: "closed"` and expects `422 INVALID_TRANSITION`, acknowledging "the handler remaps every ConstraintViolation from the DAO uniformly."

**Impact**: The stable public error code `INVALID_TRANSITION` is semantically incorrect for cases 2–4. A CLI/UI client matching on `error.code == "INVALID_TRANSITION"` to surface a lifecycle-specific message ("You can't move a resolved finding back to open") would incorrectly fire for a bad severity patch. The `message` field carries accurate constraint text, but the **code** — which is the stable programmatic contract — is misleading. This compounds as new validated fields are added: any future enum field on the PATCH path inherits `INVALID_TRANSITION` for free, widening the semantic drift with zero guardrail.

**Fix**: Two options (either resolves the issue):

- **(a) Handler-level**: inspect the `constraint` string prefix (the DAO already formats transition errors with `"invalid status transition"` and enum errors with `"invalid severity"` / `"invalid status"` / `"invalid target_executor"`). Map transition errors to `INVALID_TRANSITION` and the rest to a generic `INVALID_INPUT` or a new `INVALID_ENUM_VALUE`.
- **(b) DAO-level (preferred for long-term)**: introduce a dedicated `LocalDbError::IllegalTransition { from, to }` variant so the handler can match it precisely, leaving `ConstraintViolation` for enum-membership failures mapped to `INVALID_INPUT`. This also gives the handler structured `from`/`to` data for richer error `details`.

**Severity rationale**: Warning (not Critical) because the `message` is accurate, pre-1.0 allows breaking changes, and the test documents the behavior. But it affects the public API contract and has no guardrail against further drift — a core maintainability concern.

### 🟢 Suggestion

#### S-1: Self-loop rejection (`from == to`) — document prominently in API surface docs

**Location**: `crates/nexus-local-db/src/findings.rs` line 157 (`is_valid_transition`) and line 752 (`update_finding`)

`is_valid_transition` rejects `from == to`, forcing callers to omit `status` from the patch to refresh `updated_at`. This is well-documented in the DAO docstring and tested (handler test case (c)). However, common PATCH semantics often treat "same value" as a no-op success — a client doing `GET → modify description → PATCH(status: current_status, description: new)` would get a 422. This is a deliberate design choice, not a bug, but it's worth surfacing in the API endpoint documentation (beyond the DAO rustdoc) so CLI/UI consumers don't trip over it. No code change required; consider a note in the handler docstring or the spec §3 API table.

#### S-2: Document the actionable-set scope boundary on untouched query functions

**Location**: `count_open_findings_by_severity` (line 1026) and `list_stale_open_findings` (line 940)

These functions still query `status = 'open'` literally — which is **correct** (stale detection is about unactioned open findings; the severity summary is the "open" bucket specifically, not the actionable set). But a future maintainer seeing `ACTIONABLE_FINDING_STATUSES = { open, triaged }` might wonder why these functions don't use it. A one-line comment on each noting "V1.49 actionable-set widening applies only to the prompt-consumer surface (`list_open_findings_for_chapter`); this function intentionally queries `open` only" would prevent confusion. No behavior change.

#### S-3: `enforce_status_transition` TOCTOU — note the single-statement CAS alternative

**Location**: `crates/nexus-local-db/src/findings.rs` lines 645–678 (`enforce_status_transition`) and line 698 (`update_finding` docstring)

The read-before-write pattern has a theoretical TOCTOU window under concurrent writes. The docstring already documents this and notes SQLite serializes writes (low practical risk). For future hardening, consider noting the single-statement CAS alternative (`UPDATE findings SET status = ?, updated_at = ? WHERE creator_id = ? AND finding_id = ? AND <transition is valid for current status>`) using a SQL `CASE` expression — though the current two-statement approach is adequate for SQLite's serialized write model.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-1 | git-diff + manual-reasoning | `handlers/findings.rs:319-335` `.map_err(match ConstraintViolation → INVALID_TRANSITION)`; DAO constraint strings at `findings.rs:670-677, 714-721, 725-732, 736-743` | High |
| S-1 | manual-reasoning | `findings.rs:157-160` (`from == to` rejection); handler test `findings_lifecycle_rejects_illegal_transitions_with_422` case (c) | High |
| S-2 | manual-reasoning | `findings.rs:952` (`status = 'open'` in stale query), `findings.rs:1035` (`status = 'open'` in count query) | High |
| S-3 | doc-rule | `findings.rs:696-698` (TOCTOU note in docstring) | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

The implementation is architecturally sound — single-SSOT transition table, zero-drift actionable-set propagation, clean helper extraction, well-structured hermetic tests, and a correctly-documented migration no-op. The one blocking finding (W-1) is a public API contract issue: the `INVALID_TRANSITION` error code is overloaded for all DAO `ConstraintViolation` subtypes, making it semantically incorrect for non-transition validation failures (bad severity, bad executor, unknown status membership). The fix is low-cost and should be applied before the API contract solidifies. The three Suggestions are documentation/ergonomics improvements with no behavior change required.
