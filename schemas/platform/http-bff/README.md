# Platform HTTP BFF Wire Schemas

Request/response JSON Schemas for **platform BFF HTTP** bodies (`nexus-platform` observes these on the wire).

- **Not** daemon Local API (`/v1/local/*`) — those DTOs are in `crates/nexus-contracts/src/local/`.
- V1.20 removed **daemon proxies** for world/explore; clients call **platform HTTP** directly. Schemas here remain wire contracts for platform.
- V1.62 (2026-06-23) moved these files from flat `schemas/platform/` into `schemas/platform/http-bff/` (consumer-scope split from `platform/sync/`).

## Index (by prefix)

| Prefix | Count | Examples |
| --- | --- | --- |
| `context-assembly` | 1 | `context-assembly-v1.schema.json` |
| `explore-*` | 8 | browse, search, feed, hit, AI summary/answer |
| `memory-web-*` | 2 | list request/response |
| `notifications-*` | 5 | list, mark-read, inbox item |
| `official-creator-quota-*` | 1 | quota response |
| `publish-*` | 5 | story/chapter/history |
| `social-graph-*` | 3 | feed, relationship |
| `world-*` | 4 | fork, snapshot |
| `creator-runtime-policy-*` | 1 | policy response |
| `me-entitlements-*` | 1 | entitlements response |

**Consumer:** `@42ch/nexus-contracts` (npm) + `nexus-cloud-sync` Rust HTTP client.

**Layout spec:** [schemas-directory-layout.md](../../../.mstar/knowledge/specs/schemas-directory-layout.md) §3.1.
