# Local API — Creator CRUD Schemas

JSON Schemas for the **Creator management** daemon endpoints under `/v1/local/creators`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P1 — promoted from inline handler DTOs in `crates/nexus-daemon-runtime/src/api/handlers/creators.rs`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/local/creators` | `list-creators-query.schema.json`, `list-creators-response.schema.json`, `creator-info.schema.json` |
| `GET /v1/local/creators/{id}` | `creator-detail.schema.json` |
| `GET /v1/local/creators/active` | `active-creator-response.schema.json` |
| `POST /v1/local/creators/active` | `set-active-creator-request.schema.json`, `set-active-creator-response.schema.json` |
| `POST /v1/local/creators/logout` | `logout-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
