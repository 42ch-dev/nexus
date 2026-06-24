---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-24-v1.64-local-api-hardening"
verdict: "Request Changes"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-25

## Scope
- plan_id: `2026-06-24-v1.64-local-api-hardening`
- Review range / Diff basis: `c8f93e18..0afa42b2`
- Working branch (verified): `iteration/v1.64`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 31 (diff scope: handlers/works.rs, handlers/findings.rs, handlers/kb.rs, handlers/agent_host.rs, handlers/creators.rs, handlers/memory.rs, handlers/workspaces.rs, api/pagination.rs, api/errors.rs, api/middleware.rs, api/mod.rs; nexus-contracts generated/local_api/{common,works,findings,kb}; packages/nexus-contracts generated/local-api/{common,works,findings,kb}; schemas/local-api/{common,works,findings,kb}; nexus42/src/commands/creator/run.rs; tests/{works_api.rs,findings_api.rs}; tooling/check-wire-drift.sh; tests/schema_drift_detection.rs)
- Commit range (if not identical to Review range line, explain): `c8f93e18..0afa42b2` (matches Review range; HEAD = `0afa42b2` is the status-update commit on top)
- Tools run: `git diff c8f93e18..0afa42b2 --stat`, `cargo clippy -p nexus-contracts -p nexus-daemon-runtime --no-deps -- -D warnings`, `./tooling/check-wire-drift.sh`, `cargo test --test schema_drift_detection -p nexus-contracts`, plus targeted reads of hotspots (handlers + pagination + errors + middleware + schema files)

## Findings

### 🔴 Critical
- _(none)_

### 🟡 Warning

**W-1 (cross-track drift): The F-E1 wire envelope and the UI client parser are inconsistent; structured `code`/`message` are silently lost at the UI boundary.**

This is the single most important architectural issue in Wave 1, and it spans both plans (P0's wire contract + P1's client parser), so it is also recorded in `qc1.md` for the P1 plan.

Evidence trail:

1. The runtime response envelope (defined in `crates/nexus-daemon-runtime/src/api/errors.rs:55-73`, serialized via `to_response_body()` at `errors.rs:248-269`, sent through `IntoResponse` at `errors.rs:271-277`):

   ```json
   {
     "success": false,
     "error": {
       "code": "INVALID_INPUT",
       "message": "...",
       "details": { "...": "..." },
       "request_id": "req_..."   // injected by middleware
     }
   }
   ```

   The `error.request_id` field is injected post-hoc by `attach_request_id` middleware (`api/middleware.rs:111-147`), which assumes the JSON body has an `error` object to mutate. This is **intentional, observed in production tests** (`api/middleware.rs:380-426`), and documented in `schemas/local-api/common/README.md:13` and `crates/nexus-contracts/src/generated/local_api/common/error_response.rs:3-10`.

2. The `error-response.schema.json` schema (`schemas/local-api/common/error-response.schema.json`) models only the **inner** `ErrorResponse` detail (`{ code, message, details? }`). The wire envelope wrapping is not modeled by any schema — it lives only in the handler error type and the middleware. The README explicitly states the wrapping; the convention spec does not.

3. The convention spec `local-api-surface-conventions.md` §3.1 shows the canonical error envelope as a **bare** object at the top level:

   ```json
   {
     "code": "work_not_found",
     "message": "Work not found. Check the Work ID and try again.",
     "details": { "work_id": "..." }
   }
   ```

   This is the shape a reader would expect to find on the wire. It is **misleading**: the actual wire is the nested `{ success, error, request_id? }` envelope. The convention doc never mentions the wrapping. Anyone using §3.1 as a wire-shape reference will write a parser that fails to find `code` at the top level.

4. The Web UI's `BrowserClient.fromBody` (`apps/web/src/lib/nexus/errors.ts:45-53`) reads `parsed.code` and `parsed.message` from the **top level** of the response body. For a daemon-routed error, the top-level `code` is `undefined` (it lives at `body.error.code`), so:

   - `code` falls back to `http_<status>` (e.g. `http_400`).
   - `message` falls back to `Request failed with status <status>`.
   - The structured `details` object is dropped.

   This means **the very UX promise of F-E1 — one parsed `ErrorResponse` shape for unified UI error handling (web-ui.md §4.2, §12.4) — does not hold for any Works or Findings handler that returns `NexusApiError`**, which includes every error path P2 (screens) will exercise on create/patch/get Works and list/create/update/delete Findings.

5. The deferral `R-V164-FE1-ORCH` is documented in `status.json` and accepted at `low` severity, with the closure_note justifying it as "orchestration endpoints are READ-only in the MVP" — but the MVP screen list in `web-ui.md` §6.1 explicitly includes **Sessions, Schedule, Capabilities, Presets** view, all of which go through the orchestration handlers (presets.rs, sessions.rs, schedules.rs) that return ad-hoc `(StatusCode, String)` tuples (verified in `crates/nexus-daemon-runtime/src/api/handlers/orchestration/presets.rs:14,46`; `sessions.rs:21`; `schedules.rs:55,106`). These endpoints will return **bare plain-string error bodies**, not even the wrapped envelope — neither the current `fromBody` parser nor a hypothetical fixed parser can recover a structured code from them. The MVP READ experience for these screens will degrade to `"Request failed with status 500"` with no actionable message. **The deferral rationale understates the MVP impact.**

**Fix (proposed; trivial — should not block merge but must not be silently dropped):**

- `apps/web/src/lib/nexus/errors.ts:45-53`: change `fromBody` to unwrap `body.error` first when present (mirror the daemon runtime envelope). One tiny change; ~6 lines. The doc comment already calls this out as the planned tightening once F-E1 lands — F-E1 *has* landed and the merge already happened.
- `local-api-surface-conventions.md` §3.1: clarify the wire envelope is `{ success: false, error: ErrorResponse, request_id? }` (or remove the bare shape example). Update the §3.2 code table's claim that the wire is `{ code, message, details? }` — or normalize to `ErrorResponse` consistently across docs/schemas.
- `R-V164-FE1-ORCH` closure_note + severity: re-evaluate against the MVP READ screen list. Either bump severity (medium) with explicit MVP READ acknowledgment, or migrate the four orchestration handlers in this iteration. ~1300 LoC of `schedules.rs` is the largest block; sessions/presets are smaller and could be migrated with the Web UI MVP.

Severity mapping for `status.json.residual_findings`: `high` (security/correctness/data → `high`; this affects MVP UX across at least 5 MVP screens; non-blocking for compile/CI but substantive and undecided).

---

### 🟢 Suggestion

**S-1: Three local hand-written `PaginationInfo`/`PaginationEnvelope` structs shadow the canonical `nexus_contracts::PaginationInfo`; future pagination-field changes will need 4-site edits.**

Evidence:

- `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs:87` — local `PaginationInfo { limit: usize, next_cursor, has_more }`.
- `crates/nexus-daemon-runtime/src/api/handlers/kb.rs:71-75` — local `PaginationInfo { limit: usize, next_cursor, has_more }`.
- `crates/nexus-daemon-runtime/src/api/handlers/{creators.rs:49,memory.rs:46,workspaces.rs:49}` — local `PaginationEnvelope { limit: usize, next_cursor, has_more }`.
- `crates/nexus-daemon-runtime/src/api/handlers/{works.rs:293,findings.rs:171}` — canonical `nexus_contracts::PaginationInfo { limit: i64, next_cursor, has_more }`.

`limit` differs by type (`usize` vs `i64`) — purely an internal concern today (both serialize identically), but the `usize` variants can't be re-exported from the schema-driven `nexus-contracts` types without refactoring, and a future schema bump (e.g. `total_remaining: i64`) would need 4 sites of edits instead of 1.

The P0 diff correctly added `has_more` to all three local structs to align with the canonical — that is good drift closure — but the structural duplication remains.

Suggested fix (low-risk, V1.65+): migrate all five peer handlers to `nexus_contracts::PaginationInfo` (i.e. `limit: i64`). Coordinate with F-P3 (works→items rename sweep) which already needs to touch every list endpoint.

**S-2: F-P1 vs F-P3 deferral boundary is clean — confirmed; durable tracking confirmed.**

- `local-api-surface-conventions.md` §2.2 + §4: F-P1 (cursor) closes; F-P3 (items rename) deferred. Both surfaces explicit.
- `list-works-response.schema.json:6` description explicitly says "the `works` -> `items` rename is deferred to F-P3."
- `R-V164-SURF-003` (`F-P3`) lives in `status.json.residual_findings` with `decision: defer`, target V1.65+.
- Compass §5 item #2: P0 implementer keeps `works`; P2 implementer covers via TanStack Query transformer.

Boundary is clean and durably tracked. UI adapter ownership (P-last) is the only soft spot — confirm at P-last dispatch.

**S-3: F-P1 cursor encoding (`v1:<offset>`) is sound for the V1.20 loopback threat model; flagged for future-defense-in-depth by qc2.**

- `api/pagination.rs:19` `CURSOR_PREFIX = "v1:"` plus `offset_page_meta` is correct, opaque, version-prefixed, and offset-bounded.
- `decode_offset_cursor` (`pagination.rs:37-63`) returns the canonical `INVALID_INPUT` (HTTP 400) on malformed tokens, mapping to convention §3.2's coarse code.
- Pattern is intentionally offset-backed rather than id-backed (the docs at `pagination.rs:7-13` explain KB uses id-keyed cursors instead — that inconsistency is intentional and documented).

Future (V1.66+) defense-in-depth if Local API ever leaves pure loopback: cursor tokens should carry a MAC (qc2 already flagged this). Out of scope for V1.64.

**S-4: `cli/creator/run.rs` deserialization updated correctly for the F-P2 new shape (`items, pagination`).**

- `crates/nexus42/src/commands/creator/run.rs:668-684`: hand-written local `FindingsListResponse { items: Vec<Finding> }` is minimal and correct for the single-call site. Inline definition is appropriate here (one-shot CLI deserialization, not a wire contract).
- Drift risk: if `error-response.schema.json` grows new fields, the CLI does not consume `ErrorResponse` anywhere (the CLI uses the raw `nexus_local_db` types and HTTP status codes directly). No cross-handler enum drift on the CLI side.

**S-5: `Works` cursor handler (`list_works` at `handlers/works.rs:569-630`) follows the documented `limit + 1 → has_more` pattern correctly.**

- `offset_page_meta(fetched, limit, offset)` then `records.truncate(limit)` is correct and avoids a separate count query.
- Default `limit = 100`, max `500` (`works.rs:583`). Good defaults.
- `LIMIT 1 / 500` clamp at the DAO boundary is not in this diff (pre-existing); no change needed.

**S-6: Codegen idempotency — confirmed; regenerated artifacts match the schema changes.**

- `crates/nexus-contracts/src/generated/local_api/works/list_works_response.rs`, `.../findings/list_findings_response.rs`, `.../kb/pagination_info.rs`, `.../common/{mod.rs,error_response.rs}` all match the schemas.
- `packages/nexus-contracts/src/generated/local-api/...` mirror.
- Schema drift detection (`schema_drift_detection.rs`) registered for `ErrorResponse` (Strict) and all new/changed ListWorksResponse/ListFindingsResponse. `122 schemas / 121 structs drift-clean` per the closure_note in `status.json`.
- `@42ch/nexus-contracts` bumped `0.4.0 → 0.5.0` (minor for additive, pre-1.0 breaking for Works list shape change). Acceptable per root `AGENTS.md`: "API shapes ... may change without a deprecation period" before 1.0; the Works break ships together with its first consumer (the Web UI).

---

## Source Trace
- Finding ID: W-1 (F-E1 wire envelope mismatch)
- Source Type: manual-reasoning (cross-file) + git-diff
- Source Reference:
  - `crates/nexus-daemon-runtime/src/api/errors.rs:55-73` (runtime envelope definition)
  - `crates/nexus-daemon-runtime/src/api/middleware.rs:111-147` (middleware-injected `error.request_id`)
  - `schemas/local-api/common/error-response.schema.json` (inner schema only)
  - `schemas/local-api/common/README.md:13` (envelope documentation)
  - `.mstar/knowledge/specs/local-api-surface-conventions.md` §3.1 (bare-shape misleading example)
  - `apps/web/src/lib/nexus/errors.ts:45-53` (parser reads top-level `code`/`message`)
  - `crates/nexus-daemon-runtime/src/api/handlers/orchestration/{presets.rs:46,sessions.rs:21,schedules.rs:106}` (ad-hoc `(StatusCode, String)` tuples — not even the envelope)
  - `.mstar/knowledge/specs/web-ui.md` §6.1 (MVP screen list — Sessions, Schedule, Capabilities, Presets all hit orchestration handlers)
- Confidence: High (verified by direct read of every referenced file; runtime envelope serialized form confirmed by tests at `api/middleware.rs:380-426`)

- Finding ID: S-1 (PaginationInfo struct duplication)
- Source Type: git-diff + manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/{agent_host,kb,creators,memory,workspaces,works,findings}.rs`
- Confidence: High

- Finding ID: S-2 (F-P1/F-P3 boundary clean)
- Source Type: doc-rule + manual-reasoning
- Source Reference: `local-api-surface-conventions.md` §2.2 + §4, `list-works-response.schema.json:6`, `status.json.residual_findings[R-V164-SURF-003]`
- Confidence: High

- Finding ID: S-3 (cursor encoding sound)
- Source Type: manual-reasoning + linter
- Source Reference: `api/pagination.rs:7-80`, qc2 finding QC2-W1-001
- Confidence: High

- Finding ID: S-4 (CLI deserialization)
- Source Type: git-diff + manual-reasoning
- Source Reference: `crates/nexus42/src/commands/creator/run.rs:665-684`
- Confidence: High

- Finding ID: S-5 (list_works limit+1 pattern)
- Source Type: manual-reasoning
- Source Reference: `handlers/works.rs:569-630`
- Confidence: High

- Finding ID: S-6 (codegen idempotency)
- Source Type: linter + git-diff
- Source Reference: `./tooling/check-wire-drift.sh` PASS, `cargo test --test schema_drift_detection` 4/4 PASS, `git diff` against generated dirs matches schemas
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 6 |

**Verdict**: Request Changes

Rationale: One unresolved `Warning` (W-1: F-E1 wire envelope mismatch) materially undermines the MVP product promise of unified UI error handling across at least 5 MVP screens. The fix is small (~6 lines in `errors.ts` + one convention doc clarification), and the underlying runtime design is intentional and well-tested, so this is a non-merge-blocking reconciliation rather than a redesign. However, W-1 is shipped as a cross-plan issue and PM should confirm the fix lands in the same Wave (or register it explicitly with `severity: high` and a V1.65+ target).

CI status: `cargo clippy -p nexus-contracts -p nexus-daemon-runtime --no-deps -- -D warnings` PASS; `./tooling/check-wire-drift.sh` PASS; `cargo test --test schema_drift_detection` 4/4 PASS. No CI failures; the Warning is an architectural coherence gap rather than a build/lint/test failure.

Per-finding machine severity (for PM residual registration):
- W-1 → `high` (substantive MVP UX; not data loss; UI still functions for 2xx)
- S-1 → `medium` (technical debt; future migration block)
- S-2 → `nit` (confirmation only; no action)
- S-3 → `low` (future hardening noted)
- S-4 → `nit` (confirmation only)
- S-5 → `nit` (confirmation only)
- S-6 → `low` (codegen discipline preserved; register the 0.4.0 → 0.5.0 npm/Rust bump in tech debt summary if not already)