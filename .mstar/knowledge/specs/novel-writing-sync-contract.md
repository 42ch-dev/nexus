# Novel-Writing Sync Module Contract

**Status**: Normative (module contract) — **V1.36 layout migration pending** (P2 implement)  
**Document class**: Companion  
**Scope**: Workspace scan rules for novel-writing sync module  
**Primary layout SSOT**: [novel-workflow-profile.md](novel-workflow-profile.md) §3, §7  
**Supersedes**: workspace-root `Stories/<story_ref>/` scan rules (pre-1.0; no compatibility shims)

## 1. Artifact Discovery

The sync module scans the workspace for novel-writing artifacts when `work_profile == novel`:

### Primary scan directories

- `Works/<work_ref>/Stories/` — Chapter正文 files (`*.md`) only
- `Works/<work_ref>/Outlines/` — **Not** scanned as chapters (metadata/planning)

### Discovery rules

- Only `.md` files **directly under** `Works/<work_ref>/Stories/` are sync chapter candidates
- Hidden files (starting with `.`) are skipped
- `README.md`, `work-status.md`, `Outlines/**`, `Worldbuilding/**`, `Logs/**` are **never** chapter candidates
- Workspace-root `Stories/<story_ref>/` is **not** scanned (legacy; removed pre-1.0)
- Each `work_ref` directory under `Works/` represents one novel Work's artifact tree

### Output per work

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

## 2. Sync Input (from local DB)

The sync module reads from `works` table (and related world binding when present):

- `work_id` — identifies the Work
- `work_ref` — locates `Works/<work_ref>/`
- `work_profile` — must be `novel` for this contract
- `workspace_slug` / `workspace_path` — locates the workspace root
- `world_id` — optional parent world binding
- `status` — completed Works may skip sync regeneration (configurable)

Legacy `world_stories.story_ref` + workspace-root `Stories/` paths are **not** normative after V1.36 P2.

## 3. Output Bundle Shape

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

## 4. Idempotency

- Repeated sync of the same Work produces the same bundle (content-hash based)
- If no chapter files have changed since last sync, the bundle is not regenerated
- Chapter ordering is alphabetical by filename within `Works/<work_ref>/Stories/`

## 5. Platform Handoff Boundary

- The sync module produces `StoryBundle`s
- **Target (long-term):** platform upload is handled by **`nexus-cloud-sync`** when the CLI runs `nexus42 sync push` (cloud product line). The module does **not** call platform HTTP directly.
- **Legacy (pre–V1.21):** some builds still route upload through the `nexus-sync` crate and `POST /v1/local/sync/push` on the daemon; that path is **retired** per [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) §5–§6.
- Wire bundles use types from `@42ch/nexus-contracts` / `schemas/domain/` + `schemas/cloud-sync/` (no duplicate DTOs)
- **V1.36 scope**: structured sync only; platform publish (DF-59) is explicitly OUT
