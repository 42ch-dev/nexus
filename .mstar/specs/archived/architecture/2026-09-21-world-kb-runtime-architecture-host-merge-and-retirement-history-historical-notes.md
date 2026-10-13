# World KB Runtime Architecture — Host Merge and Retirement History Historical Notes

**Provenance**: frozen from [`specs/active/architecture/world-kb-runtime-architecture.md`](../../active/architecture/world-kb-runtime-architecture.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 (2026-09-21) retired the `nexus-daemon-runtime` host that the frozen v1.190 P0 (2026-09-17) adapter/pack-bridge notes describe; the frozen text also carries the V1.139 `nexus-kb`→`nexus-knowledge` merge record, the V1.40 plan mapping, and the V1.29 back-compat note. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (49 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Normative — V1.74 Shipped (§2 `kb_relationships` store + symmetric read projection; prior V1.51 §5.5 LLM pathway + §6 OCC extension). **V1.139 SPOKE alignment**: crate `nexus-kb` merged INTO `nexus-knowledge`; `KeyBlock` → `KnowledgeEntry` (technical wire name). Crate table §2 updated to reflect current topology. Full terminology sweep deferred to P3.  
```

### at pre-freeze line ~12

```markdown
World KB concerns were split across `nexus-knowledge` (formerly `nexus-kb`, merged V1.139), `nexus-moment-context-assembly`, `nexus-orchestration` presets (`kb-extract`), and Layer 1 rules (now at `embedded-rules/writing-craft.md`). Taxonomy in spec (§5.1.1) diverged from `kb-extract` prompts (`Character`/`Ability`/…). V1.40 closes DF-63 with a single runtime layering model.
```

### at pre-freeze line ~21

```markdown
| **World KB domain** | `nexus-knowledge` | `crates/nexus-knowledge/src/world_kb/` owns `KnowledgeEntryRecord`, `SourceAnchor`, taxonomy/body validation, `KbStore` traits and query types, and extraction preparation/persistence coordination. The former `nexus-kb` crate was merged here in V1.139. |
```

### at pre-freeze line ~24

```markdown
| **User knowledge** | `nexus-knowledge` | User-scoped global knowledge — **also owns World KB after V1.139 merger** |
```

### at pre-freeze line ~30

```markdown
V1.74 adds `kb_relationships` as the first-class relationship store under the World KB graph. Source/target entities FK to `kb_key_blocks`; source anchors remain optional JSON projection ids validated by the daemon. `GET graph` reads stored rows and emits derived reverse projections for `symmetric=true` without writing duplicate rows.
```

### at pre-freeze line ~32

```markdown
### 2.1 `nexus-core` — single World/KB/narrative service-family authority (v1.190 P0 — Normative)
```

### at pre-freeze line ~38

```markdown
> **Historical v1.190 adapter/bridge notes (below).** The daemon HTTP adapters and transitional CLI pack bridge described in the next three paragraphs belong to the pre-retirement host topology, not current implementation anchors. Current pack service authority is `CoreService::import_world_pack` in `crates/nexus-core/src/world_pack.rs`; the `nexus-daemon-runtime` host was retired in v1.193.
```

### at pre-freeze line ~40

```markdown
**Denial → retained envelopes.** The guard surfaces the neutral `CoreError::WorldOwnerDenied { world_id, reason }`; the daemon adapter maps it to `Forbidden { resource: "world {id}", reason }` and HTTP 403. Each transport family keeps its retained envelope verbatim: World-KB-family routes render `world {id}` 404 plus the cross-author/unowned 403 reasons; timeline routes render `world {id} not found` 404 plus `you do not own this world` 403.
```

### at pre-freeze line ~42

```markdown
**Daemon HTTP handlers are thin translations.** The narrative/worlds, world_kb, fork, world_rules, world_findings, world_kb_pack, timeline and timeline_events handlers only resolve the active creator + stored principal, call the matching `CoreService` method and map the neutral error onto the retained status/body envelopes. World-KB canonical mutations keep the ownership check, expected-version CAS (`kb_key_blocks.revision` / `kb_extract_jobs.version`) and the durable `core_changes` outbox inside one private transaction in core; no handler keeps business SQL or a second mutation path.

**Pack bridge — retained authorized transition.** The retained local CLI pack composition is the direct-core caller `apps/nexus42/src/commands/creator/world/kb/pack.rs` → `CoreService::import_world_pack` / `export_world_pack` (`crates/nexus-core/src/world_pack.rs`); the former `nexus-daemon-runtime::pack_import` bridge module was deleted with the crate in v1.193 P2. The CLI translates its arguments into the core request and projects the core report back into the summary shape; conflict detection, remapping, provenance and persistence live in the core pack surface. The doc-hidden `CoreService::import_legacy_world_pack` remains transitional with a single deletion owner (P6-T1) — it admits the caller against stored state (registered creator, world ownership, non-query-only pool with a live registered writer) before importing, and no new callers may attach.
```

### at pre-freeze line ~50

```markdown
### 3.1 Quality loop (V1.39 shipped)
```

### at pre-freeze line ~59

```markdown
### 3.2 Knowledge loop (V1.40 P3)
```

### at pre-freeze line ~74

```markdown
| Layer 2 per-work rules | `Works/<work_ref>/Rules/novel-rules.md` | user-editable |
| Layer 3 history | `Works/<work_ref>/Rules/novel-rules-history.md` | append-only |
```

### at pre-freeze line ~95

```markdown
**V1.40 ships**: `source_kind=work_chapter`, `profile_hint=novel` only. Schema and CLI accept other kinds as reserved.
```

### at pre-freeze line ~105

```markdown
Retire work-entry-only job semantics in V1.40; keep wire `BlockType` from contracts (grill-me #10).
```

### at pre-freeze line ~107

```markdown
### 5.5 LLM extraction pathway (V1.51 T-A P0 — Normative)
```

### at pre-freeze line ~109

```markdown
V1.51 T-A P0 closes `R-V150KBED-01` by swapping the V1.50 heuristic
```

### at pre-freeze line ~127

```markdown
The hook is keyed on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID` (unchanged
from V1.50); the supervisor threads its `CapabilityRegistry` (new optional
field in V1.51) into the hook. The `proposed_payload` JSON gains
```

### at pre-freeze line ~132

```markdown
queryable/sortable. Heuristic rows keep the V1.50 shape (columns `NULL`,
```

### at pre-freeze line ~136

```markdown
### 5.5.7 Relationship candidate extraction (V1.76 γ — Normative)
```

### at pre-freeze line ~138

```markdown
V1.76 extends the extraction pathway to also **propose relationships** from
```

### at pre-freeze line ~147

```markdown
  ├─ candidates → persist_candidates → kb_extract_jobs (pending)   [V1.51 path]
```

### at pre-freeze line ~170

```markdown
### 5.5.1 Cross-chapter reconciliation (V1.51 T-A P1 — Normative)
```

### at pre-freeze line ~172

```markdown
V1.51 T-A P1 closes `R-V150KBED-08` by extending the V1.50 chapter-scoped
```

### at pre-freeze line ~181

```markdown
creator kb rescan --work <work_ref>  (V1.51 T-A P1)
```

### at pre-freeze line ~200

```markdown
  canonical_name_guess, world_id) WHERE status NOT IN ('failed')` (V1.50 P1
```

### at pre-freeze line ~224

```markdown
(`extract_candidates_from_text`), identical to the V1.50 chapter-scoped path,
```

### at pre-freeze line ~244

```markdown
**T-B P1 CAS hook.** The V1.51 T-B P1 plan generalises the V1.50 P0
```

### at pre-freeze line ~246

```markdown
column + `cas_update` helper). The work-scoped upsert uses the V1.50
```

### at pre-freeze line ~256

```markdown
### 6.1 OCC protection (V1.51 T-B P1)
```

### at pre-freeze line ~258

```markdown
`kb_extract_jobs.version` (added V1.51 T-B P1 migration `202606190001`) closes the
```

### at pre-freeze line ~315

```markdown
## 7. V1.40 plan mapping

| Slice | Architecture touch |
| --- | --- |
| P0.5 | `embedded-rules/` migration; this document |
| P1 | Wire `BlockType` + novel `body` validation in `nexus-knowledge` |
| P2 | Refactor `fetch_world_kb` + `format_chapter_kb_block` |
| P3 | Extend `kb.extract_work` + artifact jobs + `WorkFields.world_id` |
| P3 (tail) | `schedule.enqueue_child` + review-master `sync_world_kb` |
| P4 | Hygiene only — no duplicate P0.5 |

---

```

### at pre-freeze line ~332

```markdown
- Backward compatibility with V1.29 work-entry-only job rows (may wipe in pre-release).
```
