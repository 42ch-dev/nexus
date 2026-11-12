# ADR: Work Artifact IR — Read/Write Ports for Creative Work Output

Status: implemented — P0–P2 shipped through v1.235 and P4–P5 shipped in v1.236; P3 added only the recording stub, with Simulate selection still open
Date: 2026-11-10
Scope: Detailed design for a **Work Artifact IR** that is the **SSOT** for a Work's durable creative artifacts (chapter bodies, outlines, review/quality logs, derived narrative and rules files, scaffolded tree layout, chapter-state fields that today live in `work_chapters`, path rules, frontmatter/kind vocabulary), plus a production port for byte commit and reconcile. The wider Load → Validate → Dry-run → Commit lifecycle is an open consequence, not shipped behavior.
Source: migrated from the retired tracked specs corpus

## Problem

1. **Direct I/O in the domain/tool layer.** The highest-volume Work output path called `std::fs::write` inside a tool handler. Scaffold, logs, rules, narrative index, and quality logs each invented their own write helper.
2. **Durability was inconsistent.** Outline and `commit_fs` used fsync and rename; `execute_write_file` did not.
3. **Boundary policy was duplicated.** `resolve_guarded_path_async`, `checked_work_directory`, and assorted `Works/<work_ref>/...` joins confined paths differently.
4. **Layout was hard-coded** in scaffolds, seed_chapters, sync discovery (`crates/nexus-orchestration/src/sync_module.rs` scans `Works/<work_ref>/Stories/*.md`), and tests.
5. **Production dry-run still has no selected stub at this seam.** A Simulate or test that exercises produce with the live tool can create real chapter files.
6. **HTTP body remains read-only** (intentional). A Work Artifact port preserves that product split.
7. **Concurrency authority was split.** Outline / HTTP patch acquired `WorkLock` (`crates/nexus-core/src/content.rs`); the hot-path tool write did not.
8. **Append kinds had no port op.** `rules_history` is append-only; `quality_log` is advisory append-ish; a naïve create/replace/delete port cannot express them.

### Original production inventory

A Work's creative output is a workspace tree plus SQLite metadata pointers. Chapter bytes are not in SQLite. Representative write paths were:

| Surface | Original owner and behavior |
| --- | --- |
| Chapter body | Agent tool `fs/write_text_file` → `execute_write_file` in `crates/nexus-core/src/execution/capabilities.rs`; guarded path followed by bare `std::fs::write` |
| Chapter HTTP/Core API | `crates/nexus-core/src/content.rs`; body read-only, metadata patch only, under `WorkLock` |
| Outline | `crates/nexus-core/src/outline.rs`; temp + fsync + rename + final-file and parent-dir fsync under `WorkLock` |
| Scaffold | Novel/essay/script/game-bible builtins with local direct filesystem helpers and rollback |
| Review report | Agent-written Markdown; pure parser; findings persisted to SQLite |
| Missing-KB log | Direct `std::fs::write` under `Logs/kb/missing/` |
| Derived narrative/rules | Assorted temp+rename or append writers |
| Work create/delete | DB-owned Work row plus best-effort tree removal |
| Completion lock | Derived `.completion-lock.json`; `works.completion_locked_at` remains authoritative |

`WorkspaceExecutor` and `WorkspaceCommitExecutor` already provided a structured workspace port through `commit_fs`, but the manuscript hot path bypassed it. Its fixed limits (`MAX_FILE_BYTES = 1 MiB`, `MAX_CHANGES = 128`, `MAX_TOTAL_BYTES = 8 MiB`), non-Unix refusal, and missing-parent behavior made it unsuitable as an unchanged chapter-body backend.

The FL-E stage named `persist` maps to `kb-extract`; it is not manuscript persistence. Body writing belongs to produce.

## Decision

### 1. Document position and ownership

Work Artifact IR is the SSOT for Work durable creative artifacts and their chapter-state, path, and frontmatter contracts. It is neither a derived snapshot, a port-local cache, nor a read model of `work_chapters`. Database rows are an index/cache; reconcile rebuilds or refreshes that index from IR and the filesystem projection.

Workflow IR answers what strategy graph is generated, composed, simulated, approved, and run. Work Artifact IR answers what durable files and metadata constitute a Work and how production commits and reconciles them. The two remain separate documents, types, versions, and landing paths. They share port/adapter practice, not one runtime type. Workflow Simulate selection of the recording stub is not wired into the shipped execution plane; the P3 reopen condition is recorded in `Consequences`.

Work Artifact IR coordinates with [chapter-content-local-api.md](../../../../docs/specs/runtime/chapter-content-local-api.md), [workflow-profile.md](../../../../docs/specs/novel-writing/workflow-profile.md), [quality-loop.md](../../../../docs/specs/novel-writing/quality-loop.md), [sync-contract.md](../../../../docs/specs/novel-writing/sync-contract.md), [creator-workflow.md](../../../../docs/specs/creator/creator-workflow.md), and the essay/script/game-bible profile documents. Those documents retain HTTP wire, profile behavior, stage chains, gates, prompts, completion semantics, and World binding; Work Artifact IR owns artifact layout and chapter-state authority.

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms.

### 2. Ownership boundaries

Work Artifact IR owns:

| Concern | Contract |
| --- | --- |
| Novel, essay, script, and game-bible artifact layout | Defined by the IR and its profile rules |
| Chapter state | `chapter`, `volume`, `slug`, `status`, planned/actual word counts, `outline_path`, `body_path`, mirrors, and hashes |
| Paths | Single-volume `chNN`, multi-volume `vVV-chNN`, outline forms, and `Stories/<file>.md` discovery |
| Frontmatter and kinds | Closed artifact vocabulary and profile metadata contracts |
| Reconcile | IR wins; DB is index/cache |
| Pointer and content hash | First-class IR fields |

The following stay outside this IR:

- `works` row and Work run status (`draft`, `active`, `paused`, `completed`, `archived`), intake, and run cursor.
- `completion_locked_at` and completion semantics; `.completion-lock.json` is derived only.
- SessionStorage, graph-flow sessions, and checkpoints.
- World KB, World binding, prompt World context, Memory, KnowledgeStore, and settlement stores. Missing-KB log files remain artifacts.
- Strategy nodes, preset states, graph-flow Task ids, inner-graph bindings, strategy Approve/Apply identity, and orchestration run state.

### 3. Architecture

```text
  Producers (unchanged product split)
  ┌───────────────┐  ┌─────────────┐  ┌──────────────┐
  │ Agent tools    │  │ Core outline│  │ Scaffold /   │
  │ fs/write_…    │  │ HTTP PATCH  │  │ hooks / logs │
  └───────┬───────┘  └──────┬──────┘  └──────┬───────┘
          └────────┬────────┴─────────┬───────┘
                   ▼
          Work Artifact Port
             (commit / reconcile)
                   │
     ┌─────────────┴──────────────────┐
     ▼                                ▼
Production byte adapter          Metadata index adapter
(fsync_write_atomic;             (work_chapters index;
 parent mkdir; WorkLock)          reconcile diff/apply)
     │                                │
     └──────────────┬─────────────────┘
                    ▼
Work Artifact IR + filesystem projection + DB index/cache rows
```

1. All production Work byte writes and reconcile admissions go through the port. Produce, scaffold, outline, review-log, rules, and narrative-index paths do not add direct `std::fs::write` or `tokio::fs::write` calls.
2. `RecordingWorkArtifactPort` exists for tests, but no production Simulate/dry-run execution plane selects it; production write isolation is therefore not claimed.
3. HTTP `chapter_body` remains read-only; the port does not revive a browser manuscript editor.
4. Path guards, frontmatter contracts, workspace confinement, and `WorkLock` are owned once by the port rather than recopied at callers.
5. Port scope is `Works/<work_ref>/` only. Non-Work paths retain the separate guarded workspace fallback and are not forced through Work Artifact IR.
6. The existing on-disk Markdown tree remains the filesystem projection, so sync and presets retain their layout.
7. Mutating commits hold or inherit `WorkLock`.

### 4. Work Artifact IR model

#### 4.1 Document and artifact identity

A snapshot describes one Work at one revision:

| Field | Role |
| --- | --- |
| `work_id` | Stable Work UUID key (`wrk_…`) |
| `work_ref` | Human slug used in `Works/<work_ref>/` |
| `work_profile` | `novel`, `essay`, `script`, `game_bible`, or a later profile |
| `revision` | Snapshot revision identity; the shipped port exposes no CAS token |
| `workspace_root` | Binding to the pinned workspace, not a free path in artifact payloads |

Each artifact record carries:

| Field | Role |
| --- | --- |
| `artifact_id` | Stable id, optionally derived from kind + volume/chapter/rel_path |
| `kind` | Closed vocabulary below |
| `rel_path` | Workspace-relative path under `Works/<work_ref>/…` |
| `content_hash` | Hash of payload bytes or empty-dir marker |
| `payload` | Artifact bytes or text, never a strategy-graph node |
| `metadata` | Frontmatter and chapter-state fields |
| `role` | `canonical` for SSOT bytes or `derived` for subordinate projections |

Workflow IR nodes, `preset.yaml` states, graph-flow Task ids, inner-graph bindings, strategy Approve/Apply identity, Memory entries, KnowledgeStore / World KB bodies, and SessionStorage checkpoints never appear in this IR.

#### 4.2 Artifact kinds

| Kind | Typical path | Notes |
| --- | --- | --- |
| `work_readme` | `Works/<work_ref>/README.md` | Idempotent scaffold create |
| `work_rules` | `AGENTS.md` with legacy `Rules/` read fallback | Layer-2 rules |
| `rules_history` | History beside rules | Append-only |
| `work_outline` | `Outlines/outline.md` | Work-level / essay |
| `volume_outline` | `Outlines/volume-outline.md` or `volume-{n}-outline.md` | Multi-volume |
| `chapter_outline` | `Outlines/chapters/chNN-outline.md` or `vVV-chNN-outline.md` | HTTP-writable metadata surface |
| `foreshadowing_index` | `Outlines/foreshadowing.md` | Derived and human-visible |
| `event_index` | `Outlines/event-index.md` | Derived index |
| `chapter_body` | `Stories/chNN-<slug>.md` or `vVV-chNN-<slug>.md` | HTTP-read-only; agent-written |
| `essay_draft` | `Drafts/draft.md` | Essay |
| `script_document` | `Scripts/script.md` | Script |
| `script_beats` | `Beats/beat-sheet.md` | Script |
| `script_characters` | `Characters/characters.md` | Script |
| `game_bible_section` | `Design/<section>.md` | Game bible |
| `review_report` | `Logs/review/review-report.md` | Parser remains pure |
| `quality_log` | `Logs/kb/missing/…` | Advisory append semantics |
| `chronology_document` | Outline/chronology files | Derived narrative |
| `log_stub_dir` | `Logs/{brainstorm,write,review,publish}/` | Tree entry |
| `completion_lock_file` | `.completion-lock.json` | Derived only |
| `tree_directory` | Scaffold directories | Directory artifact |

A new profile kind MUST NOT smuggle strategy, memory, or knowledge-store records into the IR.

#### 4.3 Chapter state and payload separation

For novel Works, chapter state carries chapter, volume (default 1), slug, status (`not_started`, `outlined`, `draft`, `finalized`, with `published` reserved), optional planned/actual word counts, outline/body paths, and content hashes. Essay, script, and game-bible section/status metadata lives on the corresponding artifact records.

`patch_chapter` remains a metadata mutation on IR: revision bumps while payload hash stays unchanged. It uses the port and `WorkLock`, never a parallel DB-only authority.

Payload is file content or a directory marker. Metadata is parsed frontmatter and chapter state. Reconcile compares disk frontmatter and layout against IR and refreshes the DB index; the port exposes the diff rather than requiring callers to walk `Stories/`.

#### 4.4 Profile layouts

**Novel:**

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

**Essay:** `README.md`, `Outlines/outline.md`, `Drafts/draft.md`, `Logs/{write,review}/`; it does not use `Stories/` without a separate profile decision.

**Script:** `README.md`, `Scripts/script.md`, `Beats/beat-sheet.md`, `Characters/characters.md`, `Logs/{write,review}/`.

**Game bible:** `README.md`, `Design/{overview,pillars,characters,factions,species,locations,mechanics,magic_system,technology,economy,progression,lore}.md`, `Logs/{design,review}/`.

#### 4.5 Path and frontmatter rules

| Rule | Form |
| --- | --- |
| Single-volume chapter body | `Works/<work_ref>/Stories/chNN-<slug>.md` |
| Single-volume chapter outline | `Works/<work_ref>/Outlines/chapters/chNN-outline.md` |
| Multi-volume chapter body | `Works/<work_ref>/Stories/vVV-chNN-<slug>.md` |
| Multi-volume chapter outline | `Works/<work_ref>/Outlines/chapters/vVV-chNN-outline.md` |
| Volume outline | `Outlines/volume-outline.md` or `Outlines/volume-{n}-outline.md` |
| Novel sync discovery | `Works/<work_ref>/Stories/*.md` only |

Path construction lives in the IR/port. A default `works_root` means workspace-relative `Works`. A supplied value is accepted only when it equals `<canonical_workspace_root>/Works` by lexical join against the canonical root; empty, external, parent-relative, nested, and all other values fail with `InputInvalid` before Work claim, mkdir, or byte mutation. Commit separately requires `workspace_root` to canonicalize to that same root.

Minimum novel chapter frontmatter is `title`, `chapter`, optional `volume`, `status`, optional `word_count`, and optional `world_refs`. Profile documents retain outline heading and non-novel section frontmatter behavior consistent with these artifact contracts.

Reconcile refreshes the DB index from IR plus the filesystem projection. `port.reconcile()` is the single admission/write seam; no parallel JSON snapshot tree is introduced. The Markdown tree remains the on-disk projection and `work_chapters` remains the query index, not a competing prose authority.

### 5. Read/write adapters

#### 5.1 Port operations

The shipped `WorkArtifactPort` declares exactly two operations:

| Op | Behavior |
| --- | --- |
| `commit(WorkArtifactWrite)` | Commit one artifact's bytes under the per-Work runtime lock. The write carries `workspace_root`, `work_ref`, `rel_child`, and `content`. |
| `reconcile(WorkReconcile, recheck)` | Compute and optionally apply the `work_chapters` ↔ Markdown-tree diff. `WorkReconcile.dry_run = true` returns the read-only diff without acquiring the lock or writing; otherwise the apply phase runs under the lock. |

There is no separate intent list in the shipped interface. `commit` does not automatically run reconcile, and neither operation accepts an expected content hash.

#### 5.2 Production and recording adapters

- **Production byte path:** `fsync_write_atomic` semantics—temp + fsync + rename + final fsync + parent-dir fsync—across supported platforms. The port owns boundary re-checks, parent mkdir, and `WorkLock`.
- **Structured commit:** preset `workspace.commit` change lists MAY retain `WorkspaceExecutor` / `commit_fs`; this is not the sole manuscript backend while non-Unix refusal and fixed caps remain.
- **Metadata index:** `work_chapters` and selected `works` pointer columns mirror IR.
- **Recording adapter:** `RecordingWorkArtifactPort` records intended writes and content digests without filesystem, SQLite, or lock operations, but it is test-only/dead-code outside tests until a Simulate execution plane selects it. It is not a shipped production dry-run path.

`execute_write_file`, outline services, scaffold builtins, review ingest, quality-loop logs, narrative index, rules layers, auto-chronology, and chapter transitions depend on the port rather than concrete filesystem helpers.

If Work structured bundles use `commit_fs`, file/change/total caps are reviewed and parameterized by kind/plan rather than silently retaining manuscript-incompatible defaults. Streaming and hash-while-stage avoid holding whole files twice. Unix, macOS, and Windows behavior is explicit.

Parent-directory creation is a first-class guarded operation with failure cleanup. `tree_directory` and scaffold mkdir preserve first-write behavior; a thin `commit_fs` forwarder against missing parents is not valid.

For `fs/write_text_file` specifically:

1. Tool id and outside-workspace `Forbidden` behavior remain stable.
2. Work paths use `WorkArtifactPort::commit`; other workspace paths use the existing guarded fallback.
3. The JSON result remains `{ "written": true }` absent a versioned tool change.
4. Work-path commits acquire `WorkLock` or write under the existing schedule holder.

### 6. Commit and reconcile lifecycle

The shipped port covers byte commit and reconcile admission, not the full Load → Validate → Dry-run → Commit surface:

```text
Work byte write → boundary/identity/lock checks → durable commit
Reconcile request → unlocked diff → dry-run report OR locked apply
```

A byte commit validates `rel_child`, resolves the active creator/workspace and Work, rechecks the confined target, creates parents, acquires or inherits the Work lock, and writes durable bytes. It does not apply Workflow IR or automatically reconcile the chapter index.

Reconcile computes its diff without the lock. A request with `dry_run = true` stops at the report and writes nothing. A mutating reconcile rechecks caller authorization, acquires the runtime lock, writes any frontmatter-status resync through the same byte path, applies the DB index operations, and releases the lock on success or failure.

Derived completion-lock files remain projections of DB run state. The wider load/read/validate/plan/append operations and production Simulate stub selection are open items in `Consequences`, not stages available to callers today.

### 6.1 Direct-write guard (P5)

P5 makes the Work port unconditional and default-on: no runtime flag, environment variable, or configuration selects a different Work byte writer. `tooling/check-work-artifact-writes.sh` statically scans the inventoried production Work-writer modules and fails new direct `std::fs::write`, `tokio::fs::write`, or imported `fs::write` calls outside exact path+symbol exemptions. CI proves the guard fails on synthetic direct-write and mixed port-plus-direct-write fixtures before requiring the repository scan to pass.

The allowlist contains exactly three synchronous rollback restores:

| Exact path | Exact symbol | Disposition |
| --- | --- | --- |
| `crates/nexus-orchestration/src/capability/builtins/essay_scaffold.rs` | `ScaffoldTransaction::drop` | Restore only pre-existing bytes during rollback; never create content |
| `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs` | `ScaffoldTransaction::drop` | Restore only pre-existing bytes during rollback; never create content |
| `crates/nexus-orchestration/src/capability/builtins/script_scaffold.rs` | `ScaffoldTransaction::drop` | Restore only pre-existing bytes during rollback; never create content |

Each exemption is keyed by the same exact path and symbol in `tooling/work-artifact-write-allowlist.tsv` and leaves when scaffold rollback gains an async/port-based restore path. The removed `rules_history.rs` writer and migrated `work_chapters.rs` write have no exemptions. P5 does not claim that P3 production Simulate stub selection has shipped.

## Alternatives considered

- **Merge Work Artifact IR into Workflow IR.** Rejected: graph authoring and storage/commit authority would mix, chapter prose could become strategy nodes, and the two independently scoped designs could no longer land separately.
- **Keep direct per-caller filesystem helpers.** Rejected: durability, guards, locking, layout, and dry-run isolation would continue to drift.
- **Use `commit_fs` unchanged for every Work write.** Rejected: fixed caps, non-Unix refusal, and missing-parent behavior would regress manuscript writes.
- **Add a Windows `commit_fs` backend as the primary byte path (Option B).** Not selected: `ReplaceFileW` / `MoveFileExW` and reparse-point-safe opens require a larger platform matrix than the port-owned `fsync_write_atomic` route (Option A).
- **Introduce a third ad-hoc production writer.** Rejected: Work bytes use the port over `fsync_write_atomic`; structured preset bundles may retain `WorkspaceExecutor`.
- **Force all workspace writes through Work Artifact IR.** Rejected: non-Work paths keep the guarded workspace fallback.
- **Make the DB or `.completion-lock.json` authoritative over IR/run state.** Rejected: `work_chapters` is an index/cache and completion lock remains derived.
- **Make HTTP chapter bodies writable or merge Memory/KnowledgeStore/SessionStorage into this port.** Rejected as separate product and runtime responsibilities.
- **A second orchestration runtime or browser manuscript co-writer.** Out of scope.

## Consequences

The shipped port provides one guarded, durable, lock-aware seam for Work byte output while preserving non-Work fallback writes, read-only HTTP bodies, and existing layouts. Reconcile converges DB index pointers with the Markdown tree and supports its own read-only `dry_run` diff. These facts do not imply a general production dry-run for Work writes.

Git history and the shipped source establish the phase boundary: P0/P1 introduced the port and hot-path routing; P2 routed the domain Work writers through it; P3 added `RecordingWorkArtifactPort` but not its production selection; P4 admitted reconcile through the port; P5 made the port default-on and added the direct-write guard.

The resulting standing behavior is:

1. Work-path `draft_chapter` / `fs/write_text_file` writes use `WorkArtifactPort::commit`.
2. The P2 domain Work writers share the port.
3. Reconcile converges the `work_chapters` index with the tree; its `dry_run` request returns a diff without writing.
4. Novel sync still discovers canonical `Stories/*.md`.
5. The Work port has no Workflow IR compile/runtime dependency.
6. First-write parent creation matches prior tool behavior.
7. The P5 guard rejects new direct writes outside the three exact rollback exemptions.

The open costs and design sensitivities are explicit:

1. **Open — intended port surface:** `load`, `read`, `validate`, `plan`, and `append` are not shipped operations. `WorkArtifactWrite` has no `expected_hash`, and the port has no CAS/hash-precondition contract or typed hash-conflict result. The intents behind those operations, including append-only history semantics and conflict reporting, remain unsatisfied rather than being attributed to `commit` or `reconcile`.
2. **Open — P3 production dry-run isolation:** `RecordingWorkArtifactPort` is unused outside tests and is not selected by a production Simulate execution plane. The reopen condition is plan `v235-work-ir-p3`: the first Simulate execution plane must select the stub at the per-dispatch construction point so one selection covers every Work write. Until then, no production dry-run isolation property is claimed; reconcile's read-only `dry_run` diff is a separate, shipped feature.
3. Canonical snapshot encoding and revision semantics remain design sensitivities; no port-level revision precondition is claimed.
4. Structured Work bundles retain `WorkspaceExecutor` only where that seam fits; preset `workspace.commit` remains the clear case.
5. Layout freezing versus profile-specific path maps must preserve sync and preset assumptions.
6. Review-report parsing stays pure; the port owns I/O.
7. Multi-volume paths use validated builders or patterns without duplicating path rules.
8. Scaffold rollback and mixed FS+DB transaction ordering must remain atomic around IR revision changes; the three exact rollback exemptions are the bounded residue.
9. `WorkLock` holder-kind conventions distinguish tool, HTTP, and CLI acquisitions without weakening serialization.

References for the implementation anchors and sibling contracts are [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md), the profile/specification documents linked above, and the Work Artifact IR implementation under `crates/nexus-core/src/execution/`, `crates/nexus-orchestration/src/`, and `crates/nexus-local-db/src/work_chapters.rs`.
