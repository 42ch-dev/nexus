# ExploreCreatorCard Schema Field Gap Fix

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three missing fields to `explore-creator-card.schema.json` so the wire type covers all `public_explore` fields identified in the platform spec matrix (Plan 16 Task 1).

**Architecture:** Pure JSON Schema update → codegen → contracts version bump. No Rust logic changes; no new crate dependencies. The platform side will consume the updated `ExploreCreatorCard` TypeScript type after running `pnpm contracts:link`.

**Tech Stack:** JSON Schema (draft-07), nexus-codegen (Node.js), tsup (TypeScript bundler)

**Cross-repo dependency:** This plan **unblocks** nexus-platform Plan 16 Task 2 (domain service implementation). Platform must run `pnpm contracts:link` after this plan merges.

---

## Background

The V1-spec wire schema sprint (`2026-04-10-v1-spec-wire-schema-sprint`) delivered a minimal `explore-creator-card.schema.json` with 7 fields. During platform Plan 16 spec matrix analysis, three `public_explore` fields were identified as missing:

| Field | Type | Rationale |
|-------|------|-----------|
| `is_platform_owned` | `boolean` | Drives "Official" badge in UI. Defined on `Creator` entity (domain schema). Product requirement. |
| `created_at` | `string` (Timestamp) | "Member since" display. Standard public directory metadata. Defined on `Creator` entity. |
| `public_world_count` | `integer` | Count of public active worlds. Derived/computed field. Already computed in platform `ExploreService`. |

Reference: platform spec matrix §3.1 (rows 5, 12) and §3.2 (row 14).

---

## Task 1: Add Missing Fields to Schema

**Files:**
- Modify: `schemas/platform/explore-creator-card.schema.json`

- [ ] **Step 1: Add `is_platform_owned` property**

Insert after the `follower_count` property (before `visibility`):

```json
    "is_platform_owned": {
      "type": "boolean",
      "default": false,
      "description": "Whether this is a platform-hosted creator (drives 'Official' badge)"
    },
```

- [ ] **Step 2: Add `created_at` property**

Insert after `is_platform_owned`:

```json
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Creator registration timestamp ('Member since' display)"
    },
```

- [ ] **Step 3: Add `public_world_count` property**

Insert after `created_at`:

```json
    "public_world_count": {
      "type": "integer",
      "minimum": 0,
      "description": "Count of public active worlds owned by this creator"
    },
```

- [ ] **Step 4: Verify the final schema validates**

Run: `pnpm run validate-schemas`
Expected: `✓ All schemas valid` (57 valid, 0 invalid)

- [ ] **Step 5: Commit**

```bash
git add schemas/platform/explore-creator-card.schema.json
git commit -m "feat(contracts): add is_platform_owned, created_at, public_world_count to ExploreCreatorCard schema"
```

---

## Task 2: Run Codegen and Rebuild Contracts

**Files:**
- Modify (generated): `packages/nexus-contracts/src/generated/ExploreCreatorCard.ts`
- Modify: `packages/nexus-contracts/src/generated/CommonTypes.ts` (if Timestamp import needed)
- Modify: `packages/nexus-contracts/package.json` (version bump)

- [ ] **Step 1: Run the codegen pipeline**

```bash
pnpm run codegen
```

This runs: `nexus-codegen build → codegen → nexus-contracts build`.
Expected: No errors. The generated `ExploreCreatorCard.ts` should now include the three new optional fields.

- [ ] **Step 2: Verify generated TypeScript**

Check `packages/nexus-contracts/src/generated/ExploreCreatorCard.ts`:

```typescript
export interface ExploreCreatorCard {
  schema_version: number;
  creator_id: string;
  display_name: string;
  bio?: string;
  avatar_url?: string;
  follower_count?: number;
  is_platform_owned?: boolean;       // NEW
  created_at?: string;                // NEW (Timestamp = date-time string)
  public_world_count?: number;        // NEW
  visibility?: Visibility;
}
```

Also verify the import line includes `Timestamp` if the codegen adds it:

```typescript
import type { SchemaVersion, Visibility, Timestamp } from './CommonTypes';
```

- [ ] **Step 3: Typecheck the contracts package**

```bash
pnpm --filter @42ch/nexus-contracts run typecheck
```

Expected: No errors.

- [ ] **Step 4: Bump contracts version to 0.2.4**

In `packages/nexus-contracts/package.json`, change:

```json
"version": "0.2.3"
```

to:

```json
"version": "0.2.4"
```

- [ ] **Step 5: Commit**

```bash
git add packages/nexus-contracts/
git commit -m "feat(contracts): codegen 0.2.4 — ExploreCreatorCard with is_platform_owned, created_at, public_world_count"
```

---

## Task 3: Verify and Merge

- [ ] **Step 1: Full schema validation**

```bash
pnpm run validate-schemas
```

Expected: `✓ All schemas valid`

- [ ] **Step 2: Verify no Rust breakage**

```bash
cargo check --workspace 2>&1 | tail -5
```

Expected: No errors. (Rust generated code should also reflect the new fields.)

- [ ] **Step 3: Verify Rust generated code**

Check `crates/nexus-contracts/src/generated/explore_creator_card.rs` — it should have the three new optional fields.

- [ ] **Step 4: Update sprint coverage matrix**

Edit `.agents/plans/reports/2026-04-10-v1-spec-wire-schema-sprint/coverage-matrix.md`:
- Update the ExploreCreatorCard row to show 10 fields (was 7), all complete.
- Add a note: "2026-04-10: gap fix — added is_platform_owned, created_at, public_world_count per platform Plan 16 spec matrix."

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "docs(16): update coverage matrix for ExploreCreatorCard gap fix"
```

---

## Completion Criteria

- [ ] `explore-creator-card.schema.json` has 10 properties (was 7)
- [ ] `ExploreCreatorCard.ts` generated interface includes `is_platform_owned`, `created_at`, `public_world_count` as optional fields
- [ ] `@42ch/nexus-contracts` version is `0.2.4`
- [ ] `pnpm run validate-schemas` passes
- [ ] `cargo check --workspace` passes
- [ ] Coverage matrix updated

## Downstream Handoff

After this plan merges to `main` in the nexus repo:

1. **nexus-platform**: Run `pnpm contracts:link` (re-links local nexus checkout)
2. **nexus-platform Plan 16**: Update spec matrix §4.1 to remove "not in current schema" notes
3. **nexus-platform Plan 16 Task 2**: `@fullstack-dev` can now implement domain service with complete wire type
