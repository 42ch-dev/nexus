# JSON Schema Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define the foundational JSON Schema files for Nexus wire contracts, establishing schema versioning, core domain entities (Bundle, Creator, World, KeyBlock, Timeline, Memory), and validation infrastructure.

**Architecture:** JSON Schema files in `schemas/` directory, organized by domain. Each schema includes `$schema`, `$id`, `schema_version`, and type definitions. Validation tools ensure schema correctness before code generation.

**Tech Stack:** JSON Schema Draft-07, ajv (validator), json-schema-to-typescript (future codegen)

---

## Files to Create

**Create in `schemas/`:**
- `meta/meta.schema.json` - Meta schema with versioning rules
- `common/common.schema.json` - Common types (timestamps, IDs, enums)
- `domain/bundle.schema.json` - Bundle envelope schema
- `domain/creator.schema.json` - Creator entity schema
- `domain/world.schema.json` - World entity schema
- `domain/key-block.schema.json` - Key Block schema
- `domain/timeline.schema.json` - Timeline schema
- `domain/memory.schema.json` - Memory/MemoryItem schema
- `validation/schema-validator.js` - Schema validation script

---

## Task 1: Create Schema Directory Structure and Meta Schema

**Files:**
- Create: `schemas/meta/meta.schema.json`
- Create: `schemas/common/.gitkeep`, `schemas/domain/.gitkeep`, `schemas/platform/.gitkeep`

- [ ] **Step 1: Create schema subdirectories**

Run: `mkdir -p schemas/meta schemas/common schemas/domain schemas/platform schemas/cli-sync schemas/acp-runtime`

Expected: Subdirectories created

- [ ] **Step 2: Create meta schema with versioning rules**

Create file: `schemas/meta/meta.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/meta/meta.schema.json",
  "title": "Nexus Meta Schema",
  "description": "Meta schema defining schema versioning and structure rules for all Nexus schemas",
  "type": "object",
  "required": ["$schema", "$id", "schema_version", "title", "type"],
  "properties": {
    "$schema": {
      "type": "string",
      "const": "http://json-schema.org/draft-07/schema#",
      "description": "Must use JSON Schema Draft-07"
    },
    "$id": {
      "type": "string",
      "format": "uri",
      "pattern": "^https://nexus\\.42ch\\.io/schemas/.+\\.schema\\.json$",
      "description": "Schema identifier URI following Nexus namespace convention"
    },
    "schema_version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+\\.\\d+$",
      "description": "Schema version in SemVer format (e.g., '1.0.0')"
    },
    "title": {
      "type": "string",
      "minLength": 1,
      "description": "Human-readable schema title"
    },
    "description": {
      "type": "string",
      "description": "Detailed schema description"
    },
    "type": {
      "type": "string",
      "enum": ["object", "array", "string", "number", "integer", "boolean", "null"],
      "description": "Root type of schema"
    }
  },
  "additionalProperties": true
}
```

Expected: Meta schema created with versioning rules

- [ ] **Step 3: Create meta README**

Create file: `schemas/meta/README.md`

```markdown
# Meta Schemas

This directory contains meta schemas defining structure and validation rules for all Nexus schemas.

## Files

- `meta.schema.json`: Defines required fields (`$schema`, `$id`, `schema_version`, `title`, `type`) for all Nexus schemas

## Schema Versioning

All Nexus schemas must include:
- `schema_version`: SemVer string (e.g., "1.0.0")
- `$id`: URI following `https://nexus.42ch.io/schemas/<path>/<name>.schema.json`

Version bumps follow SemVer:
- **Major**: Breaking changes to existing fields
- **Minor**: New optional fields
- **Patch**: Documentation/description updates only
```

Expected: README created

- [ ] **Step 4: Commit meta schema**

Run: `git add schemas/meta && git commit -m "feat(schema): add meta schema with versioning rules"`

Expected: Commit successful

---

## Task 2: Create Common Types Schema

**Files:**
- Create: `schemas/common/common.schema.json`

- [ ] **Step 1: Create common types schema**

Create file: `schemas/common/common.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/common/common.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Common Types",
  "description": "Common type definitions shared across all Nexus domain schemas",
  "definitions": {
    "Timestamp": {
      "type": "string",
      "format": "date-time",
      "description": "ISO 8601 datetime string"
    },
    "UUID": {
      "type": "string",
      "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
      "description": "UUID v4 identifier"
    },
    "WorldRef": {
      "type": "string",
      "pattern": "^world_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
      "description": "World reference identifier (prefix: 'world_')"
    },
    "CreatorRef": {
      "type": "string",
      "pattern": "^creator_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
      "description": "Creator reference identifier (prefix: 'creator_')"
    },
    "StoryRef": {
      "type": "string",
      "pattern": "^story_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
      "description": "Story reference identifier (prefix: 'story_')"
    },
    "KBRef": {
      "type": "string",
      "pattern": "^kb_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
      "description": "Key Block reference identifier (prefix: 'kb_')"
    },
    "DeltaSequence": {
      "type": "integer",
      "minimum": 0,
      "description": "Monotonically increasing sequence number for deltas"
    },
    "ManuscriptPhase": {
      "type": "string",
      "enum": ["brainstorm", "write", "review", "provisional", "canon"],
      "description": "Manuscript lifecycle phase"
    },
    "ManuscriptState": {
      "type": "string",
      "enum": ["draft", "proposed", "confirmed", "published"],
      "description": "Manuscript state within phase"
    },
    "TimePolicy": {
      "type": "string",
      "enum": ["linear", "branching", "mergeable"],
      "description": "World timeline evolution policy"
    },
    "Visibility": {
      "type": "string",
      "enum": ["private", "shared", "public"],
      "description": "Visibility/access level"
    },
    "SchemaVersion": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+\\.\\d+$",
      "description": "Schema version in SemVer format"
    }
  }
}
```

Expected: Common types schema created with reusable definitions

- [ ] **Step 2: Create common README**

Create file: `schemas/common/README.md`

```markdown
# Common Types

Reusable type definitions imported by domain schemas.

## Definitions

### Identifiers
- `UUID`: UUID v4 string
- `WorldRef`: World reference (`world_<uuid>`)
- `CreatorRef`: Creator reference (`creator_<uuid>`)
- `StoryRef`: Story reference (`story_<uuid>`)
- `KBRef`: Key Block reference (`kb_<uuid>`)
- `DeltaSequence`: Integer sequence number

### Enums
- `ManuscriptPhase`: Lifecycle phase (brainstorm/write/review/provisional/canon)
- `ManuscriptState`: State within phase (draft/proposed/confirmed/published)
- `TimePolicy`: Timeline policy (linear/branching/mergeable)
- `Visibility`: Access level (private/shared/public)

### Timestamps
- `Timestamp`: ISO 8601 datetime string
```

Expected: README created

- [ ] **Step 3: Commit common schema**

Run: `git add schemas/common && git commit -m "feat(schema): add common types schema with reusable definitions"`

Expected: Commit successful

---

## Task 3: Create Bundle Envelope Schema

**Files:**
- Create: `schemas/domain/bundle.schema.json`

- [ ] **Step 1: Create bundle envelope schema**

Create file: `schemas/domain/bundle.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/bundle.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Bundle Envelope",
  "description": "Bundle envelope containing delta operations for world synchronization",
  "type": "object",
  "required": ["schema_version", "bundle_id", "world_ref", "submitting_creator_id", "bundle_type", " deltas", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/SchemaVersion",
      "description": "Schema version of this bundle envelope"
    },
    "bundle_id": {
      "type": "string",
      "description": "Unique bundle identifier"
    },
    "world_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldRef",
      "description": "Reference to the world this bundle applies to"
    },
    "submitting_creator_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/CreatorRef",
      "description": "ID of creator submitting this bundle"
    },
    "bundle_type": {
      "type": "string",
      "enum": ["delta", "snapshot", "fork", "merge"],
      "description": "Type of bundle operation"
    },
    "manuscript_phase": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/ManuscriptPhase",
      "description": "Optional: Manuscript phase of this bundle (recommended for downstream validation)"
    },
    "deltas": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["delta_type", "sequence"],
        "properties": {
          "delta_type": {
            "type": "string",
            "enum": ["create_kb", "update_kb", "delete_kb", "create_memory", "update_memory", "confirm_kb", "fork_world"],
            "description": "Type of delta operation"
          },
          "sequence": {
            "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/DeltaSequence",
            "description": "Delta sequence number"
          },
          "payload": {
            "type": "object",
            "description": "Delta payload (structure varies by delta_type)"
          }
        }
      },
      "minItems": 1,
      "description": "Array of delta operations in this bundle"
    },
    "last_confirmed_delta_sequence": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/DeltaSequence",
      "description": "Optional: Last confirmed delta sequence before this bundle (for conflict detection)"
    },
    "world_revision": {
      "type": "integer",
      "minimum": 0,
      "description": "Optional: World revision number (for optimistic locking)"
    },
    "created_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Bundle creation timestamp"
    },
    "metadata": {
      "type": "object",
      "properties": {
        "notes": {
          "type": "string",
          "description": "Optional notes about this bundle"
        },
        "tags": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Optional tags for categorization"
        }
      },
      "description": "Optional metadata"
    }
  },
  "additionalProperties": false
}
```

Expected: Bundle schema created

- [ ] **Step 2: Commit bundle schema**

Run: `git add schemas/domain/bundle.schema.json && git commit -m "feat(schema): add bundle envelope schema with delta operations"`

Expected: Commit successful

---

## Task 4: Create Creator Schema

**Files:**
- Create: `schemas/domain/creator.schema.json`

- [ ] **Step 1: Create creator entity schema**

Create file: `schemas/domain/creator.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/creator.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Creator Entity",
  "description": "Creator entity - a first-class citizen with pairing, permissions, and persistence layers",
  "type": "object",
  "required": ["schema_version", "creator_id", "display_name", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "creator_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/CreatorRef",
      "description": "Unique creator identifier"
    },
    "display_name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 100,
      "description": "Creator display name"
    },
    "bio": {
      "type": "string",
      "maxLength": 500,
      "description": "Optional creator bio"
    },
    "subscription_tier": {
      "type": "string",
      "enum": ["free", "basic", "pro", "official"],
      "default": "free",
      "description": "Subscription tier determining creator quotas and capabilities"
    },
    "is_official_creator": {
      "type": "boolean",
      "default": false,
      "description": "Whether this is an official (platform-hosted) creator"
    },
    "paired_user_id": {
      "type": "string",
      "description": "Optional: User ID this creator is paired with (for user-owned creators)"
    },
    "permissions": {
      "type": "array",
      "items": {
        "type": "string",
        "enum": ["read", "write", "confirm", "fork", "merge", "admin"]
      },
      "default": ["read", "write"],
      "description": "Creator permission set"
    },
    "creator_quota": {
      "type": "object",
      "properties": {
        "max_worlds": {
          "type": "integer",
          "minimum": 0,
          "description": "Maximum number of worlds this creator can maintain"
        },
        "max_kbs_per_world": {
          "type": "integer",
          "minimum": 0,
          "description": "Maximum Key Blocks per world"
        },
        "max_proposals_per_month": {
          "type": "integer",
          "minimum": 0,
          "description": "Maximum proposals per month (for official creators)"
        }
      },
      "description": "Creator quota limits based on tier"
    },
    "created_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Creator registration timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    },
    "metadata": {
      "type": "object",
      "description": "Optional metadata (avatar_url, links, etc.)"
    }
  },
  "additionalProperties": false
}
```

Expected: Creator schema created

- [ ] **Step 2: Commit creator schema**

Run: `git add schemas/domain/creator.schema.json && git commit -m "feat(schema): add creator entity schema with pairing and permissions"`

Expected: Commit successful

---

## Task 5: Create World Schema

**Files:**
- Create: `schemas/domain/world.schema.json`

- [ ] **Step 1: Create world entity schema**

Create file: `schemas/domain/world.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/world.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus World Entity",
  "description": "World entity - a narrative universe maintained by creators with timeline evolution",
  "type": "object",
  "required": ["schema_version", "world_ref", "display_name", "time_policy", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "world_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldRef",
      "description": "Unique world identifier"
    },
    "display_name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 200,
      "description": "World display name"
    },
    "description": {
      "type": "string",
      "maxLength": 1000,
      "description": "Optional world description/summary"
    },
    "creator_refs": {
      "type": "array",
      "items": {
        "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/CreatorRef"
      },
      "minItems": 1,
      "description": "Creators maintaining this world"
    },
    "time_policy": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/TimePolicy",
      "description": "Timeline evolution policy"
    },
    "visibility": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Visibility",
      "default": "private",
      "description": "World visibility/access level"
    },
    "fork_parent_world_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldRef",
      "description": "Optional: Parent world if this is a fork"
    },
    "fork_timestamp": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Optional: Timestamp when fork was created"
    },
    "current_timeline_sequence": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/DeltaSequence",
      "description": "Current timeline sequence number"
    },
    "world_revision": {
      "type": "integer",
      "minimum": 0,
      "description": "World revision number for optimistic locking"
    },
    "created_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "World creation timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    },
    "metadata": {
      "type": "object",
      "properties": {
        "tags": {
          "type": "array",
          "items": {"type": "string"},
          "description": "World tags/genres"
        },
        "cover_image_url": {
          "type": "string",
          "format": "uri",
          "description": "Optional cover image URL"
        }
      },
      "description": "Optional metadata"
    }
  },
  "additionalProperties": false
}
```

Expected: World schema created

- [ ] **Step 2: Commit world schema**

Run: `git add schemas/domain/world.schema.json && git commit -m "feat(schema): add world entity schema with fork and timeline policy"`

Expected: Commit successful

---

## Task 6: Create Key Block Schema

**Files:**
- Create: `schemas/domain/key-block.schema.json`

- [ ] **Step 1: Create key block schema**

Create file: `schemas/domain/key-block.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/key-block.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Key Block",
  "description": "Key Block - an immutable narrative event/entity in world timeline",
  "type": "object",
  "required": ["schema_version", "kb_ref", "world_ref", "kb_type", "sequence", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "kb_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/KBRef",
      "description": "Unique Key Block identifier"
    },
    "world_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldRef",
      "description": "World this KB belongs to"
    },
    "kb_type": {
      "type": "string",
      "enum": ["event", "character", "location", "object", "concept", "relationship"],
      "description": "Type of Key Block content"
    },
    "sequence": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/DeltaSequence",
      "description": "Timeline sequence number"
    },
    "title": {
      "type": "string",
      "minLength": 1,
      "maxLength": 200,
      "description": "KB title"
    },
    "content": {
      "type": "object",
      "description": "KB content (structure varies by kb_type)"
    },
    "confirming_creator_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/CreatorRef",
      "description": "Optional: Creator who confirmed this KB"
    },
    "confirmed_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Optional: Confirmation timestamp"
    },
    "can_confirm_canon": {
      "type": "boolean",
      "default": false,
      "description": "Whether this KB can be confirmed as canon"
    },
    "source_anchor_refs": {
      "type": "array",
      "items": {
        "type": "string",
        "description": "SourceAnchor reference"
      },
      "description": "Optional: Source anchors for this KB (story references)"
    },
    "created_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "KB creation timestamp"
    },
    "metadata": {
      "type": "object",
      "description": "Optional metadata"
    }
  },
  "additionalProperties": false
}
```

Expected: Key Block schema created

- [ ] **Step 2: Commit key block schema**

Run: `git add schemas/domain/key-block.schema.json && git commit -m "feat(schema): add key block schema with confirmation and source anchors"`

Expected: Commit successful

---

## Task 7: Create Timeline and Memory Schemas

**Files:**
- Create: `schemas/domain/timeline.schema.json`
- Create: `schemas/domain/memory.schema.json`

- [ ] **Step 1: Create timeline schema**

Create file: `schemas/domain/timeline.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/timeline.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Timeline",
  "description": "Timeline - ordered sequence of Key Blocks and events in a world",
  "type": "object",
  "required": ["schema_version", "world_ref", "current_sequence", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "world_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldRef",
      "description": "World this timeline belongs to"
    },
    "current_sequence": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/DeltaSequence",
      "description": "Current maximum sequence number in timeline"
    },
    "timeline_type": {
      "type": "string",
      "enum": ["main", "branch", "fork"],
      "default": "main",
      "description": "Timeline type"
    },
    "branch_parent_timeline_id": {
      "type": "string",
      "description": "Optional: Parent timeline if this is a branch/fork"
    },
    "kb_refs": {
      "type": "array",
      "items": {
        "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/KBRef"
      },
      "description": "Ordered list of Key Block references in this timeline"
    },
    "created_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Timeline creation timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    }
  },
  "additionalProperties": false
}
```

Expected: Timeline schema created

- [ ] **Step 2: Create memory schema**

Create file: `schemas/domain/memory.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/memory.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Memory/MemoryItem",
  "description": "Memory - structured memory item for creator experience and world context",
  "type": "object",
  "required": ["schema_version", "memory_id", "world_ref", "memory_type", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "memory_id": {
      "type": "string",
      "description": "Unique memory identifier"
    },
    "world_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldRef",
      "description": "World this memory belongs to"
    },
    "creator_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/CreatorRef",
      "description": "Creator who created this memory"
    },
    "memory_type": {
      "type": "string",
      "enum": ["experience", "soul", "knowledge", "reference_excerpt"],
      "description": "Type of memory item"
    },
    "title": {
      "type": "string",
      "minLength": 1,
      "maxLength": 200,
      "description": "Memory title"
    },
    "content": {
      "type": "object",
      "properties": {
        "text": {
          "type": "string",
          "description": "Memory text content"
        },
        "structured_data": {
          "type": "object",
          "description": "Optional structured data (varies by memory_type)"
        }
      },
      "description": "Memory content"
    },
    "kb_ref": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/KBRef",
      "description": "Optional: Associated Key Block"
    },
    "source_anchor_ref": {
      "type": "string",
      "description": "Optional: Source anchor for reference excerpts"
    },
    "weight": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "default": 0.5,
      "description": "Memory retrieval weight (0-1)"
    },
    "created_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Memory creation timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    },
    "metadata": {
      "type": "object",
      "description": "Optional metadata"
    }
  },
  "additionalProperties": false
}
```

Expected: Memory schema created

- [ ] **Step 3: Commit timeline and memory schemas**

Run: `git add schemas/domain/timeline.schema.json schemas/domain/memory.schema.json && git commit -m "feat(schema): add timeline and memory schemas"`

Expected: Commit successful

---

## Task 8: Create Schema Validation Script

**Files:**
- Create: `tooling/validation/schema-validator.js`
- Create: `tooling/validation/package.json`

- [ ] **Step 1: Create validation directory**

Run: `mkdir -p tooling/validation`

Expected: Directory created

- [ ] **Step 2: Create validation package.json**

Create file: `tooling/validation/package.json`

```json
{
  "name": "nexus-schema-validator",
  "version": "0.1.0",
  "private": true,
  "description": "JSON Schema validation tool for Nexus schemas",
  "main": "schema-validator.js",
  "scripts": {
    "validate": "node schema-validator.js"
  },
  "dependencies": {
    "ajv": "^8.12.0"
  }
}
```

Expected: Package manifest created

- [ ] **Step 3: Create schema validator script**

Create file: `tooling/validation/schema-validator.js`

```javascript
#!/usr/bin/env node

/**
 * Nexus Schema Validator
 *
 * Validates all JSON Schema files in schemas/ directory against:
 * 1. JSON Schema Draft-07 specification
 * 2. Nexus meta schema requirements ($id, schema_version, etc.)
 */

const Ajv = require('ajv');
const addFormats = require('ajv-formats');
const fs = require('fs');
const path = require('path');

// Initialize AJV validator
const ajv = new Ajv({ strict: true, allErrors: true });
addFormats(ajv);

// Meta schema requirements
const META_SCHEMA = {
  type: 'object',
  required: ['$schema', '$id', 'schema_version', 'title', 'type'],
  properties: {
    $schema: { const: 'http://json-schema.org/draft-07/schema#' },
    $id: { type: 'string', format: 'uri' },
    schema_version: { type: 'string', pattern: '^\\d+\\.\\d+\\.\\d+$' },
    title: { type: 'string', minLength: 1 },
    type: { type: 'string' }
  }
};

function findSchemaFiles(dir) {
  const files = [];
  const items = fs.readdirSync(dir, { withFileTypes: true });
  
  for (const item of items) {
    const fullPath = path.join(dir, item.name);
    if (item.isDirectory()) {
      files.push(...findSchemaFiles(fullPath));
    } else if (item.isFile() && item.name.endsWith('.schema.json')) {
      files.push(fullPath);
    }
  }
  
  return files;
}

function validateSchema(filePath) {
  console.log(`Validating: ${filePath}`);
  
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    const schema = JSON.parse(content);
    
    // Check meta requirements
    const metaValid = ajv.validate(META_SCHEMA, schema);
    if (!metaValid) {
      console.error(`  ❌ Meta validation failed:`);
      console.error(ajv.errorsText(ajv.errors));
      return false;
    }
    
    // Validate against JSON Schema Draft-07
    const valid = ajv.validateSchema(schema);
    if (!valid) {
      console.error(`  ❌ Schema syntax validation failed:`);
      console.error(ajv.errorsText(ajv.errors));
      return false;
    }
    
    console.log(`  ✓ Valid`);
    return true;
    
  } catch (err) {
    console.error(`  ❌ Error: ${err.message}`);
    return false;
  }
}

function main() {
  const schemasDir = path.join(__dirname, '..', '..', 'schemas');
  
  if (!fs.existsSync(schemasDir)) {
    console.error('schemas/ directory not found');
    process.exit(1);
  }
  
  console.log('Nexus Schema Validator');
  console.log('======================');
  console.log('');
  
  const schemaFiles = findSchemaFiles(schemasDir);
  
  if (schemaFiles.length === 0) {
    console.log('No schema files found');
    process.exit(0);
  }
  
  console.log(`Found ${schemaFiles.length} schema files`);
  console.log('');
  
  let validCount = 0;
  let invalidCount = 0;
  
  for (const file of schemaFiles) {
    if (validateSchema(file)) {
      validCount++;
    } else {
      invalidCount++;
    }
  }
  
  console.log('');
  console.log('Summary:');
  console.log(`  Valid: ${validCount}`);
  console.log(`  Invalid: ${invalidCount}`);
  
  if (invalidCount > 0) {
    console.error('\n❌ Validation failed');
    process.exit(1);
  } else {
    console.log('\n✓ All schemas valid');
    process.exit(0);
  }
}

main();
```

Expected: Validator script created

- [ ] **Step 4: Install validation dependencies**

Run: `cd tooling/validation && npm install`

Expected: Dependencies installed

- [ ] **Step 5: Run validation**

Run: `cd tooling/validation && npm run validate`

Expected: All schemas validated successfully

- [ ] **Step 6: Update root package.json to include validation script**

Read `package.json` and add to scripts:

```json
{
  "scripts": {
    "validate-schemas": "node tooling/validation/schema-validator.js"
  }
}
```

Expected: Validation script added to root package

- [ ] **Step 7: Commit validation tooling**

Run: `git add tooling/validation package.json && git commit -m "feat(tooling): add schema validation tool with AJV"`

Expected: Commit successful

---

## Task 9: Create Domain README and Update CI

**Files:**
- Create: `schemas/domain/README.md`
- Modify: `.github/workflows/ci.yml` (add schema validation step)

- [ ] **Step 1: Create domain schemas README**

Create file: `schemas/domain/README.md`

```markdown
# Domain Schemas

Core domain entity schemas for Nexus.

## Schemas

### Bundle (`bundle.schema.json`)
Envelope for delta operations (create/update/delete KB, memory, world fork, etc.)

### Creator (`creator.schema.json`)
First-class citizen with pairing, permissions, subscription tiers, and quotas.

### World (`world.schema.json`)
Narrative universe with timeline evolution policy (linear/branching/mergeable), fork support.

### Key Block (`key-block.schema.json`)
Immutable narrative event/entity in world timeline. Can be confirmed as canon.

### Timeline (`timeline.schema.json`)
Ordered sequence of Key Blocks in a world.

### Memory (`memory.schema.json`)
Structured memory item (experience, soul, knowledge, reference excerpt).

## Common Types

All domain schemas import reusable types from `schemas/common/common.schema.json`:
- Identifiers (UUID, WorldRef, CreatorRef, KBRef, etc.)
- Enums (ManuscriptPhase, ManuscriptState, TimePolicy, Visibility)
- Timestamps

## Validation

Run: `pnpm run validate-schemas`
```

Expected: Domain README created

- [ ] **Step 2: Update CI workflow to validate schemas**

Read `.github/workflows/ci.yml`, update `validate-schemas` job:

```yaml
  validate-schemas:
    name: Validate JSON Schemas
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - name: Install validation dependencies
        run: cd tooling/validation && npm install
      - name: Validate schemas
        run: node tooling/validation/schema-validator.js
```

Expected: CI workflow updated

- [ ] **Step 3: Commit README and CI update**

Run: `git add schemas/domain/README.md .github/workflows/ci.yml && git commit -m "docs(schema): add domain README and update CI for schema validation"`

Expected: Commit successful

---

## Verification

- [ ] **Final verification: Validate all schemas**

Run: `pnpm run validate-schemas`

Expected: All schemas pass validation with no errors

- [ ] **Verify schema structure**

Run: `tree schemas -L 2`

Expected output:
```
schemas/
├── common/
│   ├── README.md
│   └── common.schema.json
├── domain/
│   ├── README.md
│   ├── bundle.schema.json
│   ├── creator.schema.json
│   ├── key-block.schema.json
│   ├── memory.schema.json
│   ├── timeline.schema.json
│   └── world.schema.json
├── meta/
│   ├── README.md
│   └── meta.schema.json
├── platform/
├── cli-sync/
└── acp-runtime/
```

- [ ] **Verify CI workflow**

Run: `cat .github/workflows/ci.yml`

Expected: `validate-schemas` job includes validation script execution

---

## Completion

After all tasks complete:
- [ ] Update `.agents/plans/status.json` with completion status
- [ ] Create git tag: `git tag v0.1.0-schemas -a -m "Phase 0: JSON Schema foundation initialized"`
- [ ] Push to remote: `git push origin main --tags`

---

**Plan saved to:** `.agents/plans/2025-04-05-json-schema-foundation.md`