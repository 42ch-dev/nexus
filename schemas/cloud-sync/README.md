# Cloud Sync Wire Schemas

JSON Schemas for the **structured sync protocol** between OSS CLI (`nexus42 sync …`) and platform (cloud product line).

| File | Role |
| --- | --- |
| `bundle.schema.json` | Sync-specific bundle view (refines `domain/bundle`; codegen skips duplicate struct — see `tooling/codegen/src/schema-loader.ts`) |
| `sync-pull-request.schema.json` | Pull cursor request |
| `sync-pull-response.schema.json` | Pull response (bundles) |
| `conflict-response.schema.json` | Conflict payload |

**Consumer:** `nexus-cloud-sync` crate (`legacy-sync` feature). **Not** daemon Local API — `/v1/local/sync/*` is retired per [local-cloud-crate-architecture.md](../../.mstar/knowledge/specs/local-cloud-crate-architecture.md).

**Platform protocol docs:** `nexus-platform` `v1-spec/cli-sync/sync-contract-v1.md` (platform repo folder name; OSS wire tree uses `schemas/cloud-sync/`).

**Layout:** [schemas-directory-layout.md](../../.mstar/knowledge/specs/schemas-directory-layout.md) §3.2.
