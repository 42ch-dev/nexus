# 2026-04-10-platform-http-schema-alignment

**Goal**: Update nexus JSON Schemas to capture the full HTTP API request shapes used by nexus-platform, enabling the platform to import wire types instead of hand-defining validators.

**Trigger**: Plan 22 (nexus-platform) identified that all platform validators are INCOMPATIBLE with existing contract types — nexus schemas lack fields the platform needs (title, summary, idempotency, pagination controls).

**Background**: The existing schemas were designed for CLI/sync-oriented interactions. The platform HTTP API has additional concerns (display fields, idempotency, pagination) that need contract-level representation. This plan brings schemas into alignment with actual API shapes.

**Depends on**: None (orthogonal to feature waves)
**Blocks**: nexus-platform Plan 22 (T3, T5 — publish & fork/snapshot validator migration)

---

## Current State (Gap Analysis)

| Schema | Current Fields | Platform Needs | Gap |
|--------|---------------|----------------|-----|
| `publish-story-request` | schema_version, world_id, manuscript_id, story_manifest_id? | +title, +summary?, +chapter_ids, +idempotency_key, +sync_command_id? | Missing 4 fields |
| `publish-chapter-request` | **DOES NOT EXIST** | world_id, story_manifest_id, title, idempotency_key, sync_command_id? | **New schema** |
| `publish-history-request` | schema_version, world_id, manuscript_id, cursor?, limit? | world_id?, artifact_type?, limit?, cursor? | manuscript_id not used by platform; missing artifact_type |
| `world-fork-request` | schema_version, parent_world_id*, child_world_id*, forked_from_event_id*, created_by_creator_id* | fork_title?, forked_from_event_id? (rest from URL/auth) | Most required fields should be optional or removed |
| `world-snapshot-request` | schema_version, world_id, at_event_id? | +branch_id?, +key_block_limit?, +timeline_event_limit? | Missing 3 pagination/filter fields |

\* `additionalProperties: false` blocks extension.

---

## Tasks

### Task 1: Update `publish-story-request.schema.json`

Add fields to capture the full platform API shape:

- [ ] Add `title` (string, required, min 1, max 256)
- [ ] Add `summary` (string, optional, max 4096)
- [ ] Add `chapter_ids` (array of string, required, min 1)
- [ ] Add `idempotency_key` (string, required)
- [ ] Add `sync_command_id` (string, optional)
- [ ] Make `story_manifest_id` remain optional
- [ ] Remove `manuscript_id` from required (platform derives or doesn't use)

### Task 2: Create `publish-chapter-request.schema.json`

New schema for `POST /v1/publish/chapters`:

- [ ] Required: `schema_version`, `world_id`, `story_manifest_id`, `idempotency_key`
- [ ] Optional: `title` (string), `summary` (string), `sync_command_id` (string)

### Task 3: Update `publish-history-request.schema.json`

- [ ] Make `manuscript_id` optional (platform doesn't require it)
- [ ] Add `artifact_type` (enum: "chapter", "story", optional) — platform's filter

### Task 4: Update `world-fork-request.schema.json`

The platform route derives parent_world_id from URL, created_by_creator_id from auth, and child_world_id server-side. Only `forked_from_event_id` (and optionally `fork_title`) comes from the body.

- [ ] Make `parent_world_id` optional (can come from URL path param)
- [ ] Make `child_world_id` optional (server-generated)
- [ ] Make `created_by_creator_id` optional (injected from auth)
- [ ] Add `fork_title` (string, optional)
- [ ] Keep `forked_from_event_id` optional

### Task 5: Update `world-snapshot-request.schema.json`

- [ ] Add `branch_id` (string, optional)
- [ ] Add `key_block_limit` (integer, optional, min 1, max 500)
- [ ] Add `timeline_event_limit` (integer, optional, min 1, max 200)
- [ ] Keep `at_event_id` optional

### Task 6: Verify Explore AI schemas

- [ ] Check if `explore-ai-answer-request` and `explore-ai-summary-request` match platform validators
- [ ] Update if gaps found (expected to be minor — Plan 19 was built recently)

### Task 7: Regenerate contracts

- [ ] Run `pnpm run codegen` in nexus
- [ ] Build `nexus/packages/nexus-contracts`
- [ ] Bump contracts version (patch or minor depending on breaking changes)
- [ ] Verify generated types include new fields

---

## Out of Scope

- Context Assembly schemas (`context-assembly-v1`) — already consumed by platform, minor differences acceptable
- Social Graph schemas — not yet consumed, can be addressed later
- Notification schemas — not yet consumed, can be addressed later
- Memory Web schemas — not yet consumed, can be addressed later
- Adding OpenAPI output — V1.2 concern
