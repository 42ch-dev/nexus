# Changelog

All notable changes to the `@42ch/nexus-contracts` package will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.29.0] - 2026-08-06

### Added

- **Daemon KB pack wire contracts (V1.152 P0 DF-77):** added 4 additive schemas under `daemon-api/kb/` for Narrative Knowledge Pack export/import routes (`POST /v1/daemon/worlds/:world_id/kb/pack/export`, `POST /v1/daemon/worlds/:world_id/kb/pack/import`):
  - `PackExportRequest` — optional `include_deprecated`, `include_anchors`, `title`, `pack_version`, `description` (all optional; empty body = defaults)
  - `PackExportResponse` — handbook pack envelope: `modules` (opaque object), `entries` / `relations` (opaque spoke object arrays per V1.139 fallback), optional `source_anchors`
  - `PackImportRequest` — opaque `pack` object + required `conflict` enum (`skip` | `rename` | `overwrite`) + optional `include_anchors`
  - `PackImportResponse` — per-atom-type `AtomCounts` summaries for entries and relations plus `details[]` with `kind`, `id`, `outcome`, optional `reason`

### Consumer Impact

- **Additive only** — no existing schemas modified; generated types for existing consumers are unchanged.
- New Rust types: `nexus_contracts::generated::daemon_api::kb::{PackExportRequest, PackExportResponse, PackImportRequest, PackImportResponse, ...}`.
- New TypeScript types: `@42ch/nexus-contracts` exports the same shapes under `daemon-api/kb/pack-*.ts`.
- **Daemon-only routes:** both routes live on the Daemon API tier2 surface; neither is a Connect op.
## [0.28.1] - 2026-08-05

### Added

- **Moment inspector + directive wire contracts (V1.151 P0 DF-76):** added 3 additive schemas under `daemon-api/inspector/` for the two new Daemon HTTP surfaces (`POST /v1/daemon/inspector/moment`, `POST /v1/daemon/moment-directive`):
  - `MomentInspectRequest` — `world_id` (required), optional `work_id`, optional `generation_stage` (`intake` | `research` | `produce` | `review` | `persist` | `work_maintenance` | `system_maintenance` | `unspecified`; maps via `GenerationStage::as_str`/`parse`, unknown → `unspecified`)
  - `MomentInspectResponse` — mirrors the enriched inspector packet from `nexus-moment-context-assembly::inspector::build_inspector_packet` 1:1: spoke `modules` (`placement[]` + `activation_trace[]`), `slot_map[]` (`entry_id` → slot id), `budget` (token estimates + nullable `cap`/`remaining`), `moment_directive` — **status/metadata only, no body field by construction (AC-I3)**; root `additionalProperties: false` per the H1 locked pattern (product-local sections grow by extending schema + codegen together, never silently)
  - `MomentDirectiveRequest` — `action` (`set` | `show` | `clear`) + scoped `scope` (`kind`: `work` | `world`, `id`), optional `body`, `insert_depth` (`head` | `mid` | `tail`), `ttl_kind` (`generations` | `chapters`), `ttl_remaining` (≥1; input name matches the read-back column per the spec §5 H5 lock), `clear_on_scene_change`, `replace`; validation mirrors CLI `handle_set`

### Consumer Impact

- **Additive only** — no existing schemas modified; generated types for existing consumers are unchanged.
- New Rust types: `nexus_contracts::generated::daemon_api::inspector::{MomentInspectRequest, MomentInspectResponse, MomentInspectResponseModules, MomentInspectResponseBudget, MomentInspectResponseMomentDirective, MomentInspectResponseSlotMapItem, MomentDirectiveRequest, MomentDirectiveRequestScope, ...}`.
- New TypeScript types: `@42ch/nexus-contracts` exports the same shapes under `daemon-api/inspector/moment-inspect-request.ts`, `moment-inspect-response.ts`, and `moment-directive-request.ts`.
- **Daemon-only routes:** both routes live on the Daemon API tier2 surface; neither is a Connect op.

## [0.28.0] - 2026-08-04

### Added

- **Daemon check route wire contracts (V1.148 P2):** added 2 additive schemas under `daemon-api/check/` for the spoke `orchestrate_check` Daemon HTTP surface (`POST /v1/daemon/check` — closes V1.146 Non-Goal 5a):
  - `CheckRequest` — `world_id` (ownership key) + spoke-mirrored `scope` selector (`scope_id` must equal `world_id`), `rule_refs`, embedded `rules`, `checker_kinds`, `extensions`
  - `CheckResponse` — mirrors the spoke `CheckResponse` oneOf 1:1: success branch `{ findings: [...] }` (spoke Finding wire shape, mirrored inline) XOR error branch `{ error: ErrorEnvelope }`; HTTP 200 may carry either branch, spoke Reject results surface as 4xx/5xx daemon error-envelope responses

### Consumer Impact

- **Additive only** — no existing schemas modified; generated types for existing consumers are unchanged.
- New Rust types: `nexus_contracts::generated::daemon_api::check::{CheckRequest, CheckRequestScope, CheckResponse, NexusDaemonCheckResponseSuccessFindingsItem, ...}`.
- New TypeScript types: `@42ch/nexus-contracts` exports the same shapes under `daemon-api/check/check-request.ts` and `daemon-api/check/check-response.ts`.
- **Daemon-only route:** `check` is **not** a Connect op — Connect (N-C0) refuses `check`; the route lives on the Daemon API tier2 surface (`require_api_key` + `require_active_creator`).

## [0.27.0] - 2026-08-01

### Added

- **Per-World timeline events read contracts (V1.147 P2):** added 2 additive schemas under `daemon-api/timeline/` for the World Timeline events route:
  - `TimelineEventInfo` — one `narrative_timeline_events` row: id, branch_id, event_type, status, sequence_no, title, summary, affected/caused event id lists, source_command_id, metadata, parsed `extensions` (extensions_nexus_json — compute provenance: module_id, module_version, run_id, source_kind), created_at
  - `ListTimelineEventsResponse` — cursor-paginated list envelope (items / has_more / next_cursor) for `GET /v1/daemon/worlds/:world_id/timeline/events`

### Consumer Impact

- **Additive only** — no existing schemas modified; generated types for existing consumers are unchanged.
- New Rust types: `nexus_contracts::generated::daemon_api::timeline::{TimelineEventInfo, ListTimelineEventsResponse}`.
- New TypeScript types: `@42ch/nexus-contracts` exports the same shapes under `daemon-api/timeline/timeline-event-info.ts` and `daemon-api/timeline/list-timeline-events-response.ts`.

## [0.26.0] - 2026-07-31

### Added

- **Compute run wire contracts (V1.147 P0):** added 7 additive schemas under `daemon-api/compute/` for the direct Control Room compute invocation lane:
  - `RunRequest` / `RunResponse` — invoke a WASM module against an owned World (POST /run)
  - `RunAcceptRequest` / `RunAcceptResponse` — atomically accept a succeeded Run's proposals (POST /runs/:run_id/accept)
  - `RunSummary` / `RunDetail` — paginated list item and full detail for Runs history (GET /runs, GET /runs/:run_id)
  - `RunListResponse` — cursor-paginated Runs list envelope
- `RunResponse.proposals` and `RunDetail.proposals` reuse the existing `ComputeOutput` envelope via `$ref` (4-part: state_delta, timeline_events, new_key_blocks, battle_report).
- `RunResponse.error` and `RunDetail.error` reuse the existing `ErrorResponse` schema.

### Consumer Impact

- **Additive only** — no existing schemas modified; generated types for existing consumers are unchanged.
- New Rust types: `nexus_contracts::generated::daemon_api::compute::{RunRequest, RunResponse, RunAcceptRequest, RunAcceptResponse, RunSummary, RunDetail, RunListResponse}`.
- New TypeScript types: `@42ch/nexus-contracts` exports the same shapes under `daemon-api/compute/run-*.ts`.

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