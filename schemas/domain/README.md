# Domain Schemas

Core domain entity schemas for Nexus. All aligned with `data-model-v1.md`.

## Schemas

### DeltaBundle (`bundle.schema.json`)
Envelope for delta operations (world_sync, memory_sync, publish_metadata). 13+ required fields per bundle-envelope-schema-v1.md §5.

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
- Enums (ManuscriptPhase, TimePolicy, Visibility, BlockType, MemoryType, BundleType) — all aligned with data-model-v1.md §7
- Timestamps (ISO 8601 / RFC 3339 UTC)

## Value Objects

- `schemas/common/source-anchor.schema.json`: SourceAnchor for story provenance. §6.1
- `schemas/common/version-ref.schema.json`: VersionRef for baseline versioning. §6.2

## Validation

Run: `pnpm run validate-schemas`
