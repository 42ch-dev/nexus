# Handoff — V1-Spec wire schema sprint (nexus OSS)

**Date:** 2026-04-10  
**npm:** `@42ch/nexus-contracts` **0.2.2** (additive; same wire `schema_version` / `LATEST_SCHEMA_VERSION` **1**).  
**Rust crate:** `nexus-contracts` **0.1.0** (workspace; lock consumers on `schema_version`).

## New generated types (representative)

Platform wire DTOs under `schemas/platform/` — all re-exported from `packages/nexus-contracts/src/generated/index.ts` and `crates/nexus-contracts/src/generated/mod.rs`:

- **16 / W3:** `ExploreCreatorCard`
- **17:** `SocialGraphRelationshipRequest`, `SocialGraphRelationshipResponse`, `SocialGraphFeedRequest`, `SocialGraphFeedResponse`
- **18:** `MemoryWebListRequest`, `MemoryWebListResponse`
- **19:** `ExploreAiAnswerRequest`, `ExploreAiAnswerResponse`, `ExploreAiSummaryRequest`, `ExploreAiSummaryResponse`
- **20:** `NotificationsInboxItem`, `NotificationsListRequest`, `NotificationsListResponse`, `NotificationsMarkReadRequest`, `NotificationsMarkReadResponse`

## Platform smoke

After `pnpm install` in **nexus-platform** (or consumer), bump semver lock to `@42ch/nexus-contracts@0.2.2` and run your existing `contracts:link` / typecheck workflow. No breaking changes vs `0.2.1` for existing exports.

## Codegen change

`tooling/codegen/src/ts-generator.ts`: array-of-`$ref` properties now propagate `commonRef` / `deltaRef` into per-file `import type { … } from './CommonTypes'`.
