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

**Goal**: Update nexus JSON Schemas to align with ADR-002 domain model changes.

**Expected Changes** (from plan):
- **G1**: KeyBlock field renaming (`kb_ref` → `key_block_id`, `title` → `canonical_name`, `content` → `body`)
- **G2**: SourceAnchor structure change (add `story_summary_refs` array)
- **G3**: MemoryItem missing fields (assumed 8 new fields)

**Actual Work Performed**:
- **G1** (KeyBlock): ✅ Already aligned with ADR-002 — no changes needed
- **G2** (SourceAnchor): ✅ Already aligned with ADR-002 — no changes needed
- **G3** (MemoryItem): ⚠️ Fixed `memory_kind` enum (4→8 values per ADR-001)
  - Plan assumption was incorrect: fields already present
  - Real issue: enum incomplete

**Actual Impact**:
- JSON Schemas: 1 file updated (memory.schema.json)
- Generated types: TypeScript + Rust regenerated
- `@42ch/nexus-contracts` package: 0.1.0 → 0.2.0
- CI: Fixed pnpm version mismatch (8→9)

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

- [x] **Step 1**: Read current schema
  ```bash
  cat schemas/key-block.schema.json
  ```

- [x] **Step 2**: Verify field names already correct
  - ✅ `key_block_id` (not `kb_ref`)
  - ✅ `canonical_name` (not `title`)
  - ✅ `body` object with `{summary, attributes, tags}` (not `content`)

- [x] **Step 3**: Verify body structure matches ADR-002

- [x] **Step 4**: Validate schema
  ```bash
  pnpm run validate-schemas
  ```

**Result**: KeyBlock schema already aligned with ADR-002. No changes needed.

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

- [x] **Step 1**: Read current schema
  - Verified `source-anchor.schema.json` already has correct structure

- [x] **Step 2**: Verify `SourceSummaryRef` inline definition
  - ✅ `story_manifest_id` (pattern: `^stm_[a-zA-Z0-9]+$`)
  - ✅ `summary_unit_id` (pattern: `^sum_[a-zA-Z0-9]+$`)
  - ✅ `unit_kind` field

- [x] **Step 3**: Verify structure matches ADR-002
  - ✅ `story_summary_refs` array
  - ✅ `excerpt` field
  - ✅ `summary` field

- [x] **Step 4**: Validate schema
  ```bash
  pnpm run validate-schemas
  ```

**Result**: SourceAnchor schema already aligned with ADR-002. No changes needed.

---

## Task 3: Update MemoryItem JSON Schema

**File**: `schemas/memory-item.schema.json`

### Changes

**Issue Identified**: MemoryKind enum was incomplete, not missing fields.

- ❌ Plan assumption: "Add 8 missing fields"
- ✅ Actual issue: `memory_kind` enum had only 4 values, should have 8 (per ADR-001)

**Fields already present in schema**:
- `embedding_ref` ✅
- `source_refs` ✅
- `last_accessed_at` ✅
- `last_reinforced_at` ✅

**Enum fix required**:
- Old: `["generic", "story_summary", "research_material", "review_note"]` (4 values)
- New: `["story_summary", "research_material", "review_note", "character_note", "world_building", "plot_outline", "theme_analysis", "custom"]` (8 values per ADR-001)

### Migration Steps

- [x] **Step 1**: Read current schema
  ```bash
  cat schemas/domain/memory.schema.json
  ```

- [x] **Step 2**: Update MemoryKind enum to 8 values
  - Removed: `"generic"`
  - Added: `"character_note"`, `"world_building"`, `"plot_outline"`, `"theme_analysis"`, `"custom"`

- [x] **Step 3**: Validate schema
  ```bash
  pnpm run validate-schemas
  ```

- [x] **Step 4**: Commit
  ```bash
  git add schemas/domain/memory.schema.json
  git commit -m "feat(schema): update MemoryItem memory_kind enum per ADR-001

- Expand memory_kind enum from 4 to 8 values
- Remove 'generic', add 5 new values
- Align with ADR-001 and v1-spec §5.8

Refs: ADR-001, ADR-002"
  ```

**Result**: MemoryItem `memory_kind` enum updated. All fields already present.

---

## Task 4: Regenerate Types

**Files**: Generated in `packages/nexus-contracts/src/generated/`

### Steps

- [x] **Step 1**: Run codegen
  ```bash
  pnpm run codegen
  ```

- [x] **Step 2**: Verify generated TypeScript types
  - ✅ `Memory` interface has updated `memory_kind` enum (8 values)

- [x] **Step 3**: Verify generated Rust types
  - ✅ Rust structs generated successfully

- [x] **Step 4**: Run typecheck
  ```bash
  pnpm run typecheck
  ```

- [x] **Step 5**: Commit (combined with schema changes)
  - Committed as part of schema update commit

**Result**: TypeScript and Rust types regenerated successfully. 22 schemas validated.

---

## Task 5: Update Package Version

**File**: `packages/nexus-contracts/package.json`

### Steps

- [x] **Step 1**: Bump version
  - Current: `0.1.0`
  - New: `0.2.0` (pre-release minor bump)
  - Reason: MemoryKind enum expansion (ADR-001)

- [x] **Step 2**: Create CHANGELOG
  - Created `packages/nexus-contracts/CHANGELOG.md`
  - Documented MemoryKind enum changes
  - Listed alignment with ADR-001, ADR-002
  - Included migration guide

- [x] **Step 3**: Commit
  ```bash
  git add packages/nexus-contracts/package.json packages/nexus-contracts/CHANGELOG.md
  ```

**Result**: Version bumped to 0.2.0 with comprehensive CHANGELOG.

---

## Task 6: Update Documentation

**File**: `README.md`, `packages/nexus-contracts/README.md`

### Steps

- [x] **Step 1**: Verify README.md
  - ✅ Root README.md already comprehensive
  - No changes needed

- [x] **Step 2**: CHANGELOG.md provides migration guide
  - ✅ Created with detailed changes and references

**Result**: Documentation adequate. Migration guide in CHANGELOG.md.

---

## Task 7: Verification

- [x] **Step 1**: Run full validation
  ```bash
  pnpm run validate-schemas  # ✅ 22 schemas valid
  pnpm run codegen          # ✅ TypeScript + Rust generated
  pnpm run typecheck        # ✅ All types pass
  pnpm run build            # ✅ Build successful
  ```

- [x] **Step 2**: Verify CI passes
  - ✅ GitHub Actions CI all jobs passed
  - ✅ Schema validation, codegen, Rust fmt/clippy, TypeScript, tests all green

- [x] **Step 3**: Verify generated types
  - ✅ TypeScript consumers compile successfully
  - ✅ Rust crate builds successfully

**Result**: All validations passed. CI green.

---

## Task 8: Publish (Skipped)

**Decision**: Skip publishing. Package ready but not published per user request.

**Status**:
- [x] Code ready for publish
- [ ] Publishing skipped (not requested)
- [ ] Git tag skipped (not requested)

**Rationale**: User requested merge without publish or notification.

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

- [x] All JSON Schemas validate successfully (22/22 valid)
- [x] Generated TypeScript types match nexus-platform Phase 2 models
- [x] Generated Rust types compile
- [x] Package version bumped to 0.2.0
- [x] Documentation updated with migration guide (CHANGELOG.md)
- [x] Ready for nexus-platform Task 8 (contract consumption)

---

## Execution Summary

**Actual Work Performed**:

1. **KeyBlock Schema**: Already aligned with ADR-002 ✅
   - No changes needed
   - Fields: `key_block_id`, `canonical_name`, `body` already correct

2. **SourceAnchor Schema**: Already aligned with ADR-002 ✅
   - No changes needed
   - Structure: `story_summary_refs`, `summary`, `excerpt` already correct

3. **MemoryItem Schema**: Fixed `memory_kind` enum ⚠️
   - Updated enum from 4 to 8 values (per ADR-001)
   - All fields already present (embedding_ref, source_refs, last_accessed_at, last_reinforced_at)

**Key Findings**:
- Plan assumption "add 8 missing fields" was incorrect
- Actual issue: MemoryKind enum incomplete (4 vs 8 values)
- KeyBlock and SourceAnchor already correct in current schemas

**Commits**:
- `818dc6e`: Schema update + codegen + version bump
- `430ad1f`: status.json update
- `e30d18f`: CI pnpm version fix

**Files Changed** (4):
- `schemas/domain/memory.schema.json`
- `packages/nexus-contracts/src/generated/Memory.ts`
- `packages/nexus-contracts/package.json`
- `packages/nexus-contracts/CHANGELOG.md`

**Validation**:
- ✅ 22 schemas valid
- ✅ Codegen successful
- ✅ Typecheck pass
- ✅ CI all jobs green
- ✅ Rust tests pass (480+ tests)

**Effort**: S — ~1 agent session (less than estimated ~2.2 sessions)

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