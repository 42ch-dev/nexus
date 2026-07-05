# Daemon API — Workspace CRUD Schemas

JSON Schemas for the **Workspace management** daemon endpoints under `/v1/daemon/workspaces` and `/v1/daemon/workspace`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

V1.63 P1 — promoted from inline handler DTOs in `crates/nexus-daemon-runtime/src/api/handlers/workspaces.rs`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/daemon/workspaces` | `list-workspaces-query.schema.json`, `list-workspaces-response.schema.json`, `workspace-summary.schema.json` |
| `POST /v1/daemon/workspaces` | `create-workspace-request.schema.json`, `create-workspace-response.schema.json` |
| `GET /v1/daemon/workspace` | `active-workspace-response.schema.json` |
| `POST /v1/daemon/workspace/active` | `set-active-workspace-request.schema.json`, `set-active-workspace-response.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
