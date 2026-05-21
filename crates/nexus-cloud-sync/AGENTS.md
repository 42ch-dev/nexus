# nexus-cloud-sync — Cloud Sync Transport

`nexus-cloud-sync` provides the sync mechanism for CLI ↔ Platform synchronization
using Command, DeltaBundle, and Outbox patterns.

## Key Rules

- **Contract types:** shares generated types from `nexus-contracts`. Do NOT hand-write duplicate DTOs.
- **Feature flags control module visibility.** The `legacy-sync` feature gates the cloud HTTP pipeline
  (outbox, push/pull, platform client). Daemon consumers should NOT enable this feature.
- Crate was renamed from `nexus-sync` as part of the v1.21 local/cloud isolation.

## Feature Flags

| Flag | Default | Purpose |
|------|---------|---------|
| `legacy-sync` | off | Enables outbox, sync_client, platform_client, pull_apply, pool, partial_apply modules |

When `legacy-sync` is off, the following modules are available:
- `canonical_hash`, `command`, `conflict`, `delta_bundle`, `device_flow_client`, `device_id`, `errors`, `precheck`

## Architecture

- **canonical_hash**: BLAKE3-based canonical hashing for delta bundles
- **command**: Sync command types built on generated `SyncCommand`
- **conflict**: Conflict resolution strategies
- **delta_bundle**: Bundle builder with metadata fields
- **device_flow_client**: RFC 8628 Device Authorization Grant client
- **device_id**: Stable machine identifier (UUID v4)
- **errors**: Sync-layer error types
- **precheck**: Local validation before HTTP upload

### Legacy modules (require `legacy-sync`)

- **outbox**: SQLite-backed local operation queue
- **sync_client**: HTTP client for platform sync API
- **platform_client**: HTTP client for creator registration/verification
- **pull_apply**: Apply platform pull responses to local outbox
- **pool**: SQLite connection pool for outbox
- **partial_apply**: Phase A/B partial success semantics
