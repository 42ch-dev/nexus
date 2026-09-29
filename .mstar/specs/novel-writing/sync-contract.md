# Novel-Writing Sync Module Contract

**Status**: Normative — shipped library contract (V1.36 Works layout); not an integrated cloud upload path  
**Document class**: Companion  
**Scope**: Filesystem chapter discovery and in-memory bundle construction for novel-writing artifacts  
**Implementation owner**: [`nexus-orchestration::sync_module`](../../../crates/nexus-orchestration/src/sync_module.rs), exported by [`lib.rs`](../../../crates/nexus-orchestration/src/lib.rs)  
**Last verified**: 2026-09-29 — implementation and consumer search; no runtime change  
**Primary layout SSOT**: [workflow-profile.md](workflow-profile.md) §3, §7  
**Supersedes**: workspace-root `Stories/<story_ref>/` scan rules (pre-1.0; no compatibility shims)

## 0. Current — Implementation status and boundary

**Reading boundary:** §§0–5 describe the current shipped library. §6 preserves the original V1.36 target contract as historical design text; its data shapes, DB filtering, idempotency expectations, and transport proposal are not claims of current implementation.

The V1.36 layout migration is implemented in `crates/nexus-orchestration/src/sync_module.rs`: `discover_works` scans `Works/<work_ref>/Stories/`, and `build_story_bundle` / `build_story_bundle_with_db` construct `StoryBundle`s. The [V1.36 layout contract tests](../../../crates/nexus-orchestration/tests/sync_module_works_layout.rs) exercise discovery, exclusions, ordering, removal of the workspace-root fallback, and bundle construction. This is a retained, exported library contract, not a retired module.

The discovery type is `DiscoveredWork`, not the earlier design name `NovelWorkArtifacts`. Searches of `apps/nexus-service`, `crates/nexus-core`, `tooling/`, and `packages/` found no implementation or consumer of these sync symbols; the scanner is not owned by `nexus-narrative` or `nexus-cloud-sync`. The current application/cloud sources do not call the scanner or bundle builders. **Shipped here means the library implementation, not end-to-end chapter upload** (see §5).

## 1. Current — Artifact Discovery

`discover_works(workspace_dir)` discovers artifacts by filesystem layout only. It does not read `works` or filter `work_profile`; a caller is responsible for selecting novel Works.

### Primary scan directories

- `Works/<work_ref>/Stories/` — Chapter正文 files (`*.md`) only
- `Works/<work_ref>/Outlines/` — **Not** scanned as chapters (metadata/planning)

### Discovery rules

- Only `.md` files **directly under** `Works/<work_ref>/Stories/` are sync chapter candidates
- Hidden Work directories and hidden chapter files (starting with `.`) are skipped
- `README.md`, `foreshadowing.md`, and `event-index.md` are skipped. `Outlines/**`, `Logs/**`, and `Rules/**` are not scanned. Discovery reads filenames only; optional `work_chapters` metadata enrichment happens during bundle construction (§2), not by parsing frontmatter.
- `Works/<work_ref>/Worldbuilding/` subtree is **not present** in V1.36 (world content lives in World KB per [entity-scope-model.md §5.4](../architecture/entity-scope-model.md) + [workflow-profile.md §3.5](workflow-profile.md))
- Workspace-root `Stories/<story_ref>/` is **not** scanned (legacy; removed pre-1.0)
- Each non-hidden directory under `Works/` with a `Stories/` directory yields a `DiscoveredWork`, even when its chapter list is empty. Works and chapter filenames are sorted alphabetically. Missing/unreadable `Works/` yields no Works; a missing `Stories/` skips that Work, while an unreadable `Stories/` leaves its chapter list empty.

### Output per work

```rust
struct DiscoveredWork {
    work_ref: String,       // directory name under Works/
    chapters: Vec<String>, // sorted chapter filenames under Stories/
}
```

## 2. Current — Bundle Inputs and Optional DB Enrichment

The caller supplies `workspace_dir`, `world_id`, `work_id`, and the `DiscoveredWork`. These helpers do not look up the Work row, verify its profile or world binding, or skip completed Works.

- `build_story_bundle` reads the discovered files as UTF-8, skips unreadable files, and computes SHA-256 hashes. It returns `None` when no files can be read or the chapter count cannot fit in `u32`; an empty but readable file is still included.
- `build_story_bundle_with_db` optionally queries `work_chapters` by `work_id` for `chapter`, `status`, and `actual_word_count`. It matches rows to filenames by parsing the chapter number from `ch<number>[-suffix].md`.
- Without a DB pool, or without a matching row, `status` and `actual_word_count` remain `None`. A DB query failure returns `None`, not an unenriched bundle.
- No outline aggregate, frontmatter status parsing, or configurable completed-Work filtering is implemented by this module.

Legacy `world_stories.story_ref` + workspace-root `Stories/` paths are **not** normative after V1.36 P2.

## 3. Current — Output Bundle Shape

The helpers produce the following in-memory Rust types, owned by `sync_module.rs`. `StoryBundle` is not the schema-generated platform `Bundle` wire envelope (§5):

```rust
struct StoryBundle {
    world_id: String,  // caller supplies an empty string for a worldless Work
    work_id: String,
    work_ref: String,
    chapters: Vec<ChapterContent>,
    chapter_count: u32,
    synced_at: String,  // ISO 8601
}

struct ChapterContent {
    filename: String,
    content_hash: String,  // SHA-256 of content
    content: String,
    status: Option<String>,         // optional work_chapters enrichment
    actual_word_count: Option<u32>,  // optional work_chapters enrichment
}
```

## 4. Current — Hashing and Repeated Builds

- Unchanged chapter bytes produce unchanged SHA-256 `content_hash` values.
- Every build reads files again and sets `synced_at` to the current RFC 3339 timestamp. There is no persisted hash cache or unchanged-bundle suppression; whole-bundle identity is not guaranteed across calls.
- Builders preserve the supplied chapter list order; `discover_works` supplies alphabetical filename order within `Works/<work_ref>/Stories/`.

## 5. Current — Platform Handoff Boundary

- The module constructs local `StoryBundle`s containing full chapter text; it does not serialize a platform request or call HTTP.
- **Current cloud path:** [`nexus42 platform sync push`](../cli/cli-spec.md#142-默认模式) delegates to [`commands/sync/mod.rs`](../../../apps/nexus42/src/commands/sync/mod.rs), which constructs a schema-generated `Bundle` via `nexus-cloud-sync::delta_bundle::BundleBuilder` and passes it to `SyncClient::push_bundle`. It does not invoke this chapter scanner or accept its `StoryBundle`.
- **Historical target (DR-54):** connecting the chapter library to cloud transport was a separate proposal, not evidence of shipped upload integration. Default sync must not upload full manuscript text; an explicit publication path is distinct from structured sync ([CLI Master §14.2](../cli/cli-spec.md#142-默认模式)).
- **Legacy (pre–V1.21):** the `nexus-sync` crate and daemon `POST /v1/local/sync/push` path are retired; they are not alternate scanner owners (see [local-cloud-crate-architecture.md](../archived/local-cloud-crate-architecture.md) §5–§6).
- Platform wire types remain schema-generated contracts; the local `StoryBundle` and `ChapterContent` above are not generated DTOs. Platform publish (DF-59) remains outside this module contract.

---

## 6. Historical V1.36 target contract (as specified)

> **Historical only — not current implementation authority.** The following text preserves the original §§1–5 from commit `5cadd8e86`, with only heading depth/numbering changed to nest the record here. These were the V1.36 target shapes and expectations, not evidence that profile filtering, Work-row lookup, unchanged-bundle suppression, or chapter upload shipped. Current behavior and ownership are documented separately in §§0–5 above; the current Status header supersedes the original migration label.

### 6.1 Artifact Discovery

The sync module scans the workspace for novel-writing artifacts when `work_profile == novel`:

#### Primary scan directories

- `Works/<work_ref>/Stories/` — Chapter正文 files (`*.md`) only
- `Works/<work_ref>/Outlines/` — **Not** scanned as chapters (metadata/planning)

#### Discovery rules

- Only `.md` files **directly under** `Works/<work_ref>/Stories/` are sync chapter candidates
- Hidden files (starting with `.`) are skipped
- `README.md`, `Outlines/**`, `Logs/**` are **never** chapter candidates. Per-chapter metadata is derived from the **`work_chapters` table** in `state.db` (per [workflow-profile.md §4.1](workflow-profile.md)); the legacy `work-status.md` file is removed in V1.36.
- `Works/<work_ref>/Worldbuilding/` subtree is **not present** in V1.36 (world content lives in World KB per [entity-scope-model.md §5.4](../architecture/entity-scope-model.md) + [workflow-profile.md §3.5](workflow-profile.md))
- Workspace-root `Stories/<story_ref>/` is **not** scanned (legacy; removed pre-1.0)
- Each `work_ref` directory under `Works/` represents one novel Work's artifact tree

#### Output per work

```
NovelWorkArtifacts {
  work_ref: String           // directory name under Works/
  work_id: String            // from state.db works table
  chapters: Vec<Chapter>     // ordered by filename under Stories/
  outline: Option<String>    // optional aggregate; per-chapter outlines live under Outlines/
}
Chapter {
  filename: String           // e.g., "ch01-introduction.md"
  content: String            // file content
  status: Option<String>     // from frontmatter when present
}
```

### 6.2 Sync Input (from local DB)

The sync module reads from `works` table (and related world binding when present):

- `work_id` — identifies the Work
- `work_ref` — locates `Works/<work_ref>/`
- `work_profile` — must be `novel` for this contract
- `workspace_slug` / `workspace_path` — locates the workspace root
- `world_id` — optional parent world binding
- `status` — completed Works may skip sync regeneration (configurable)

Legacy `world_stories.story_ref` + workspace-root `Stories/` paths are **not** normative after V1.36 P2.

### 6.3 Output Bundle Shape

The sync module produces a `StoryBundle` (wire name unchanged for contract stability) per novel Work:

```rust
struct StoryBundle {
    world_id: Option<String>,
    work_id: String,
    work_ref: String,
    chapters: Vec<ChapterContent>,
    chapter_count: u32,
    synced_at: String,  // ISO 8601
}

struct ChapterContent {
    filename: String,
    content_hash: String,  // SHA-256 of content
    content: String,
}
```

### 6.4 Idempotency

- Repeated sync of the same Work produces the same bundle (content-hash based)
- If no chapter files have changed since last sync, the bundle is not regenerated
- Chapter ordering is alphabetical by filename within `Works/<work_ref>/Stories/`

### 6.5 Platform Handoff Boundary

- The sync module produces `StoryBundle`s
- **Target (long-term):** platform upload is handled by **`nexus-cloud-sync`** when the CLI runs `nexus42 sync push` (cloud product line). The module does **not** call platform HTTP directly. **Durable roadmap:** DR-54.
- **Legacy (pre–V1.21):** some builds still route upload through the `nexus-sync` crate and `POST /v1/local/sync/push` on the daemon; that path is **retired** per [local-cloud-crate-architecture.md](../archived/local-cloud-crate-architecture.md) §5–§6.
- Wire bundles use types from `@42ch/nexus-contracts` / `schemas/domain/` + `schemas/platform/sync/` (no duplicate DTOs)
- **V1.36 scope**: structured sync only; platform publish (DF-59) is explicitly OUT
