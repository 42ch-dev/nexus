# Platform Sync Wire Schemas

JSON Schemas for the **CLI ↔ platform sync protocol** (`nexus-platform` observes these on the wire; `nexus-cloud-sync` is the Rust consumer).

V1.62 (2026-06-23) consolidated this folder: sync-payload files moved here from `schemas/domain/` (bundle, delta, sync-command) and `schemas/cloud-sync/` (sync-pull-*, conflict-response). The old `schemas/cloud-sync/` folder was removed.

## Files (7)

| File | Role |
| --- | --- |
| `bundle.schema.json` | **Codegen canonical** `Bundle` envelope type (was `domain/bundle.schema.json`) |
| `bundle-refinement.schema.json` | Validation refinement (allOf of canonical bundle with CLI V1.0 constraints). **Codegen-skipped** — see `tooling/codegen/src/schema-loader.ts` `SKIP_STRUCT_GENERATION_REL_PATHS`. (Was `cloud-sync/bundle.schema.json`; renamed to avoid basename collision with the canonical bundle.) |
| `delta.schema.json` | `Delta` aggregate operation (was `domain/delta.schema.json`) |
| `sync-command.schema.json` | `SyncCommand` attribution record (was `domain/sync-command.schema.json`) |
| `sync-pull-request.schema.json` | Pull request (was `cloud-sync/`) |
| `sync-pull-response.schema.json` | Pull response (was `cloud-sync/`) |
| `conflict-response.schema.json` | Conflict report (was `cloud-sync/`) |

**Platform protocol docs:** `nexus-platform` `v1-spec/cli-sync/sync-contract-v1.md` (platform repo folder name; OSS wire tree uses `schemas/platform/sync/`).

**Consumer:** `@42ch/nexus-contracts` (npm) + `nexus-cloud-sync` Rust (`legacy-sync`).

**Layout spec:** [schemas-directory-layout.md](../../../.mstar/knowledge/specs/schemas-directory-layout.md) §3.2.
