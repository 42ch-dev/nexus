# CLI ↔ Platform Sync Wire Schemas (Cloud Line)

JSON Schemas for the **structured sync protocol** between OSS CLI (`nexus42 sync …`) and platform.

| File | Role |
| --- | --- |
| `bundle.schema.json` | Sync-specific bundle view (refines `domain/bundle`; codegen skips duplicate struct — see `tooling/codegen/src/schema-loader.ts`) |
| `sync-pull-request.schema.json` | Pull cursor request |
| `sync-pull-response.schema.json` | Pull response (bundles) |
| `conflict-response.schema.json` | Conflict payload |

**Consumer:** `nexus-cloud-sync` crate (`legacy-sync` feature). **Not** daemon Local API — `/v1/local/sync/*` is retired per [local-cloud-crate-architecture.md](../../.agents/knowledge/specs/local-cloud-crate-architecture.md).

## Target directory name

Long-term folder name **`cloud-sync/`** (aligned with crate `nexus-cloud-sync`). Physical path may remain **`cli-sync/`** until a coordinated `$id`/`$ref` rename — see [schemas-directory-layout.md](../../.agents/knowledge/specs/schemas-directory-layout.md) §5.

**Platform protocol docs:** `nexus-platform` `v1-spec/cli-sync/sync-contract-v1.md` (folder name in platform repo is independent of OSS directory rename).
