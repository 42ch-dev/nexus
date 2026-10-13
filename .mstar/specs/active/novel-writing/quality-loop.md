# Novel Quality Loop — Normative Specification v1

**Status**: Normative — novel quality-loop SSOT: findings lifecycle (6-state), review presets, two-layer rules runtime, logs, KB extraction + closure, 96h escalation  
**Document class**: Feature line (quality-loop supplement)  
**Created**: 2026-06-09  
**Last updated**: 2026-10-13  
**Scope**: Local-first quality loop for `work_profile: novel` — findings, review routing, rules, logs, 96h escalation, on-demand audit cross-refs  
**Coordinates with**:

- [workflow-profile.md](workflow-profile.md) — layout, preset gates, completion (quality-loop detail in sibling spec)
- [creator-workflow.md](../creator/creator-workflow.md) — FL-E `review` stage and auto-chain
- [orchestration-engine.md](../orchestration/orchestration-engine.md) — presets, scheduled tasks
- [cli-spec.md](../cli/cli-spec.md) — status/banner surfaces
- [manuscript-audit.md](manuscript-audit.md) — DF-69 on-demand audit
- [author-experience.md](author-experience.md) — quickstart §5 cross-refs

---

## 1. Purpose

The quality loop is a durable backplane over the inline `llm_judge` 五问 finalize gate ([workflow-profile.md §5.1](workflow-profile.md)): findings lifecycle, auxiliary review presets, layered rules, process logs, and 96h master-decision escalation — all local DB + CLI, no Redis or platform workers.

---

## 2. Findings lifecycle

### 2.1 Schema (normative minimum)

| Column | Type | Notes |
| --- | --- | --- |
| `finding_id` | TEXT PK | ULID |
| `work_id` | TEXT FK | |
| `chapter` | INTEGER NULL | Optional chapter binding |
| `kind` | TEXT | `continuity`, `craft`, `plot_hole`, `world_inconsistency`, … |
| `severity` | TEXT | `info`, `minor`, `major`, `blocker` |
| `status` | TEXT | 6-state — `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate` |
| `target_executor` | TEXT | `write`, `brainstorm`, `none`, `master` |
| `body` | TEXT | Human-readable finding |
| `rule_suggestion` | TEXT NULL | Optional prose suggestion for Layer 2 rules; persisted on finding row only |
| `created_at` / `updated_at` | INTEGER | Unix epoch |

Indexes: `(work_id, status)`, `(work_id, chapter, status)`, and composite `idx_findings_status_updated_at` on `(status, updated_at)`.

### 2.2 Executor routing

| `target_executor` | Preset / action |
| --- | --- |
| `write` | Re-run or continue `novel-writing` (`produce`) |
| `brainstorm` | `novel-brainstorm` |
| `none` | User resolves manually |
| `master` | `novel-review-master` |

Auto-chain must not fork driver when routing spawns auxiliary schedules; at most one active FL-E driver per Work remains invariant.

### 2.3 Extended status enum (F6 lifecycle)

> **Authority (§2.3–§2.4 and executor routing):** The root [Findings Lifecycle — Cross-Profile Master](../contracts/findings-lifecycle.md) owns the cross-profile six-state lifecycle, `target_executor` routing semantics, and UI remediation surface. The enum and transition graph below are the novel-side reference, not a second normative lifecycle authority. This spec retains only novel-specific producer-side responsibilities: review verdicts, finding creation, orchestration hooks, produce-prompt selection, and retention/prune.

The findings lifecycle is a 6-state model:

| Status | Meaning |
| --- | --- |
| `open` | New finding; not yet triaged |
| `triaged` | Reviewed; actionable for write/brainstorm routing |
| `in_review` | Under master review (`novel-review-master` active) |
| `resolved` | Addressed; eligible for retention prune |
| `wont_fix` | Explicitly waived; never pruned by retention DAO |
| `duplicate` | Superseded by another finding; terminal |

### 2.4 Allowed transitions

```text
open → triaged | in_review | resolved | wont_fix | duplicate
triaged → in_review | resolved | wont_fix | duplicate
in_review → resolved | wont_fix | duplicate
resolved → (terminal; may be pruned by retention policy)
wont_fix → (terminal)
duplicate → (terminal)
```

Invalid transitions return `422` with stable error code on Daemon API (see §2.7 error classification).

### 2.5 Actionable set for prompt consumer

Findings with status ∈ `{ open, triaged }` are included in `open_findings_block`. Status `in_review` is **excluded** from produce prompts unless a future spec amends.

### 2.6 Migration

Existing rows remain valid. No automatic status rewrite on migration. Default for new rows: `open`.

### 2.7 API error classification

| Error | Trigger | HTTP |
| --- | --- | --- |
| `INVALID_TRANSITION` | Status change violates allowed transition graph (§2.4) | 422 |
| `INVALID_INPUT` | Unknown status value, missing fields, or malformed payload | 400 |

---

## 3. Presets

| Preset ID | Role |
| --- | --- |
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer). See §8 for output contract + [workflow-profile.md §5.3.3](workflow-profile.md#533-novel-chapter-review-gates-review-quality-pass) for preset gates. |
| `novel-brainstorm` | Ideation from open findings |
| `novel-review-master` | Master decision surface |
| `novel-manuscript-audit` | On-demand chapter audit — review and/or extract (see [manuscript-audit.md](manuscript-audit.md)) |

### 3.4 Review-master admission surface

There is no CLI preset-dispatch entrance (the retired dedicated `review-master` subcommand and the generic `creator run <preset_id>` entry are frozen in [creator-run-preset-entry.md](../../archived/creator/2026-09-21-creator-run-preset-entry.md)). Preset admission goes through the core execution surface (schedule add via `POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`); the retained CLI scheduling surface is `creator works cron` declaration editing. Findings listing lives on `creator works status`.

Normative semantics (per the archived Master body):

| Behavior | Requirement |
| --- | --- |
| Finding-scoped run | Runs or enqueues `novel-review-master` scoped to one finding |
| Auto-schedule | Opt-in: enqueue `novel-review-master` when 96h stale findings exist (mirrors DF-67 Work setting) |
| Driver interaction | Must not fork or cancel active FL-E auto-chain driver |

**Presentation** (minimum):

- Use `creator works status` to list open findings with severity breakdown
- On empty findings: `creator works status [<work_id>]` surfaces a clear "no findings yet" message (rendered as `none open`); master-decision review admission is via the execution surface (schedule add), not a CLI entry

## 4. Rules architecture (DF-65)

Two-layer model (a history layer is not part of the model). The runtime prefers `Works/<work_ref>/AGENTS.md` for Layer 2; legacy `Rules/novel-rules.md` read-only when absent.

| Layer | Location | Mutability | Purpose |
| --- | --- | --- | --- |
| Layer 1 — Shared writing craft rules | User override: `~/.nexus42/rules/writing-craft.md`; in-repo default: `crates/nexus-orchestration/embedded-rules/writing-craft.md` | Immutable per version; user override may pin/replace | Cross-Work craft guidance for all `novel-writing` runs (五问 gate rationale) |
| Layer 2 — Per-work rules (agent-facing) | `Works/<work_ref>/AGENTS.md` | User-editable; `creator works rules reset` restores scaffold | Per-Work style constraints for agents/presets (POV, tense, tone) |

**Deprecated paths**: `Works/<work_ref>/Rules/novel-rules.md`, `novel-rules-history.md` — no migration; new scaffolds use `AGENTS.md`.

Rules are distinct from **SOUL** (creator voice) and **World KB** (fictional facts per [workflow-profile.md](workflow-profile.md) §3.5).

---

## 5. Logs structure (DF-66)

Quality-loop work uses subdirectories under `Works/<work_ref>/Logs/`:

```text
Works/<work_ref>/Logs/
  brainstorm/    # brainstorm session outputs
  write/         # drafting process logs
  review/        # review outputs, findings notes, review-report.md (producer)
  kb/            # KB candidate audit trails (pending/rejected/missing)
  publish/       # reserved until platform publish (DF-59)
```

Write discipline:

1. Logs are process evidence — not canonical正文, World KB, or SOUL.
2. Promotion to findings, rules, or KB must be explicit.
3. `Logs/**` is **not** scanned by the chapter sync module ([sync-contract.md](sync-contract.md); [workflow-profile.md](workflow-profile.md) §7).

### 5.5 Missing-KB detection at finalize

When a `novel-writing` schedule completes and the active chapter transitions to `finalized`, the orchestration supervisor runs an advisory missing-KB scan over the finalized chapter prose. The scan reuses the same `nexus.llm.extract` pathway as review-time extraction (T-A P0), falling back to the capitalized-noun heuristic when no worker is available.

1. **Trigger**: `ScheduleSupervisor::on_schedule_terminal` for a completed `novel-writing` schedule.
2. **Input**: the finalized chapter body from `Works/<work_ref>/Stories/` (the chapter indicated by `works.current_chapter` after finalization).
3. **Diff**: extracted canonical names are compared against existing `confirmed` `KnowledgeEntry` rows in the Work's World (`kb_key_blocks.canonical_name`). Candidates whose name is absent are classified as `missing`.
4. **Output**: an advisory log file written to
   `Works/<work_ref>/Logs/kb/missing/<YYYY-MM-DD>-ch<chapter>.md`.
   The file uses YAML frontmatter (`generated_at`, `world_id`, `work_id`, `work_ref`, `chapter`, `candidate_count`, `candidates`) and a human-readable Markdown body.
5. **Scope**: single chapter, single Work. Missing candidates are **not** written to `kb_extract_jobs`; they are surfaced only through the log file and the `creator world kb pending --missing-only` CLI view.
6. **Idempotency**: re-running finalize on the same day overwrites the same log file so repeated transitions do not accumulate duplicate entries.
7. **Best-effort**: errors are logged at `warn!` and do **not** fail the schedule terminal transition.

### 5.6 Auto-promote high-confidence KB candidates (Normative)

The CLI command `creator world kb adopt --auto <world_ref>` promotes pending `kb_extract_jobs` rows to confirmed KnowledgeEntries without per-row author confirmation, provided every safety predicate holds:

| Predicate | Reason |
| --- | --- |
| `llm_confidence >= 0.95` | High LLM self-reported confidence only; heuristic rows (`llm_confidence IS NULL`) are skipped. |
| Non-empty `llm_source_quote` | Provenance-backed: the candidate carries a verbatim chapter excerpt. |
| `source_chapter_id IS NOT NULL` | Provenance-backed: the candidate is tied to a specific chapter. |
| `ValidationMode::Novel` passes | The generated KnowledgeEntry body has a valid `novel_category` and canonical name. |
| No duplicate `canonical_name` in the world | The world's active KnowledgeEntries do not already contain the same name. |

Process:

1. List `promotion_status='pending'` candidates for the world ordered by creation time.
2. For each candidate, evaluate the predicates inside a dedicated transaction.
3. On success, insert a confirmed KnowledgeEntry (`status='confirmed'`), flip the `kb_extract_jobs` row to `promotion_status='confirmed'`, and set `auto_promoted_at`, `auto_promoted_reason`, and `auto_promoted_by`.
4. On any predicate failure, skip the candidate with a reason recorded in `--json` output; the row remains `pending`.
5. After all candidates, write a best-effort audit log per promoted row under `Works/<work_ref>/Logs/kb/auto-promoted/<YYYY-MM-DD>-<extract_job_id>.md`.

Each candidate uses its own transaction with a CAS version guard (`kb_extract_jobs.version`) so one failure does not roll back unrelated promotions and stale rows are not flipped.

---

## 6. Master-decision timeout (DF-67)

1. Daemon task every 24h queries open findings with `created_at < now - 96h`.
2. `creator works status` banner lists stale count + `novel-review-master` hint.
3. Automatic `novel-review-master` schedule: **opt-in only** (Work setting or CLI flag).

---

## 7. Acceptance (spec-level)

1. Findings CRUD isolated per creator.
2. Review stage in auto-chain can create findings without canceling driver.
3. Rules and Logs paths match [workflow-profile.md](workflow-profile.md) layout.
4. No Redis, external cron, or platform dependency.

---

## 8. Reflection-loop output contract

The canonical preset id is `novel-chapter-review`.

**Scope**: Normative behavior for the FL-E `review` preset after P0 implement. Applies to **both** auto-chain review stage and on-demand preset admission.

### 8.1 Inputs (minimum)

| Input | Source | Required |
| --- | --- | --- |
| `work_id` | Schedule / CLI | Yes |
| `chapter` | `work_chapters` selection for review pass | Yes when multi-chapter |
| `body_path` / `outline_path` | Chapter artifacts | Best-effort |
| Rules context | Layer 1 embedded + Layer 2 (`Works/<work_ref>/AGENTS.md`; shipped runtime reads `Rules/novel-rules.md` until follow-up migration) | Best-effort |

### 8.2 Finding creation

1. On terminal success of review preset, call existing `create_finding_from_review` (or supervisor-equivalent) **≥1** time per review pass.
2. Each finding MUST set: `work_id`, `chapter` (when known), `kind`, `severity`, `status=open`, `target_executor`, `body`.
3. Optional `rule_suggestion` text MAY be stored on the finding row; accepting a suggestion appends to `Works/<work_ref>/AGENTS.md` via the §9.3 accept path.

### 8.3 Idempotency

Re-running review on the same chapter SHOULD avoid duplicate open findings with identical `body` hash within a 24h window (implementer may use content hash or finding kind+chapter dedupe — lock in P0 plan).

### 8.4 Auto-chain invariant

Finding creation MUST NOT fork or cancel the active FL-E auto-chain driver schedule.

---

*Quality-loop normative SSOT for this domain; profile gates in [workflow-profile.md §5.3.3](workflow-profile.md#533-novel-chapter-review-gates-review-quality-pass).*

---

## 9. Findings maturity

Findings maturity closes the novel quality loop: durable findings enrich the writing prompt, accepted suggestions mutate the runtime Layer 2, and a retention policy prevents unbounded growth.

### 9.1 Producer

- `novel-chapter-review` preset id hoisted to `crates/nexus-orchestration/src/preset_ids.rs::NOVEL_CHAPTER_REVIEW_PRESET_ID` (SSOT).
- `Works/<work_ref>/Logs/review/review-report.md` is parsed to populate `kind`/`severity`/`body`/optional `rule_suggestion` per finding row (per §2.1 vocabulary).
- Report file read is bounded (256 KiB cap); on miss/parse failure the placeholder synthesis is used and a `tracing::warn!` is emitted with `work_id`/`chapter`/`schedule_id`/`size_bytes`/`cap_bytes`.
- Findings are persisted in a single SQLite transaction (idempotent retries).
- R-V147P0-01 (review-report parsing), R-V147P0-05 (RVM schedule_id PK collision hotfix), R-V147P0-06 (preset-id SSOT) — **closed**.

### 9.2 Consumer

- Open findings for the active `work_id` + chapter (or work-level with `chapter IS NULL`) are summarized into a `open_findings_block` and injected into `novel-writing` outline + draft prompts via the `{{open_findings_block}}` template variable.
- Cap: 8 findings max, 400 chars/body, 3200 chars total block.
- Empty input → no block (no sentinel noise; `{{#if open_findings_block}}` guard in templates).

### 9.3 Rules runtime

- Runtime `read_rules_layers` prefers `Works/<work_ref>/AGENTS.md` (Layer 2 normative); falls back to legacy `Rules/novel-rules.md` read-only when `AGENTS.md` absent.
- New scaffolds write `AGENTS.md` (not `Rules/novel-rules.md`).
- Accept path: `creator works findings accept <finding_id>` appends a structured entry to `AGENTS.md` (idempotent on `finding_id` marker, atomic temp+rename, timestamped).
- Reset path: `creator works rules reset [<work_id>]` restores the default scaffold (supports `--dry-run` for preview, `--yes`/`-y` to skip prompt; default prompts via `dialoguer`).
- R-V147P0-04 (AGENTS.md runtime + accept + reset) — **closed**.

> **Work-scoped vs world-scoped rules runtimes (DR-64):** §9.3 is the work-scoped rules runtime (`read_rules_layers`); it is distinct from the check callback — `POST /v1/daemon/check` runs the frozen mental checker pair (`stale_belief_drift` / `dramatic_irony_asymmetry`), plus the deterministic rule evaluator over world-scoped `spoke_rules` (four constraint families; empty `rule_refs` auto-includes `status=active`; foreign-world ids fail closed). Iteration locks: `v1.166-quality-loop-locks.md`.

### 9.4 Data hygiene

- Retention: `prune_resolved_findings_older_than(pool, now_epoch, retention_seconds)` DAO removes `resolved` rows whose `updated_at` is older than `RETENTION_DEFAULT_DAYS` (default 90 days). Skips `open` and `wont_fix` rows. CLI command (e.g. `creator works findings prune`) is a future wiring item; the DAO is the seam.
- `FindingPatch.rule_suggestion` is tri-state `Option<Option<String>>`: `Some(Some(value))` sets, `Some(None)` clears to NULL, `None` leaves unchanged. Wire PATCH uses `deserialize_some` helper to accept explicit null.
- New composite index `idx_findings_status_updated_at` on `(status, updated_at)`.
- R-V147P0-02 (retention policy) and R-V147P0-03 (NULL clear) — **closed**.

### 9.5 Acceptance

- All P0–P3 acceptance criteria pass hermetically.
- All R-V147P0-* targets (6) closed (R-V147P0-01, 02, 03, 04, 05, 06).
- R-V147P1-01 (intake re-trigger on existing Work) — **deferred**.

---

