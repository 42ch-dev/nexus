# World KB Runtime Architecture

**Status**: Active (V1.40 grill-me locked)  
**Authority**: Implementation SSOT below normative specs. Does not override [entity-scope-model.md](specs/entity-scope-model.md) or [novel-workflow-profile.md](specs/novel-workflow-profile.md).  
**Iteration**: [v1.40-novel-world-kb-delivery-compass-v1.md](../iterations/v1.40-novel-world-kb-delivery-compass-v1.md)

---

## 1. Problem

World KB concerns were split across `nexus-kb`, `nexus-moment-context-assembly`, `nexus-orchestration` presets (`kb-extract`), and Layer 1 rules (now at `embedded-rules/writing-craft.md`). Taxonomy in spec (§5.1.1) diverged from `kb-extract` prompts (`Character`/`Ability`/…). V1.40 closes DF-63 with a single runtime layering model.

---

## 2. Crate responsibilities

| Layer | Crate | Responsibility |
| --- | --- | --- |
| **Domain (local)** | `nexus-kb` | KeyBlocks, SourceAnchors, taxonomy validation, `ingest_from_artifact()`, `KbStore` CRUD/query |
| **Domain (narrative)** | `nexus-narrative` | World entity, timeline binding |
| **Read SSOT** | `nexus-moment-context-assembly` | `WorldKbQueryBuilder` — shared filter/taxonomy logic; `assemble_moment` (wide session snapshot) and `build_chapter_kb_block` (narrow prompt slice) |
| **User knowledge** | `nexus-knowledge` | User-scoped global knowledge — **not** World KB |
| **Execution** | `nexus-orchestration` | Presets + capabilities; LLM inner graphs; schedule/job lifecycle — **no** KB domain rules |
| **Persistence mechanics** | `nexus-local-db` | SQLite migrations, `kb_extract_jobs`, `kb_key_blocks` tables |

Platform integration reads World KB through `assemble_moment` / moment-context-assembly contracts, not through orchestration presets.

---

## 3. Two loops (do not conflate)

### 3.1 Quality loop (V1.39 shipped)

```text
reflection-loop → findings → novel-brainstorm / novel-review-master
```

- `novel-review-master`: human decisions on findings; may read World KB context; does **not** own ingest.
- Layer 1/2 **rules** via `read_rules_layers()` — prose craft, not fictional facts.

### 3.2 Knowledge loop (V1.40 P3)

```text
persist → kb-extract (schedule) → World KB KeyBlocks + SourceAnchors
```

- Optional: `novel-review-master` **child schedule** waits for `kb-extract` when `preset.input.refresh_kb` (World-bound) so review sees freshly promoted KB.

---

## 4. Layer 1 rules (separate from World KB)

| Layer | Shipped default | User override |
| --- | --- | --- |
| Layer 1 craft rules | `crates/nexus-orchestration/embedded-rules/writing-craft.md` | `~/.nexus42/rules/writing-craft.md` |
| Layer 2 per-work rules | `Works/<work_ref>/Rules/novel-rules.md` | user-editable |
| Layer 3 history | `Works/<work_ref>/Rules/novel-rules-history.md` | append-only |

**Not** in `embedded-presets/` (presets are state machines with `preset.yaml` only).

---

## 5. Generic ingest job model

`kb_extract_jobs` remains for async ingest, retry, and `creator kb extract-status`. Jobs are **artifact-locator** based (multi creative-type ready):

```text
job {
  work_id,
  world_id,
  source_kind:   work_chapter | work_section | work_artifact | reference_doc
  source_locator: { relative_path } | { artifact_id } | { reference_id }
  profile_hint:  novel | screenplay | essay | generic   // selects extract prompt template
}
```

**V1.40 ships**: `source_kind=work_chapter`, `profile_hint=novel` only. Schema and CLI accept other kinds as reserved.

Domain path (extend existing stack — grill-me #13):

```text
kb-extract preset (LLM inner_graph)
  → capability kb.extract_work (extended input)
  → nexus-kb validation + KeyBlock upsert + SourceAnchor
```

Retire work-entry-only job semantics in V1.40; keep wire `BlockType` from contracts (grill-me #10).

---

## 6. Read path

```text
novel-writing outline/draft
  → refactor of fetch_world_kb → format_chapter_kb_block (moment-context-assembly)
  → KbStore::query (nexus-kb)
  → compact YAML block in preset template vars

platform context assemble-moment
  → assemble_moment → fetch_world_kb (existing)
  → wider scope / token budget than chapter block
```

Do **not** implement a second query implementation inside `nexus-orchestration`.

---

## 7. V1.40 plan mapping

| Slice | Architecture touch |
| --- | --- |
| P0.5 | `embedded-rules/` migration; this document |
| P1 | Wire `BlockType` + novel `body` validation in `nexus-kb` |
| P2 | Refactor `fetch_world_kb` + `format_chapter_kb_block` |
| P3 | Extend `kb.extract_work` + artifact jobs + `WorkFields.world_id` |
| P3 (tail) | `schedule.enqueue_child` + review-master `sync_world_kb` |
| P4 | Hygiene only — no duplicate P0.5 |

---

## 8. Explicit non-goals

- Merging `novel-review-master` and `kb-extract` into one preset.
- World KB logic in `embedded-rules/` or preset-only prompt strings.
- Backward compatibility with V1.29 work-entry-only job rows (may wipe in pre-release).
- Renaming `kb.extract_work` or adding parallel `BlockType` enum.
