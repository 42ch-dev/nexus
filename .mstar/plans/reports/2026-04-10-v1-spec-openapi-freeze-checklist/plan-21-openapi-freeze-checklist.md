# Plan 21 — OpenAPI / HTTP freeze audit (checklist)

**Role**: Cross-repo process. This file is the **nexus** copy for handoff; execution happens on the platform repository and OpenAPI SSOT.

**v1-spec anchors**: `schema/codegen-strategy-v1.md` §6 (minimal HTTP freeze set), `cli-sync/platform-capability-map-v1.md`.

## Minimal freeze set (§6) route families

- [ ] Auth (`signup` / `login` / `refresh` / `logout` / device flow) — OpenAPI vs handlers
- [ ] Worlds CRUD + owner/bootstrap
- [ ] World members family
- [ ] Publish (`POST` chapters/stories, `GET` list, `DELETE` publication)
- [ ] Entitlements + quota — **DTOs** in nexus: `MeEntitlementsResponse`, `OfficialCreatorQuotaResponse` (`schemas/platform/*.schema.json`)
- [ ] Sync bundles / cursor / delta

## Markers

- [ ] Evolving routes tagged `x-nexus-stability: evolving` (or release-note equivalent)
- [ ] No new public routes in the freeze set without schema + OpenAPI update

## Sign-off

- [ ] Platform + contracts owners agree **`@42ch/nexus-contracts`** semver for the next coordinated bump
