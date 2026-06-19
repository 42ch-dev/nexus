# Nexus LLM Extract Capability

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.51 Shipped (T-A P0). The `nexus.llm.extract` capability, `LlmExtractTask`, and the `kb_extract_jobs` payload extension landed together; review-time extraction in `novel-review-master` swapped from the V1.50 heuristic to this LLM pathway (closes `R-V150KBED-01`). |
| **Document class** | Master |
| **Scope** | `nexus.llm.extract` capability contract; `LlmExtractTask` lifecycle; `kb_extract_jobs.proposed_payload` LLM extension; worker-pool reuse; integration with `novel-review-master` review-time extraction |
| **Last updated** | 2026-06-19 — V1.51 T-A P0 closeout; status confirmed Normative |
| **Related** | [orchestration-engine.md](./orchestration-engine.md) §4.4.1 (`LlmJudgeTask` sibling), [entity-scope-model.md](./entity-scope-model.md) §5.5 (World KB promotion), [world-kb-runtime-architecture.md](../world-kb-runtime-architecture.md) §5.5, [cli-spec.md](./cli-spec.md) §6.2G, [local-db-schema.md](./local-db-schema.md) §4.1.2 |

This Master is normative for the `nexus.llm.extract` capability surface. It is a
sibling to the `judge.llm` capability (`LlmJudgeTask`) — both reuse the V1.32
LLM worker pool and the `WorkerHandleProvider` seam, but emit different output
contracts: `judge.llm` emits a GO/NOGO verdict; `nexus.llm.extract` emits a
`Vec<KbCandidate>` carrying LLM-judged `block_type`, `canonical_name`,
`confidence`, and a verbatim `source_quote`.

---

## 1. Capability contract

**Name (registry key):** `nexus.llm.extract`

The `nexus.` prefix is the V1.51+ logical-capability naming convention for LLM
capabilities (compass §0.1 #7). The legacy `judge.llm` capability keeps its
short name for backward compatibility; no rename.

**Crate:** `nexus-orchestration` (`capability::builtins::LlmExtract`).

**Registration:** registered in all three `CapabilityRegistry` constructors
(`with_builtins`, `with_builtins_and_pool`, `with_runtime_deps`). Standalone
mode (no `WorkerHandleProvider`) returns `CapabilityError::WorkerUnavailable` —
no heuristic fallback inside the capability itself. The fallback decision lives
in the review-time hook caller (§5).

### 1.1 Input schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["prompt", "chapter_prose"],
  "properties": {
    "prompt": { "type": "string", "description": "Extraction instruction template (rendered by LlmExtractTask)" },
    "chapter_prose": { "type": "string", "description": "Verbatim chapter body to extract entities from" },
    "_creator_id": { "type": "string", "description": "Context-injected creator identity (security; not user-supplied)" },
    "_session_id": { "type": "string", "description": "Context-injected session identity (security; not user-supplied)" }
  }
}
```

Identity fields (`_creator_id`, `_session_id`) are injected by orchestration
context, **not** accepted from user/preset input — same security rule as
`judge.llm` (SEC-V131-01: prevents cross-creator IPC routing / IDOR).

### 1.2 Output schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["candidates"],
  "properties": {
    "candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["canonical_name", "block_type", "confidence", "source_quote"],
        "properties": {
          "canonical_name": { "type": "string", "description": "LLM-extracted canonical entity name" },
          "block_type": { "type": "string", "enum": ["character", "ability", "scene", "organization", "item", "conflict", "info_point", "event"], "description": "LLM-judged wire BlockType (snake_case)" },
          "summary": { "type": ["string", "null"], "description": "Optional one-line descriptor" },
          "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "LLM self-reported confidence" },
          "source_quote": { "type": "string", "description": "Verbatim chapter excerpt justifying the extraction" }
        }
      }
    }
  }
}
```

`block_type` values are the wire `BlockType` enum (SSOT:
`schemas/common/common.schema.json`; entity-scope-model §5.1.1). Implementations
MUST NOT introduce a parallel enum.

### 1.3 Worker invocation

The capability builds an extraction prompt (template + chapter prose framing),
calls `worker/acp_prompt` via `WorkerHandleProvider` with `deny_all` tool policy
(extraction is read-only — no tools, no side-effect), and parses the LLM
response JSON into `candidates`. Malformed LLM JSON is logged at `warn!` and
yields an empty candidate list (best-effort; the caller's hook is non-blocking).

---

## 2. Task lifecycle (`LlmExtractTask`)

`LlmExtractTask` mirrors `LlmJudgeTask` (orchestration-engine §4.4.1):

```text
1. Render `template` content against the orchestration context (handlebars).
2. Build capability input: { prompt: <rendered>, chapter_prose, _creator_id, _session_id }.
3. Resolve `nexus.llm.extract` (or configured capability name) via CapabilityRegistry.
4. Invoke capability; parse output.candidates into Vec<KbCandidate>.
5. WorkerUnavailable → return empty Vec (caller decides fallback).
```

`LlmExtractTask` is the unit the orchestrator routes a `kind: llm_extract` exit
condition / enter action to (acceptance criterion §4.1). It does NOT persist
candidates itself — persistence is the caller's responsibility (the review-time
hook), keeping the task pure and hermetically testable.

**Public surface:**

- `LlmExtractTask::new(template, capability_name, registry) -> Self`
- `LlmExtractTask::evaluate(&self, context) -> Result<Vec<KbCandidate>, GraphError>`

`KbCandidate` is defined in `nexus-orchestration::quality_loop` and is shared
between the heuristic fallback and the LLM pathway so callers treat both
uniformly (§3).

---

## 3. Payload schema (`kb_extract_jobs.proposed_payload` extension)

The `proposed_payload` TEXT column (V1.50 T-B P1) holds a `KeyBlockBody` JSON.
V1.51 T-A P0 extends the **content** of that JSON (no DDL change to the column
itself) to carry the LLM-extracted metadata, and adds two dedicated queryable
columns.

### 3.1 `proposed_payload` JSON shape (LLM pathway)

```json
{
  "summary": "One-line descriptor from LLM",
  "attributes": {
    "novel_category": "<derived from block_type per entity-scope-model §5.1.1 mapping>",
    "aliases": ["<canonical_name>"]
  },
  "tags": ["novel", "llm-extracted"],
  "block_type": "<snake_case wire value>",
  "canonical_name": "<LLM-extracted name>",
  "confidence": 0.0,
  "source_quote": "<verbatim chapter excerpt>"
}
```

The heuristic pathway (fallback when no worker) emits the V1.50 shape with
`tags: ["novel","heuristic-extracted"]` and omits the four LLM keys (or sets
`confidence: 0.0`, `source_quote: ""`, `block_type: "character"`).

### 3.2 Dedicated columns (migration `202606180006_kb_extract_jobs_llm_payload.sql`)

| Column | Type | Default | Purpose |
| --- | --- | --- | --- |
| `llm_confidence` | `REAL` | `NULL` | LLM self-reported confidence (0.0–1.0); `NULL` for heuristic rows. Sortable / filterable in `kb pending`. |
| `llm_source_quote` | `TEXT` | `NULL` | Verbatim chapter excerpt; `NULL` for heuristic rows. Surfaced on `adopt`. |

Existing rows (V1.50 heuristic) default to `NULL` for both — additive, no
destructive change. `block_type` reuses the existing `block_type_guess` column;
`canonical_name` reuses `canonical_name_guess`. The four LLM keys are therefore
recoverable from either the dedicated columns OR the `proposed_payload` JSON
(the adopt CLI reads columns first, falls back to JSON parse for backward
compat with V1.50 rows).

---

## 4. Worker pool reuse

`nexus.llm.extract` reuses the V1.32 LLM worker pool via the shared
`WorkerHandleProvider` trait injected through `CapabilityRuntimeDeps`. No new
worker pool, no new IPC channel, no new session type. This is compass §0.1 #7
("reuse the LLM worker pool") and non-goal §3 ("New LLM worker pool" is OUT).

The same `WorkerHandleProvider` instance backs `judge.llm`, `context.summarize`,
`acp.prompt`, and `nexus.llm.extract` — a single daemon-side worker manager
serves all LLM capabilities.

---

## 5. Integration with `novel-review-master`

The review-time extraction hook
(`quality_loop::extract_kb_candidates_for_review`) fires on every
`novel-review-master` schedule reaching the supervisor's terminal pipeline
(keyed on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID`).

V1.51 T-A P0 rewires that hook:

```text
extract_kb_candidates_for_review(pool, schedule_id, ws_dir, registry: Option<&CapabilityRegistry>)
  ├─ registry present + worker available → LlmExtractTask::evaluate → Vec<KbCandidate>
  │     with block_type + confidence + source_quote from LLM
  └─ registry None OR WorkerUnavailable → heuristic fallback (extract_candidates_from_text)
        with block_type="character", confidence=0.0, source_quote=""
```

The supervisor threads its `CapabilityRegistry` (added as an optional field in
V1.51) into the hook; the daemon boot constructs the registry and passes it
through. Hermetic tests that invoke the hook with `registry=None` get the
heuristic fallback, preserving V1.50 no-worker test behavior.

Candidates are persisted via `insert_pending_with_llm` (LLM pathway) or
`insert_pending` (heuristic fallback) into `kb_extract_jobs` with
`promotion_status='pending'`. The author confirms via
`creator world kb adopt <id>`, which surfaces `llm_confidence` +
`llm_source_quote` (cli-spec §6.2G).

### 5.1 Safe-default semantics

- LLM returns malformed JSON → empty candidate list + `warn!` log; hook
  completes (best-effort, non-blocking).
- WorkerUnavailable → heuristic fallback (the V1.50 behavior) so a daemon
  without a configured worker still produces character-name candidates rather
  than silently extracting nothing.
- No `nexus.llm.extract` registered → heuristic fallback (defensive; lets
  partial registries keep working).

The fallback is the **only** heuristic code path retained; it exists solely so
no-worker environments (hermetic tests, daemon-without-worker) remain
functional. Production daemons with a worker always take the LLM pathway.

---

## 6. Non-goals (V1.51 T-A P0)

- Cross-chapter reconciliation (`creator kb rescan --work`) — T-A P1.
- Missing-KB detection — T-A P2.
- Write-time extraction — explicitly OUT (compass §1.2 O12).
- New `schemas/` wire JSON — OUT (compass §0.1 #9); the `KbCandidate` struct
  and the payload extension are local-only Rust types + SQLite columns.
- Renaming `judge.llm` to `nexus.llm.judge` — OUT; only the new capability
  adopts the `nexus.` prefix.

---

## 7. Acceptance mapping

| Acceptance criterion (plan §4) | Where satisfied |
| --- | --- |
| §4.1 `nexus.llm.extract` registered; `kind: llm_extract` routes to `LlmExtractTask` | §1, `capability/mod.rs`, `tasks/mod.rs` |
| §4.2 `LlmExtractTask` hermetic tests (golden → golden, mock worker) | `tasks/mod.rs` `llm_extract_task_*` tests |
| §4.3 `novel-review-master` uses llm_extract; E2E asserts payload carries 4 LLM keys | §5, `tests/novel_review_master.rs` |
| §4.4 adopt shows confidence + source_quote | cli-spec §6.2G, `creator_world_kb_adopt.rs` |
| §4.5 R-V150KBED-01 closed | `status.json` residual_findings |
| §4.7 additive DB migration | §3.2, `202606180006_kb_extract_jobs_llm_payload.sql` |
| §4.8 wire contracts unchanged | §6 (no `schemas/` change) |
