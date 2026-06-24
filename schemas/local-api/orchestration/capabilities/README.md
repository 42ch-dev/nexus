# Local API — Orchestration Capabilities READ Schemas

JSON Schemas for the **Capability Registry** daemon endpoints under `/v1/local/orchestration/capabilities`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P3 — promoted from handler DTOs in `crates/nexus-contracts/src/local/orchestration/http.rs`.

**Note:** Orchestration schedule list DTOs were already promoted in V1.63 P1 under `schemas/local-api/schedule/` — no duplication needed.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/local/orchestration/capabilities` | `capability-info.schema.json`, `list-capabilities-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Handler:** `crates/nexus-daemon-runtime/src/api/handlers/orchestration/capabilities.rs`
- **Compass:** `.mstar/iterations/v1.63-essay-profile-and-local-api-foundation-delivery-compass-v1.md` §1.1 Track C T12
