# Local API — Works CRUD Schemas

JSON Schemas for the **Works** daemon endpoints under `/v1/local/works`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P1 — promoted from inline handler DTOs in `crates/nexus-daemon-runtime/src/api/handlers/works.rs`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/local/works` | `create-work-request.schema.json`, `create-work-response.schema.json` |
| `GET /v1/local/works` | `list-works-query.schema.json`, `list-works-response.schema.json`, `work-summary.schema.json` |
| `GET /v1/local/works/{id}` | `work-detail-response.schema.json` |
| `PATCH /v1/local/works/{id}` | `patch-work-request.schema.json` |
| `POST /v1/local/works/{id}/inspiration` | `append-inspiration-request.schema.json`, `append-inspiration-response.schema.json` |
| `POST /v1/local/works/{id}/completion-lock/release` | `release-completion-lock-request.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
