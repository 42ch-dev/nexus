# Changelog

All notable changes to the `@42ch/nexus-contracts` package will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.25.0] - 2026-07-26

### Changed

- **Vocabulary migration (KeyBlock → KnowledgeEntry):** updated human-readable JSON Schema `description` prose across `common`, `daemon-api/canvas/world-kb/*`, `daemon-api/compute/*`, and `platform/http-bff/*` to refer to the entity concept as **KnowledgeEntry** / "knowledge entries" instead of the legacy "KeyBlock".
- **Frozen identifiers retained (intentional, per V1.139 compass Q8/Q9):** type names (`BlockType`, `KeyBlockStatus`, `Nexus WorldKbKeyBlockStateResponse`), field names (`key_block_id`, `key_blocks`, `new_key_blocks`, `target_key_block_id`, `required_key_block_types`, `affected_key_block_ids`, `key_block_limit`, `key_block_attributes`, `key_block_state`), the `DeltaType` enum value `"key_block"`, HTTP route `/kb/key-blocks/{key_block_id}/state`, file names, `$id` URIs, and SQLite column references (`kb_key_blocks.*`) are **unchanged** — only description text was edited.

### Consumer Impact

- **No wire-type changes.** Generated TypeScript (and Rust) types are byte-identical to 0.24.0; JSON Schema `description` strings are not emitted into generated types. Consumers upgrading from 0.24.0 need no code changes.
- The minor bump (0.24.0 → 0.25.0) reflects the coordinated V1.139 vocabulary rename under pre-1.0 breaking-change discipline, not a wire-format break.

## [0.19.0] - 2026-07-05

### Changed

- **BREAKING**: Renamed the local daemon surface from **Local API** to **Daemon API**.
  - HTTP path prefix changed from `/v1/local/*` to `/v1/daemon/*`.
  - Generated TypeScript module tree moved from `local-api/` to `daemon-api/`.
  - Generated Rust module tree moved from `local_api/` to `daemon_api/`.
  - Resource identifier in `403 Forbidden` details changed from `"daemon-daemon-api"` to `"daemon-api"`.

### Added

- Opt-in remote bind support: non-loopback HTTP binds now require both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1`.

### Consumer Impact

- Consumers of `@42ch/nexus-contracts` must update imports from `local-api` paths to `daemon-api` paths and route calls to `/v1/daemon/*`.

## [0.12.0] - 2026-06-30

### Added

- `WorldKbRelationshipProjection` gains `needs_review` (boolean) and `source` (`manual` | `extraction`) for the V1.76 extraction-suggestion gate + provenance.
- `WorldKbRelationshipInput` gains optional `needs_review` so the existing patch-relationship route can promote a suggestion (clear the gate) without a second promotion state machine.

### Changed

- `GET /v1/local/worlds/{world_id}/kb/graph` now defaults to excluding `needs_review = 1` relationships (suggestions). Pass `?include_suggested=true` to surface them. Existing data is unaffected (all rows default to `needs_review = 0`).

## [0.4.0] - 2026-04-18

### Changed

- Narrowed `@42ch/nexus-contracts` to wire contracts only. Types that `nexus-platform` does not consume have been removed from this package; they now live as hand-written Rust in `crates/nexus-contracts/src/local/`.
- See `.mstar/archived/knowledge/schemas-boundary.md` §2 for the boundary rule.

### Removed

The following TypeScript types are no longer emitted (platform confirmed zero usage):

- `AgentProfile`
- `DaemonStatusV2`
- `LocalIdentity`
- `ManuscriptState`
- `Meta`
- `OutboxEntry`
- `ReferenceSource`
- `RegistryManifest`
- `RuntimeMode`
- `WorkspaceBinding`

### Platform Impact

**None** — none of these types were imported in `nexus-platform` (pre-merge `rg` in WS5 Task 2 confirmed zero uses).

## [0.2.0] - 2026-04-08

### Changed

#### MemoryItem Schema
- **BREAKING**: Updated `memory_kind` enum to align with ADR-001 and v1-spec §5.8
  - Removed: `"generic"`
  - Added: `"character_note"`, `"world_building"`, `"plot_outline"`, `"theme_analysis"`, `"custom"`
  - Final enum (8 values): `story_summary`, `research_material`, `review_note`, `character_note`, `world_building`, `plot_outline`, `theme_analysis`, `custom`

### Added

- Alignment with nexus-platform Phase 2 domain model changes (ADR-002)
- All schemas now fully compliant with v1-spec domain model definitions

### Technical Notes

- **KeyBlock** and **SourceAnchor** schemas were already aligned with ADR-002 requirements
- Generated TypeScript and Rust types updated to reflect MemoryItem enum changes
- All schemas validated successfully (22 schemas)

### References

- ADR-001: MemoryKind Expansion
- ADR-002: Domain Model G1-G6 Fixes
- v1-spec: `domain/data-model-v1.md` §5.8

## [0.1.0] - 2025-04-05

### Added

- Initial release of `@42ch/nexus-contracts`
- Generated TypeScript types from JSON Schema
- Generated Rust types from JSON Schema
- Core domain models: User, Creator, World, KeyBlock, TimelineEvent, MemoryItem, StoryManifest, etc.
- Sync contract types: DeltaBundle, SyncCommand, OutboxEntry
- Platform integration types: Bundle, ConflictResponse