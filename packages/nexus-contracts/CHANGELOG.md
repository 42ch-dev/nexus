# Changelog

All notable changes to the `@42ch/nexus-contracts` package will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-04-18

### Changed

- Narrowed `@42ch/nexus-contracts` to wire contracts only. Types that `nexus-platform` does not consume have been removed from this package; they now live as hand-written Rust in `crates/nexus-contracts/src/local/`.
- See `.agents/knowledge/schemas-boundary-v1.md` §2 for the boundary rule.

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