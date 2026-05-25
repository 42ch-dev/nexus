# nexus-moment-context-assembly — Per-moment Context Assembly

Per-moment context assembly for ACP sessions. Three strategies:

- **Stage-0** (default, local-only): assembles context from SOUL sections, long-term memories,
  fragment keywords, and the user prompt.
- **Moment assembly** (SSOT, local-only): aggregates from all four local domains —
  creator memory, narrative state, World KB, and User knowledge. **`assemble_moment` is the single
  local assembly SSOT** — all CLI context assembly flows through `nexus42 platform context
  assemble-moment` (V1.28+).
- **Stage-1** (`cloud-stage` feature): two-stage assembly that merges platform API response
  with local data.

## Key Rules

- **`assemble_moment` is the SSOT** — there is no separate `assemble-local` command. Stage0 /
  degradation / optional two-stage behavior are flags on `assemble-moment`.
- **No cloud-sync dependency by default** — daemon builds use Stage-0 only.
- `cloud-stage` feature gate adds `nexus-cloud-sync` dependency and `TwoStageAssembly`.
- Token budget / truncation uses chars/4 heuristic (spec §9.3).
- **Personality section is NEVER truncated** — cross-domain truncation preserves personality
  and applies remaining budget to `world_state`, `timeline`, `world_kb`, `user_knowledge` in order.
- KB queries use `KbStore::query` with `KbQuery` (limit, text_search, block_type) — not
  unbounded `list_by_world`.
- `MomentRequest` carries optional KB query parameters (`kb_limit`, `kb_text_search`,
  `kb_block_type`) and `knowledge_limit` for structured queries.

## Dependencies

- `nexus-creator-memory` (for `LongTermMemory`)
- `nexus-contracts` (for `RuntimeMode`, `BlockType`, local types)
- `nexus-narrative` (for `NarrativeGateway` — world state, timeline, events)
- `nexus-kb` (for `KbStore`, `KbQuery` — World-scoped key blocks with structured queries)
- `nexus-knowledge` (for `KnowledgeStore` — User-scoped knowledge entries)
- `nexus-cloud-sync` (optional, behind `cloud-stage` feature)
