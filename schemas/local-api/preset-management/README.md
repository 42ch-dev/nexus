# Local API — Preset Management Schemas

JSON Schemas for the **Preset Management** daemon endpoints under `/v1/local/presets`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P3 — promoted from inline handler DTOs in `crates/nexus-daemon-runtime/src/api/handlers/preset_management.rs`.

**Scope:** Full CRUD surface (list, scaffold, validate, reload). The handler does not currently implement show/update/delete endpoints — those are deferred.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/local/presets` | `preset-summary.schema.json`, `list-presets-response.schema.json` |
| `POST /v1/local/presets` | `scaffold-preset-request.schema.json`, `scaffold-preset-response.schema.json` |
| `POST /v1/local/presets:validate` | `validate-preset-request.schema.json`, `validate-preset-response.schema.json` |
| `POST /v1/local/presets/{id}:reload` | `reload-preset-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Handler:** `crates/nexus-daemon-runtime/src/api/handlers/preset_management.rs`
- **Compass:** `.mstar/iterations/v1.63-essay-profile-and-local-api-foundation-delivery-compass-v1.md` §1.1 Track C T13
