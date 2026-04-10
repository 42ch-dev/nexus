# JSON Schema Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define the foundational JSON Schema files for Nexus wire contracts, establishing schema versioning, core domain entities (DeltaBundle, Creator, World, KeyBlock, TimelineEvent, MemoryItem), and validation infrastructure.

**Architecture:** JSON Schema files in `schemas/` directory, organized by domain. Each schema includes `$schema`, `$id`, `schema_version` (integer), and type definitions. Validation tools ensure schema correctness before code generation.

**Tech Stack:** JSON Schema Draft-07, ajv (validator), json-schema-to-typescript (future codegen)

**Reference alignment:** All enums and field names follow V1.0 spec enum and aggregate definitions. Bundle envelope follows `bundle.schema.json`.

---

## Files to Create

**Create in `schemas/`:**
- `meta/meta.schema.json` - Meta schema with versioning rules
- `common/common.schema.json` - Common types (timestamps, IDs, enums)
- `common/source-anchor.schema.json` - SourceAnchor value object
- `common/version-ref.schema.json` - VersionRef value object
- `domain/bundle.schema.json` - DeltaBundle envelope schema
- `domain/creator.schema.json` - Creator entity schema
- `domain/pairing.schema.json` - Pairing entity schema
- `domain/world.schema.json` - World entity schema
- `domain/world-membership.schema.json` - WorldMembership entity schema
- `domain/key-block.schema.json` - KeyBlock schema
- `domain/timeline-event.schema.json` - TimelineEvent schema (replaces old timeline.schema.json)
- `domain/memory.schema.json` - MemoryItem schema
- `domain/story-manifest.schema.json` - StoryManifest schema
- `domain/sync-command.schema.json` - SyncCommand schema
- `domain/outbox-entry.schema.json` - OutboxEntry schema
- `tooling/validation/schema-validator.js` - Schema validation script

---

## Task 1: Create Schema Directory Structure and Meta Schema

**Files:**
- Create: `schemas/meta/meta.schema.json`
- Create: `schemas/common/.gitkeep`, `schemas/domain/.gitkeep`, `schemas/platform/.gitkeep`

- [x] **Step 1: Create schema subdirectories**

Run: `mkdir -p schemas/meta schemas/common schemas/domain schemas/platform schemas/cli-sync schemas/acp-runtime`

Expected: Subdirectories created

- [x] **Step 2: Create meta schema with versioning rules**

Create file: `schemas/meta/meta.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/meta/meta.schema.json",
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
      "type": "integer",
      "minimum": 1,
      "description": "Schema version as integer (e.g., 1)"
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

- [x] **Step 3: Create meta README**

Create file: `schemas/meta/README.md`

```markdown
# Meta Schemas

This directory contains meta schemas defining structure and validation rules for all Nexus schemas.

## Files

- `meta.schema.json`: Defines required fields (`$schema`, `$id`, `schema_version`, `title`, `type`) for all Nexus schemas

## Schema Versioning

All Nexus schemas must include:
- `schema_version`: Integer (e.g., `1`)
- `$id`: URI following `https://nexus42.invalid/schemas/<path>/<name>.schema.json`

Version bumps follow integer monotonic increment:
- **Breaking changes**: Increment to next integer
- **Backward-compatible additions**: Patch-level description updates only (same integer)
```

Expected: README created

- [x] **Step 4: Commit meta schema**

Run: `git add schemas/meta && git commit -m "feat(schema): add meta schema with versioning rules"`

Expected: Commit successful

---

## Task 2: Create Common Types Schema

**Files:**
- Create: `schemas/common/common.schema.json`

- [x] **Step 1: Create common types schema**

Create file: `schemas/common/common.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/common/common.schema.json",
  "schema_version": 1,
  "title": "Nexus Common Types",
  "description": "Common type definitions shared across all Nexus domain schemas.",
  "definitions": {
    "Timestamp": {
      "type": "string",
      "format": "date-time",
      "description": "ISO 8601 / RFC 3339 UTC datetime string"
    },
    "WorldId": {
      "type": "string",
      "pattern": "^wld_[a-zA-Z0-9]+$",
      "description": "World ID (prefix: 'wld_')"
    },
    "CreatorId": {
      "type": "string",
      "pattern": "^ctr_[a-zA-Z0-9]+$",
      "description": "Creator ID (prefix: 'ctr_')"
    },
    "UserId": {
      "type": "string",
      "pattern": "^usr_[a-zA-Z0-9]+$",
      "description": "User ID (prefix: 'usr_')"
    },
    "KeyBlockId": {
      "type": "string",
      "pattern": "^kb_[a-zA-Z0-9]+$",
      "description": "KeyBlock ID (prefix: 'kb_')"
    },
    "TimelineEventId": {
      "type": "string",
      "pattern": "^evt_[a-zA-Z0-9]+$",
      "description": "TimelineEvent ID (prefix: 'evt_')"
    },
    "BundleId": {
      "type": "string",
      "pattern": "^bdl_[a-zA-Z0-9]+$",
      "description": "DeltaBundle ID (prefix: 'bdl_')"
    },
    "CommandId": {
      "type": "string",
      "pattern": "^cmd_[a-zA-Z0-9]+$",
      "description": "SyncCommand ID (prefix: 'cmd_')"
    },
    "WorkspaceId": {
      "type": "string",
      "pattern": "^wrk_[a-zA-Z0-9]+$",
      "description": "Workspace ID (prefix: 'wrk_')"
    },
    "DeltaSequence": {
      "type": "integer",
      "minimum": 0,
      "description": "Monotonically increasing sequence number for deltas"
    },
    "ManuscriptPhase": {
      "type": "string",
      "enum": ["brainstorm", "draft", "review", "finalize", "published"],
      "description": "Manuscript lifecycle phase"
    },
    "TimePolicy": {
      "type": "string",
      "enum": ["manual", "owner_driven", "event_driven"],
      "description": "World timeline evolution policy"
    },
    "Visibility": {
      "type": "string",
      "enum": ["private", "unlisted", "public"],
      "description": "Visibility/access level"
    },
    "BlockType": {
      "type": "string",
      "enum": ["character", "ability", "scene", "organization", "item", "conflict", "info_point", "event"],
      "description": "KeyBlock content type"
    },
    "MemoryType": {
      "type": "string",
      "enum": ["canon", "working", "experience"],
      "description": "MemoryItem type"
    },
    "BundleType": {
      "type": "string",
      "enum": ["world_sync", "memory_sync", "publish_metadata"],
      "description": "DeltaBundle type"
    },
    "SchemaVersion": {
      "type": "integer",
      "minimum": 1,
      "description": "Schema version as integer (e.g., 1)"
    }
  }
}
```

Expected: Common types schema created with reusable definitions

- [x] **Step 2: Create common README**

Create file: `schemas/common/README.md`

```markdown
# Common Types

Reusable type definitions imported by domain schemas.

## Definitions

### Identifiers
- `WorldId`: World ID (`wld_xxx`)
- `CreatorId`: Creator ID (`ctr_xxx`)
- `UserId`: User ID (`usr_xxx`)
- `KeyBlockId`: KeyBlock ID (`kb_xxx`)
- `TimelineEventId`: TimelineEvent ID (`evt_xxx`)
- `BundleId`: DeltaBundle ID (`bdl_xxx`)
- `CommandId`: SyncCommand ID (`cmd_xxx`)
- `WorkspaceId`: Workspace ID (`wrk_xxx`)
- `DeltaSequence`: Integer sequence number

### Enums
- `ManuscriptPhase`: brainstorm / draft / review / finalize / published
- `TimePolicy`: manual / owner_driven / event_driven
- `Visibility`: private / unlisted / public
- `BlockType`: character / ability / scene / organization / item / conflict / info_point / event
- `MemoryType`: canon / working / experience
- `BundleType`: world_sync / memory_sync / publish_metadata

### Timestamps
- `Timestamp`: ISO 8601 / RFC 3339 UTC datetime string
```

Expected: README created

- [x] **Step 3: Commit common schema**

Run: `git add schemas/common && git commit -m "feat(schema): add common types schema with reusable definitions"`

Expected: Commit successful

---

## Task 3: Create DeltaBundle Envelope Schema

**Files:**
- Create: `schemas/domain/bundle.schema.json`

- [x] **Step 1: Create DeltaBundle envelope schema**

Create file: `schemas/domain/bundle.schema.json`

Aligned with: `bundle.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/bundle.schema.json",
  "schema_version": 1,
  "title": "Nexus DeltaBundle Envelope",
  "description": "DeltaBundle envelope containing delta operations for world synchronization.",
  "type": "object",
  "required": [
    "schema_version",
    "bundle_id",
    "command_id",
    "workspace_id",
    "world_id",
    "creator_id",
    "submitting_creator_id",
    "bundle_type",
    "idempotency_key",
    "canonical_hash",
    "base_versions",
    "deltas",
    "created_at"
  ],
  "properties": {
    "schema_version": {
      "type": "integer",
      "const": 1,
      "description": "Envelope schema version"
    },
    "bundle_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/BundleId",
      "description": "Unique bundle instance ID"
    },
    "command_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CommandId",
      "description": "Attributing SyncCommand ID"
    },
    "workspace_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorkspaceId",
      "description": "Local workspace binding"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId",
      "description": "Target world"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId",
      "description": "Initiating creator"
    },
    "submitting_creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId",
      "description": "Actual submitting creator (may equal creator_id in single-creator scenarios)"
    },
    "bundle_type": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/BundleType",
      "description": "world_sync | memory_sync | publish_metadata"
    },
    "manuscript_phase": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/ManuscriptPhase",
      "description": "Optional but recommended: manuscript phase for downstream gate validation"
    },
    "output_manuscript": {
      "type": "boolean",
      "description": "Whether this execution requires manuscript output"
    },
    "idempotency_key": {
      "type": "string",
      "minLength": 1,
      "description": "Client-generated idempotency key"
    },
    "canonical_hash": {
      "type": "string",
      "description": "Content hash (sha256:hexlower)"
    },
    "base_versions": {
      "type": "object",
      "required": [],
      "properties": {
        "world_revision": {
          "type": ["integer", "null"],
          "minimum": 0,
          "description": "World revision at client-side baseline"
        },
        "timeline_head_id": {
          "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/TimelineEventId",
          "description": "Timeline head event ID at baseline"
        },
        "canon_revision": {
          "type": ["integer", "null"],
          "minimum": 0,
          "description": "Optional canon revision"
        }
      },
      "description": "Optimistic concurrency baseline. At least world_revision or timeline_head_id should be provided."
    },
    "last_confirmed_delta_sequence": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/DeltaSequence",
      "description": "Last confirmed delta sequence for conflict detection"
    },
    "deltas": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["delta_type", "operation", "payload", "local_timestamp"],
        "properties": {
          "delta_type": {
            "type": "string",
            "enum": ["world", "key_block", "timeline_event", "fork_branch", "memory_item", "story_manifest"],
            "description": "Target aggregate type for this delta"
          },
          "operation": {
            "type": "string",
            "enum": ["create", "update", "upsert", "delete", "append"],
            "description": "Operation to apply"
          },
          "target_entity_type": {
            "type": "string",
            "description": "Sub-type (e.g., 'character' when delta_type='key_block')"
          },
          "target_entity_id": {
            "type": "string",
            "description": "Target entity ID (null for create)"
          },
          "payload": {
            "type": "object",
            "description": "Delta payload (validated by per-type sub-schema)"
          },
          "source_anchor": {
            "$ref": "https://nexus42.invalid/schemas/common/source-anchor.schema.json",
            "description": "Optional source anchor for provenance"
          },
          "local_timestamp": {
            "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
            "description": "Local timestamp of this delta"
          }
        }
      },
      "minItems": 1,
      "description": "Ordered list of delta operations"
    },
    "bundle_apply_status": {
      "type": "string",
      "enum": ["all_success", "partial", "failed"],
      "description": "Server-side write-back: bundle-level apply result"
    },
    "delta_results": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["delta_index", "delta_apply_status"],
        "properties": {
          "delta_index": {
            "type": "integer",
            "minimum": 0,
            "description": "Index into deltas[]"
          },
          "delta_apply_status": {
            "type": "string",
            "enum": ["applied", "rejected", "skipped_dependency"],
            "description": "Per-delta apply result"
          },
          "error_code": {
            "type": "string",
            "description": "Error code if rejected"
          },
          "applied_entity_revision": {
            "type": ["integer", "null"],
            "description": "Entity revision after successful apply"
          }
        }
      },
      "description": "Server-side per-delta results"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Bundle creation timestamp (RFC 3339 UTC)"
    }
  },
  "additionalProperties": false
}
```

Expected: DeltaBundle envelope schema created with 13+ required fields

- [x] **Step 2: Commit bundle schema**

Run: `git add schemas/domain/bundle.schema.json && git commit -m "feat(schema): add DeltaBundle envelope schema"`

Expected: Commit successful

---

## Task 4: Create Creator Schema

**Files:**
- Create: `schemas/domain/creator.schema.json`

- [x] **Step 1: Create creator entity schema**

Create file: `schemas/domain/creator.schema.json`

Aligned with: `schemas/domain/creator.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/creator.schema.json",
  "schema_version": 1,
  "title": "Nexus Creator Entity",
  "description": "Creator entity - a first-class creative agent that can be user-owned or agent-registered.",
  "type": "object",
  "required": ["schema_version", "creator_id", "display_name", "status", "registration_source", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId",
      "description": "Unique creator identifier"
    },
    "user_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/UserId",
      "description": "Default paired User ID (null if unpaired)"
    },
    "display_name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 100,
      "description": "Creator display name"
    },
    "status": {
      "type": "string",
      "enum": ["active", "archived", "locked"],
      "description": "Creator status"
    },
    "is_platform_owned": {
      "type": "boolean",
      "default": false,
      "description": "Whether this is a platform-hosted creator"
    },
    "api_key_ref": {
      "type": "string",
      "description": "Reference to platform-stored ACP/agent credential"
    },
    "registration_source": {
      "type": "string",
      "enum": ["cli", "web_agent", "platform"],
      "description": "How this creator was registered"
    },
    "persona_summary": {
      "type": "string",
      "maxLength": 500,
      "description": "Optional creator persona summary"
    },
    "style_profile": {
      "type": "object",
      "properties": {
        "tone": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Style tone tags"
        },
        "narrative_preferences": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Narrative preference tags"
        },
        "forbidden_patterns": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Forbidden pattern tags"
        }
      },
      "description": "Optional style profile"
    },
    "experience_revision": {
      "type": "integer",
      "minimum": 0,
      "default": 0,
      "description": "Current experience revision (0 = template-only)"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Creator registration timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    }
  },
  "additionalProperties": false
}
```

Expected: Creator schema created

- [x] **Step 2: Commit creator schema**

Run: `git add schemas/domain/creator.schema.json && git commit -m "feat(schema): add creator entity schema"`

Expected: Commit successful

---

## Task 5: Create World Schema

**Files:**
- Create: `schemas/domain/world.schema.json`

- [x] **Step 1: Create world entity schema**

Create file: `schemas/domain/world.schema.json`

Aligned with: `schemas/domain/world.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/world.schema.json",
  "schema_version": 1,
  "title": "Nexus World Entity",
  "description": "World entity - a narrative universe maintained by creators with timeline evolution.",
  "type": "object",
  "required": ["schema_version", "world_id", "owner_creator_id", "title", "slug", "status", "visibility", "time_policy", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId",
      "description": "Unique world identifier"
    },
    "owner_creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId",
      "description": "World owner creator ID"
    },
    "title": {
      "type": "string",
      "minLength": 1,
      "maxLength": 200,
      "description": "World title"
    },
    "slug": {
      "type": "string",
      "pattern": "^[a-z0-9-]+$",
      "description": "URL-friendly world slug"
    },
    "status": {
      "type": "string",
      "enum": ["active", "paused", "archived"],
      "description": "World status"
    },
    "visibility": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Visibility",
      "default": "private",
      "description": "World visibility"
    },
    "time_policy": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/TimePolicy",
      "default": "manual",
      "description": "Timeline evolution policy"
    },
    "canon_revision": {
      "type": "integer",
      "minimum": 0,
      "description": "Current canon revision number"
    },
    "current_timeline_head_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/TimelineEventId",
      "description": "Current timeline head event ID"
    },
    "current_time_pointer": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/TimelineEventId",
      "description": "World time progression pointer"
    },
    "root_fork_branch_id": {
      "type": "string",
      "description": "Root fork branch ID"
    },
    "world_rules": {
      "type": "object",
      "properties": {
        "time_moves_forward": {
          "type": "boolean",
          "default": true
        },
        "history_mutation_requires_fork": {
          "type": "boolean",
          "default": true
        }
      },
      "description": "World rule flags"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "World creation timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    }
  },
  "additionalProperties": false
}
```

Expected: World schema created

- [x] **Step 2: Commit world schema**

Run: `git add schemas/domain/world.schema.json && git commit -m "feat(schema): add world entity schema"`

Expected: Commit successful

---

## Task 6: Create KeyBlock Schema

**Files:**
- Create: `schemas/domain/key-block.schema.json`

- [x] **Step 1: Create key block schema**

Create file: `schemas/domain/key-block.schema.json`

Aligned with: `schemas/domain/key-block.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/key-block.schema.json",
  "schema_version": 1,
  "title": "Nexus KeyBlock",
  "description": "KeyBlock - a structured knowledge unit in a world timeline.",
  "type": "object",
  "required": ["schema_version", "key_block_id", "world_id", "block_type", "canonical_name", "status", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "key_block_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/KeyBlockId",
      "description": "Unique KeyBlock identifier"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId",
      "description": "World this KB belongs to"
    },
    "block_type": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/BlockType",
      "description": "KeyBlock content type"
    },
    "canonical_name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 200,
      "description": "Canonical name for this KeyBlock"
    },
    "status": {
      "type": "string",
      "enum": ["provisional", "confirmed", "deprecated", "merged", "deleted"],
      "description": "KeyBlock status"
    },
    "revision": {
      "type": "integer",
      "minimum": 0,
      "description": "KeyBlock revision number"
    },
    "body": {
      "type": "object",
      "properties": {
        "summary": {
          "type": "string",
          "description": "Structured summary"
        },
        "attributes": {
          "type": "object",
          "description": "Key-value attributes"
        },
        "tags": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Classification tags"
        }
      },
      "description": "KeyBlock body content"
    },
    "source_anchor": {
      "$ref": "https://nexus42.invalid/schemas/common/source-anchor.schema.json",
      "description": "Source anchor for provenance"
    },
    "created_from_command_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CommandId",
      "description": "SyncCommand that created this KB"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "KB creation timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    }
  },
  "additionalProperties": false
}
```

Expected: KeyBlock schema created

- [x] **Step 2: Commit key block schema**

Run: `git add schemas/domain/key-block.schema.json && git commit -m "feat(schema): add KeyBlock schema"`

Expected: Commit successful

---

## Task 7: Create TimelineEvent and MemoryItem Schemas

**Files:**
- Create: `schemas/domain/timeline-event.schema.json` (replaces old timeline.schema.json)
- Create: `schemas/domain/memory.schema.json`

- [x] **Step 1: Create timeline event schema**

Create file: `schemas/domain/timeline-event.schema.json`

Aligned with: `schemas/domain/timeline-event.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/timeline-event.schema.json",
  "schema_version": 1,
  "title": "Nexus TimelineEvent",
  "description": "TimelineEvent - a canonical event on the world timeline with causality and sequence.",
  "type": "object",
  "required": ["schema_version", "timeline_event_id", "world_id", "branch_id", "event_type", "status", "sequence_no", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "timeline_event_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/TimelineEventId",
      "description": "Unique TimelineEvent identifier"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId",
      "description": "World this event belongs to"
    },
    "branch_id": {
      "type": "string",
      "description": "Fork branch ID (root branch or specific fork)"
    },
    "event_type": {
      "type": "string",
      "enum": ["story_advance", "state_update", "fork_marker", "official_progression", "publish_marker"],
      "description": "Type of timeline event"
    },
    "status": {
      "type": "string",
      "enum": ["canon", "provisional", "rejected"],
      "description": "Event status"
    },
    "sequence_no": {
      "type": "integer",
      "minimum": 0,
      "description": "Sequence number within the branch"
    },
    "title": {
      "type": "string",
      "minLength": 1,
      "maxLength": 200,
      "description": "Event title"
    },
    "summary": {
      "type": "string",
      "description": "Event summary"
    },
    "caused_by_event_ids": {
      "type": "array",
      "items": {
        "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/TimelineEventId"
      },
      "description": "Preceding events that caused this one"
    },
    "affected_key_block_ids": {
      "type": "array",
      "items": {
        "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/KeyBlockId"
      },
      "description": "KeyBlocks affected by this event"
    },
    "source_command_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CommandId",
      "description": "SyncCommand that triggered this event"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Event creation timestamp"
    }
  },
  "additionalProperties": false
}
```

Expected: TimelineEvent schema created

- [x] **Step 2: Create memory item schema**

Create file: `schemas/domain/memory.schema.json`

Aligned with: `schemas/domain/memory.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/memory.schema.json",
  "schema_version": 1,
  "title": "Nexus MemoryItem",
  "description": "MemoryItem - structured memory for creator experience and world context.",
  "type": "object",
  "required": ["schema_version", "memory_item_id", "creator_id", "world_id", "memory_type", "status", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "memory_item_id": {
      "type": "string",
      "pattern": "^mem_[a-zA-Z0-9]+$",
      "description": "Unique MemoryItem identifier (prefix: 'mem_')"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId",
      "description": "Creator who owns this memory"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId",
      "description": "World this memory belongs to"
    },
    "memory_type": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/MemoryType",
      "description": "canon | working | experience"
    },
    "memory_kind": {
      "type": "string",
      "enum": ["generic", "story_summary", "research_material", "review_note"],
      "description": "Content morphology sub-type"
    },
    "status": {
      "type": "string",
      "enum": ["active", "superseded", "archived"],
      "description": "MemoryItem status"
    },
    "summary": {
      "type": "string",
      "description": "Memory summary text"
    },
    "embedding_ref": {
      "type": "string",
      "description": "Reference to vector embedding"
    },
    "source_refs": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["kind", "id"],
        "properties": {
          "kind": {
            "type": "string",
            "description": "Source reference kind (e.g., 'command')"
          },
          "id": {
            "type": "string",
            "description": "Source entity ID"
          }
        }
      },
      "description": "Source references for provenance"
    },
    "last_accessed_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last access timestamp (nullable)"
    },
    "last_reinforced_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last reinforcement timestamp (nullable)"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Memory creation timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Last update timestamp"
    }
  },
  "additionalProperties": false
}
```

Expected: MemoryItem schema created

- [x] **Step 3: Commit timeline event and memory item schemas**

Run: `git add schemas/domain/timeline-event.schema.json schemas/domain/memory.schema.json && git commit -m "feat(schema): add TimelineEvent and MemoryItem schemas"`

Expected: Commit successful

---

## Task 7A: Create Common Value Object Schemas

**Files:**
- Create: `schemas/common/source-anchor.schema.json`
- Create: `schemas/common/version-ref.schema.json`

- [x] **Step 1: Create source anchor schema**

Create file: `schemas/common/source-anchor.schema.json`

Aligned with: `schemas/common/source-anchor.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/common/source-anchor.schema.json",
  "schema_version": 1,
  "title": "Nexus SourceAnchor",
  "description": "Value object for referencing platform Story summary entities without uploading full text.",
  "type": "object",
  "properties": {
    "story_summary_refs": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["story_manifest_id", "summary_unit_id", "unit_kind"],
        "properties": {
          "story_manifest_id": {
            "type": "string",
            "pattern": "^stm_[a-zA-Z0-9]+$",
            "description": "StoryManifest ID"
          },
          "summary_unit_id": {
            "type": "string",
            "pattern": "^sum_[a-zA-Z0-9]+$",
            "description": "Summary unit ID"
          },
          "unit_kind": {
            "type": "string",
            "description": "Unit kind (e.g., 'chapter_summary')"
          }
        }
      },
      "description": "References to platform Story summary entities"
    },
    "excerpt": {
      "type": "string",
      "description": "Optional excerpt text"
    },
    "summary": {
      "type": "string",
      "description": "Optional anchor summary"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 2: Create version ref schema**

Create file: `schemas/common/version-ref.schema.json`

Aligned with: `schemas/common/version-ref.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/common/version-ref.schema.json",
  "schema_version": 1,
  "title": "Nexus VersionRef",
  "description": "Value object describing the baseline version of a bundle/entity/world.",
  "type": "object",
  "required": ["entity_type", "entity_id", "revision"],
  "properties": {
    "entity_type": {
      "type": "string",
      "description": "Entity type (e.g., 'world')"
    },
    "entity_id": {
      "type": "string",
      "description": "Entity ID"
    },
    "revision": {
      "type": "integer",
      "minimum": 0,
      "description": "Revision number at baseline"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 3: Commit value object schemas**

Run: `git add schemas/common/source-anchor.schema.json schemas/common/version-ref.schema.json && git commit -m "feat(schema): add SourceAnchor and VersionRef value object schemas"`

Expected: Commit successful

---

## Task 7B: Create Pairing Schema

**Files:**
- Create: `schemas/domain/pairing.schema.json`

- [x] **Step 1: Create pairing schema**

Create file: `schemas/domain/pairing.schema.json`

Aligned with: `schemas/domain/pairing.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/pairing.schema.json",
  "schema_version": 1,
  "title": "Nexus Pairing",
  "description": "Pairing entity describing Creator <-> User association.",
  "type": "object",
  "required": ["schema_version", "pairing_id", "creator_id", "user_id", "pairing_source", "status", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "pairing_id": {
      "type": "string",
      "pattern": "^prg_[a-zA-Z0-9]+$",
      "description": "Unique pairing identifier (prefix: 'prg_')"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId"
    },
    "user_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/UserId"
    },
    "pairing_source": {
      "type": "string",
      "enum": ["auto_cli", "manual_web", "platform_auto"],
      "description": "How the pairing was established"
    },
    "status": {
      "type": "string",
      "enum": ["active", "revoked"],
      "description": "Pairing status"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "revoked_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Revocation timestamp (null if active)"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 2: Commit pairing schema**

Run: `git add schemas/domain/pairing.schema.json && git commit -m "feat(schema): add Pairing schema"`

Expected: Commit successful

---

## Task 7C: Create WorldMembership Schema

**Files:**
- Create: `schemas/domain/world-membership.schema.json`

- [x] **Step 1: Create world membership schema**

Create file: `schemas/domain/world-membership.schema.json`

Aligned with: `schemas/domain/world-membership.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/world-membership.schema.json",
  "schema_version": 1,
  "title": "Nexus WorldMembership",
  "description": "WorldMembership entity describing Creator-World relationship with roles and permissions.",
  "type": "object",
  "required": ["schema_version", "membership_id", "world_id", "creator_id", "role", "membership_status", "joined_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "membership_id": {
      "type": "string",
      "pattern": "^mbr_[a-zA-Z0-9]+$",
      "description": "Unique membership identifier (prefix: 'mbr_')"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId"
    },
    "role": {
      "type": "string",
      "enum": ["owner", "maintainer", "collaborator", "official_creator"],
      "description": "Membership role"
    },
    "membership_status": {
      "type": "string",
      "enum": ["active", "invited", "suspended", "removed"],
      "description": "Membership status"
    },
    "joined_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "permissions": {
      "type": "object",
      "properties": {
        "can_sync_kb": {
          "type": "boolean",
          "default": true
        },
        "can_publish": {
          "type": "boolean",
          "default": true
        },
        "can_fork": {
          "type": "boolean",
          "default": true
        },
        "can_invite_official_creator": {
          "type": "boolean",
          "default": false
        },
        "can_confirm_canon": {
          "type": "boolean",
          "default": false
        }
      },
      "description": "Permission flags"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 2: Commit world membership schema**

Run: `git add schemas/domain/world-membership.schema.json && git commit -m "feat(schema): add WorldMembership schema"`

Expected: Commit successful

---

## Task 7D: Create StoryManifest Schema

**Files:**
- Create: `schemas/domain/story-manifest.schema.json`

- [x] **Step 1: Create story manifest schema**

Create file: `schemas/domain/story-manifest.schema.json`

Aligned with: `schemas/domain/story-manifest.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/story-manifest.schema.json",
  "schema_version": 1,
  "title": "Nexus StoryManifest",
  "description": "StoryManifest entity for platform-side chapter/arc manifest and summary.",
  "type": "object",
  "required": ["schema_version", "story_manifest_id", "world_id", "creator_id", "manifest_type", "status", "title", "summary_unit_id", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "story_manifest_id": {
      "type": "string",
      "pattern": "^stm_[a-zA-Z0-9]+$",
      "description": "Unique StoryManifest identifier (prefix: 'stm_')"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId"
    },
    "manifest_type": {
      "type": "string",
      "enum": ["chapter", "arc", "story", "excerpt"],
      "description": "Manifest type"
    },
    "status": {
      "type": "string",
      "enum": ["summary_ready", "staged_for_publish", "published", "archived"],
      "description": "Manifest status"
    },
    "title": {
      "type": "string",
      "minLength": 1,
      "description": "Story title"
    },
    "summary_unit_id": {
      "type": "string",
      "pattern": "^sum_[a-zA-Z0-9]+$",
      "description": "Platform-side summary unit ID"
    },
    "summary_text": {
      "type": "string",
      "description": "Platform-authoritative summary text"
    },
    "output_manuscript": {
      "type": "boolean",
      "default": true,
      "description": "Whether manuscript output is enabled"
    },
    "manuscript_storage": {
      "type": "string",
      "enum": ["none", "local_workspace", "platform_sandbox"],
      "description": "Manuscript storage location"
    },
    "local_path": {
      "type": "string",
      "description": "Local workspace path (when manuscript_storage=local_workspace)"
    },
    "sandbox_path": {
      "type": ["string", "null"],
      "description": "Platform sandbox path (when manuscript_storage=platform_sandbox)"
    },
    "content_hash": {
      "type": ["string", "null"],
      "description": "Content hash (sha256:xxx)"
    },
    "published_artifact_id": {
      "type": ["string", "null"],
      "description": "Published artifact reference"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 2: Commit story manifest schema**

Run: `git add schemas/domain/story-manifest.schema.json && git commit -m "feat(schema): add StoryManifest schema"`

Expected: Commit successful

---

## Task 7E: Create SyncCommand Schema

**Files:**
- Create: `schemas/domain/sync-command.schema.json`

- [x] **Step 1: Create sync command schema**

Create file: `schemas/domain/sync-command.schema.json`

Aligned with: `schemas/domain/sync-command.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/sync-command.schema.json",
  "schema_version": 1,
  "title": "Nexus SyncCommand",
  "description": "SyncCommand entity representing a business action with audit attribution.",
  "type": "object",
  "required": ["schema_version", "command_id", "workspace_id", "world_id", "creator_id", "command_type", "origin", "status", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "command_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CommandId"
    },
    "workspace_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorkspaceId"
    },
    "world_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId"
    },
    "creator_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/CreatorId"
    },
    "command_type": {
      "type": "string",
      "enum": ["advance_world", "inject_future_event", "extract_kb", "sync_push", "sync_pull", "fork_world", "publish_story"],
      "description": "Normalized business action type"
    },
    "origin": {
      "type": "string",
      "enum": ["local_user", "local_agent", "official_creator", "system"],
      "description": "Command origin"
    },
    "output_manuscript": {
      "type": "boolean",
      "default": true,
      "description": "Whether this execution requires manuscript output"
    },
    "status": {
      "type": "string",
      "enum": ["pending", "running", "completed", "failed", "cancelled"],
      "description": "Command execution status"
    },
    "requested_by": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/UserId",
      "description": "User who requested the command"
    },
    "started_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "completed_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 2: Commit sync command schema**

Run: `git add schemas/domain/sync-command.schema.json && git commit -m "feat(schema): add SyncCommand schema"`

Expected: Commit successful

---

## Task 7F: Create OutboxEntry Schema

**Files:**
- Create: `schemas/domain/outbox-entry.schema.json`

- [x] **Step 1: Create outbox entry schema**

Create file: `schemas/domain/outbox-entry.schema.json`

Aligned with: `schemas/domain/outbox-entry.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/outbox-entry.schema.json",
  "schema_version": 1,
  "title": "Nexus OutboxEntry",
  "description": "OutboxEntry entity representing a local send queue item.",
  "type": "object",
  "required": ["schema_version", "outbox_entry_id", "bundle_id", "idempotency_key", "delivery_state", "created_at"],
  "properties": {
    "schema_version": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/SchemaVersion"
    },
    "outbox_entry_id": {
      "type": "string",
      "pattern": "^obx_[a-zA-Z0-9]+$",
      "description": "Unique outbox entry identifier (prefix: 'obx_')"
    },
    "bundle_id": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/BundleId"
    },
    "idempotency_key": {
      "type": "string",
      "minLength": 1,
      "description": "Idempotency key for deduplication"
    },
    "delivery_state": {
      "type": "string",
      "enum": ["staged", "ready", "sent", "acked", "conflicted", "failed"],
      "description": "Delivery state"
    },
    "retry_count": {
      "type": "integer",
      "minimum": 0,
      "default": 0,
      "description": "Number of delivery retries attempted"
    },
    "last_error": {
      "type": ["string", "null"],
      "description": "Last error message (null if no error)"
    },
    "next_retry_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp",
      "description": "Next retry timestamp (null if not retrying)"
    },
    "created_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "updated_at": {
      "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/Timestamp"
    }
  },
  "additionalProperties": false
}
```

- [x] **Step 2: Commit outbox entry schema**

Run: `git add schemas/domain/outbox-entry.schema.json && git commit -m "feat(schema): add OutboxEntry schema"`

Expected: Commit successful

---

**Files:**
- Create: `tooling/validation/schema-validator.js`
- Create: `tooling/validation/package.json`

- [x] **Step 1: Create validation directory**

Run: `mkdir -p tooling/validation`

Expected: Directory created

- [x] **Step 2: Create validation package.json**

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

- [x] **Step 3: Create schema validator script**

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
    schema_version: { type: 'integer', minimum: 1 },
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

- [x] **Step 4: Install validation dependencies**

Run: `cd tooling/validation && npm install`

Expected: Dependencies installed

- [x] **Step 5: Run validation**

Run: `cd tooling/validation && npm run validate`

Expected: All schemas validated successfully

- [x] **Step 6: Update root package.json to include validation script**

Read `package.json` and add to scripts:

```json
{
  "scripts": {
    "validate-schemas": "node tooling/validation/schema-validator.js"
  }
}
```

Expected: Validation script added to root package

- [x] **Step 7: Commit validation tooling**

Run: `git add tooling/validation package.json && git commit -m "feat(tooling): add schema validation tool with AJV"`

Expected: Commit successful

---

## Task 9: Create Domain README and Update CI

**Files:**
- Create: `schemas/domain/README.md`
- Modify: `.github/workflows/ci.yml` (add schema validation step)

- [x] **Step 1: Create domain schemas README**

Create file: `schemas/domain/README.md`

```markdown
# Domain Schemas

Core domain entity schemas for Nexus.

## Schemas

### DeltaBundle (`bundle.schema.json`)
Envelope for delta operations (world_sync, memory_sync, publish_metadata). 13+ required fields.

### Creator (`creator.schema.json`)
First-class creative agent (user-owned or agent-registered) with style profile and experience tracking. §5.2

### Pairing (`pairing.schema.json`)
Creator <-> User association record with pairing source and audit history. §5.2A

### World (`world.schema.json`)
Narrative universe with time_policy (manual/owner_driven/event_driven), visibility, and fork support. §5.3

### WorldMembership (`world-membership.schema.json`)
Creator-World bridge with role, permissions, and membership status. §5.4

### KeyBlock (`key-block.schema.json`)
Structured knowledge unit with block_type (character/ability/scene/etc.), status, and source anchor. §5.5

### TimelineEvent (`timeline-event.schema.json`)
Canonical timeline event with causality, sequence, and affected KeyBlocks. §5.6

### MemoryItem (`memory.schema.json`)
Structured memory item (canon/working/experience) with embedding and source refs. §5.8

### StoryManifest (`story-manifest.schema.json`)
Platform manifest for story chapters/arcs with summary and manuscript storage info. §5.9

### SyncCommand (`sync-command.schema.json`)
Business action command with audit attribution and origin tracking. §5.10

### OutboxEntry (`outbox-entry.schema.json`)
Local outbox queue item with delivery state and retry logic. §5.13

## Common Types

All domain schemas import reusable types from `schemas/common/common.schema.json`:
- Identifiers (WorldId, CreatorId, UserId, KeyBlockId, TimelineEventId, BundleId, CommandId, WorkspaceId)
- Enums (ManuscriptPhase, TimePolicy, Visibility, BlockType, MemoryType, BundleType)
- Timestamps (ISO 8601 / RFC 3339 UTC)

## Value Objects

- `schemas/common/source-anchor.schema.json`: SourceAnchor for story provenance. §6.1
- `schemas/common/version-ref.schema.json`: VersionRef for baseline versioning. §6.2

## Validation

Run: `pnpm run validate-schemas`
```

Expected: Domain README created

- [x] **Step 2: Update CI workflow to validate schemas**

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

- [x] **Step 3: Commit README and CI update**

Run: `git add schemas/domain/README.md .github/workflows/ci.yml && git commit -m "docs(schema): add domain README and update CI for schema validation"`

Expected: Commit successful

---

## Verification

- [x] **Final verification: Validate all schemas**

Run: `pnpm run validate-schemas`

Expected: All schemas pass validation with no errors

- [x] **Verify schema structure**

Run: `tree schemas -L 2`

Expected output:
```
schemas/
├── common/
│   ├── README.md
│   ├── common.schema.json
│   ├── source-anchor.schema.json
│   └── version-ref.schema.json
├── domain/
│   ├── README.md
│   ├── bundle.schema.json
│   ├── creator.schema.json
│   ├── key-block.schema.json
│   ├── memory.schema.json
│   ├── outbox-entry.schema.json
│   ├── pairing.schema.json
│   ├── story-manifest.schema.json
│   ├── sync-command.schema.json
│   ├── timeline-event.schema.json
│   ├── world.schema.json
│   └── world-membership.schema.json
├── meta/
│   ├── README.md
│   └── meta.schema.json
├── platform/
├── cli-sync/
└── acp-runtime/
```

- [x] **Verify CI workflow**

Run: `cat .github/workflows/ci.yml`

Expected: `validate-schemas` job includes validation script execution

---

## Completion

After all tasks complete:
- [x] Update `.agents/plans/status.json` with completion status
- [ ] Create git tag: `git tag v0.1.0-schemas -a -m "Phase 0: JSON Schema foundation initialized"`
- [ ] Push to remote: `git push origin main --tags`

---

**Plan saved to:** `.agents/plans/2025-04-05-json-schema-foundation.md`