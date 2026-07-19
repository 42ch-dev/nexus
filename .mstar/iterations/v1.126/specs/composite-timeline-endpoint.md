# Spec — Composite Timeline daemon endpoint (V1.126 P2)

**Status:** product-reviewed, architect-locked, writing-hygiene done (Phase 1 §1.6 seat 3 inline fallback — empty subagent response; PM applied flagged hygiene per V1.124 pattern)
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1126-3
**Plan:** [`2026-07-20-v1.126-p2-composite-timeline-endpoint`](../../../plans/2026-07-20-v1.126-p2-composite-timeline-endpoint.md)
**Wire contracts:** `wire_contracts_changed: true` — additive schema only; no existing route or DTO mutated. Per repo policy, schema + codegen + Rust handler + web consumer land in one commit.

## Problem

`apps/web/src/components/global-timeline/global-timeline-view.tsx` today fires N=5–10 parallel `GET /v1/daemon/worlds/{world_id}/kb/graph` calls (one per visible World — see `global-timeline-view.tsx::graphQueries` L129–135) just to compose the global Timeline list rows (`{ layer, eraCount, eventCount, lastEditedText }` per row — L176–191). The V1.123 P3 plan capped this at N=5 and acknowledged the N+1 risk (V1.123 P3 residuals: "Global Timeline N+1 fetch risk", "Composite-endpoint work deferred to V1.124+"). `apps/web/src/pages/worlds-page.tsx` shows only "last edited Xh ago" (L101–113) because per-World era/event counts would add another N+1 fetch round. `DF-V1122-DEEPER-WB` has been rolled forward three times (V1.122 → V1.123 → V1.124+ → V1.125+) and `DF-V1123-COMPOSITE-ENDPOINT` was named as a V1.124+ candidate in the V1.123 compass. V1.126 picks up both — **smallest author-valuable slice** (counts + last activity only).

## Normative decisions (PM initial — pending seat 1/2/3)

1. **Composite endpoint — overview shape only (product-locked at seat 1 — counts + last activity, no event rows).** P2 ships `GET /v1/daemon/timeline/overview` returning a cursor-paginated overview across visible Worlds. The full per-World `GET /v1/daemon/worlds/{world_id}/timeline` route (full `TimelineEvent` row access — causality graph, fork-marker progression, publish-marker history) stays open under `DF-V1122-DEEPER-WB` (remainder slice).
2. **Response shape (locked by PM at seat 1 — smallest author-valuable slice; pending architect ratification):**

   ```jsonc
   {
     "worlds": [
       {
         "world_id": "string",
         "title": "string | null",
         "era_count": 0,
         "event_count": 0,
         "last_event_at": "2026-07-20T00:00:00Z | null"
       }
       // max 20 items per page
     ],
     "cursor": "opaque-string | null",
     "total_worlds": 0
   }
   ```

   **`recent_events[]` is intentionally NOT in the response.** The global Timeline view (`global-timeline-view.tsx` L176–191) renders rows from `{ layer, eraCount, eventCount, lastEditedText }` — it does not display event rows; per-World drill-down still calls `kb/graph` for the detail. Adding `recent_events` would balloon the payload to 20 × 100 = 2000 events while no UX in V1.126 consumes them. If a future "expand World row inline" UX needs recent events, it can call the per-World drill-down — that route stays open under `DF-V1122-DEEPER-WB` remainder.
3. **Pagination.** Cursor-based, lexicographic by `world_id`. Default page size 20 worlds. Total payload: 20 × (~5 small fields) ≈ < 2 KB — well under typical daemon response budgets and fits the "smallest author-valuable slice" rule.
4. **Aggregation source.** Handler reads from existing internal `NarrativeGateway::get_timeline()` + `WorldKbGraphResponse` (the same sources the web client fans out to today). No new persistence, no schema migration.
5. **Wire contract — additive.** New schema `schemas/daemon/responses/timeline-overview-response.schema.json` — no `$ref` to `timeline-event.schema.json` (no event rows in the slice). No existing schema mutated. Bundle envelope `schema_version` bumped. Codegen regenerates `@42ch/nexus-contracts` + `crates/nexus-contracts/src/generated/`.
6. **Web consumer migration scope.** `global-timeline-view.tsx` migrates to **one** `useTimelineOverview` call (replaces N=5–10 `kb/graph` fan-out). `worlds-page.tsx` activity column reads `era_count` + `event_count` + `last_event_at` from the same overview call (replaces the "last edited only" projection — V1.123 P3 T3 residual). Per-World drill-down (clicking into a World Timeline) still calls `kb/graph` as before.
7. **NexusClient surface.** Add `getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse>` to `NexusClient` interface. Browser impl in `desktop-capabilities.ts` (HTTP); desktop impl unchanged (browser fetch path).
8. **Repo policy — single commit.** Per root `AGENTS.md`, schema + codegen + Rust handler + web consumer land in **one commit**. CI `Rust fmt & clippy` + `web-build` run sequentially; both must pass.
9. **Tracker discipline — `DF-V1123-COMPOSITE-ENDPOINT` implicit registration (flagged at seat 1).** The DF ID `DF-V1123-COMPOSITE-ENDPOINT` was named in the V1.123 compass line 413 item (j) and is referenced as a `tracking_link` in 5 `status.json::residual_findings` entries — but it was **never registered as a row in the active §2.3 open features table** of `knowledge/deferred-features-cross-version-tracker.md` (verified at seat 1: only `DF-V1123-CROSS-SURFACE-BINDING` exists in V1.123-registered DFs). P2's T4 task therefore:
   - **Appends a `DF-V1123-COMPOSITE-ENDPOINT` row directly to the shipped archive** (`archived/shipped-features-tracker.md`) — labelled "V1.123 compass-named; implicitly registered via status.json tracking_links; closed by V1.126 P2 without an open-row phase".
   - Does **not** attempt to "close a row that doesn't exist" in §2.3 (the active table).
   - `DF-V1122-DEEPER-WB` is updated in place (it exists in §2.3): "V1.126 P2 ships the overview slice; full per-World route stays open under the same DF ID."

## Architecture locks (architect seat 2)

> Ratified 2026-07-20. All AQ verdicts are final — implementers treat these as non-negotiable architecture contracts.

### ND-A1 — Handler file location (AQ-6)

- **New `crates/nexus-daemon-runtime/src/api/handlers/timeline.rs` is LOCKED.**
- **Rationale:** The composite endpoint is a new timeline-overview surface, not a narrative state read. `narrative.rs` currently handles World list/get (`list_worlds`, `get_world`) — these are narrative state routes. The new `timeline.rs` is scoped to timeline aggregation (overview counts + cursor pagination). Separating the handler avoids bloating an already-focused file and allows `timeline.rs` to accumulate future timeline-specific handlers (e.g., the per-World timeline route promotion under `DF-V1122-DEEPER-WB` remainder).
- **Module declaration:** Add `pub mod timeline;` to `crates/nexus-daemon-runtime/src/api/handlers/mod.rs`.
- **Route registration** in `crates/nexus-daemon-runtime/src/api/mod.rs` follows the existing pattern. Handler signature:

  ```rust
  // crates/nexus-daemon-runtime/src/api/handlers/timeline.rs
  pub async fn get_timeline_overview(
      State(state): State<WorkspaceState>,
      Query(pagination): Query<CursorPagination>,
  ) -> Result<Json<TimelineOverviewResponse>, NexusApiError> { ... }
  ```
- **Error envelope:** Returns `NexusApiError` (single-source rule per `crates/nexus-daemon-runtime/AGENTS.md`). No ad-hoc JSON error bodies.
- **Existing handlers are not moved.** `narrative.rs` handlers (`list_worlds`, `get_world`) stay in place.

### ND-A2 — Response shape (CONFIRMED)

- **Final locked shape** (matches seat 1 ND-2, no `recent_events`):

  ```jsonc
  {
    "worlds": [
      {
        "world_id": "string",
        "title": "string | null",
        "era_count": 0,        // count of block_type=era entities
        "event_count": 0,      // count of block_type=event entities
        "last_event_at": "2026-07-20T00:00:00Z | null"
      }
    ],
    "cursor": "opaque-string | null",
    "total_worlds": 0
  }
  ```
- **Aggregation source:** Handler reads from existing `NarrativeGateway::list_worlds()` for world identity + `updated_at`, then per-world `WorldKbGraphResponse` (via `kb/graph` internal or equivalent) for era/event counts. No new persistence, no schema migration.
- **`last_event_at` derivation:** `max(timestamp)` across all `block_type=event` entities for that World from `WorldKbGraphResponse.entities[]`. `null` when the World has zero events.
- **Cursor pagination:** Lexicographic by `world_id`. Default page size 20 worlds. Cursor is an opaque base64-encoded `world_id` of the last item in the current page; `null` on the last page.

### ND-A3 — Route registration pattern

- **Exact registration** in `crates/nexus-daemon-runtime/src/api/mod.rs` (in the daemon routes section):

  ```rust
  .route(
      "/v1/daemon/timeline/overview",
      get(handlers::timeline::get_timeline_overview),
  )
  ```
- Follows the existing handler registration pattern (e.g., `narrative.rs` handlers registered at `/v1/daemon/narrative/worlds`). The `timeline` path prefix is new.
- **Existing routes are NOT modified.** `/v1/daemon/worlds/{world_id}/kb/graph` (`world_kb.rs`), `/v1/daemon/narrative/worlds` (`narrative.rs`), and `/v1/daemon/narrative/worlds/{world_id}` (`narrative.rs`) are preserved unchanged.

### ND-A4 — Codegen + single-commit policy

- **Single commit** spanning: schema (`schemas/daemon/responses/timeline-overview-response.schema.json`) → `pnpm run codegen` (TypeScript + Rust DTOs) → Rust handler (`timeline.rs` + `mod.rs` route) → web consumer (`NexusClient` + queries + views).
- **CI sequence:** `Rust fmt & clippy` → `web-build` (both must pass). If the schema adds new generated files, verify `cargo check -p nexus-contracts` clean after codegen.
- **Bundle envelope:** `schemas/bundle/*.schema.json` `schema_version` bumped (minor).
- **No edits to existing schemas.** The new schema is additive.

### ND-A5 — NexusClient interface addition

- New method on `NexusClient` interface (`apps/web/src/lib/nexus/types.ts`):
  ```ts
  getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse>;
  ```
- Browser implementation in `apps/web/src/lib/nexus/desktop-capabilities.ts`:
  ```ts
  async getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse> {
    const params = cursor ? `?cursor=${encodeURIComponent(cursor)}` : '';
    return this.fetchDaemon(`/v1/daemon/timeline/overview${params}`);
  }
  ```
- React Query hook: `useTimelineOverview(cursor?)` in `apps/web/src/api/queries.ts` — cursor-paginated, `staleTime: 10_000` (overview data changes less frequently than per-World graph).

### ND-A6 — Response type registration in generated code

- Codegen places `TimelineOverviewResponse` in:
  - `crates/nexus-contracts/src/generated/daemon/responses/timeline_overview_response.rs`
  - `packages/nexus-contracts/src/generated/daemon/responses/timeline-overview-response.ts`
- Internal DTO types (`WorldOverviewItem`, `CursorPagination`) are generated from the schema.

### ND-A7 — Wire contracts verdict

- **`wire_contracts_changed: true` — CONFIRMED.** Additive schema only; no existing route or DTO mutated. The new schema triggers codegen → `@42ch/nexus-contracts` minor version bump → daemon handler + web consumer. Per repo policy, all changes land in one commit.

## Architecture notes (implementer)

| Component | Change |
|-----------|--------|
| New `schemas/daemon/responses/timeline-overview-response.schema.json` | Additive response schema (see ND-2 — no `$ref` to `timeline-event.schema.json`; counts + `last_event_at` only) |
| `schemas/bundle/*.schema.json` | Bundle envelope `schema_version` bump |
| `tooling/codegen/*` | If the new response needs new codegen hint, update; otherwise no change |
| `crates/nexus-contracts/src/generated/` | Auto-regenerated DTOs |
| `crates/nexus-daemon-runtime/src/api/handlers/timeline.rs` (NEW — ND-A1 locked) | `get_timeline_overview` handler: aggregate era/event counts from `NarrativeGateway::list_worlds()` + per-World `WorldKbGraphResponse` (internal); cursor pagination; `last_event_at` from max event timestamp |
| `crates/nexus-daemon-runtime/src/api/handlers/mod.rs` | Add `pub mod timeline;` module declaration (ND-A1) |
| `crates/nexus-daemon-runtime/src/api/mod.rs` | Register `GET /v1/daemon/timeline/overview` route: `.route("/v1/daemon/timeline/overview", get(handlers::timeline::get_timeline_overview))` (ND-A3) |
| `apps/web/src/lib/nexus/types.ts` | Add `getTimelineOverview` to `NexusClient` interface |
| `apps/web/src/lib/nexus/desktop-capabilities.ts` | Browser impl: `GET /v1/daemon/timeline/overview` |
| `apps/web/src/api/queries.ts` | New `useTimelineOverview(cursor?)` query (cursor-paginated, infinite query if React Query v5) |
| `apps/web/src/components/global-timeline/global-timeline-view.tsx` | Migrate to one `useTimelineOverview` call; per-World rows render from the overview payload |
| `apps/web/src/pages/worlds-page.tsx` | Activity column reads `era_count` + `event_count` + `last_event_at` from `useTimelineOverview` (no extra fetch — same query) |

## Acceptance (author-observable + technical)

| ID | What we see |
|----|-------------|
| AC-V1126-3 | `GET /v1/daemon/timeline/overview` returns the overview shape (no `recent_events`); web consumes one call; `worlds-page.tsx` activity column gains era/event counts; `DF-V1122-DEEPER-WB` row updated; `DF-V1123-COMPOSITE-ENDPOINT` archive-only row appended per ND-9 |

## Out of scope

Full per-World `GET /v1/daemon/worlds/{world_id}/timeline` route (stays under `DF-V1122-DEEPER-WB` remainder); `recent_events[]` rows in the overview response (intentionally dropped at seat 1 as over-engineering — see ND-2; per-World drill-down still calls `kb/graph` for event detail); cross-surface data binding (`DF-V1123-CROSS-SURFACE-BINDING`); multi-timeline (`DF-V1123-MULTI-TIMELINE`); new schemas for Moment-on-wire; cross-World merge view (`DF-V1123-GLOBAL-TIMELINE-MERGE`); Work-scoped `GET /v1/daemon/works/{work_id}/timeline` route (NG-13).
