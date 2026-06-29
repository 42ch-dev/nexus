# World KB Runtime Architecture

**Status**: Normative — V1.74 Shipped (§2 `kb_relationships` store + symmetric read projection; prior V1.51 §5.5 LLM pathway + §6 OCC extension)
**Authority**: Implementation SSOT below normative specs. Does not override [entity-scope-model.md](specs/entity-scope-model.md) or [novel-writing/workflow-profile.md](specs/novel-writing/workflow-profile.md).  
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

V1.74 adds `kb_relationships` as the first-class relationship store under the World KB graph. Source/target entities FK to `kb_key_blocks`; source anchors remain optional JSON projection ids validated by the daemon. `GET graph` reads stored rows and emits derived reverse projections for `symmetric=true` without writing duplicate rows.

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

### 5.5 LLM extraction pathway (V1.51 T-A P0 — Normative)

V1.51 T-A P0 closes `R-V150KBED-01` by swapping the V1.50 heuristic
(`block_type_guess='character'` for every capitalized noun phrase) for an
LLM-driven extraction pathway. The new `nexus.llm.extract` capability
([llm-extract.md](specs/llm-extract.md)) is a sibling to `judge.llm`: both
reuse the V1.32 LLM worker pool via `WorkerHandleProvider`, but
`nexus.llm.extract` emits `Vec<KbCandidate>` carrying LLM-judged `block_type`,
`canonical_name`, `confidence`, and a verbatim `source_quote`.

```text
novel-review-master (terminal) → supervisor hook
  → quality_loop::extract_kb_candidates_for_review(pool, sched, ws, registry)
     ├─ registry + worker available → LlmExtractTask → nexus.llm.extract
     │     → Vec<KbCandidate> with block_type/confidence/source_quote
     └─ no worker / WorkerUnavailable → heuristic fallback (V1.50 behavior)
  → insert_pending_with_llm / insert_pending → kb_extract_jobs (pending)
  → creator world kb adopt <id> (surfaces confidence + source_quote)
```

The hook is keyed on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID` (unchanged
from V1.50); the supervisor threads its `CapabilityRegistry` (new optional
field in V1.51) into the hook. The `proposed_payload` JSON gains
`block_type`/`canonical_name`/`confidence`/`source_quote` keys; two dedicated
columns (`llm_confidence REAL`, `llm_source_quote TEXT`) make confidence
queryable/sortable. Heuristic rows keep the V1.50 shape (columns `NULL`,
`tags: ["novel","heuristic-extracted"]`). Cross-chapter reconciliation
(T-A P1) and missing-KB detection (T-A P2) build on this pathway.

### 5.5.1 Cross-chapter reconciliation (V1.51 T-A P1 — Normative)

V1.51 T-A P1 closes `R-V150KBED-08` by extending the V1.50 chapter-scoped
`creator kb rescan <work_ref>/<chapter>` (T-B P2) with a work-scoped mode
`creator kb rescan --work <work_ref>`. The work-scoped mode reconciles
extraction candidates **across all chapters** of a Work so a recurring
entity (e.g. a character appearing in chapters 3, 5, 7) collapses to a
single `pending` candidate row carrying cross-chapter provenance, rather
than N independent pending rows the author must adopt one-by-one.

```text
creator kb rescan --work <work_ref>  (V1.51 T-A P1)
  → resolve_work(work_ref) → world_id
  → require_world_owner(world_id, creator)         (§5.5.3 author gate)
  → work_chapters::list_chapters(work_id)          (ordered by volume, chapter)
  → for each chapter with a body_path:
       read chapter prose → extract_candidates_from_text (heuristic)
  → aggregate_candidates_by_canonical_name(...)    (group by lowercased name)
       → one AggregatedCandidate per canonical_name
         carrying source_chapters: [3,5,7] provenance
  → [non-dry only] acquire Works/<work_ref>/.lock  (T-B P0 advisory lock)
  → upsert_pending_candidate ONCE per aggregate   (DB uniqueness collapses
       (creator, canonical_name, world) → 1 row; source_chapter_id = lowest;
       proposed_payload carries source_chapters array)
  → diff_and_apply refreshes confirmed KeyBlock bodies (§5.5 unchanged)
```

**Reconciliation rules (entity-scope-model §5.5.2 invariant preserved):**

- The `kb_extract_jobs` DB uniqueness `(creator_id, work_entry_id =
  canonical_name_guess, world_id) WHERE status NOT IN ('failed')` (V1.50 P1
  migration) is what collapses N same-name per-chapter candidates into 1 row.
  The work-scoped upsert calls `upsert_pending_candidate` **once per
  aggregate** (not once per chapter), so the merged row's `source_chapter_id`
  is the lowest referencing chapter and its `proposed_payload` carries a
  `source_chapters` array (e.g. `[3,5,7]`) recording every chapter that
  referenced the entity.
- A `confirmed` row is terminal (§5.5.2): the rescan never mutates a promoted
  `KeyBlock`'s origin candidate; it only refreshes the confirmed `KeyBlock`
  **body** via `diff_and_apply` (same as the chapter-scoped path).
- A `pending` row whose canonical_name matches an aggregate is `Updated`
  (payload + `source_chapter_id` refreshed); a brand-new aggregate is
  `Inserted`; a previously-pending candidate whose name vanished from **all**
  chapters is removed (stale cleanup across the whole work, not just one
  chapter).

**`--dry-run` cross-chapter reuse summary.** Before any DB write, the dry
path groups aggregates and reports, per canonical_name, the chapter list and
whether an active `KeyBlock` already exists for that name (advisory "no new
candidate needed" when the entity is already confirmed). Example line:
`Entity 'Aelin' referenced in chapters 3, 5, 7; existing KB row found → no
new candidate`. The dry path acquires **no** advisory lock (it is read-only).

**Extraction pathway used.** Work-scoped rescan uses the **heuristic**
(`extract_candidates_from_text`), identical to the V1.50 chapter-scoped path,
so the two modes produce consistent candidates for the same prose. The
`canonical_name` field — made first-class by T-A P0 — is the grouping key.
Wiring the rescan to the `nexus.llm.extract` LLM pathway (so aggregates carry
LLM-judged `block_type` + `confidence` + verbatim `source_quote`) is
**out of scope** for T-A P1 (the LLM pathway is a review-time/finalize-time
concern; rescan is a sync tool). The aggregation is pathway-agnostic:
`aggregate_candidates_by_canonical_name` operates on `KbCandidate` regardless
of origin, so a future plan can swap the extractor without touching the
reconciliation logic.

**Advisory lock integration (T-B P0).** Because the work-scoped rescan
mutates `kb_extract_jobs` rows that may also be touched by the daemon cron or
`creator run`, it acquires `Works/<work_ref>/.lock` (file `flock` + heartbeat)
**before** the cross-chapter upsert, on the non-dry path only. On contention
it returns `E_LOCK` (exit 75, `EX_TEMPFAIL`); on I/O failure it returns
`E_LOCK_IO` (exit 78, `EX_CONFIG`) — the dual exit-code contract from T-B P0.
This is the same lock `creator world kb adopt`, `creator works cron set`, and
`creator run` acquire; lock ordering is file-lock-before-DB, never reversed.

**T-B P1 CAS hook.** The V1.51 T-B P1 plan generalises the V1.50 P0
`set_schedule_json_tx` CAS pattern to `kb_extract_jobs` (adding a `version`
column + `cas_update` helper). The work-scoped upsert uses the V1.50
non-versioned `upsert_pending_candidate` today; T-B P1 will migrate it to the
versioned CAS path (a single call-site swap — see the `TODO(T-B P1)` marker
in `kb_rescan_work_hermetic`). The advisory lock acquired here is the
cross-process guard; CAS is the per-row optimistic guard; both compose.

---

## 6. Read path

### 6.1 OCC protection (V1.51 T-B P1)

`kb_extract_jobs.version` (added V1.51 T-B P1 migration `202606190001`) closes the
TOCTOU window between promotion-row reads and `mark_confirmed` writes in the
`creator world kb adopt` path. The adopt flow must:

1. Read the promotion row — capture `version = V`.
2. Validate and create the `KeyBlock`.
3. Call `mark_confirmed_in_tx_with_cas(tx, job_id, V)` — the UPDATE includes
   `AND version = V`.
4. On `VersionMismatch` (rows_affected == 0, version changed), surface
   `E_VERSION` (exit 76) to the CLI and advise retry.

Cross-chapter rescan (T-A P1) and missing-KB detection (T-A P2) also write to
`kb_extract_jobs` and **must** pass the read-preimage version through
`upsert_pending_candidate` to avoid overwriting fresher `proposed_payload` /
`promotion_status` values.

```text
┌────────────────────────────────────────────────┐
│ creator world kb adopt <extract_job_id>        │
├────────────────────────────────────────────────┤
│ 1. load_pending_candidate() → version = 0     │
│ 2. validate → KeyBlock                         │
│ 3. BEGIN TRANSACTION                           │
│ 4. insert KeyBlock                             │
│ 5. mark_confirmed_in_tx_with_cas(version=0)    │
│    ┌─ UPDATE ... SET version=version+1         │
│    │  WHERE job_id=? AND status='pending'      │
│    │  AND version=0                            │
│    └─ rows_affected==1 → COMMIT               │
│       rows_affected==0 → check cause:          │
│         - status≠pending → rollback (already   │
│           confirmed/rejected by another writer)│
│         - version≠0 → E_VERSION exit 76        │
│           (stale preimage; retry)              │
└────────────────────────────────────────────────┘
```

The CAS is applied **inside** the file-lock scope (concurrency.md §2.4): file lock
→ DB transaction → CAS. No deadlock risk.

### 6.2 Query path

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
