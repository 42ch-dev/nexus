# Sync Contract — V1.36 Target Contract and Legacy Path History Historical Notes

**Provenance**: frozen from [`specs/active/novel-writing/sync-contract.md`](../../active/novel-writing/sync-contract.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-29 — commit `84e00ae76` ("docs: preserve historical V1.36 sync contract sections") demoted the original V1.36 target contract to a historical §6 inside the live doc; that record now lives here (V1.36 itself shipped 2026-06-07, commit `4ef057680`). Nothing here is current authority for the shipped library — the live §§0–5 are.
**Freeze date**: 2026-10-13

---

## Removed header lines

**Supersedes**: workspace-root `Stories/<story_ref>/` scan rules (pre-1.0; no compatibility shims)

`**Last verified**: 2026-09-29 — implementation and consumer search; no runtime change` (trailing change-log clause removed).

## §0 removed reading-boundary paragraph

**Reading boundary:** §§0–5 describe the current shipped library. §6 preserves the original V1.36 target contract as historical design text; its data shapes, DB filtering, idempotency expectations, and transport proposal are not claims of current implementation.

## §0 removed sentence

The discovery type is `DiscoveredWork`, not the earlier design name `NovelWorkArtifacts`.

## §1 / §2 removed qualifiers

- `**not present** in V1.36` (Worldbuilding subtree rule; kept without the version).
- Legacy `world_stories.story_ref` + workspace-root `Stories/` paths are **not** normative after V1.36 P2. (×2 — §§2 and historical §6.2)

## §5 removed bullets (reframed, live rule kept)

- **Historical target (DR-54):** connecting the chapter library to cloud transport was a separate proposal, not evidence of shipped upload integration. Default sync must not upload full manuscript text; an explicit publication path is distinct from structured sync ([CLI Master §14.2]).
- **Legacy (pre–V1.21):** the `nexus-sync` crate and daemon `POST /v1/local/sync/push` path are retired; they are not alternate scanner owners (see local-cloud-crate-architecture.md §5–§6).

## §6 Historical V1.36 target contract (as specified) — removed whole section

> **Historical only — not current implementation authority.** The following text preserves the original §§1–5 from commit `5cadd8e86`, with only heading depth/numbering changed to nest the record here. These were the V1.36 target shapes and expectations, not evidence that profile filtering, Work-row lookup, unchanged-bundle suppression, or chapter upload shipped. Current behavior and ownership are documented separately in §§0–5 above; the current Status header supersedes the original migration label.

### 6.1 Artifact Discovery

The sync module scans the workspace for novel-writing artifacts when `work_profile == novel`:

#### Primary scan directories

- `Works/<work_ref>/Stories/` — Chapter正文 files (`*.md`) only
- `Works/<work_ref>/Outlines/` — **Not** scanned as chapters (metadata/planning)

#### Discovery rules

- Only `.md` files **directly under** `Works/<work_ref>/Stories/` are sync chapter candidates
- Hidden files (starting with `.`) are skipped
- `README.md`, `Outlines/**`, `Logs/**` are **never** chapter candidates. Per-chapter metadata is derived from the **`work_chapters` table** in `state.db` (per [workflow-profile.md §4.1](../../active/novel-writing/workflow-profile.md)); the legacy `work-status.md` file is removed in V1.36.
- `Works/<work_ref>/Worldbuilding/` subtree is **not present** in V1.36 (world content lives in World KB per entity-scope-model.md §5.4 + workflow-profile.md §3.5)
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
- **Legacy (pre–V1.21):** some builds still route upload through the `nexus-sync` crate and `POST /v1/local/sync/push` on the daemon; that path is **retired** (historical split record: local-cloud-crate-architecture.md §5–§6).
- Wire bundles use types from `@42ch/nexus-contracts` / `schemas/domain/` + `schemas/platform/sync/` (no duplicate DTOs)
- **V1.36 scope**: structured sync only; platform publish (DF-59) is explicitly OUT
