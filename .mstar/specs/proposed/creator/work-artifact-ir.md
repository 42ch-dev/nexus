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
| **Status** | **Proposed** — not implement authority. Nothing here schedules crates or claims a shipped feature. |
| **Document class** | Exploration |
| **Scope** | Detailed design for a **Work Artifact IR** that is the **SSOT** for a Work's durable creative artifacts (chapter bodies, outlines, review/quality logs, derived narrative and rules files, scaffolded tree layout, chapter-state fields that today live in `work_chapters`, path rules, frontmatter/kind vocabulary), plus **read/write ports** that production and dry-run share. Lifecycle is **Load → Validate → Dry-run → Commit** (a storage-and-commit pipeline, not a strategy Generate → Execute pipeline). |
| **Coordinates with** | [chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md) (HTTP wire; today asserts `work_chapters` metadata SSOT — target authority moves here; amendment in a later promotion PR), [workflow-profile.md](../../active/novel-writing/workflow-profile.md) (profile behavior; today owns layout + chapter-state — target layout/chapter-state authority moves here; amendment in a later promotion PR), [quality-loop.md](../../active/novel-writing/quality-loop.md), [sync-contract.md](../../active/novel-writing/sync-contract.md), [creator-workflow.md](../../active/creator/creator-workflow.md), [essay-profile.md](../../active/creator/essay-profile.md) / [script-profile.md](../../active/creator/script-profile.md) / [game-bible-profile.md](../../active/creator/game-bible-profile.md), [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) (`WorkspaceExecutor`, SessionStorage), Canvas + Workflow IR redesign ([PR #483](https://github.com/42ch-dev/nexus/pull/483); sibling Exploration — strategy SSOT only; path lands with #483) |
| **Relationship to #483** | #483 is the detailed redesign of **strategy orchestration**: Workflow IR is the SSOT for strategy graphs (Generate \| Compose → Validate → Simulate → Approve → Apply → Execute). **This document** is a sibling Exploration for **Work output**. The two MUST remain separate documents, separate types, and separately versioned landings. They MAY share the ports/adapters *practice*. They MUST NOT merge into one IR or one runtime type. Shared edge only: if a Workflow Simulate path would write Work artifacts, it MUST use this port's stub. |
| **Path choice** | Lands under `proposed/creator/` because `.mstar/specs/AGENTS.md` has no `work/` class axis, and Work profiles plus `work-experience-model.md` already live under `creator/`. `novel-writing/` is too narrow (this IR spans novel, essay, script, and game-bible Works). |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only (if adopted).

**Not implement authority.** Per [`.mstar/specs/AGENTS.md`](../../AGENTS.md), Exploration under `proposed/` never outranks active Masters (chapter-content API, workflow profile, orchestration engine, local-db schema). Adoption requires a later Draft/Master promotion and link-migration sweep. **This Exploration declares the target SSOT.** It does **not** pretend that the two Shipped Masters below have already been amended.

### 0.1 Why a separate IR (not an appendix of #483)

Workflow IR answers: *what strategy graph do we generate, compose, simulate, approve, and run?*

Work Artifact IR answers: *what durable files and metadata constitute this Work, and how do production vs dry-run read and commit them?*

Folding the second into the first would:

1. Mix a graph-authoring SSOT with a storage/commit SSOT.
2. Tempt Apply/Execute to treat chapter prose as strategy-graph nodes.
3. Block parallel review and landing — Work I/O debt exists today even if Workflow IR is not yet adopted.

**Anti-pattern:** adding Work tree schemas, `Stories/` writers, or `body_path` reconcile rules into the Workflow IR document or crate.

### 0.2 Core claim (owner-locked)

**Work Artifact IR is the SSOT** for Work durable creative artifacts and their chapter-state / path / frontmatter contracts. This is the change this Exploration exists to make. It is **not** a derived snapshot, a port-local cache, or a read model of `work_chapters`.

Today's Shipped Masters still outrank this file until promotion:

- [workflow-profile.md](../../active/novel-writing/workflow-profile.md) owns novel artifact layout (§3) and chapter-state SSOT (`work_chapters`, §4.1) and currently resolves status disagreement with **DB wins** (§4.5.3).
- [chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md) asserts `work_chapters` is the chapter metadata SSOT (§2).

**Target after promotion (same PR as IR Draft/Master):** IR wins; DB rows are an **index / cache**; reconcile rebuilds the index from IR (+ filesystem projection). The two Masters keep profile behavior / HTTP wire and **drop** the layout + metadata-SSOT claims in favor of links here. Until that amendment lands, implementers MUST treat those Masters as current authority and this document as the migration destination.

### 0.3 What moves into this IR vs what stays put

**Moves into Work Artifact IR (target SSOT):**

| Concern | Today's owner (Shipped) | Target |
| --- | --- | --- |
| Novel writer/read-only layout table | `workflow-profile.md` §3 | This IR §3.5 |
| Essay / script / game-bible subtrees (`Drafts/`, `Scripts/`, `Beats/`, `Characters/`, `Design/`, …) | profile Feature lines §3 | This IR §3.5 |
| Chapter-state fields (`chapter` / `volume` / `slug` / `status` / `planned_word_count` / `outline_path` / `body_path` / mirrors) | `work_chapters` + `workflow-profile.md` §4.1 + chapter-content-local-api §2 | This IR §3.3 + §3.6 |
| Path rules (single-volume `chNN`, multi-volume `vVV-chNN`, `Outlines/volume-{n}-outline.md`, `Stories/<file>.md`) | scaffold + `work_chapters.rs` + presets | This IR §3.6 |
| Artifact frontmatter schema + kind vocabulary | scattered templates / §3.2 sketch | This IR §3.2 (normative table) + §3.7 |
| Reconcile authority | DB wins on status disagreement (`workflow-profile.md` §4.5.3) | IR wins; DB is index/cache; reconcile = IR rebuild op (§3.8) |
| Pointer + content hash | DB path strings + ad-hoc | IR first-class fields |

**Stays put (MUST NOT move into this IR):**

| Concern | Why |
| --- | --- |
| `works` row and Work **run** status (`draft` / `active` / `paused` / `completed` / `archived`, intake, current_chapter as run cursor) | Work-level run authority, not artifact SSOT |
| `completion_locked_at` / completion semantics | Run lock; `.completion-lock.json` remains a **derived** observation file |
| SessionStorage / graph-flow session + checkpoint rows | Orchestration run-state |
| World KB, World binding (`world_id`), prompt World context blocks | Cross-Work knowledge / binding — profile behavior stays in workflow-profile / entity-scope |
| Memory (`spoke-memory` / Creator–Character memory) | Separate family; N4-R already on Spoke `MemoryPort` |
| User `KnowledgeStore` / World KB settlement stores | Separate knowledge surfaces; not Work tree artifacts (missing-KB **log files** under `Logs/kb/` stay in scope as artifacts) |

### 0.4 Later promotion PR (does not block this Exploration)

Per `.mstar/specs/AGENTS.md`:

1. Promote this Exploration → Draft/Master and `git mv` `proposed/creator/work-artifact-ir.md` → `active/…` in the **same** change as the Master amendments below.
2. Amend `workflow-profile.md`: keep stage chain, preset mapping, gates, prompt conventions, completion semantics, World-binding behavior; **remove** normative ownership of artifact layout (§3 writer/read-only table) and chapter-state SSOT (§4.1 / §4.5.3 “DB wins”); replace with links to Work Artifact IR; rewrite reconcile so **IR wins, DB is index/cache**.
3. Amend `chapter-content-local-api.md`: keep HTTP wire (routes, DTOs, body read-only); **delete** the “`work_chapters` is the chapter metadata SSOT” assertion; link IR.
4. Full relative-link sweep in that same PR. Do **not** leave a window where both Masters and IR claim SSOT.

This Exploration MAY merge while still Proposed. The amendment PR is a **follow-up**, not a prerequisite for accepting the direction.

---

## 1. Current inventory (evidence)

Evidence is from production sources under `crates/` and `apps/` (not tests, build scripts, or this Exploration itself). Representative functions are named so a later implementer can re-verify; line numbers below are orientation, not a frozen contract.

### 1.1 Where a Work lives today

A Work's creative output is a **workspace tree** plus **SQLite metadata pointers**. For a novel Work the conventional layout is:

```text
Works/<work_ref>/
  README.md
  AGENTS.md                          # Layer-2 rules (V1.48+); legacy Rules/ may still exist
  Outlines/
    outline.md                       # work-level outline (essay / some profiles)
    volume-outline.md                # or volume-{n}-outline.md (multi-volume)
    foreshadowing.md
    event-index.md
    chapters/chNN-outline.md         # or vVV-chNN-outline.md
  Stories/
    chNN-<slug>.md                   # chapter body (or vVV-chNN-<slug>.md)
  Logs/
    brainstorm/ | write/ | review/ | publish/
    review/review-report.md
    kb/missing/<YYYY-MM-DD>-chN.md
  .completion-lock.json              # derived observation file; DB column is run authority
```

Essay, script, and game-bible scaffolds write a different subtree under the same `Works/<work_ref>/` root (see §3.5).

`work_chapters` rows store `outline_path` and `body_path` as relative strings such as `Works/<work_ref>/Stories/ch01-ch01.md`. The chapter **bytes are not in SQLite**. Reconcile walks the filesystem and updates pointers (`crates/nexus-local-db/src/work_chapters.rs`, `reconcile_from_filesystem` / `compute_reconcile_diff`). Today, on status disagreement, **DB wins** (`workflow-profile.md` §4.5.3) — that authority direction is what this Exploration replaces after promotion.

### 1.2 Write paths (uneven durability, scattered owners)

| Surface | Owner | How it writes | Format |
| --- | --- | --- | --- |
| **Chapter body (hot path)** | Agent tool `fs/write_text_file` → `execute_write_file` in `crates/nexus-core/src/execution/capabilities.rs` | After `resolve_guarded_path_async`, **bare** `std::fs::create_dir_all` + `std::fs::write` (no temp/rename, no fsync) | Markdown + YAML frontmatter |
| **Prompt contract** | `crates/nexus-preset/embedded-presets/novel-writing/prompts/draft-chapter.md` | Instructs the agent to write `{{body_path}}` and create `Stories/` if missing | Same |
| **Chapter HTTP/Core API** | `crates/nexus-core/src/content.rs` `chapter_body` / `patch_chapter` | Body is **read-only** (`read_only: true`, `read_guarded_file`). `patch_chapter` mutates SQLite metadata only (slug/status/volume/`planned_word_count`; `title` is rejected as display-only) and holds `WorkLock` | Metadata in DB |
| **Web UI** | `apps/web` chapter page | Copies the path to the clipboard; does not persist body bytes | — |
| **Outline** | `crates/nexus-core/src/outline.rs` via `content::fsync_write_atomic` / `atomic_write_text` | Temp + fsync + rename + final-file fsync + parent-dir fsync; acquires `WorkLock` | Markdown + frontmatter |
| **Scaffold** | `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs` `write_file_idem` / `create_dir_all_idem`; essay / script / game-bible scaffold `write_file` | Direct `std::fs` (novel uses a local `ScaffoldTransaction` rollback; others often temp+rename). Creates `Stories/` **empty**; does not write chapter bodies | Templates, dirs |
| **Review report ingest** | Review preset/agent writes `Logs/review/review-report.md`; `crates/nexus-orchestration/src/auto_chain.rs` `persist_review_findings_for_schedule` reads with `std::fs::read_to_string`; `review_report.rs::parse_review_report` is a **pure** parser; findings go to SQLite | Report is Markdown on disk; findings are DB rows | Markdown → SQL |
| **Missing-KB log** | `crates/nexus-orchestration/src/quality_loop.rs::write_missing_kb_log` | Direct `std::fs::write` under `Logs/kb/missing/` | YAML frontmatter + Markdown |
| **Derived narrative / rules** | `narrative_index.rs::promote_outline_to_index`; `rules_layers.rs` / `rules_history.rs`; `auto_chronology.rs`; `novel_chapter_transition.rs` (all under `crates/nexus-orchestration/src/`) | Assorted `std::fs` temp+rename or **append** | Markdown / history tables |
| **Work create / delete** | `crates/nexus-core/src/works.rs` `create_work` / `delete_work` | Create writes **DB only** (`create_work_atomic`). Delete then best-effort `std::fs::remove_dir_all(Works/<work_ref>)`; comments state **database is SSOT** for the Work row | Tree removal |
| **Completion lock file** | `crates/nexus-orchestration/src/completion_lock.rs::write_completion_lock` | Temp+rename `.completion-lock.json`. **Authoritative lock is** `works.completion_locked_at` | JSON (derived) |

### 1.3 An adapter already exists — the hot path does not use it

Orchestration already has a production workspace port:

- Trait: `nexus_orchestration::capability::WorkspaceExecutor` (`crates/nexus-orchestration/src/capability/mod.rs`) — `open` / `commit`.
- Production binding: `WorkspaceCommitExecutor` in `crates/nexus-core/src/execution/executor.rs`.
- Durable mechanics: `commit_workspace` → `session_commit` → `crates/nexus-core/src/execution/commit_fs.rs` (Unix atomic create / replace-verified / delete-verified).

Preset capabilities `workspace.open` / `workspace.commit` go through this port. **`fs/write_text_file` does not.** That is the core modularity failure: two write authorities, and the manuscript hot path is the weaker one.

**Hard constraints of today's `commit_fs` (must not be ignored):**

- Caps: `MAX_FILE_BYTES = 1 MiB`, `MAX_CHANGES = 128`, `MAX_TOTAL_BYTES = 8 MiB` (`commit_fs.rs`).
- Non-Unix builds: `unsupported::refuse` rejects **all** mutations. Windows release packages today write chapter bodies via bare `std::fs::write`; naïvely routing them through `commit_fs` would break manuscript writes on Windows.
- Parent directories: change list is `Create` / `Modify` / `Delete` of files; commit walks an existing parent and fails if the parent dir is missing. `execute_write_file` currently `create_dir_all`s parents — a thin forwarder that only calls `commit_fs` would regress first-write / scaffold behavior.

### 1.4 Do not confuse FL-E stage `persist` with manuscript persist

`crates/nexus-orchestration/src/stage_gates.rs` maps `preset_for_stage("persist")` to **`kb-extract`**. The stage name means "extract knowledge after review," not "write the chapter body." Body writing happens during **produce** (`novel-writing` / `draft_chapter`) via the agent tool path in §1.2.

### 1.5 Pain points

1. **Direct I/O in the domain/tool layer.** The highest-volume Work output path calls `std::fs::write` inside a tool handler. Scaffold, logs, rules, narrative index, and quality logs each invent their own write helper.
2. **Durability is inconsistent.** Outline and `commit_fs` fsync and rename; `execute_write_file` does not.
3. **Boundary policy is duplicated.** `resolve_guarded_path_async`, `checked_work_directory`, and assorted `Works/<work_ref>/...` joins each confine paths differently.
4. **Layout is hard-coded** in scaffolds, seed_chapters, sync discovery (`crates/nexus-orchestration/src/sync_module.rs` scans `Works/<work_ref>/Stories/*.md`), and tests.
5. **Dry-run has no stub at this seam.** A Simulate or test that exercises produce today can create real chapter files if the tool is live.
6. **HTTP body remains read-only** (intentional). A Work Artifact port MUST preserve that product split.
7. **Concurrency authority is split.** Outline / HTTP patch acquire `WorkLock` (`crates/nexus-core/src/content.rs`); the hot-path tool write does **not**. Same class of defect as bypassing durable commit.
8. **Append kinds have no port op.** `rules_history` is append-only; `quality_log` is advisory append-ish; the naïve create/replace/delete port shape cannot express them.

---

## 2. Target architecture

```text
  Producers (unchanged product split)
  ┌───────────────┐  ┌─────────────┐  ┌──────────────┐
  │ Agent tools    │  │ Core outline│  │ Scaffold /   │
  │ fs/write_…    │  │ HTTP PATCH  │  │ hooks / logs │
  └───────┬───────┘  └──────┬──────┘  └──────┬───────┘
          │                 │                 │
          └────────┬────────┴─────────┬───────┘
                   ▼
          Work Artifact Port
          (load / validate / dry-run / commit / append / lock)
                   │
     ┌─────────────┼──────────────────┐
     ▼             ▼                  ▼
Production      Metadata            Dry-run
adapter         index adapter       stub
(byte path:     (work_chapters      (record intended
 fsync_write_    as cache of IR)     writes; MUST NOT
 atomic;                             touch the Work tree)
 structured
 commits MAY
 use commit_fs)
     │             │
     ▼             ▼
Work Artifact IR  (SSOT for this Work's artifacts + chapter-state)
+ filesystem projection + DB index/cache rows
```

### 2.1 Normative target rules

1. **Work Artifact IR MUST be the SSOT** for a Work's durable creative artifacts **and** for the chapter-state / path / frontmatter contracts listed in §0.3. It MUST NOT describe strategy graphs, preset states, or graph-flow Tasks. It MUST NOT be treated as a derived snapshot of `work_chapters`.
2. **All production reads and writes of those artifacts MUST go through a Work Artifact port** (or a documented facade over it). New direct `std::fs::write` / `tokio::fs::write` call sites in produce / scaffold / outline / review-log / rules / narrative-index paths MUST NOT be added once the port is the default.
3. **Simulate, dry-run, and test fixtures that claim isolation MUST stub the port** and MUST NOT create, replace, or delete files under `Works/`.
4. **Workflow IR and Work Artifact IR MUST stay separate** — separate documents, crates or modules, versions, and land PRs.
5. **HTTP `chapter_body` MUST remain read-only.** The port does not revive a browser manuscript editor.
6. **After promotion, IR is authoritative; DB rows are an index/cache.** `body_path` / `outline_path` / status / slug / volume live in the IR; `work_chapters` mirrors them for query. Reconcile rebuilds the index from IR + filesystem projection. **Until promotion**, today's Masters (DB wins) remain implement authority — see §0.2.
7. **Path guards, frontmatter contracts, workspace confinement, and `WorkLock` MUST be preserved** (and SHOULD be implemented once, inside the port, not recopied at each caller).
8. **Port scope is `Works/<work_ref>/` only.** `fs/write_text_file` today may write any path under the workspace root. Non-Work paths MUST keep a separate workspace-level fallback (existing guarded write or a future workspace port); they MUST NOT be forced through Work Artifact IR.

### 2.2 What "replace direct writes" means

After migration acceptance (§6.3), the product MUST be able to:

- Draft a chapter (agent `fs/write_text_file` or a successor tool) without the tool handler calling `std::fs::write` itself, for paths under `Works/<work_ref>/`.
- Write outlines, scaffolds, review/quality logs, and derived rules/index files through the same port.
- Append to append-only kinds (`rules_history`, and quality logs when treated as append) without fake full-file replace CAS.
- Run a dry-run of those operations with zero Work-tree mutation.
- Keep legacy on-disk layout as the default filesystem projection so sync and presets keep working during the transition.
- Hold or inherit `WorkLock` for mutating commits that today's outline/HTTP surfaces already serialize.

---

## 3. Work Artifact IR model

### 3.1 Document kind and identity

A **Work Artifact IR snapshot** describes one Work at one revision. Conceptual identity:

| Field | Role |
| --- | --- |
| `work_id` | Stable Work UUID key (`wrk_…`) |
| `work_ref` | Human slug used in the tree (`Works/<work_ref>/`) |
| `work_profile` | `novel` / `essay` / `script` / `game_bible` (and future profiles) |
| `revision` | CAS token for the snapshot (content hash over included artifacts + declared tree + chapter-state section) |
| `workspace_root` | Binding to the pinned workspace, not a free path inside artifact payloads |

Each **artifact record** inside the snapshot:

| Field | Role |
| --- | --- |
| `artifact_id` | Stable id for this artifact (MAY be derived from kind + volume/chapter/rel_path) |
| `kind` | Closed vocabulary, §3.2 |
| `rel_path` | Workspace-relative path under `Works/<work_ref>/…` |
| `content_hash` | Hash of payload bytes (or of the empty-dir marker) |
| `payload` | Artifact **bytes or text** — chapter prose, outline markdown, log text. This is payload, **not** a strategy-graph node |
| `metadata` | Frontmatter and chapter-state fields (status, chapter, volume, slug, word_count, world_refs, …) |
| `role` | `canonical` (SSOT bytes) or `derived` (e.g. `.completion-lock.json`, which MUST remain subordinate to `works.completion_locked_at`) |

**MUST NOT** appear in this IR: Workflow IR nodes, `preset.yaml` states, graph-flow Task ids, inner-graph bindings, Approve/Apply identity of a strategy, Memory entries, KnowledgeStore / World KB item bodies, SessionStorage checkpoints.

### 3.2 Artifact kinds (normative closed set for v1)

| Kind | Typical path | Notes |
| --- | --- | --- |
| `work_readme` | `Works/<work_ref>/README.md` | Scaffolded; idempotent create |
| `work_rules` | `Works/<work_ref>/AGENTS.md` (legacy `Rules/` fallback on read) | Layer-2 rules |
| `rules_history` | history file beside rules | **Append-only** — port MUST support `append` (or exclude from CAS replace) |
| `work_outline` | `Outlines/outline.md` | Work-level / essay |
| `volume_outline` | `Outlines/volume-outline.md` or `Outlines/volume-{n}-outline.md` | Multi-volume |
| `chapter_outline` | `Outlines/chapters/chNN-outline.md` or `…/vVV-chNN-outline.md` | HTTP-writable today |
| `foreshadowing_index` | `Outlines/foreshadowing.md` | Derived from outlines; also human-visible |
| `event_index` | `Outlines/event-index.md` | |
| `chapter_body` | `Stories/chNN-<slug>.md` or `Stories/vVV-chNN-<slug>.md` | HTTP-read-only; agent-written |
| `essay_draft` | `Drafts/draft.md` | Essay profile |
| `script_document` | `Scripts/script.md` | Script profile |
| `script_beats` | `Beats/beat-sheet.md` | Script profile |
| `script_characters` | `Characters/characters.md` | Script profile |
| `game_bible_section` | `Design/<section>.md` | Game-bible profile |
| `review_report` | `Logs/review/review-report.md` | Agent-written; parser remains pure |
| `quality_log` | `Logs/kb/missing/…` | Advisory; prefer append semantics |
| `chronology_document` | Outlines / chronology files via `auto_chronology` | |
| `log_stub_dir` | `Logs/{brainstorm,write,review,publish}/` etc. | Tree entries |
| `completion_lock_file` | `.completion-lock.json` | **Derived only** |
| `tree_directory` | any directory created by scaffold | First-class so dry-run can plan mkdir |

The kind set MAY grow with a profile, but a new kind MUST NOT smuggle strategy-graph, memory, or knowledge-store records into this IR.

### 3.3 Chapter-state section (target SSOT)

For `work_profile: novel`, the IR snapshot MUST carry a **chapter-state** section that is the target authority for fields today stored in `work_chapters`:

| Field | Role |
| --- | --- |
| `chapter` | Chapter number within volume |
| `volume` | Nullable / default 1; multi-volume uses 1..N |
| `slug` | Filename slug |
| `status` | `not_started` \| `outlined` \| `draft` \| `finalized` (`published` reserved) |
| `planned_word_count` | Optional |
| `actual_word_count` | Optional; often mirrored from body frontmatter |
| `outline_path` | Workspace-relative path |
| `body_path` | Workspace-relative path |
| `content_hash` / outline hash | First-class IR fields |

Essay / script / game-bible profiles use section/status frontmatter per their Feature lines; those fields live on the corresponding artifact records rather than `work_chapters`.

**HTTP `patch_chapter` today** mutates DB metadata without rewriting bytes. After promotion that remains a legal **metadata** mutation on the IR (revision bumps; payload hash unchanged). It MUST go through the port (and `WorkLock`), not a parallel DB-only authority that can diverge silently from IR.

### 3.4 Metadata vs payload

- **Payload** is the file contents (markdown, json, or a directory marker).
- **Metadata** is structured fields parsed from frontmatter and/or held in the chapter-state section.
- The IR MUST keep them distinct.
- Reconcile compares disk frontmatter + path layout against the IR and refreshes the DB index; the port SHOULD expose that diff rather than leaving callers to walk `Stories/` themselves.

### 3.5 Profile layouts (target ownership)

Normative layout trees move here from the profile Masters (summaries; full prompt/gate behavior stays in those Masters):

**Novel** (from `workflow-profile.md` §3):

```text
Works/<work_ref>/
  README.md
  Outlines/
    volume-outline.md | volume-{n}-outline.md
    chapters/chNN-outline.md | vVV-chNN-outline.md
    foreshadowing.md
    event-index.md
  Stories/
    chNN-<slug>.md | vVV-chNN-<slug>.md
  Logs/**
```

**Essay** (`essay-profile.md` §3): `README.md`, `Outlines/outline.md`, `Drafts/draft.md`, `Logs/{write,review}/` — MUST NOT use `Stories/` unless a future plan changes the profile.

**Script** (`script-profile.md` §3): `README.md`, `Scripts/script.md`, `Beats/beat-sheet.md`, `Characters/characters.md`, `Logs/{write,review}/`.

**Game bible** (`game-bible-profile.md` §3): `README.md`, `Design/{overview,pillars,characters,factions,species,locations,mechanics,magic_system,technology,economy,progression,lore}.md`, `Logs/{design,review}/`.

### 3.6 Path rules (first-class)

| Rule | Form |
| --- | --- |
| Single-volume chapter body | `Works/<work_ref>/Stories/chNN-<slug>.md` |
| Single-volume chapter outline | `Works/<work_ref>/Outlines/chapters/chNN-outline.md` |
| Multi-volume chapter body | `Works/<work_ref>/Stories/vVV-chNN-<slug>.md` (evidence: `work_chapters.rs` uses `v{volume:02}-{ch_nn}-…`) |
| Multi-volume chapter outline | `Works/<work_ref>/Outlines/chapters/vVV-chNN-outline.md` |
| Volume outline | `Outlines/volume-outline.md` or `Outlines/volume-{n}-outline.md` |
| Sync discovery root (novel) | `Works/<work_ref>/Stories/*.md` only |

Path construction MUST live in the IR/port, not as duplicated string formats in scaffold, seed, and sync.

### 3.7 Frontmatter contracts (sketch → normative on promotion)

Minimum novel chapter body frontmatter (from `workflow-profile.md` §4.3): `title`, `chapter`, optional `volume`, `status`, optional `word_count`, optional `world_refs`. Outline heading contracts and essay/script/game-bible section frontmatter remain as in their profile Masters until promotion copies the normative tables here.

### 3.8 Reconcile authority (target)

| Today (Shipped) | Target (after promotion) |
| --- | --- |
| Status disagreement: **DB wins**; next run re-syncs frontmatter | **IR wins**; DB `work_chapters` is rebuilt/updated as index/cache |
| `reconcile_from_filesystem` rebuilds rows from disk | `port.reconcile()` rebuilds IR (+ index) from disk projection **or** refreshes index from IR — IR remains the named SSOT |
| chapter-content-local-api: `work_chapters` is metadata SSOT | Wire DTO still reads/writes through Daemon; authority citation points at IR |

### 3.9 Encoding (open, see §8)

On-disk projection during transition MUST remain the current Markdown tree so sync and presets keep working. Whether a canonical IR snapshot is also persisted (JSON/CBOR beside the tree, or only reconstructed in process + DB index) is an open question. This Exploration does **not** require a second parallel tree of IR files in v1.

---

## 4. Read/write adapters

### 4.1 Port shape (conceptual)

A single port, name illustrative (`WorkArtifactPort`):

| Op | Behavior |
| --- | --- |
| `load(work_id \| work_ref)` | Read IR snapshot: tree listing + chapter-state + metadata + optional payload fetch |
| `read(rel_path)` | Guarded read of one artifact under `Works/<work_ref>/` |
| `validate(intent)` | Path confinement, kind/frontmatter/schema, status transitions, no-escape, lock preconditions |
| `plan(intent)` | Return a commit plan (creates / replaces / deletes / mkdir / append) **without** writing |
| `commit(plan)` | Durable apply under `WorkLock`; then index reconcile |
| `append(kind, rel_path, bytes)` | Append-only mutation for `rules_history` / advisory logs — MUST NOT be faked as blind full-file replace when history semantics matter |
| `reconcile()` | Filesystem ↔ IR ↔ DB-index diff + apply |

`intent` is a list of planned artifact mutations (kind, rel_path, payload, expected_hash). It is **not** a Workflow IR document.

### 4.2 Adapters

| Adapter | Role |
| --- | --- |
| **Production filesystem (byte path)** | Default for manuscript/outline/log **bytes**. End-state durability is **`fsync_write_atomic` semantics** (temp + fsync + rename + final fsync + parent-dir fsync; cross-platform). Port owns CAS/hash and `WorkLock`. |
| **Structured commit (optional)** | Preset `workspace.commit` change lists MAY continue to use `WorkspaceExecutor` / `commit_fs` for small structured bundles. MUST NOT be the sole manuscript body backend while `commit_fs` refuses non-Unix and caps at 1 MiB. |
| **Metadata index** | `nexus-local-db` `work_chapters` / selected `works` pointer columns as **cache of IR**, not a second prose store. |
| **Dry-run stub** | Records the plan, returns the would-be snapshot and hashes, **MUST NOT** mutate `Works/` or index rows (except in a throwaway test DB). |
| **Future backends (non-goals for v1, but the port MUST allow)** | Alternate object store, encrypted volume, or remote workspace. |

Callers (`execute_write_file` for Work paths, outline services, scaffold builtins, `auto_chain` report ingest, `quality_loop` log write, `narrative_index`, `rules_layers`, `auto_chronology`) MUST depend on the port, not on a concrete `std::fs` helper.

### 4.3 Relationship to `WorkspaceExecutor` / `commit_fs` / `fsync_write_atomic`

This proposal MUST NOT invent a third production writer "because Work IR exists." It also MUST NOT pretend today's `commit_fs` can absorb all manuscript writes unchanged.

**Locked direction (owner recommendation = Option A):**

1. **Byte persistence end state** for Work artifact payloads = `fsync_write_atomic` semantics, owned by the port (CAS + hash + parent mkdir + `WorkLock`).
2. **`commit_fs` / `WorkspaceExecutor`** remain the seam for preset **structured** `workspace.commit` bundles. They are not required to become the chapter-body writer.
3. **Caps:** if/when Work plans use `commit_fs`, `MAX_FILE_BYTES` / `MAX_CHANGES` / `MAX_TOTAL_BYTES` MUST be parameterized by kind / plan (chapter bodies per profile limits, e.g. 8 MiB), not left at the fixed 1 MiB / 128 / 8 MiB defaults without review. Streaming/hash-while-stage is preferred over holding whole files twice.
4. **Platform matrix:** Unix / macOS / Windows backends and failure modes MUST be documented in §7. Leaving "non-Unix refuse" for callers to discover is forbidden for the Work byte path.
5. **Option B (not chosen unless revisited):** add a real Windows `commit_fs` backend (`ReplaceFileW` / `MoveFileExW` / reparse-point-safe opens). Larger than A; needs a platform test matrix. This Exploration records B as an alternative, not the default.

**Parent directories (Greptile P2):** before any adapter claims to be a thin translator over `WorkspaceExecutor`/`commit_fs`, it MUST provide guarded parent-directory creation equivalent to today's `execute_write_file` `create_dir_all`, including cleanup on failure. `tree_directory` intents and scaffold mkdir MUST be first-class plan entries. A forwarder that only emits file Create/Modify/Delete against missing parents is a regression.

**Transitional step:** the port's first filesystem backend MAY wrap `fsync_write_atomic` (already used by outlines) so `execute_write_file` matches outline durability before any session-commit integration.

**Not acceptable as the end state:** leaving `execute_write_file` on bare `std::fs::write` while other callers use the port.

### 4.4 `fs/write_text_file` specifically

`execute_write_file` is the first and most important call site:

1. Keep the tool id and path-guard failure modes (outside-workspace → Forbidden).
2. **Branch on path:**
   - Under `Works/<work_ref>/` → `port.plan` / `port.commit` (Work Artifact IR).
   - Elsewhere under the workspace → existing guarded write / future workspace-level port (**fallback**). Do not widen Work IR to the whole workspace just to preserve today's tool breadth.
3. Preserve the JSON result shape `{ "written": true }` unless a versioned tool bump is explicit.
4. In dry-run / Simulate, inject the stub adapter so Work-path writes report success without touching disk.
5. Acquiring `WorkLock` (or an equivalent port-owned lock) is required for Work-path commits that mutate shared Work state.

### 4.5 `WorkLock`

`WorkLock` (`crates/nexus-core/src/content.rs`; used by outline + HTTP patch) is existing concurrency authority. The port MUST own or inherit it for mutating commits. Hot-path tool writes that bypass the lock today are a defect this migration closes, not an allowed exception.

### 4.6 Append

Kinds marked append-only (`rules_history`) and advisory append logs (`quality_log`) MUST use `append` (or an explicit exclusion from replace-CAS). Implementers MUST NOT silently rewrite the entire history file under a replace hash that races with concurrent appenders unless a single-writer lock and full-rewrite protocol are documented.

---

## 5. Lifecycle — Load, Validate, Dry-run, Commit

Work Artifact IR is a **storage and commit** pipeline. It MUST NOT copy #483's Generate → Execute authoring lifecycle; there is no "Approve a strategy graph" step here.

```text
Load  →  Validate  →  Dry-run?  →  Commit
                 ↘  (reject)
```

| Stage | Meaning | Side effects |
| --- | --- | --- |
| **Load** | Resolve Work identity, tree, chapter-state, pointers, optional payloads | Read-only |
| **Validate** | Path guard, kind/frontmatter, status transitions, hash expectations, lock preconditions, no traversal | Read-only |
| **Dry-run** | `plan(intent)` against a stub or a copy-on-write view | **MUST NOT** write `Works/` or settle World KB / cloud sync |
| **Commit** | Durable apply via the production adapter under lock; then index reconcile | The only stage that mutates the Work tree / IR revision |

Rules:

- Produce / scaffold / outline writes that run in a **Simulate** or isolated test MUST take the Dry-run path (stub port).
- Commit MUST be CAS-aware when `expected_hash` is present (same spirit as workspace commit hash conflicts).
- Load/Validate MUST NOT start an orchestration session or Apply a Workflow IR.
- Derived artifacts (`completion_lock_file`) MAY be written on Commit only as projections of DB run state; they MUST NOT become authoritative over `completion_locked_at`.

---

## 6. Migration plan

**Prerequisite:** this Exploration is accepted as direction + detailed design (still not implement authority until Draft/Master promotion). Implementation of #482 / #483 is **not** a blocker: this landing MAY proceed in parallel. Shared constraint: if a Workflow Simulate path can reach a Work write, it MUST use this port's stub (cross-link [PR #483](https://github.com/42ch-dev/nexus/pull/483)). Prefer having at least the port stub definition before #483 P2 Simulator relies on Work writes.

### 6.1 Phase plan (steady, gated, no big-bang rewrite)

| Phase | Replace / add | Exit gate |
| --- | --- | --- |
| **P0 Port stub + inventory** | Trait + in-process FS adapter behind a flag; catalogue remaining direct-write sites; define lock + append + mkdir in the trait | Tests compile; no production behavior change; inventory list checked in or referenced from this spec |
| **P1 Hot path** | Route Work-path `execute_write_file` through the port; adapter uses `fsync_write_atomic` semantics + parent mkdir + `WorkLock`; non-Work paths keep fallback | Draft-chapter writes under `Works/` no longer call bare `std::fs::write` in the tool handler; path-guard tests still pass; Windows manuscript write still works |
| **P2 Sibling writers** | Outline, scaffolds, review-log ingest/write, missing-KB log, rules/history (**append**), narrative index, auto-chronology, chapter-transition file writes | Those modules do not call `std::fs::write` / `tokio::fs::write` directly (helpers may live only inside the adapter) |
| **P3 Dry-run stub** | Inject stub in Simulate / isolated tests; plan JSON for one novel produce sample | A produce dry-run leaves `Works/` unchanged |
| **P4 Reconcile admission** | Reconcile is a port op oriented toward **IR authority** (index refresh); dual-read window documented until Master amendment | Existing reconcile behavior preserved or explicitly migrated; callers use the port |
| **P5 Default-on + lint** | Flag default on; CI/lint fails new direct writes in the inventoried modules | Acceptance §6.3 green |

Do **not** require P5 before P1 ships behind a flag. Prefer "hot path first, then close the remaining doors."

Promotion + Master amendments (§0.4) MAY land in parallel with P4/P5 but MUST NOT leave dual SSOT claims.

### 6.2 Must keep / must not break

- Workspace path guards and `chapter_body_path_forbidden` / outside-root Forbidden behavior.
- HTTP chapter body read-only contract ([chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md)).
- Frontmatter fields required by `draft-chapter.md` and reconcile (`title`, `chapter`, `status`, `word_count`, `world_refs` as currently contracted).
- Default on-disk layout under profile trees during the dual-read window (`sync_module`, seed_chapters, presets).
- Cross-platform manuscript writes (no silent Windows refuse on the byte path).
- `WorkLock` serialization for mutating authoring surfaces.
- DB as SSOT for Work **rows** and for `completion_locked_at` (run authority — unchanged).
- World binding / World KB / Memory / KnowledgeStore ownership unchanged (§0.3).

### 6.3 Acceptance

All MUST hold on a reference workspace:

1. A `draft_chapter` / `fs/write_text_file` success for a Work path writes (or dry-runs) **only** through the port.
2. Outline PATCH and novel/essay/script/game-bible scaffold use the same port.
3. Dry-run of those operations does not create, replace, or delete any file under `Works/`.
4. Reconcile still converges DB index pointers with the tree / IR.
5. Sync discovery still finds novel `Stories/*.md` at the canonical layout (unless an explicit layout migration is accepted separately).
6. No Workflow IR type is required to compile or run the Work port.
7. Append-only kinds do not rely on undocumented full-file replace races.
8. Parent-directory creation for first writes matches today's tool behavior (no missing-parent regression).

---

## 7. Safety invariants

1. **No silent tree mutation in Dry-run / Simulate.** Stub or record only.
2. **Single production writer family for Work bytes.** End state is the port over `fsync_write_atomic` semantics (plus optional `commit_fs` for structured bundles), not a new ad-hoc `std::fs` helper per builtin.
3. **Path confinement.** Every Work `rel_path` is resolved under the pinned workspace root and under `Works/<work_ref>/`; traversal and symlink escape fail closed.
4. **Pointer/byte integrity.** Commit that moves a chapter file MUST update `body_path` in the same logical operation or fail and leave a reconcile-needed report.
5. **Derived files are not authority.** `.completion-lock.json` cannot unlock or lock a Work against `completion_locked_at`.
6. **Separation from Workflow IR.** A Work Commit MUST NOT Apply a strategy; a strategy Apply MUST NOT be the only way to write a chapter.
7. **Platform honesty.** Unix / macOS / Windows Work byte backends and failure modes are explicit; non-Unix `commit_fs` refuse MUST NOT be the manuscript path.
8. **Lock honesty.** Mutating commits take `WorkLock` (or port-equivalent); tool hot path is not exempt.

---

## 8. Open questions

1. Exact canonical snapshot encoding (in-memory only vs persisted JSON/CBOR) and whether `revision` is a tree hash, a DB row version, or both.
2. Whether any Work structured bundles should still prefer `WorkspaceExecutor` sessions after Option A, or only preset `workspace.commit` keeps that path.
3. How aggressively to freeze `Works/<work_ref>/…` layout vs allowing profile-specific path maps behind the port (`sync_module` and presets currently assume the novel layout for chapter sync).
4. Whether review-report **parsing** stays a pure function with the port only doing I/O (recommended: yes).
5. Error-code stability for `fs/write_text_file` when the port returns hash conflicts (`FILE_WRITE_FAILED` vs a new typed conflict).
6. Whether multi-volume path forms need a dedicated IR path-builder type vs validated string patterns.
7. Whether scaffold transactional rollback (`ScaffoldTransaction`) is fully subsumed by port plans, and how mixed FS+DB transactions (seed_chapters + patch_work_tx) are ordered around IR revision bumps.
8. Exact holder_kind conventions when the port acquires `WorkLock` on behalf of tools vs HTTP vs CLI.

---

## 9. Non-goals

- **Merging this IR with Workflow IR / `preset.yaml` / Strategy Canvas** ([PR #483](https://github.com/42ch-dev/nexus/pull/483)). Strategy graphs stay there.
- **Memory I/O.** Memory crates and providers are redesigned on Spoke `MemoryPort` (N1–N4-R shipped; N4-T/N5 separate). This Exploration does not analyze them and does not recommend changes to them.
- **Replacing `SessionStorage`** (`SqliteSessionStorage` for graph-flow session/checkpoint rows). That port already exists; it is run-state, not Work artifacts.
- **Replacing `KnowledgeStore` or World KB settlement** as the primary knowledge graph. Missing-KB *logs* under `Logs/kb/` are in scope as artifacts; the KB / KnowledgeStore themselves are not.
- **Moving `works` run status or `completion_locked_at` into this IR.**
- **Moving World binding / World KB continuity contracts out of workflow-profile.**
- **Home / workspace registration, Connect identity and config, host `meta.json` / `workspace.json`.** Host infrastructure, not Work artifacts.
- **UI `localStorage`** (locale, entrance, theme, connection).
- **Ordinary tracing / logging, build scripts, test fixtures, wasm module cache.**
- **A second orchestration runtime**, or reviving a human manuscript co-writer in the Web SPA.
- **Implementing this Exploration without a later Draft/Master promotion**, or amending the two Shipped Masters in *this* PR.
- **Downgrading Work Artifact IR to a derived snapshot / port-local model.** Owner-locked: IR is the SSOT.

---

## 10. References

- [`.mstar/specs/AGENTS.md`](../../AGENTS.md) — Exploration / proposed lifecycle rules
- [chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md) — HTTP body read-only, outline writable; today metadata SSOT assertion (to amend)
- [workflow-profile.md](../../active/novel-writing/workflow-profile.md) — novel layout + chapter-state (to amend); stage/gates/completion stay
- [essay-profile.md](../../active/creator/essay-profile.md) / [script-profile.md](../../active/creator/script-profile.md) / [game-bible-profile.md](../../active/creator/game-bible-profile.md) — non-novel trees
- [quality-loop.md](../../active/novel-writing/quality-loop.md) — review report and findings
- [sync-contract.md](../../active/novel-writing/sync-contract.md) — `Stories/` discovery
- [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) — `WorkspaceExecutor`, SessionStorage, source freeze
- Canvas + Workflow IR redesign — [PR #483](https://github.com/42ch-dev/nexus/pull/483) (sibling Exploration; strategy SSOT only)
- Dynamic workflow direction — [PR #482](https://github.com/42ch-dev/nexus/pull/482)
- Code anchors:
  - `crates/nexus-core/src/execution/capabilities.rs` (`execute_write_file`)
  - `crates/nexus-core/src/execution/commit_fs.rs` (caps + non-Unix refuse)
  - `crates/nexus-core/src/execution/executor.rs` (`WorkspaceCommitExecutor`)
  - `crates/nexus-core/src/content.rs` (`fsync_write_atomic`, `WorkLock`, `patch_chapter`, `chapter_body`)
  - `crates/nexus-core/src/outline.rs`
  - `crates/nexus-core/src/works.rs`
  - `crates/nexus-orchestration/src/capability/mod.rs` (`WorkspaceExecutor`)
  - `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs`
  - `crates/nexus-orchestration/src/{auto_chain,quality_loop,narrative_index,rules_layers,rules_history,auto_chronology,completion_lock,sync_module,review_report}.rs`
  - `crates/nexus-local-db/src/work_chapters.rs` (path forms + reconcile)
  - `crates/nexus-preset/embedded-presets/novel-writing/prompts/draft-chapter.md`
