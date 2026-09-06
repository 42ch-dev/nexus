# Daemon API — Character memory schemas

JSON Schemas for the Character memory bearer endpoints under `/v1/daemon/characters/{character_id}/memory`. Cross-language contracts consumed by generated TypeScript and Rust clients.

v1.184 P3 Task 3 — generated request/response DTOs backed by the dedicated `character_*` SQLite repositories. The server resolves the owner from the active-Creator config and stored `characters` rows; request bodies never carry `owner_creator_id`. All scopes require an active owned Character; a non-null `binding_id` additionally requires the exact active binding in an owned active World. Failures reject before any DB row, file, or synthesis side effect.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/characters/{character_id}/memory/pending-review` | `capture-character-pending-review-request.schema.json`, `capture-character-pending-review-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}/memory/pending-review` | `list-character-pending-reviews-query.schema.json`, `list-character-pending-reviews-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}/memory/pending-review/count` | `count-character-pending-reviews-query.schema.json`, `count-character-pending-reviews-response.schema.json` |
| `DELETE /v1/daemon/characters/{character_id}/memory/pending-review/{pending_id}` | `delete-character-pending-review-response.schema.json` |
| `POST /v1/daemon/characters/{character_id}/memory/review` | `review-character-memory-request.schema.json`, `review-character-memory-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}/memory/fragments` | `list-character-memory-fragments-query.schema.json`, `list-character-memory-fragments-response.schema.json` |
| `POST /v1/daemon/characters/{character_id}/memory/fragments/{fragment_id}:promote` | `promote-character-fragment-request.schema.json`, `promote-character-fragment-response.schema.json` (stable 409 `version_mismatch` / `character_fragment_already_shared` via ErrorResponse) |

Item DTOs: `character-pending-review-info.schema.json`, `character-memory-fragment-info.schema.json`. Pagination envelope: `schemas/daemon-api/kb/pagination-info.schema.json`.
