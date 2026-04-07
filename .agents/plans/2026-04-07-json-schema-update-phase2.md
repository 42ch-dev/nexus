# JSON Schema Update for Phase 2 Alignment

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update nexus JSON Schemas to align with ADR-002 domain model field renaming and structure changes from nexus-platform Phase 2.

**Architecture:** This plan coordinates with nexus-platform Phase 2 Domain Model Alignment (`.agents/plans/phase2-domain-model-alignment.md`). JSON Schema updates must precede or run in parallel with nexus-platform implementation to unblock Task 8 (contract consumption).

**Tech Stack:** JSON Schema (Draft 2020-12), TypeScript codegen, Rust codegen

**Prerequisites:**
- nexus-platform Phase 2 plan locked and ready for execution
- ADR-002 approved

---

## Executive Summary

nexus-platform Phase 2 Domain Model Alignment (per ADR-002) introduces field-level changes to KeyBlock, SourceAnchor, and MemoryItem aggregates. These changes require corresponding JSON Schema updates in the nexus repository so that generated TypeScript types in `@42ch/nexus-contracts` match the updated platform models.

**Key Changes:**
- **G1**: KeyBlock field renaming (`kb_ref` → `key_block_id`, `title` → `canonical_name`, `content` → `body`)
- **G2**: SourceAnchor structure change (add `story_summary_refs` array)
- **G3**: MemoryItem missing fields (8 new fields from v1-spec §5.8)

**Impact:**
- JSON Schemas: 3 files updated
- Generated types: TypeScript + Rust
- `@42ch/nexus-contracts` package: MINOR version bump

---

## Dependency Chain

```
nexus-platform Phase 2 (Task 8)
  ↓ (blocked on)
nexus JSON Schema Update (this plan)
  ↓ (unblocks)
@42ch/nexus-contracts regeneration
  ↓ (enables)
nexus-platform contract consumption
```

**Timing**: This plan should start **immediately** or in parallel with nexus-platform Phase 2 Tasks 1-7.

---

## Task 1: Update KeyBlock JSON Schema

**File**: `schemas/key-block.schema.json`

### Changes

#### Field Renames

| Old Field | New Field | Type | Breaking Change |
|-----------|-----------|------|-----------------|
| `kb_ref` | `key_block_id` | string | Yes (field name) |
| `title` | `canonical_name` | string | Yes (field name) |
| `content` | `body` | object | Yes (structure + name) |

#### New `body` Structure

```json
{
  "body": {
    "type": "object",
    "properties": {
      "summary": {
        "type": "string",
        "description": "Human-readable summary of the key block"
      },
      "attributes": {
        "type": "object",
        "additionalProperties": true,
        "description": "Type-specific attributes (character traits, item properties, etc.)"
      },
      "tags": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Classification tags (e.g., faction:empire)"
      }
    },
    "required": ["summary"]
  }
}
```

### Migration Steps

- [ ] **Step 1**: Read current schema
  ```bash
  cat schemas/key-block.schema.json
  ```

- [ ] **Step 2**: Rename fields in properties
  - `kb_ref` → `key_block_id`
  - `title` → `canonical_name`
  - `content` → `body`

- [ ] **Step 3**: Replace `content` definition with `body` object structure

- [ ] **Step 4**: Update `required` array
  - Remove `kb_ref`, `title`, `content`
  - Add `key_block_id`, `canonical_name`, `body`

- [ ] **Step 5**: Update examples to use new field names

- [ ] **Step 6**: Validate schema
  ```bash
  pnpm run validate-schemas
  ```

- [ ] **Step 7**: Commit
  ```bash
  git add schemas/key-block.schema.json
  git commit -m "feat(schema): update KeyBlock field names per ADR-002

- Rename kb_ref → key_block_id
- Rename title → canonical_name
- Replace content with body object {summary, attributes, tags}
- Update required fields
- Update examples

Refs: ADR-002, nexus-platform Phase 2"
  ```

---

## Task 2: Update SourceAnchor JSON Schema

**File**: `schemas/source-anchor.schema.json`

### Changes

#### Structure Transformation

**Before** (current):
```json
{
  "anchor_ref": "string",
  "story_ref": "string",
  "excerpt": "string?"
}
```

**After** (v1-spec §6.1):
```json
{
  "story_summary_refs": [
    {
      "story_manifest_id": "string",
      "summary_unit_id": "string",
      "unit_kind": "chapter_summary | arc_summary | ..."
    }
  ],
  "excerpt": "string?",
  "summary": "string"
}
```

### Migration Steps

- [ ] **Step 1**: Define `SourceSummaryRef` subschema
  - Add to `schemas/common/` or inline in source-anchor schema
  - Properties: `story_manifest_id`, `summary_unit_id`, `unit_kind`
  - Enum for `unit_kind`: `chapter_summary`, `arc_summary`, `story_summary`

- [ ] **Step 2**: Update `source-anchor.schema.json`
  - Remove `anchor_ref`, `story_ref`
  - Add `story_summary_refs` (array of SourceSummaryRef)
  - Add `summary` field
  - Keep `excerpt`

- [ ] **Step 3**: Update required fields
  - Required: `story_summary_refs`, `summary`
  - Optional: `excerpt`

- [ ] **Step 4**: Add examples

- [ ] **Step 5**: Validate schema

- [ ] **Step 6**: Commit
  ```bash
  git add schemas/
  git commit -m "feat(schema): restructure SourceAnchor per ADR-002

- Replace anchor_ref/story_ref with story_summary_refs array
- Add summary field
- Define SourceSummaryRef subschema
- Align with v1-spec §6.1

Refs: ADR-002, nexus-platform Phase 2"
  ```

---

## Task 3: Update MemoryItem JSON Schema

**File**: `schemas/memory-item.schema.json`

### Changes

Add 8 missing fields from v1-spec §5.8:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `embedding_ref` | string | No | Reference to vector embedding |
| `source_refs` | array | No | Array of source references |
| `last_accessed_at` | string (datetime) | No | Last access timestamp |
| `last_reinforced_at` | string (datetime) | No | Last reinforcement timestamp |

**Note**: MemoryKind enum already updated in Phase 1 (ADR-001).

### Migration Steps

- [ ] **Step 1**: Add missing fields to schema properties

- [ ] **Step 2**: Verify MemoryKind enum has 8 values
  - Values: `story_summary`, `research_material`, `review_note`, `character_note`, `world_building`, `plot_outline`, `theme_analysis`, `custom`

- [ ] **Step 3**: Add examples

- [ ] **Step 4**: Validate schema

- [ ] **Step 5**: Commit
  ```bash
  git add schemas/memory-item.schema.json
  git commit -m "feat(schema): add missing MemoryItem fields per ADR-002

- Add embedding_ref, source_refs
- Add last_accessed_at, last_reinforced_at
- Verify MemoryKind enum (8 values from ADR-001)

Refs: ADR-002, nexus-platform Phase 2"
  ```

---

## Task 4: Regenerate Types

**Files**: Generated in `packages/nexus-contracts/src/generated/`

### Steps

- [ ] **Step 1**: Run codegen
  ```bash
  pnpm run codegen
  ```

- [ ] **Step 2**: Verify generated TypeScript types
  - Check `KeyBlock` interface has new field names
  - Check `SourceAnchor` interface has new structure
  - Check `MemoryItem` interface has new fields

- [ ] **Step 3**: Verify generated Rust types
  - Check corresponding Rust structs

- [ ] **Step 4**: Run typecheck
  ```bash
  pnpm run typecheck
  ```

- [ ] **Step 5**: Commit
  ```bash
  git add packages/nexus-contracts/src/generated/
  git commit -m "chore(codegen): regenerate types from updated schemas

Regenerated from:
- key-block.schema.json (field renames)
- source-anchor.schema.json (structure change)
- memory-item.schema.json (missing fields)

Refs: ADR-002"
  ```

---

## Task 5: Update Package Version

**File**: `packages/nexus-contracts/package.json`

### Steps

- [ ] **Step 1**: Bump MINOR version
  - Current: Check `version` field
  - New: Increment minor version (e.g., `1.0.0` → `1.1.0`)
  - Reason: New fields + breaking field renames

- [ ] **Step 2**: Update CHANGELOG
  - Add entry for version 1.1.0
  - List breaking changes (field renames)
  - List new fields

- [ ] **Step 3**: Commit
  ```bash
  git add packages/nexus-contracts/package.json packages/nexus-contracts/CHANGELOG.md
  git commit -m "chore(contracts): bump version to 1.1.0

BREAKING CHANGES:
- KeyBlock: kb_ref → key_block_id
- KeyBlock: title → canonical_name
- KeyBlock: content → body {summary, attributes, tags}
- SourceAnchor: structure changed to story_summary_refs

NEW FEATURES:
- MemoryItem: 8 new fields added

Refs: ADR-002"
  ```

---

## Task 6: Update Documentation

**File**: `README.md`, `packages/nexus-contracts/README.md`

### Steps

- [ ] **Step 1**: Update type documentation
  - Update KeyBlock field names
  - Update SourceAnchor structure
  - Document new MemoryItem fields

- [ ] **Step 2**: Add migration guide
  - For consumers upgrading from 1.0.0 to 1.1.0
  - List field renames and new fields

- [ ] **Step 3**: Commit
  ```bash
  git add README.md packages/nexus-contracts/README.md
  git commit -m "docs: update documentation for v1.1.0 field changes

- Document KeyBlock field renames
- Document SourceAnchor structure change
- Add migration guide for consumers

Refs: ADR-002"
  ```

---

## Task 7: Verification

- [ ] **Step 1**: Run full validation
  ```bash
  pnpm run validate-schemas
  pnpm run codegen
  pnpm run typecheck
  pnpm run build
  ```

- [ ] **Step 2**: Verify no breaking changes for unused fields
  - Confirm `kb_ref`, `title`, `content` are not used in nexus CLI

- [ ] **Step 3**: Verify new types compile
  - TypeScript consumers should compile successfully
  - Rust crate should build successfully

---

## Task 8: Publish (Optional)

**Prerequisite**: nexus-platform Phase 2 complete and ready to consume contracts.

- [ ] **Step 1**: Dry-run publish
  ```bash
  pnpm publish --dry-run
  ```

- [ ] **Step 2**: Publish to npm
  ```bash
  pnpm publish
  ```

- [ ] **Step 3**: Tag release
  ```bash
  git tag v1.1.0
  git push origin v1.1.0
  ```

---

## Effort Estimation (Agent-Oriented)

| Task | Sessions | Complexity |
|------|----------|------------|
| Task 1: KeyBlock schema | ~0.5 | S |
| Task 2: SourceAnchor schema | ~0.5 | S |
| Task 3: MemoryItem schema | ~0.25 | XS |
| Task 4: Regenerate types | ~0.25 | XS |
| Task 5: Version bump | ~0.1 | XS |
| Task 6: Documentation | ~0.25 | XS |
| Task 7: Verification | ~0.25 | XS |
| Task 8: Publish | ~0.1 | XS |
| **Total** | **~2.2 sessions** | **S** |

---

## Acceptance Criteria

- [ ] All JSON Schemas validate successfully
- [ ] Generated TypeScript types match nexus-platform Phase 2 models
- [ ] Generated Rust types compile
- [ ] Package version bumped to 1.1.0
- [ ] Documentation updated with migration guide
- [ ] Ready for nexus-platform Task 8 (contract consumption)

---

## Cross-Repo Coordination

| Milestone | nexus-plan | nexus-platform-plan |
|-----------|-----------|---------------------|
| Schema update complete | This plan (Tasks 1-3) | — |
| Types regenerated | This plan (Task 4) | — |
| Package published | This plan (Task 8) | — |
| Contract consumption | — | Phase 2 Task 8 |

**Communication**: When Task 4 is complete, notify nexus-platform PM that contracts are ready for consumption.

---

## Self-Review Checklist

- [ ] All TBD/TODO placeholders resolved
- [ ] No "similar to" shortcuts — each schema has explicit changes
- [ ] Field names match v1-spec exactly
- [ ] Neo4j property names (snake_case) mapped correctly
- [ ] Effort uses agent-oriented sessions only
- [ ] No references to paths outside this repository