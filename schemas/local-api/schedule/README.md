# Local API — Schedule CRUD Schemas

JSON Schemas for the **Orchestration Schedule** daemon endpoints under `/v1/local/orchestration/schedules`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P1 — promoted from hand-written DTOs in `crates/nexus-contracts/src/local/schedule/http.rs`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/local/orchestration/schedules` | `add-schedule-request.schema.json`, `schedule-concurrency-request.schema.json`, `add-schedule-response.schema.json` |
| `GET /v1/local/orchestration/schedules` | `list-schedules-query.schema.json`, `list-schedules-response.schema.json`, `schedule-summary.schema.json` |
| `GET /v1/local/orchestration/schedules/{id}` | `inspect-schedule-response.schema.json` |
| `PATCH /v1/local/orchestration/schedules/{id}/core-context` | `edit-core-context-request.schema.json`, `edit-core-context-response.schema.json` |
| `GET /v1/local/orchestration/schedules/{id}/core-context` | `core-context-response.schema.json` |
| `GET /v1/local/orchestration/schedules/{id}/core-context-history` | `core-context-history-response.schema.json`, `core-context-history-entry.schema.json` |
| `POST /v1/local/orchestration/schedules/{id}/signal` | `signal-schedule-request.schema.json`, `signal-schedule-response.schema.json` |
| `DELETE /v1/local/orchestration/schedules/{id}` | `delete-schedule-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
