# Local API — KB Entry CRUD Schemas

JSON Schemas for the **work-scope KB entry** daemon endpoints under `/v1/local/kb/entries`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P1 — promoted from inline handler DTOs in `crates/nexus-daemon-runtime/src/api/handlers/kb.rs`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/local/kb/entries` | `list-kb-entries-query.schema.json`, `list-kb-entries-response.schema.json`, `kb-entry-summary.schema.json`, `pagination-info.schema.json` |
| `POST /v1/local/kb/entries` | `add-kb-entry-request.schema.json`, `add-kb-entry-response.schema.json` |
| `GET /v1/local/kb/entries/{id}` | `get-kb-entry-response.schema.json` |
| `DELETE /v1/local/kb/entries/{id}` | `delete-kb-entry-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
