# Local API — Orchestration Sessions READ Schemas

JSON Schemas for the **Orchestration Engine Sessions** daemon endpoints under `/v1/local/orchestration/sessions`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P3 — promoted from handler DTOs in `crates/nexus-contracts/src/local/orchestration/http.rs`.

**Scope:** READ-only (list + detail-status). Create/signal/cancel write-side DTOs are deferred (agent-host sessions/operations/events — compass §1.2 non-goal, V1.64+).

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/local/orchestration/sessions` | `list-sessions-query.schema.json`, `list-sessions-response.schema.json`, `session-summary.schema.json` |
| `GET /v1/local/orchestration/sessions/{id}` | `session-detail-response.schema.json`, `session-summary.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Handler:** `crates/nexus-daemon-runtime/src/api/handlers/orchestration/sessions.rs`
- **Compass:** `.mstar/iterations/v1.63-essay-profile-and-local-api-foundation-delivery-compass-v1.md` §1.1 Track C T11
