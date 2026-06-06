# Novel-Writing Sync Module Contract

**Status**: Normative (module contract)  
**Document class**: Companion  
**Scope**: Workspace scan rules for novel-writing sync module (V1.15+)

## 1. Artifact Discovery

The sync module scans the workspace for novel-writing artifacts:

### Primary scan directories
- `Stories/<story_ref>/` — Story chapter files (`*.md`)
- `Stories/<story_ref>/outline.md` — Story outline (if exists)

### Discovery rules
- Only `.md` files under `Stories/` are considered sync candidates
- Hidden files (starting with `.`) are skipped
- `outline.md` is treated as metadata, not chapter content
- Each `<story_ref>` directory represents one story

### Output per story
```
Story {
  story_ref: String        // directory name
  chapters: Vec<Chapter>   // ordered by filename
  outline: Option<String>  // outline.md content if exists
}
Chapter {
  filename: String        // e.g., "ch01-introduction.md"
  content: String         // file content
}
```

## 2. Sync Input (from local DB)

The sync module reads from `world_stories` table:
- `world_id` — identifies the parent world
- `story_ref` — identifies the story
- `workspace_path` — locates the Stories directory
- `status` — only stories with status != 'draft' are synced (or configurable)

## 3. Output Bundle Shape

The sync module produces a `StoryBundle` for each story:

```rust
struct StoryBundle {
    world_id: String,
    story_ref: String,
    chapters: Vec<ChapterContent>,
    outline: Option<String>,
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

- Repeated sync of the same story produces the same bundle (content-hash based)
- If no files have changed since last sync, the bundle is not regenerated
- Chapter ordering is alphabetical by filename within each story

## 5. Platform Handoff Boundary

- The sync module produces `StoryBundle`s
- **Target (long-term):** platform upload is handled by **`nexus-cloud-sync`** when the CLI runs `nexus42 sync push` (cloud product line). The module does **not** call platform HTTP directly.
- **Legacy (pre–V1.21):** some builds still route upload through the `nexus-sync` crate and `POST /v1/local/sync/push` on the daemon; that path is **retired** per [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) §5–§6.
- Wire bundles use types from `@42ch/nexus-contracts` / `schemas/domain/` + `schemas/cloud-sync/` (no duplicate DTOs)
