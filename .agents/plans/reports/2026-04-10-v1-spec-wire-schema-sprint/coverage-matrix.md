# V1-Spec wire schema sprint — coverage matrix

Tracks conceptual platform plans from `2026-04-10-v1-spec-wire-schema-sprint` metadata vs **in-repo** JSON Schema SSOT and consumers. Updated with the Explore read slice (2026-04-10).

| Conceptual ref | Schemas under `schemas/` | Codegen (Rust + TS) | In-repo consumer | Status |
| --- | --- | --- | --- | --- |
| 16 — explore / creator profile (read) | `explore-hit`, `explore-feed-response`, `explore-browse-request`, `explore-search-request` | Yes | `SyncClient::explore_*`; daemon `POST /v1/local/explore/browse` and `.../search`; CLI `nexus42 explore browse` / `explore search` | **Done** (read path; no separate “creator profile” DTO beyond Explore hit row) |
| 17 — social graph | — | — | — | **Gap** |
| 18 — memory-web read | — | — | — | **Gap** |
| 19 — explore-ai | — | — | — | **Gap** |
| 20 — notifications | — | — | — | **Gap** |
| 21 — contracts / OpenAPI freeze audit | — | — | — | **Gap** |

## Notes

- Explore **entries** in `ExploreFeedResponse` are `serde_json::Value` / loose JSON in generated types until a stricter bundle shape is frozen in codegen.
- Platform auth for Explore follows the same daemon env pattern as sync/world: `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN` on **nexus42d**.
