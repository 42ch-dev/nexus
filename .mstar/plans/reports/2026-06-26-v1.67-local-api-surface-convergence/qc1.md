---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-26-v1.67-local-api-surface-convergence"
verdict: "Request Changes"
generated_at: "2026-06-26"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist (Reviewer #1)
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence + maintainability risk
- Report Timestamp: 2026-06-26

## Scope

- plan_id: `2026-06-26-v1.67-local-api-surface-convergence`
- Review range / Diff basis: P0 feat commit `ea94b028` ("feat(local-api)!: converge V1.67 local API surface"), merged into integration HEAD. Equivalent: `git show ea94b028` for the P0 diff; diff basis vs `origin/main`. Scope = FE1-ORCH error-envelope + CASING + F-P3 `items` + F-F1 sort + UI/CLI adaptation.
- Working branch (verified): `iteration/v1.67`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Commit range: `ea94b028..b3498361` (P0 feat commit → integration HEAD containing P0 + P-sec + Wave 1 verify)
- Files reviewed: 71 changed files (+1,269/-1,030); focused on the 19 P0-architected paths: errors.rs, sort.rs, schedules.rs, sessions.rs, presets.rs, capabilities.rs, works.rs (list path), the 4 generated response/query modules, the 4 schema JSONs, host_tool_handlers.rs, capability_registry.rs, auth_middleware.rs, middleware.rs, chapters.rs, findings.rs, nexus42/src/commands/daemon/schedule.rs, apps/web/src/api/queries.ts, apps/web/src/lib/nexus/{adapters,types,browser-client,query-keys}.ts
- Tools run:
  - `git rev-parse --show-toplevel` / `git branch --show-current` / `git log --oneline -10` — branch + HEAD alignment
  - `git show ea94b028 --stat` — diff scope
  - `rg '(StatusCode, String)|HttpStatus' crates/nexus-daemon-runtime/src` — ad-hoc tuple sweep
  - `rg '\(StatusCode,' crates/nexus-daemon-runtime/src` — residual tuple sweep
  - `cargo +nightly fmt --all --check` — formatting gate
  - `cargo clippy -p nexus-daemon-runtime -p nexus-contracts -- -D warnings` — lint gate
  - `cargo test -p nexus-daemon-runtime` + `cargo test --workspace` — test gate (506 + all workspace)
  - `cargo test -p nexus-contracts --test schema_drift_detection` — drift gate
  - `pnpm --filter @42ch/nexus-contracts run build` + `pnpm --filter web run typecheck` + `pnpm --filter web run test` — web gate
  - Spot-read of all four orchestration handlers + the new `sort.rs` + `errors.rs` + the works.rs list path + all four affected schemas

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

- **W-1 (architecture: second DTO source for `ListWorks*`/`WorkSummary`)** — `crates/nexus-daemon-runtime/src/api/handlers/works.rs` keeps hand-written `ListWorksQuery` (line 184), `ListWorksResponse` (line 194), and `WorkSummary` (line 201) that duplicate the **generated** types under `crates/nexus-contracts/src/generated/local_api/works/{list_works_query,list_works_response,work_summary}.rs`. Field-for-field identical today (so wire output is correct), but the F-P3 spec explicitly promotes `works` to a schema-backed endpoint (renamed → `items`, JSON Schema `schemas/local-api/works/list-works-response.schema.json` updated, generated DTO exists, contracts version bumped to 0.6.0). All three sibling endpoints (`schedules`, `sessions`, `capabilities`) consume the generated contracts (`nexus_contracts::local::schedule::http::ListSchedulesResponse`, `nexus_contracts::local::orchestration::http::ListSessionsResponse`, `nexus_contracts::local::orchestration::http::ListCapabilitiesResponse`). The works handler is the only one that doesn't, and it violates the AGENTS.md invariant: `Contract types: shares generated types from crates/nexus-contracts. Do NOT hand-write duplicate DTOs.` A future schema change (e.g. adding `archived_at` to `WorkSummary`) will not break the works.rs handler but will silently desync the Local API wire from `apps/web`'s `@42ch/nexus-contracts@0.6.0` types.
  - **Fix:** Migrate `crates/nexus-daemon-runtime/src/api/handlers/works.rs` to import `nexus_contracts::local_api::works::{ListWorksQuery, ListWorksResponse, WorkSummary}` (or the `local::works::http` re-export path used by `local::schedule::http`). Delete the three local `pub struct`s. Reconcile any fields the local types carry that the generated DTO does not (the only candidate is `completion_locked_at: Option<String>`, which is already present in both).
  - **Evidence:** `crates/nexus-daemon-runtime/src/api/handlers/works.rs:184-211` (local definitions); `crates/nexus-contracts/src/generated/local_api/works/{list_works_query,list_works_response,work_summary}.rs` (generated, structurally identical); `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:36-46`, `sessions.rs:14-18`, `capabilities.rs:11-14` (use generated). The other three handlers were migrated; works.rs was missed.

- **W-2 (test coverage: F-F1 sort grammar lacks direct tests)** — `parse_sort_terms` (the single-source sort parser at `crates/nexus-daemon-runtime/src/api/sort.rs:17`) has no `#[cfg(test)] mod tests` block; the per-endpoint `compare_*_summary` closures (works.rs:555, schedules.rs:697, sessions.rs:123, capabilities.rs:47) are also not directly unit-tested. Integration tests in `crates/nexus-daemon-runtime/tests/` contain no `?sort=` query string and no assertion of `<resource>_sort_invalid`. The plan AC7 says "Regression tests cover error-envelope mapping, casing, `items` shape, sort." — envelope, casing, and `items` are covered, but **sort is not**.
  - **Concrete gaps:** (a) grammar with empty input, (b) trailing/leading comma, (c) consecutive commas `,,`, (d) `-` alone (no key), (e) unknown key returns `<resource>_sort_invalid` with HTTP 400 + canonical envelope, (f) descending `-key` is honored, (g) multi-key precedence (first key wins on non-equal). The `parse_sort_terms` function is 50 lines and pure — trivially unit-testable. The compare closures are 17–20 lines each and pure.
  - **Fix:** Add `#[cfg(test)] mod tests` to `crates/nexus-daemon-runtime/src/api/sort.rs` covering the seven cases above. Optionally add one integration test per list endpoint that asserts an unsupported key yields 400 + canonical envelope with `<resource>_sort_invalid`. Tracking under residual `R-V167P0-SORT-COVERAGE` is acceptable; full closure before P-last.

- **W-3 (maintainability: error-variant sprawl — `ServiceUnavailable` + `PresetGatesFailed` are new variants without a usage-coverage test)** — `crates/nexus-daemon-runtime/src/api/errors.rs` adds two new variants: `ServiceUnavailable { message }` (HTTP 503) and `PresetGatesFailed { details: serde_json::Value }` (HTTP 422). The inline tests (`service_unavailable_maps_to_503`, `preset_gates_failed_maps_to_422`) cover `status_code()` and `error_code()` correctly, but **no integration test exercises the handler→response path** that produces these — i.e. a route handler returning `NexusApiError::service_unavailable(...)` or `NexusApiError::preset_gates_failed(&failure)` end-to-end, including the `IntoResponse` boundary producing the canonical envelope with `code: "service_unavailable"` / `code: "preset_gates_failed"`.
  - **Why it matters:** the FE1-ORCH sweep introduced these variants precisely to unblock supervisor/engine-not-configured 503s and preset-gate-failure 422s in orchestration handlers. The architect verdict (§5 #1 LOCKED) said "minimal new error support for 503/422 needed." Without an integration-level assertion that the wire body matches the canonical envelope shape (`success: false, error: { code, message, details?, request_id? }`), a future variant addition or a `to_response_body` regression could silently break the contract.
  - **Fix:** Add one integration test in `crates/nexus-daemon-runtime/tests/` per variant: (a) call a supervisor-dependent route when the supervisor is `None`, assert `body["success"] == false && body["error"]["code"] == "service_unavailable" && status == 503`; (b) call `add_schedule` with a preset whose gates fail, assert `body["error"]["code"] == "preset_gates_failed" && body["error"]["details"]["preset_id"] == "novel-writing" && status == 422`. Both can be derived from `fl_e_schedule_api.rs` fixtures.

### 🟢 Suggestion

- **S-1 (refactor: triplicate `compare_*` sort closures)** — `compare_work_record` (works.rs:555), `compare_schedule_summary` (schedules.rs:697), `compare_session_summary` (sessions.rs:123), and the inline closure in capabilities.rs:47 share the same shape: `for (key, ascending) in terms { let ord = match key.as_str() { <key arms> => <cmp>, _ => Ordering::Equal }; let ord = if *ascending { ord } else { ord.reverse() }; if ord != Ordering::Equal { return ord; } }`. The only per-endpoint difference is the match arms. Extracting a generic `compare_by_keys<T, F>(a: &T, b: &T, terms: &[(String, bool)], cmp_fn: F) -> Ordering` helper (or a macro) in `sort.rs` would centralize the dispatch + remove ~50 lines of triplicated code. Not blocking — duplication is small, regular, and tested through integration paths.

- **S-2 (naming: disambiguate `nexus_contracts::local::orchestration::http::NexusListSessionsResponse` from `…sessions::ListSessionsResponse`)** — `crates/nexus-contracts/src/local/orchestration/http.rs:640` still defines `NexusListSessionsResponse { sessions: Vec<NexusSessionInfo> }`. This is the **ACP SDK mirror type** (mirrors the official SDK `ListSessionsResponse` for cross-process ACP wire), not the Local API list. It happens to live near the Local API modules (`nexus_contracts::local::orchestration::sessions::ListSessionsResponse` with `items`), but they serve different crates/purposes. Without a one-line `//! Note: this is the ACP SDK mirror; the Local API sessions list uses …sessions::ListSessionsResponse { items }.` doc comment, future readers may misattribute the `sessions` key as a wire leak of the F-P3 rename. The grep test above confirms there is **no actual wire leakage** — `sessions` here is intentional and well-bounded.

- **S-3 (CLI: `nexus42 schedule list` does not expose `--sort`)** — `crates/nexus42/src/commands/daemon/schedule.rs:386-437` (CLI `list`) builds a `ListSchedulesQuery { …, sort: None }` without exposing `--sort` to users. The HTTP API does honor the F-F1 contract; the CLI is the only consumer that doesn't pass it through. This is acceptable (F-F1 is purely additive; users can `curl` directly), but adding `--sort=…` to the CLI matches the rest of the daemon's CLI parity and is a one-line arg. Not blocking.

- **S-4 (CLAUDE/AGENTS pointer drift)** — `apps/web/AGENTS.md` was updated to reflect that the F-P3 and F-F1 gaps are now closed server-side (the "Remaining gaps the UI adapts around" table is correctly cleared). No update to `crates/nexus-daemon-runtime/AGENTS.md` is needed; the runtime lock rules and sqlx conventions are unchanged. Suggest adding one line to `nexus-daemon-runtime/AGENTS.md` under "Key Rules" noting "**Error envelope**: handlers MUST return `NexusApiError`; the canonical `ApiErrorResponse` is wired via `IntoResponse`. Do NOT return `(StatusCode, String)` tuples for error paths. See `local-api-surface-conventions.md` §3." — codifies the FE1-ORCH invariant for future handlers. Low priority.

## Source Trace

- **Finding W-1**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:184-211` (local DTOs); `crates/nexus-contracts/src/generated/local_api/works/list_works_response.rs:15` (generated, identical shape); sibling handlers consume generated (`schedules.rs:36`, `sessions.rs:14`, `capabilities.rs:11`); spec `.mstar/knowledge/specs/local-api-surface-conventions.md:142-156` lists works as schema-backed for 0.6.0; plan §4 explicit F-P3 boundary = "4 schema-backed Web-UI list endpoints" including `works`. Confidence: High.
- **Finding W-2**: `crates/nexus-daemon-runtime/src/api/sort.rs` (50 lines, no tests); `rg '\?sort=|&sort=' crates/nexus-daemon-runtime/tests/*.rs` returns zero hits; `rg 'sort=' crates/nexus-daemon-runtime/tests/*.rs` returns zero hits; plan AC7 says "Regression tests cover … sort"; the per-endpoint compare closures live at `works.rs:555`, `schedules.rs:697`, `sessions.rs:123`, `capabilities.rs:47` and have no `#[cfg(test)] mod tests`. Confidence: High.
- **Finding W-3**: `crates/nexus-daemon-runtime/src/api/errors.rs:147-153` (new variants); `errors.rs:544-564` (inline unit tests covering `status_code()` and `error_code()` but not `IntoResponse` boundary); `rg 'service_unavailable' crates/nexus-daemon-runtime/tests/*.rs` returns zero hits; `rg 'preset_gates_failed' crates/nexus-daemon-runtime/tests/*.rs` returns zero hits. Confidence: High.
- **Finding S-1**: works.rs:555, schedules.rs:697, sessions.rs:123, capabilities.rs:47. Confidence: High.
- **Finding S-2**: `crates/nexus-contracts/src/local/orchestration/http.rs:640` (`NexusListSessionsResponse { sessions }`); `crates/nexus-contracts/src/generated/local_api/orchestration/sessions/list_sessions_response.rs:15` (canonical `{ items, pagination }`); `rg 'res\.sessions|response\.sessions' apps/web/src/` returns zero hits; the ACP SDK type is intentionally distinct. Confidence: High.
- **Finding S-3**: `crates/nexus42/src/commands/daemon/schedule.rs:386-437`. Confidence: High.
- **Finding S-4**: `apps/web/AGENTS.md` (updated to reflect closure); `crates/nexus-daemon-runtime/AGENTS.md` (no envelope rule). Confidence: Medium.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

## Residual Disposition (handoff to PM / QA)

Per `mstar-review-qc` §Residual Findings 留档门禁, the following unresolved Warning items should be tracked in `status.json` root-level `residual_findings[<plan-id>]` for follow-up. Recommend assigning stable IDs in the consolidated decision.

- **`R-V167P0-QC1-DUPE-DTO`** (Warning, owner: `@fullstack-dev`, target: same plan P-last or P-mid fix wave): migrate `works.rs` `ListWorksQuery`/`ListWorksResponse`/`WorkSummary` to generated `nexus_contracts::local_api::works::*`. Removes second DTO source.
- **`R-V167P0-QC1-SORT-COVERAGE`** (Warning, owner: `@fullstack-dev`, target: P-last): add `#[cfg(test)] mod tests` to `sort.rs` + one integration test per list endpoint asserting `<resource>_sort_invalid` on unknown key.
- **`R-V167P0-QC1-ENVELOPE-E2E`** (Warning, owner: `@fullstack-dev`, target: P-last): add one integration test per new variant (`service_unavailable`, `preset_gates_failed`) asserting the canonical envelope shape end-to-end.

The four Suggestions are non-blocking and may be cherry-picked into P-last hygiene or carried to V1.68 if cheap.

## Reviewer Notes

**What's clean and commendably executed:**

- The FE1-ORCH sweep is complete: no `(StatusCode, String)` ad-hoc error tuples remain anywhere in `crates/nexus-daemon-runtime/src/api/handlers/` (verified via `rg`). The error-envelope migration is uniform — 59 `NexusApiError` construction sites in `schedules.rs` alone, all four orchestration handlers now use the same `IntoResponse` machinery, and the new variants (`ServiceUnavailable`, `PresetGatesFailed`) are well-documented in the `errors.rs` module-level docs (the two-tier strategy is explicitly spelled out at lines 28-48).
- The CASING ratification is correctly **global, not local**: `POLICY_BLOCKED`/`INVALID_INPUT`/`NOT_SUPPORTED`/`INVALID_TRANSITION`/`CHAPTER_PATH_FORBIDDEN` are all renamed in `host_tool_handlers.rs`, `host_tool_executor_tests.rs`, `capability_registry.rs`, `chapters.rs`, `auth_middleware.rs`, `middleware.rs`. Internal classification codes inside `NexusApiError::Internal { code }` correctly remain UPPER_SNAKE per the two-tier strategy documented at `errors.rs:28-48`.
- The F-P3 `items` rename is consistent end-to-end: 4 generated Rust DTOs (`ListWorksResponse`, `ListSchedulesResponse`, `ListSessionsResponse`, `ListCapabilitiesResponse`) all carry `pub items: Vec<…>`; 4 generated TS types mirror the same; 4 JSON Schemas require `["items", "pagination"]` with `additionalProperties: false`; 4 handlers emit the shape; apps/web has no `res.works|sessions|schedules|capabilities` leakage (grep returns zero hits); the legacy `normalizeList`/`sortByDate` adapters in `apps/web/src/lib/nexus/adapters.ts` are correctly reduced to a 9-line empty placeholder with a documenting header.
- The `sort.rs` parser itself is clean: 50 lines, single function, single error path, single source of truth. The 4 list endpoints consume it via identical pattern (`parse_sort_terms(query.sort.as_deref(), &[…allowed_keys…], "<resource>")?`) — reusable per the spec's grammar.
- The npm `@42ch/nexus-contracts` version bump 0.5.0 → 0.6.0 is correct; the Rust workspace version (`0.1.0`) is intentionally separate per AGENTS.md "npm and Rust workspace versions may differ; `schema_version` is the cross-language lock." The generated `crates/nexus-contracts/src/generated/mod.rs` records `schema_version: 2` for exactly the 4 affected `List*Query/Response` pairs (line 56-58, 81-82, 102-103) — correct schema-versioning discipline.
- Schema-drift detection still passes (`cargo test -p nexus-contracts --test schema_drift_detection` → 4/4 ok). Workspace test surface is green (156 test binaries, 0 failures). Web gate green (`pnpm --filter web run typecheck` clean; `pnpm --filter web run test` 15 files, 109 tests pass).

**What blocks `Approve`:** the three Warning items are all on the architecture/maintainability axis (the lens of this reviewer seat): a second DTO source that will desync silently, missing direct test coverage for the new F-F1 grammar, and missing end-to-end coverage for the new error variants. None are runtime regressions; all three are durable maintainability risks that compound over iterations. The plan's AC7 ("Regression tests cover error-envelope mapping, casing, `items` shape, sort") explicitly names sort — so W-2 is a contract gap against the AC, not just a nice-to-have. W-1 violates the AGENTS.md invariant for the only list endpoint that wasn't migrated to generated DTOs. W-3 documents a wire-contract assertion gap introduced by the new variants.

All three are tractable (W-1 is a mechanical migration; W-2 and W-3 are additive test writes). Suggest targeted re-review by `qc-specialist` only (this seat) after the fix wave lands; qc-specialist-2 (security/correctness) and qc-specialist-3 (perf/reliability) had no findings overlapping these three and do not need a second pass.
