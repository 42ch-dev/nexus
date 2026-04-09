# Sync inventory (A1) — plan `2026-04-10-cli-sync-bidirectional-parity`

| User-facing command | CLI entry | Local daemon HTTP | Platform HTTP (`SyncClient`) |
| --- | --- | --- | --- |
| `nexus42 sync push` | `crates/nexus42/src/commands/sync.rs` → `DaemonClient::post` | `POST /v1/local/sync/push` → `handlers::sync::push` | Optional eager: `POST /v1/sync/push` (`SyncClient::push_bundle`) when `NEXUS_SYNC_EAGER_PUSH=1` |
| `nexus42 sync pull` | `crates/nexus42/src/commands/sync.rs` → `DaemonClient::post` | `POST /v1/local/sync/pull` → `handlers::sync::pull` | `POST /v1/sync/pull` (`SyncClient::pull_bundles`); requires `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN` |
| `nexus42 sync status` | `commands/sync.rs` → `DaemonClient::get` | `GET /v1/local/sync/status` | — |
| `nexus42 sync resolve` | `commands/sync.rs` → `DaemonClient::post` | `POST /v1/local/sync/resolve` | — |

| `SyncClient` method | HTTP |
| --- | --- |
| `push_bundle` | `POST {base}/v1/sync/push` |
| `pull_sync_state` | `GET {base}/v1/sync/state/{world_id}` |
| `pull_bundles` | `POST {base}/v1/sync/pull` (body: `SyncPullRequest`) |

Wire types: `schemas/cli-sync/sync-pull-request.schema.json`, `schemas/cli-sync/sync-pull-response.schema.json` → `SyncPullRequest` / `SyncPullResponse` (codegen).
