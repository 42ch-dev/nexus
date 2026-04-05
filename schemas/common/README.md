# Common Types

Reusable type definitions imported by domain schemas. All enums aligned with data-model-v1.md §7.

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

### Enums (data-model-v1.md §7)
- `ManuscriptPhase`: brainstorm / draft / review / finalize / published
- `TimePolicy`: manual / owner_driven / event_driven
- `Visibility`: private / unlisted / public
- `BlockType`: character / ability / scene / organization / item / conflict / info_point / event
- `MemoryType`: canon / working / experience
- `BundleType`: world_sync / memory_sync / publish_metadata

### Timestamps
- `Timestamp`: ISO 8601 / RFC 3339 UTC datetime string
