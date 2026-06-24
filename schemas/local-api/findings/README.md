# Local API — Findings CRUD Schemas

JSON Schemas for the **Quality Findings** daemon endpoints under `/v1/local/works/{work_id}/findings` and `/v1/local/findings/stale`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P1 — promoted from inline handler DTOs in `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/local/works/{id}/findings` | `create-finding-request.schema.json` |
| `GET /v1/local/works/{id}/findings` | `list-findings-query.schema.json`, `list-findings-response.schema.json` (F-P2 V1.64, cursor pagination) |
| `GET/POST response` | `finding-detail-response.schema.json` |
| `PATCH /v1/local/works/{id}/findings/{fid}` | `update-finding-request.schema.json` |
| `GET /v1/local/findings/stale` | `stale-findings-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
