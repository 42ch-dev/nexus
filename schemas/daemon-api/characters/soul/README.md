# Daemon API — Character SOUL schemas

JSON Schemas for the Character SOUL narrative endpoint under `/v1/daemon/characters/{character_id}/soul`. Cross-language contracts consumed by generated TypeScript and Rust clients.

v1.184 P3 Task 3 — generated request/response DTOs backed by the shared bearer-parameterized SOUL narrative pipeline (`reflect_bearer_soul`). Synthesis stays explicit/on-demand (`force_regenerate`); the insufficient-data gate runs before any ACP call. The server resolves the owner from the active-Creator config; request bodies never carry `owner_creator_id`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/characters/{character_id}/soul/reflect` | `character-soul-narrative-request.schema.json`, `character-soul-narrative-response.schema.json` |
