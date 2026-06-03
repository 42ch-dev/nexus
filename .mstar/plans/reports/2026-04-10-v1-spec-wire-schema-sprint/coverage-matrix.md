# V1-Spec wire schema sprint — coverage matrix

Tracks conceptual platform plans from `2026-04-10-v1-spec-wire-schema-sprint` vs **in-repo** JSON Schema SSOT. **v1-spec prose anchors** live in the private platform repo; this matrix lists **schema file names** only.

| Conceptual ref | Schema files (`schemas/platform/`) | Codegen (Rust + TS) | In-repo consumer (CLI/daemon) | Status |
| --- | --- | --- | --- | --- |
| 16 — explore / creator profile | `explore-hit`, `explore-feed-response`, `explore-browse-request`, `explore-search-request`, `explore-creator-card` (10 fields) | Yes | Explore read path implemented (`nexus42 explore`, daemon, `SyncClient`); creator **card** is wire-only until platform/list endpoints land | **Done** (SSOT); CLI list/detail TBD on platform routes. 2026-04-10: gap fix — added `is_platform_owned`, `created_at`, `public_world_count` per platform Plan 16 spec matrix. |
| 17 — social graph | `social-graph-relationship-request`, `social-graph-relationship-response`, `social-graph-feed-request`, `social-graph-feed-response` | Yes | — | **Done** (wire SSOT) |
| 18 — memory web read | `memory-web-list-request`, `memory-web-list-response` | Yes | — | **Done** (wire SSOT); list **item** uses inline enums aligned with `common` Memory* (codegen import limitation for nested `$ref`) |
| 19 — explore AI | `explore-ai-answer-request`, `explore-ai-answer-response`, `explore-ai-summary-request`, `explore-ai-summary-response` | Yes | — | **Done** (wire SSOT) |
| 20 — notifications | `notifications-inbox-item`, `notifications-list-request`, `notifications-list-response`, `notifications-mark-read-request`, `notifications-mark-read-response` | Yes | — | **Done** (wire SSOT) |
| 21 — contracts / OpenAPI freeze audit | — | — | — | **Checklist** — `.agents/plans/reports/2026-04-10-v1-spec-openapi-freeze-checklist/plan-21-openapi-freeze-checklist.md` (process; not a JSON SSOT file) |

## Tooling notes

- **TS codegen:** `tooling/codegen` now bubbles `CommonTypes` imports for **top-level** properties typed as `array` of `$ref` to common enums (e.g. `MemoryWebListRequest.memory_types`).
- **Nested inline objects** in generated TS still do not auto-import common `$ref` fields; `memory-web-list-response` item fields duplicate enum **values** from `common.schema.json` with a schema comment.
- **Rust:** inline string `enum` in JSON Schema still maps to `String` where the generator uses the string+enum shortcut (see `rust-generator.rs`).
- **Explore feed** entries remain `serde_json::Value` / loose JSON until a stricter hit shape is frozen in codegen.

## Auth / routing

Platform HTTP paths are **not** duplicated here (private OpenAPI). Shapes are **body DTOs** for platform teams to map to routes.
