---
plan_id: 2026-06-24-v1.64-local-api-hardening
reviewer: "@qc-specialist-2"
reviewer_index: 2
focus: security_correctness
report_suffix: qc2
verdict: Approve
generated_at: 2026-06-25T00:50:00Z
review_range: c8f93e18..0afa42b2
working_branch: iteration/v1.64
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
---

# Code Review Report — qc2 (Security & Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (cursor forgery, authz on findings list, ErrorResponse leakage, DTO shape regression, has_more invariants, same-origin trust, loopback keyless model)
- Report Timestamp: 2026-06-25T00:50:00Z

## Scope
- plan_id: 2026-06-24-v1.64-local-api-hardening AND 2026-06-24-v1.64-web-app-scaffold (Wave 1 integrated — this report written to both plan report dirs)
- Review range / Diff basis: c8f93e18..0afa42b2 (code at 0eda73fa; 0afa42b2 docs-only on top). V1.64 Wave 1: P0 + P1 merged.
- Working branch (verified): iteration/v1.64
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (`git branch --show-current=iteration/v1.64`, `git rev-parse --short HEAD=0afa42b2`)
- Files reviewed: 75 files changed in diff (P0: daemon handlers + pagination + schemas + contracts; P1: apps/web scaffold + client + CI)
- Tools run: git diff/stat/log, read (plans, specs, AGENTS.md, source), grep, cargo test -p nexus-daemon-runtime --test works_api --test findings_api (all green), manual code inspection of cursor encoding/decoding, creator_id scoping, has_more sites, BrowserClient fetch paths, dev-proxy, Tauri stub.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None (all high-risk items addressed or within documented pre-1.0 scope).

### 🟢 Suggestion
- (Low) Consider adding an explicit creator-owned work_id existence check comment or helper near the findings list path for future readers (DAO already enforces via `WHERE creator_id = ? AND work_id = ?`; handler reads creator_id first).
- (Future defense-in-depth) If the Local API ever leaves pure loopback, the current `v1:<offset>` cursor could be augmented with a MAC; today the threat model is localhost-only and DAO creator scoping is the real control.
- Minor: `NexusClientError` in browser-client is an app-side envelope; once generated `ErrorResponse` lands on the merged branch, tighten `fromBody` to validate against the contract type (already noted in apps/web/AGENTS.md).

## Source Trace
- Finding ID: QC2-W1-001 (cursor token)
- Source Type: manual-reasoning + code review of `crates/nexus-daemon-runtime/src/api/pagination.rs` + `handlers/{works,findings}.rs`
- Source Reference: `decode_offset_cursor` (validates `v1:` prefix + u32), `offset_page_meta`, DAO `list_findings`/`list_works` (always bind creator_id from `read_active_creator_id`)
- Confidence: High

- Finding ID: QC2-W1-002 (findings list authz)
- Source Type: code review + DAO source
- Source Reference: `handlers/findings.rs: list_findings_handler` (creator_id before DAO), `nexus-local-db/src/findings.rs: list_findings` (WHERE creator_id = ?), path param work_id used only as filter.
- Confidence: High

- Finding ID: QC2-W1-003 (has_more sites)
- Source Type: grep + diff
- Source Reference: 5+ peer handlers (creators, agent_host, kb, memory, workspaces, works, findings) all now emit `has_more: next_cursor.is_some()` or via `offset_page_meta`.
- Confidence: High

- Finding ID: QC2-W1-004 (BrowserClient / same-origin)
- Source Type: code review of `apps/web/src/lib/nexus/browser-client.ts`, `vite.config.ts`
- Source Reference: fetch to same-origin `/v1/local/*` (proxy in dev, daemon-served in release); no credentials; static shell carries no data.
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (minor / future) |

**Verdict**: Approve

## Evidence & Verification Performed
- Verified branch/HEAD: `iteration/v1.64 @ 0afa42b2`
- Diff basis: `c8f93e18..0eda73fa` (P0 handlers + new `pagination.rs`; P1 full scaffold)
- `cargo test -p nexus-daemon-runtime --test works_api --test findings_api`: 34 + 15 passed (no regression on cursor migration or findings list).
- Cursor: opaque `v1:<offset>`, decode rejects non-v1 or non-numeric → 400 INVALID_INPUT. Offset is only used inside creator-scoped DAO queries.
- Findings list: `Path(work_id)` + `read_active_creator_id` + DAO `WHERE creator_id = ? AND work_id = ?` (no path traversal, creator isolation preserved).
- ErrorResponse: new schema + handler usage; `details` is for structured context only (no stack traces).
- All list responses now use `PaginationInfo { limit, next_cursor, has_more }` with `has_more = next_cursor.is_some()`.
- BrowserClient: same-origin fetch, keyless per V1.20 model. TauriClient stub throws cleanly.
- Dev proxy: only `/v1/local` → localhost daemon; no non-loopback exposure.
- No secrets/PII in diff.
- Keyless localhost model integrity: SPA shell is static; data only via loopback Local API.
- Specs/AGENTS alignment: `local-api-surface-conventions.md`, `web-ui.md`, daemon-runtime AGENTS.md (sqlx macros), apps/web/AGENTS.md + DESIGN.md all read and consistent with delivered code.

## Revalidation Notes
N/A — initial Wave 1 review (no prior Request Changes on this scope for qc2).

---
*qc2 (security & correctness) for V1.64 Wave 1 (P0 + P1).*
