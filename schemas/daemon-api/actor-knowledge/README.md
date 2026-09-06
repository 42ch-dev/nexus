# Daemon API — Actor KnowledgeView schemas

JSON Schemas for Actor KnowledgeEntry create/list and the reusable KnowledgeView under `/v1/daemon/actor-knowledge` and Character knowledge list.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/actor-knowledge/view` | `view-request.schema.json`, `view-response.schema.json` |
| `POST /v1/daemon/actor-knowledge/entries` | `add-knowledge-entry-request.schema.json`, `add-knowledge-entry-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}/knowledge` | `list-character-knowledge-query.schema.json`, `list-character-knowledge-response.schema.json` |
| `DELETE /v1/daemon/characters/{character_id}/bindings/{binding_id}` | stable 409 `binding_has_owned_knowledge` via ErrorResponse |

Domain: `schemas/domain/knowledge-owner-ref.schema.json`, `actor-ref.schema.json`.
