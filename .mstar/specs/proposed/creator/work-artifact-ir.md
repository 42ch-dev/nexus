---
module: nexus creator / work artifacts
date: 2026-11-10
status: proposed
problem_type: design
category: creator
severity: info
tags:
  - work-artifact-ir
  - ports-adapters
  - workspace
  - manuscript
  - outline
  - commit-fs
  - simulation
---

# Work Artifact IR — Read/Write Ports for Creative Work Output

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. Nothing here schedules work or claims a shipped feature. |
| **Document class** | Exploration |
| **Scope** | Detailed design for a **Work Artifact IR** that is the SSOT for a Work's durable creative artifacts (chapter bodies, outlines, review/quality logs, derived narrative and rules files, scaffolded tree layout), plus **read/write ports** that production and dry-run share. Lifecycle is **Load → Validate → Dry-run → Commit** (a storage-and-commit pipeline, not a strategy Generate → Execute pipeline). |
| **Coordinates with** | [chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md) (body is HTTP-read-only; structure/outline writable), [workflow-profile.md](../../active/novel-writing/workflow-profile.md), [quality-loop.md](../../active/novel-writing/quality-loop.md), [sync-contract.md](../../active/novel-writing/sync-contract.md), [creator-workflow.md](../../active/creator/creator-workflow.md), essay / script / game-bible profile specs under `active/creator/`, [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) (`WorkspaceExecutor`, SessionStorage), Canvas + Workflow IR redesign ([PR #483](https://github.com/42ch-dev/nexus/pull/483); `.mstar/specs/proposed/orchestration/canvas-and-workflow-ir-redesign.md`, landing with #483) |
| **Relationship to #483** | #483 is the detailed redesign of **strategy orchestration**: Workflow IR is the SSOT for strategy graphs (Generate \| Compose → Validate → Simulate → Approve → Apply → Execute). **This document** is a sibling Exploration for **Work output**. The two MUST remain separate documents, separate types, and separately versioned landings. They MAY share the ports/adapters *practice*, and production writes SHOULD converge on the existing `WorkspaceExecutor` / `commit_fs` seam. They MUST NOT merge into one IR or one runtime type. |
| **Path choice** | Lands under `proposed/creator/` because `.mstar/specs/AGENTS.md` has no `work/` class axis, and Work profiles plus `work-experience-model.md` already live under `creator/`. `novel-writing/` is too narrow (this IR spans novel, essay, script, and game-bible Works). |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only (if adopted).

**Not implement authority.** Per [`.mstar/specs/AGENTS.md`](../../AGENTS.md), Exploration under `proposed/` never outranks active Masters (chapter-content API, workflow profile, orchestration engine, local-db schema). Adoption requires a later Draft/Master promotion and link-migration sweep.

### 0.1 Why a separate IR (not an appendix of #483)

Workflow IR answers: *what strategy graph do we generate, compose, simulate, approve, and run?*

Work Artifact IR answers: *what durable files and metadata constitute this Work, and how do production vs dry-run read and commit them?*

Folding the second into the first would:

1. Mix a graph-authoring SSOT with a storage/commit SSOT.
2. Tempt Apply/Execute to treat chapter prose as strategy-graph nodes.
3. Block independent review and landing — Work I/O debt exists today even if Workflow IR is not yet adopted.

**Anti-pattern:** adding Work tree schemas, `Stories/` writers, or `body_path` reconcile rules into the Workflow IR document or crate.

---

## 1. Current inventory (evidence)

Evidence is from production sources under `crates/` and `apps/` (not tests, build scripts, or this Exploration itself). Representative functions are named so a later implementer can re-verify; line numbers below are orientation, not a frozen contract.

### 1.1 What a Work writes today

A Work's creative output is a **workspace tree** plus **SQLite metadata pointers**. For a novel Work the conventional layout is:

```text
Works/<work_ref>/
  README.md
  AGENTS.md                          # Layer-2 rules (V1.48+); legacy Rules/ may still exist
  Outlines/
    outline.md                       # work-level outline (essay / some profiles)
    volume-outline.md
    foreshadowing.md
    event-index.md
    chapters/chNN-outline.md         # or vVV-chNN-outline.md
  Stories/
    chNN-<slug>.md                   # chapter body (or vVV-chNN-<slug>.md)
  Logs/
    brainstorm/ | write/ | review/ | publish/
    review/review-report.md
    kb/missing/<YYYY-MM-DD>-chN.md
  .completion-lock.json              # derived observation file; DB is SSOT
```

Essay, script, and game-bible scaffolds write a different subtree (drafts, scripts, design sections) under the same `Works/<work_ref>/` root.

`work_chapters` rows store `outline_path` and `body_path` as relative strings such as `Works/<work_ref>/Stories/ch01-ch01.md`. The chapter **bytes are not in SQLite**. Reconcile walks the filesystem and updates pointers (`crates/nexus-local-db/src/work_chapters.rs`, `reconcile_from_filesystem` / `compute_reconcile_diff`).

### 1.2 Write paths (uneven durability, several owners)

| Surface | Owner | How it writes | Format |
| --- | --- | --- | --- |
| **Chapter body (hot path)** | Agent tool `fs/write_text_file` → `execute_write_file` in `crates/nexus-core/src/execution/capabilities.rs` | After `resolve_guarded_path_async`, **bare** `std::fs::create_dir_all` + `std::fs::write` (no temp/rename, no fsync) | Markdown + YAML frontmatter |
| **Prompt contract** | `crates/nexus-preset/embedded-presets/novel-writing/prompts/draft-chapter.md` | Instructs the agent to write `{{body_path}}` and create `Stories/` if missing | Same |
| **Chapter HTTP/Core API** | `crates/nexus-core/src/content.rs` `chapter_body` / `patch_chapter` | Body is **read-only** (`read_only: true`, `read_guarded_file`). `patch_chapter` mutates SQLite metadata only (slug/status/volume/`planned_word_count`; `title` is rejected as display-only) | Metadata in DB |
| **Web UI** | `apps/web` chapter page | Copies the path to the clipboard; does not persist body bytes | — |
| **Outline** | `crates/nexus-core/src/outline.rs` via `content::fsync_write_atomic` / `atomic_write_text` | Temp + fsync + rename + final-file fsync + parent-dir fsync | Markdown + frontmatter |
| **Scaffold** | `novel_scaffold.rs` `write_file_idem` / `create_dir_all_idem`; essay / script / game-bible scaffold `write_file` | Direct `std::fs` (novel uses a local `ScaffoldTransaction` rollback; others often temp+rename). Creates `Stories/` **empty**; does not draft chapter bodies | Templates, dirs |
| **Review report ingest** | Review preset/agent writes `Logs/review/review-report.md`; `auto_chain.rs` `persist_review_findings_for_schedule` reads with `std::fs::read_to_string`; `review_report.rs::parse_review_report` is a **pure** parser; findings go to SQLite | Report is Markdown on disk; findings are DB rows | Markdown → SQL |
| **Missing-KB log** | `quality_loop.rs::write_missing_kb_log` | Direct `std::fs::write` under `Logs/kb/missing/` | YAML frontmatter + Markdown |
| **Derived narrative / rules** | `narrative_index.rs::promote_outline_to_index`; `rules_layers.rs` / `rules_history.rs`; `auto_chronology.rs`; `novel_chapter_transition.rs` | Assorted `std::fs` temp+rename or append | Markdown / history tables |
| **Work create / delete** | `crates/nexus-core/src/works.rs` `create_work` / `delete_work` | Create writes **DB only** (`create_work_atomic`). Delete then best-effort `std::fs::remove_dir_all(Works/<work_ref>)`; comments state **DB is SSOT** for the Work row | Tree removal |
| **Completion lock file** | `completion_lock.rs::write_completion_lock` | Temp+rename `.completion-lock.json`. **Authoritative lock is** `works.completion_locked_at` | JSON (derived) |

### 1.3 An adapter already exists — the hot path does not use it

Orchestration already has a production workspace port:

- Trait: `nexus_orchestration::capability::WorkspaceExecutor` (`crates/nexus-orchestration/src/capability/mod.rs`) — `open` / `commit`.
- Production binding: `WorkspaceCommitExecutor` in `crates/nexus-core/src/execution/executor.rs`.
- Durable mechanics: `commit_workspace` → `session_commit` → `crates/nexus-core/src/execution/commit_fs.rs` (Unix atomic create / replace-verified / delete-verified).

Preset capabilities `workspace.open` / `workspace.commit` go through this port. **`fs/write_text_file` does not.** That is the core modularity failure: two write authorities, and the manuscript hot path is the weaker one.

### 1.4 Do not confuse FL-E stage `persist` with manuscript persist

`crates/nexus-orchestration/src/stage_gates.rs` maps `preset_for_stage("persist")` to **`kb-extract`**. The stage name means "extract knowledge after review," not "write the chapter body." Body writing happens during **produce** (`novel-writing` / `draft_chapter`) via the agent tool path in §1.2.

### 1.5 Pain points

1. **Direct I/O in the domain/tool layer.** The highest-volume Work output path calls `std::fs::write` inside a tool handler. Scaffold, outline, rules, narrative index, and quality logs each invent their own write helper.
2. **Durability is inconsistent.** Outline and `commit_fs` fsync and rename; `execute_write_file` does not.
3. **Path policy is duplicated.** `resolve_guarded_path_async`, `checked_work_directory`, and assorted `Works/<work_ref>/...` joins each confine paths differently.
4. **Layout is hard-coded** in presets, seed_chapters, sync discovery (`sync_module.rs` scans `Works/<work_ref>/Stories/*.md`), and tests. A new backend cannot be introduced without touching all of them.
5. **Dry-run has no stub at this seam.** A Simulate or test that exercises produce today can write real chapter files if the tool is live.
6. **HTTP body remains read-only** (intentional; see chapter-content-local-api). A Work Artifact port MUST preserve that product split: agent/commit writes bodies; the Daemon chapter body route stays a reader.

---

## 2. Target architecture

```text
  Producers (unchanged product split)
  ┌───────────────┐  ┌─────────────┐  ┌──────────────┐
  │ Agent tools   │  │ Core outline│  │ Scaffold /   │
  │ fs/write_…    │  │ HTTP PATCH  │  │ hooks / logs │
  └───────┬───────┘  └──────┬──────┘  └──────┬───────┘
          │                 │                 │
          └────────────┬────┴─────────┬───────┘
                       ▼
              Work Artifact Port
              (read / validate / dry-run / commit)
                       │
         ┌─────────────┼──────────────────┐
         ▼             ▼                  ▼
   Production      Metadata            Dry-run
   adapter         adapter             stub
   (SHOULD use     (work_chapters      (record intended
    WorkspaceExecutor / body_path,     writes; MUST NOT
    commit_fs)      reconcile)         touch the Work tree)
         │             │
         ▼             ▼
   Work Artifact IR snapshot (SSOT for *this Work's artifacts*)
   + workspace bytes + DB pointers
```

### 2.1 Normative target rules

1. **Work Artifact IR MUST be the SSOT** for a Work's durable creative artifacts and their metadata pointers (kinds, identity, hashes, relative paths, frontmatter-derived fields). It MUST NOT describe strategy graphs, preset states, or graph-flow Tasks.
2. **All production reads and writes of those artifacts MUST go through a Work Artifact port** (or a documented facade over it). New direct `std::fs::write` / `tokio::fs::write` call sites in produce / scaffold / outline / review-log / rules / narrative-index paths MUST NOT be added once the port is the default.
3. **Simulate, dry-run, and test fixtures that claim isolation MUST stub the port** and MUST NOT create, replace, or delete files under `Works/`.
4. **Workflow IR and Work Artifact IR MUST stay separate** — separate documents, crates or modules, versions, and land PRs. They MAY share adapter *practice* and production writes SHOULD converge on `WorkspaceExecutor` / `commit_fs` rather than inventing a third writer.
5. **HTTP `chapter_body` MUST remain read-only.** The port does not revive a browser manuscript editor.
6. **DB pointers and file bytes MUST stay reconcilable.** `body_path` / `outline_path` remain metadata; the IR records both the pointer and the payload hash. Reconcile stays a first-class operation.
7. **Path guards, frontmatter contracts, and workspace confinement MUST be preserved** (and SHOULD be implemented once, inside the port, not recopied at each caller).

### 2.2 What "replace direct writes" means

After migration acceptance (§6.3), the product MUST be able to:

- Draft a chapter (agent `fs/write_text_file` or a successor tool) without the tool handler calling `std::fs::write` itself.
- Write outlines, scaffolds, review/quality logs, and derived rules/index files through the same port.
- Run a dry-run of those operations with zero Work-tree mutation.
- Keep legacy on-disk layout (`Works/<work_ref>/Stories|Outlines|Logs`) as the default filesystem projection so sync and presets keep working during the transition.

---

## 3. Work Artifact IR model

### 3.1 Document kind and identity

A **Work Artifact IR snapshot** describes one Work at one revision. Conceptual identity:

| Field | Role |
| --- | --- |
| `work_id` | Stable Work primary key (`wrk_…`) |
| `work_ref` | Human slug used in the tree (`Works/<work_ref>/`) |
| `work_profile` | `novel` / `essay` / `script` / `game_bible` (and future profiles) |
| `revision` | CAS token for the snapshot (content hash over included artifacts + declared tree) |
| `workspace_root` | Binding to the pinned workspace, not a free path inside artifact payloads |

Each **artifact record** inside the snapshot:

| Field | Role |
| --- | --- |
| `artifact_id` | Stable id for this artifact (MAY be derived from kind + volume/chapter/rel_path) |
| `kind` | Closed vocabulary, §3.2 |
| `rel_path` | Workspace-relative path (`Works/<work_ref>/…`) |
| `content_hash` | Hash of payload bytes (or of the empty/dir marker) |
| `payload` | Artifact **bytes or text** — chapter prose, outline markdown, log body. This is payload, **not** a strategy-graph node |
| `metadata` | Frontmatter and pointer fields (status, chapter, volume, slug, word_count, world_refs, …) |
| `role` | `canonical` (SSOT bytes) or `derived` (e.g. `.completion-lock.json`, which MUST remain subordinate to DB) |

**MUST NOT** appear in this IR: Workflow IR nodes, `preset.yaml` states, graph-flow Task ids, inner-graph bindings, Approve/Apply identity of a strategy.

### 3.2 Artifact kinds (initial closed set)

| Kind | Typical path | Notes |
| --- | --- | --- |
| `work_readme` | `Works/<work_ref>/README.md` | Scaffolded; idempotent create |
| `work_rules` | `Works/<work_ref>/AGENTS.md` (legacy `Rules/` fallback on read) | Layer-2 rules |
| `rules_history` | history file beside rules | Append-only projection |
| `work_outline` | `Outlines/outline.md` | Work-level / essay |
| `volume_outline` | `Outlines/volume-outline.md` | |
| `chapter_outline` | `Outlines/chapters/chNN-outline.md` | HTTP-writable today |
| `foreshadowing_index` | `Outlines/foreshadowing.md` | Derived from outlines; also human-visible |
| `event_index` | `Outlines/event-index.md` | |
| `chapter_body` | `Stories/chNN-<slug>.md` | HTTP-read-only; agent-written |
| `essay_draft` / `script_document` / `game_bible_section` | profile-specific trees | Same port, different kind |
| `review_report` | `Logs/review/review-report.md` | Agent-written; parser remains pure |
| `quality_log` | `Logs/kb/missing/…` | Advisory log |
| `chronology_document` | Outlines / chronology files written by `auto_chronology` | |
| `log_stub_dir` | `Logs/{brainstorm,write,review,publish}/` | Tree entries |
| `completion_lock_file` | `.completion-lock.json` | **Derived only** |
| `tree_directory` | any directory created by scaffold | First-class so dry-run can describe mkdir |

The kind set MAY grow with a profile, but a new kind MUST NOT be used to smuggle strategy-graph or memory records into this IR.

### 3.3 Metadata vs payload

- **Payload** is the file contents (markdown, JSON, or a directory marker).
- **Metadata** is structured fields parsed from frontmatter and/or stored in `work_chapters` / `works`.
- The IR MUST keep them distinct. Updating `status` in the DB without rewriting bytes (today's `patch_chapter`) remains legal; the snapshot records a metadata revision even when `content_hash` is unchanged.
- Reconcile compares disk frontmatter + path layout against DB pointers and emits a structured diff (`work_chapters` already computes one). The port SHOULD expose that diff rather than leaving callers to walk `Stories/` themselves.

### 3.4 Encoding (open, see §8)

On-disk projection during transition MUST remain the current Markdown tree so sync and presets keep working. Whether a canonical IR snapshot is also persisted (JSON/CBOR beside the tree, or only in DB) is an open question. This Exploration does **not** require a second parallel tree of IR files in v1.

---

## 4. Read/write adapters

### 4.1 Port shape (conceptual)

A single port, name illustrative (`WorkArtifactPort`):

| Op | Behavior |
| --- | --- |
| `load(work_id \| work_ref)` | Read IR snapshot: tree listing + metadata pointers + optional payload fetch |
| `read(rel_path)` | Guarded read of one artifact |
| `validate(intent)` | Path confinement, kind/frontmatter/schema, status transitions, no-escape |
| `plan(intent)` | Return a commit plan (creates / replaces / deletes / mkdir) **without** writing |
| `commit(plan)` | Durable apply; then optional metadata reconcile (`body_path`, hashes) |
| `reconcile()` | Filesystem ↔ DB pointer diff + apply, today's `reconcile_*` moved behind the port |

`intent` is a list of planned artifact mutations (kind, rel_path, payload, expected_hash). It is **not** a Workflow IR document.

### 4.2 Adapters

| Adapter | Role |
| --- | --- |
| **Production filesystem** | Default. **SHOULD** implement `commit` by staging through `WorkspaceExecutor` / `commit_fs` (atomic, hash-verified) rather than bare `std::fs::write`. Path guard lives here once. |
| **Metadata DB** | `nexus-local-db` `work_chapters` / `works` pointers and lock columns. Not a second copy of chapter prose. |
| **Dry-run stub** | Records the plan, returns the would-be snapshot and hashes, **MUST NOT** mutate `Works/` or DB pointer rows (except optionally a throwaway test DB). |
| **Future backends (non-goals for v1, but the port MUST allow)** | Alternate object store, encrypted volume, or remote workspace. Layout projection may change only with an explicit migration. |

Callers (`execute_write_file`, outline services, scaffold builtins, `auto_chain` report ingest, `quality_loop` log write, `narrative_index`, `rules_layers`, `auto_chronology`) MUST depend on the port, not on a concrete `std::fs` helper.

### 4.3 Relationship to `WorkspaceExecutor` / `commit_fs`

This proposal MUST NOT invent a third production writer "because Work IR exists."

- **Preferred:** `WorkArtifactPort`'s production adapter is a **facade** over `WorkspaceExecutor` (open a session, commit a change list, let `commit_fs` perform atomic install). Tool `fs/write_text_file` becomes a thin translator: parameters `path` + `content` → one `plan`/`commit` on the port.
- **Acceptable transitional step:** the port's first filesystem adapter may wrap `fsync_write_atomic` (already used by outlines) so `execute_write_file` at least matches outline durability *before* full session-commit integration.
- **Not acceptable as the end state:** leaving `execute_write_file` on bare `std::fs::write` while other callers use the port.

`WorkspaceExecutor` stays the orchestration capability for preset `workspace.commit`. Work Artifact IR stays the typed SSOT for *what* was written. The adapter is the seam between them.

### 4.4 `fs/write_text_file` specifically

`execute_write_file` is the first and most important call site:

1. Keep the tool id and path-guard failure modes (outside-workspace → Forbidden).
2. Replace the `spawn_blocking` + `std::fs::write` body with `port.commit` (or `plan` + `commit`).
3. Preserve the JSON result shape `{ "written": true }` unless a versioned tool bump is explicit.
4. In dry-run / Simulate, inject the stub adapter so the same tool reports success without touching disk.

---

## 5. Lifecycle — Load, Validate, Dry-run, Commit

Work Artifact IR is a **storage and commit** pipeline. It MUST NOT copy #483's Generate → Execute authoring lifecycle; there is no "Approve a strategy graph" step here.

```text
Load  →  Validate  →  Dry-run?  →  Commit
                 ↘  (reject)
```

| Stage | Meaning | Side effects |
| --- | --- | --- |
| **Load** | Resolve Work identity, tree, pointers, optional payloads | Read-only |
| **Validate** | Path guard, kind/frontmatter, status transitions, hash expectations, no traversal | Read-only |
| **Dry-run** | `plan(intent)` against a stub or a copy-on-write view | **MUST NOT** write `Works/` or settle World KB / cloud sync |
| **Commit** | Durable apply via the production adapter; then metadata reconcile as needed | The only stage that mutates the Work tree |

Rules:

- Produce / scaffold / outline writes that run in a **Simulate** or isolated test MUST take the Dry-run path (stub port).
- Commit MUST be CAS-aware when `expected_hash` is present (same spirit as workspace commit hash conflicts).
- Load/Validate MUST NOT start an orchestration session or Apply a Workflow IR.
- Derived artifacts (`completion_lock_file`) MAY be written on Commit only as projections of DB state; they MUST NOT become authoritative.

---

## 6. Migration plan

**Prerequisite:** this Exploration is accepted as direction + detailed design (still not implement authority until Draft/Master promotion). Implementation of #483 is **not** a blocker: the two landings MAY proceed in parallel. Shared constraint: if a Workflow Simulate path can reach a Work write, it MUST use this port's stub (cross-link #483 once its boundary sentence lands).

### 6.1 Phase plan (steady, gated, no big-bang rewrite)

| Phase | Replace / add | Exit gate |
| --- | --- | --- |
| **P0 Port stub + inventory** | Trait + in-process FS adapter behind a flag; catalogue remaining direct-write sites | Tests compile; no production behavior change; inventory list checked in or referenced from this spec |
| **P1 Hot path** | Route `execute_write_file` through the port; adapter at least as durable as `fsync_write_atomic`, SHOULD use `commit_fs` | Draft-chapter writes no longer call `std::fs::write` in the tool handler; path-guard tests still pass |
| **P2 Sibling writers** | Outline, scaffolds, review-log ingest/write, missing-KB log, rules/history, narrative index, auto-chronology, chapter-transition file writes | Those modules do not call `std::fs::write` / `tokio::fs::write` directly (helpers may live only inside the adapter) |
| **P3 Dry-run stub** | Inject stub in Simulate / isolated tests; plan JSON for one novel produce sample | A produce dry-run leaves `Works/` unchanged |
| **P4 Reconcile admission** | `body_path` / frontmatter reconcile is a port op; optional admission on Commit | Existing `reconcile_work_chapters` behavior preserved; callers use the port |
| **P5 Default-on + lint** | Flag default on; CI/lint (or a crate-level rule) fails new direct writes in the inventoried modules | Acceptance §6.3 green |

Do **not** require P5 before P1 ships behind a flag. Prefer "hot path first, then close the remaining doors."

### 6.2 Must keep / must not break

- Workspace path guards and `chapter_body_path_forbidden` / outside-root Forbidden behavior.
- HTTP chapter body read-only contract ([chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md)).
- Frontmatter fields required by `draft-chapter.md` and reconcile (`title`, `chapter`, `status`, `word_count`, `world_refs` as currently contracted).
- Default on-disk layout under `Works/<work_ref>/Stories|Outlines|Logs` during the dual-read window (sync_module, seed_chapters, presets).
- `WorkspaceExecutor` / `commit_fs` as the durable production mechanic (converge onto it, do not fork).
- DB as SSOT for Work **rows** and for `completion_locked_at`.

### 6.3 Acceptance

All MUST hold on a reference workspace:

1. A `draft_chapter` / `fs/write_text_file` success writes (or dry-runs) **only** through the port.
2. Outline PATCH and novel/essay/script/game-bible scaffold use the same port.
3. Dry-run of those operations does not create, replace, or delete any file under `Works/`.
4. `reconcile_work_chapters` still converges DB pointers with the tree.
5. Sync discovery still finds `Stories/*.md` at the canonical layout (unless an explicit layout migration is accepted separately).
6. No Workflow IR type is required to compile or run the Work port.

---

## 7. Safety invariants

1. **No silent tree mutation in Dry-run / Simulate.** Stub or record only.
2. **Single production writer family.** End state is the port over `WorkspaceExecutor` / `commit_fs` (or a documented equally atomic adapter), not a new ad-hoc `std::fs` helper per builtin.
3. **Path confinement.** Every `rel_path` is resolved under the pinned workspace root; traversal and symlink escape fail closed (today's `resolve_guarded_path` / `checked_work_directory` intent, implemented once).
4. **Pointer/byte integrity.** Commit that moves a chapter file MUST update `body_path` in the same logical operation or fail and leave a reconcile-needed report; it MUST NOT leave a published pointer at a missing path without detection.
5. **Derived files are not authority.** `.completion-lock.json` cannot unlock or lock a Work against the DB column.
6. **Separation from Workflow IR.** A Work Commit MUST NOT Apply a strategy; a strategy Apply MUST NOT be the only way to write a chapter.

---

## 8. Open questions

1. Exact canonical snapshot encoding (in-memory only vs persisted JSON/CBOR) and whether `revision` is a tree hash, a DB row version, or both.
2. Whether P1's production adapter must already go through `WorkspaceExecutor` sessions, or whether `fsync_write_atomic` is an allowed intermediate (this document allows the intermediate, prefers the session path).
3. How aggressively to stabilize `Works/<work_ref>/…` layout vs allowing profile-specific path maps behind the port (sync_module and presets currently assume the novel layout).
4. Whether review-report **parse** stays a pure function with the port only doing I/O (recommended: yes).
5. Error-code stability for `fs/write_text_file` when the port returns hash conflicts (`FILE_WRITE_FAILED` vs a new typed conflict).
6. Multi-volume path conventions (`chNN` vs `vVV-chNN`) as a first-class IR field vs remaining a path convention.
7. Whether scaffold transactional rollback (`ScaffoldTransaction`) is fully subsumed by `commit_fs` rollback, and how mixed FS+DB transactions (seed_chapters + patch_work_tx) are ordered.

---

## 9. Non-goals

- **Merging this IR with Workflow IR / `preset.yaml` / Strategy Canvas** ([PR #483](https://github.com/42ch-dev/nexus/pull/483)). Strategy graphs stay there.
- **Memory I/O.** Memory crates and providers are being redesigned separately. This Exploration does not analyze them and does not recommend changes to them.
- **Replacing `SessionStorage`** (`SqliteSessionStorage` for graph-flow session/checkpoint rows). That port already exists; it is run-state, not Work artifacts.
- **Replacing `KnowledgeStore` or World KB settlement** as the primary knowledge graph. Missing-KB *logs* under `Logs/kb/` are in scope as artifacts; the KB store itself is not.
- **Home / workspace registration, Connect identity and config, host `meta.json` / `workspace.json`.** Host infrastructure, not Work artifacts.
- **UI `localStorage`** (locale, entrance, theme, connection).
- **Ordinary tracing / logging, build scripts, test fixtures, wasm module cache.**
- **A second orchestration runtime**, or reviving a human manuscript co-writer in the Web SPA.
- **Implementing this Exploration without a later Draft/Master promotion.**

---

## 10. References

- [`.mstar/specs/AGENTS.md`](../../AGENTS.md) — Exploration / proposed lifecycle rules
- [chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md) — HTTP body read-only, outline writable, `body_path` pointers
- [workflow-profile.md](../../active/novel-writing/workflow-profile.md) — Work tree and stage model
- [quality-loop.md](../../active/novel-writing/quality-loop.md) — review report and findings
- [sync-contract.md](../../active/novel-writing/sync-contract.md) — `Stories/` discovery
- [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) — `WorkspaceExecutor`, SessionStorage, source freeze
- Canvas + Workflow IR redesign — [PR #483](https://github.com/42ch-dev/nexus/pull/483) (sibling Exploration; strategy SSOT only)
- Dynamic workflow direction — [PR #482](https://github.com/42ch-dev/nexus/pull/482)
- Code anchors: `crates/nexus-core/src/execution/capabilities.rs` (`execute_write_file`), `commit_fs.rs`, `executor.rs` (`WorkspaceCommitExecutor`), `content.rs` / `outline.rs`, `works.rs`; `crates/nexus-orchestration/src/capability/mod.rs` (`WorkspaceExecutor`); scaffold builtins; `auto_chain.rs`; `work_chapters.rs`; `sync_module.rs`; `embedded-presets/novel-writing/prompts/draft-chapter.md`
