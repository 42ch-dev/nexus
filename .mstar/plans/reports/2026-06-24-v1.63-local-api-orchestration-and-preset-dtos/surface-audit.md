---
report_id: 2026-06-24-v1.63-local-api-orch-preset-surface-audit
plan_id: 2026-06-24-v1.63-local-api-orchestration-and-preset-dtos
auditor: "@fullstack-dev-2"
date: 2026-06-24
scope: All promoted Local API DTOs (P1 F0 + P3 F1-F3, ~75 schemas)
status: complete
---

# Local API Surface Audit — Pagination, Error-Shape, and Filter/Sort Consistency

## 1. Scope

This audit covers all promoted Local API DTOs across 9 resource subtrees:

| # | Resource | Subtree | Wave | Schema count |
|---|----------|---------|------|-------------|
| 1 | Works | `schemas/local-api/works/` | P1 | 11 |
| 2 | KB | `schemas/local-api/kb/` | P1 | 8 |
| 3 | Findings | `schemas/local-api/findings/` | P1 | 5 |
| 4 | Schedule | `schemas/local-api/schedule/` | P1 | 16 |
| 5 | Workspace | `schemas/local-api/workspace/` | P1 | 8 |
| 6 | Creators | `schemas/local-api/creators/` | P1 | 8 |
| 7 | Orchestration Sessions | `schemas/local-api/orchestration/sessions/` | P3 | 4 |
| 8 | Orchestration Capabilities | `schemas/local-api/orchestration/capabilities/` | P3 | 2 |
| 9 | Preset Management | `schemas/local-api/preset-management/` | P3 | 7 |

## 2. Pagination Consistency

### 2.1 Observed patterns

| Pattern | Resources using it | Details |
|---------|-------------------|---------|
| **Cursor-based** | Creators, KB, Workspaces | `cursor` + `limit` query params; `pagination: PaginationInfo` in response; `items` array key |
| **Offset/limit + total** | Works | `offset` + `limit` query params; `total` integer in response; `works` array key |
| **No pagination** | Schedule, Findings, Orchestration Sessions, Capabilities, Preset Management | No pagination query params; plain array responses |

### 2.2 Findings

**F-P1 — Works uses offset/limit while peers use cursor-based pagination.** Works is the only resource using `offset`/`limit` + `total`. All other list-returning resources (creators, kb, workspaces) use cursor-based pagination via shared `PaginationInfo`. Schedule has no pagination (plausible: schedule list is expected to be small), but Works' offset/limit pattern is an outlier.

- **Severity**: Medium
- **Recommendation**: Migrate Works to cursor-based pagination, adding `PaginationInfo` and changing `works` → `items` for consistency. **Defer to V1.64+** — this requires handler changes and is structural.

**F-P2 — Findings list-query exists but no list-response schema.** `list-findings-query.schema.json` defines `offset`/`limit` as query parameters (inconsistent with peers using cursor), but there is no `list-findings-response.schema.json`. The existing response schemas are `finding-detail-response` (single item) and `stale-findings-response` (non-paginated).

- **Severity**: Low (no handler reference — may not be wired yet)
- **Recommendation**: Add `list-findings-response.schema.json` when the endpoint is implemented. Use cursor-based pattern matching peers. **Defer to V1.64+** (endpoint not yet implemented).

**F-P3 — Response array key naming inconsistency.** Works uses `works`, Schedule uses `schedules`, Orchestration Sessions uses `sessions`, Capabilities uses `capabilities`, Preset Management uses `embedded`/`system`/`user`. The cursor-based resources (Creators, KB, Workspaces) consistently use `items`.

- **Severity**: Low
- **Recommendation**: Standardize on `items` for list response arrays. Defer renaming `works`/`schedules`/`sessions`/`capabilities` as it changes handler contracts. For new endpoints (not yet implemented), prefer `items`. **Defer all to V1.64+**.

## 3. Error-Shape Consistency

### 3.1 Observed patterns

**No error schemas exist in the Local API surface.** None of the 69+ schema files define error-related properties. The daemon handlers return `(StatusCode, String)` tuples directly, and error shapes are not modeled in JSON Schema.

### 3.2 Findings

**F-E1 — No standardized error envelope.** Different handlers return different error body formats: some return plain strings, some return JSON objects with `error`/`message` keys. The Local API has no documented error contract.

- **Severity**: Low (pre-1.0; clients currently handle errors as strings)
- **Recommendation**: Add a shared `ErrorResponse` schema in `schemas/local-api/common/` with `code` (string, stable error code), `message` (string), and optional `details` (object). Do NOT apply to existing handlers in this iteration. **Defer to V1.64+** as a structural change requiring handler audit.

## 4. Filter/Sort Parameter Consistency

### 4.1 Observed patterns

| Resource | Filter params | Sort params |
|----------|--------------|-------------|
| Creators | (none) | (none) |
| KB | `creator_id`, `workspace_slug`, `scope`, `q` (text search) | (none) |
| Works | `status`, `intake_status` | (none) |
| Findings | `chapter`, `status`, `severity` | (none) |
| Schedule | `creator_id`, `status` | (none) |
| Workspace | `creator_id` | (none) |
| Orchestration Sessions | `creator_id` | (none) |
| Capabilities | (none) | (none) |
| Preset Management | (none) | (none) |

### 4.2 Findings

**F-F1 — No sort parameters defined in any list query.** No resource supports `sort_by` / `order` / `direction` query parameters. This is a future concern, not a bug.

- **Severity**: Low (sort is not a current requirement)
- **Recommendation**: When sort is added, standardize on `sort_by: string` + `sort_order: "asc" | "desc"`. **Defer to V1.64+**.

**F-F2 — `q` (text search) param exists only in KB.** The `q` parameter in `list-kb-entries-query` is a full-text search string. No other resource has a search param.

- **Severity**: Info (KB-specific feature; search is not uniform across resources)
- **Recommendation**: None for now. If other resources gain search, standardize on `q` as the param name.

**F-F3 — Preset Management list has no filtering.** `GET /v1/local/presets` returns all presets grouped by source. There is no query schema — the response shape is a single grouped object, not a paginated list.

- **Severity**: Info (design choice: grouped response, not list)
- **Recommendation**: Accept as intentional. If filtering is needed later, add query params consistent with peer patterns.

## 5. ID Naming Conventions

Generally consistent: `{resource}_id` for all resources except:

- **Workspace** uses `workspace_slug` (semantic identifier, not UUID). Consistent with domain model.
- **KB** uses `entry_id` (not `kb_entry_id`). Minor inconsistency but clear in context.

No high-severity issues. ID naming is a strength.

## 6. Schema Completeness

| Resource | List | Detail | Create | Update | Delete | Signal | Validate | Reload |
|----------|------|--------|--------|--------|--------|--------|----------|--------|
| Works | ✅ | ✅ | ✅ | ✅ (patch) | — | — | — | — |
| KB | ✅ | ✅ | ✅ | — | ✅ | — | — | — |
| Findings | ⚠️ (query only) | ✅ | ✅ | ✅ | — | — | — | — |
| Schedule | ✅ | ✅ | ✅ | — | ✅ | ✅ | — | — |
| Workspace | ✅ | — | ✅ | — | — | — | — | — |
| Creators | ✅ | ✅ | — | — | — | — | — | ✅ |
| Sessions | ✅ | ✅ | — | — | — | — | — | — |
| Capabilities | ✅ | — | — | — | — | — | — | — |
| Preset Mgmt | ✅ | — | ✅ | — | — | — | ✅ | ✅ |

**⚠️ Findings list-response is missing** (F-P2 above). **KB update** (PATCH) is not modeled — endpoint may not exist yet.

## 7. Summary & Action Plan

### In-scope fixes (low-risk, high-value)
None. All findings are structural changes requiring handler updates. Per compass §1.1 Track C T15: "Fix-waves only for low-risk high-value items (defer structural changes as residuals with V1.64+ target)."

### Residuals to register for V1.64+

| ID | Finding | Severity | Action |
|----|---------|----------|--------|
| `R-V163P3-SURF-001` | Works uses offset/limit; migrate to cursor-based pagination | medium | Migrate Works pagination to cursor/limit + PaginationInfo. Requires handler + schema update. |
| `R-V163P3-SURF-002` | Findings list-response schema missing | low | Add `list-findings-response.schema.json` when endpoint is implemented. Use cursor pattern. |
| `R-V163P3-SURF-003` | Response array key naming inconsistent (works/schedules/sessions/capabilities vs items) | low | Rename response array keys to `items` across all list response schemas. Breaking change; coordinate with handler migration. |
| `R-V163P3-SURF-004` | No standardized error envelope | low | Add shared `ErrorResponse` schema. Audit all handlers for error response format consistency. |
| `R-V163P3-SURF-005` | No sort parameters across any resource | low | Add standardized `sort_by`/`sort_order` params when sorting becomes a requirement. |

### Assessment

The Local API surface is **functional and internally consistent for its pre-1.0 maturity level**. The primary inconsistency (Works uses offset/limit vs cursor peers) is a known pattern — Works was the first resource promoted and established a simpler pattern; later resources standardized on cursor-based pagination. The surface is ready for web-ui consumption with the caveats registered as residuals above. Structural improvements should follow in a dedicated V1.64+ plan coordinated with the web-ui iteration's actual API usage patterns.
