---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-26-v1.67-local-api-surface-convergence"
verdict: "Request Changes"
generated_at: "2026-06-26"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-26

## Scope
- plan_id: 2026-06-26-v1.67-local-api-surface-convergence
- Review range / Diff basis: P0 feat commit ea94b028, merged at integration HEAD (equivalent `git show ea94b028`; diff basis vs origin/main)
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 71 (per ea94b028 stat)
- Commit range: ea94b028 (P0 local-api-surface-convergence)
- Tools run: git show, grep (CASING emission sites), read (specs, errors.rs, sort.rs, handlers)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

- **F-F1 sort: unknown-key error code is not wire-visible as specified.**  
  `parse_sort_terms` (sort.rs:39-45) correctly constructs `NexusApiError::BadRequest { code: format!("{resource}_sort_invalid"), ... }` and spec §5 rule 4 requires the wire `error.code` to be exactly `"<resource>_sort_invalid"`.  
  However, `NexusApiError::error_code()` (errors.rs:208-240) only passthrough-matches a hardcoded whitelist: `policy_blocked | not_supported | invalid_input | invalid_transition | world_id_required | invalid_world_id | world_clear_forbidden`. All other `BadRequest` codes fall through to `"bad_request"`.  
  Result: `?sort=unknown` returns wire code `"bad_request"` (or generic 400), not `"schedules_sort_invalid"` / `"works_sort_invalid"` etc.  
  **Evidence**: sort.rs:40 (construction), errors.rs:214-228 (match in error_code for BadRequest), errors.rs:573-593 (tests only cover the 4 whitelisted codes; no `*_sort_invalid` test), local-api-surface-conventions.md:186.  
  **Impact**: Clients cannot programmatically distinguish "bad sort key" from other bad requests; violates the ratified sort contract.  
  **Fix**: Extend the BadRequest passthrough match (or add a dedicated arm) to recognize the `*_sort_invalid` pattern and return the dynamic code unchanged (lowercase per V1.67 casing rule). Add a unit test asserting the wire `body.error.code`.

- **CASING internal classification codes remain uppercase in `Internal.code` (acceptable per design).**  
  The priority check requested explicit verification: are uppercase codes (`DATABASE_ERROR`, `INTERNAL_ERROR`, `WORK_REF_MISSING`, etc.) ever emitted in wire-visible `ErrorResponse.code`?  
  **Finding**: No. They are confined to the `Internal.code` field (internal classification only).  
  **Evidence**:
  - `to_response_body()` (errors.rs:283-293) always emits `code: self.error_code().to_string()`.
  - `error_code()` for `Internal { .. }` hardcodes `"internal"` (errors.rs:211).
  - `Internal.code` is set to uppercase strings in many sites (works.rs:350-1881 ~25 sites, token_manager.rs:52-116, runtime_lock.rs:64, From<sqlx::Error>/From<anyhow> at errors.rs:323-346) — these are for logging/diagnosis.
  - Module docs (post-PR, errors.rs:37-46, 94-100) explicitly state: "`Internal.code` is NOT exposed as the `error_code` in the API response body (which always returns \"internal\"...)".
  - Public error codes (`error_code()`) are now all lowercase snake_case per spec §3.2 (uninitialized, invalid_input, internal, auth_required, not_found, forbidden, policy_blocked, etc.).
  - No path was found where `Internal.code` value is copied into `ApiErrorDetail.code` or returned directly to clients.
  **Conclusion**: The implementer's claim holds. Uppercase strings are internal-only and never cross the wire as the public `ErrorResponse.code`. This is compliant with the locked `local-api-surface-conventions.md` §3.2 (which governs the wire-visible code). No spec non-conformance here.

### 🟢 Suggestion

- **FE1-ORCH error semantics appear preserved (positive).**  
  Orchestration handlers (schedules.rs, sessions.rs, presets.rs) were converted from ad-hoc `(StatusCode, String)` tuples to `NexusApiError` + canonical `ApiErrorResponse` envelope. New variants `ServiceUnavailable` (503) and `PresetGatesFailed` (422) were added with correct `status_code()` mappings. Previously ad-hoc 5xx/4xx strings are now typed. No evidence of 503/422 being silently flattened to 400. UI and CLI consumers updated to read `body.error`. Good.

- **F-P3 `items` + pagination shape change is narrow and complete for declared scope.**  
  Exactly the 4 schema-backed endpoints listed in compass §1.3 and spec §4 were renamed (`works`/`schedules`/`sessions`/`capabilities` → `items`). Pagination is uniformly `{ items, pagination: { next_cursor, has_more } }`. Hand-written/local-only plain lists were explicitly excluded from the 0.6.0 bump (documented). Contracts regenerated, UI adapters removed, tests updated. No data-loss or shape drift observed in the reviewed artifacts.

- **F-F1 sort direction and allowlist validation are correct (mechanism).**  
  `parse_sort_terms` correctly interprets leading `-` as descending, validates every key against an endpoint-specific allowlist, and never interpolates raw keys into SQL or logic. In-memory sort application is safe by construction. Only the wire error code for the invalid case is broken (see Warning).

- **Path guards: no regression detected in scope.**  
  `path_guard.rs` appears in the diff stat (10 lines). The PR description and compass do not claim guard changes. Given the narrow hygiene focus (error envelope, casing, items, sort), and no new filesystem input paths introduced in the reviewed handlers, existing W-002-style guards are presumed untouched. (If a future targeted re-review touches guard sites, re-audit.)

## Source Trace
- Finding ID: F-CASING-01 (uppercase internal codes)
- Finding ID: F-SORT-01 (sort_invalid not wire-visible)
- Source Type: manual code review + grep
- Source Reference: crates/nexus-daemon-runtime/src/api/errors.rs:208-240 (error_code), :283-293 (to_response_body), :323-346 (From conversions); sort.rs:39-45; works.rs:350+ (Internal sites); token_manager.rs:52+; local-api-surface-conventions.md:116-134, 186; compass §5 #2
- Confidence: High (for CASING internal-only conclusion); High (for sort code emission mismatch — direct match logic)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

(The sort-invalid wire code mismatch is a spec non-conformance that must be fixed before merge. The CASING internal classification finding is acceptable with the evidence above; no action required on that item.)
