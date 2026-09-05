# Daemon API — Character identity and binding schemas

JSON Schemas for Character bearer and ActorWorldBinding daemon endpoints under `/v1/daemon/characters`. Cross-language contracts consumed by generated TypeScript and Rust clients.

v1.184 P0 Task 1 — generated request/response DTOs. Handlers land in a later P0 task.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/characters` | `create-character-request.schema.json`, `create-character-response.schema.json` |
| `GET /v1/daemon/characters` | `list-characters-query.schema.json`, `list-characters-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}` | `character-detail.schema.json` |
| `POST /v1/daemon/characters/{character_id}/bindings` | `add-character-binding-request.schema.json`, `add-character-binding-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}/bindings` | `list-character-bindings-query.schema.json`, `list-character-bindings-response.schema.json` |
| `DELETE /v1/daemon/characters/{character_id}/bindings/{binding_id}` | no request body (stable 409 `last_active_actor_world_binding` via ErrorResponse) |

Domain entities: `schemas/domain/actor-ref.schema.json`, `character.schema.json`, `actor-world-binding.schema.json`.
