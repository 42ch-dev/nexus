# Novel Quality Loop — Normative Specification v1

**Status**: Normative — V1.51 Shipped (findings lifecycle F6 + KB closure overwrites/supersedes integrated)  
**Document class**: Feature line (quality-loop supplement)  
**Created**: 2026-06-09  
**Last updated**: 2026-06-19 (V1.51 P-last — findings lifecycle F6 marked Normative; no runtime change)  
**Scope**: Local-first quality loop for `work_profile: novel` — findings, review routing, rules, logs, 96h escalation, on-demand audit cross-refs  
**Coordinates with**:

- [workflow-profile.md](workflow-profile.md) — layout, preset gates, completion (quality-loop detail in sibling spec)
- [creator-workflow.md](../creator-workflow.md) — FL-E `review` stage and auto-chain
- [orchestration-engine.md](../orchestration-engine.md) — presets, daemon scheduled tasks
- [cli-spec.md](../cli-spec.md) — status/banner surfaces
- [manuscript-audit.md](manuscript-audit.md) — DF-69 on-demand audit (V1.44 P0)
- [author-experience.md](author-experience.md) — quickstart §5 cross-refs (V1.43 shipped)

**Iteration compass**: [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) · [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](../../iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md) · [v1.47-novel-quality-loop-closure-delivery-compass-v1.md](../../iterations/v1.47-novel-quality-loop-closure-delivery-compass-v1.md) (active)

---

## 1. Purpose

V1.36 shipped inline `llm_judge` 五问 on finalize. V1.39 implements a durable quality-loop backplane: findings lifecycle, auxiliary review presets, three-layer rules, process logs, and 96h master-decision escalation — all local DB + daemon + CLI, no Redis or platform workers.

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
| `status` | TEXT | **V1.49 P0**: 6-state — `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate` (supersedes V1.39 three-state `open` / `resolved` / `wont_fix`) |
| `target_executor` | TEXT | `write`, `brainstorm`, `none`, `master` |
| `body` | TEXT | Human-readable finding |
| `rule_suggestion` | TEXT NULL | **V1.47** — optional prose suggestion for Layer 2 rules; persisted on finding row only (no `AGENTS.md` write in P0) |
| `created_at` / `updated_at` | INTEGER | Unix epoch |

Indexes: `(work_id, status)`, `(work_id, chapter, status)`. **V1.49 P0**: added composite index `idx_findings_status_updated_at` on `(status, updated_at)`.

### 2.2 Executor routing

| `target_executor` | Preset / action |
| --- | --- |
| `write` | Re-run or continue `novel-writing` (`produce`) |
| `brainstorm` | `novel-brainstorm` |
| `none` | User resolves manually |
| `master` | `novel-review-master` |

Auto-chain must not fork driver when routing spawns auxiliary schedules; at most one active FL-E driver per Work remains invariant.

### 2.3 Extended status enum (V1.49 P0 — F6 lifecycle)

V1.49 P0 extends the V1.39 three-state model (`open` / `resolved` / `wont_fix`) to a 6-state lifecycle:

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

Invalid transitions return `422` with stable error code on Local API (see §2.7 error classification).

### 2.5 Actionable set for prompt consumer

Findings with status ∈ `{ open, triaged }` are included in `open_findings_block` (V1.48 naming preserved). Status `in_review` is **excluded** from produce prompts unless a future spec amends.

### 2.6 Migration

Existing rows remain valid. No automatic status rewrite on migration. Default for new rows: `open`.

### 2.7 API error classification (V1.49 P0)

| Error | Trigger | HTTP |
| --- | --- | --- |
| `INVALID_TRANSITION` | Status change violates allowed transition graph (§2.4) | 422 |
| `INVALID_INPUT` | Unknown status value, missing fields, or malformed payload | 400 |

---

## 3. Presets

| Preset ID | Role |
| --- | --- |
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer); **V1.47 P0** shipped. Named `novel-chapter-review` (replaces the former generic `reflection-loop` demo). See §8 for output contract + [workflow-profile.md §5.3.3](workflow-profile.md#533-novel-chapter-review-gates-review-quality-pass) for preset gates. |
| `novel-brainstorm` | Ideation from open findings (V1.39 P2) |
| `novel-review-master` | Master decision surface (V1.39 P2) |
| `novel-manuscript-audit` | On-demand chapter audit — review and/or extract (V1.44 P0; see [manuscript-audit.md](manuscript-audit.md)) |

### 3.4 Review-master CLI surface (V1.45 P0–P2 — generic preset dispatch)

V1.44 shipped a dedicated `review-master` subcommand. V1.45 replaces it with the generic `creator run <preset_id>` entry — `creator run novel-review-master` is the preset-id form. Findings listing moves to `creator works status` (P4 enhancement).

**Normative CLI** (Shipped V1.45):

```bash
nexus42 creator run novel-review-master [<work_id>] [--finding-id <id>] [--auto-schedule]
```

| Behavior | Requirement |
| --- | --- |
| `--finding-id` | Runs or enqueues `novel-review-master` preset scoped to one finding |
| `--auto-schedule` | Opt-in: enqueue `novel-review-master` when 96h stale findings exist (mirrors DF-67 Work setting) |
| Driver interaction | Must not fork or cancel active FL-E auto-chain driver |

**Presentation** (minimum):

- Use `creator works status` to list open findings with severity breakdown
- Quickstart §5 updated to cite `creator run novel-review-master` as primary path (V1.45 P3)
- On empty findings: `creator works status [<work_id>]` surfaces a clear "no findings yet" message and suggests `creator run novel-review-master` to enqueue a master-decision review

**Residual**: R-V143P0-002 — resolved V1.44 P1; close in P-last hygiene.

---

## 4. Rules architecture (DF-65)

**V1.47 Shipped (normative)** — two-layer model (Layer 3 history removed). **Shipped runtime** (through V1.48) prefers `Works/<work_ref>/AGENTS.md` for Layer 2; legacy `Rules/novel-rules.md` read-only when absent.

| Layer | Location | Mutability | Purpose |
| --- | --- | --- | --- |
| Layer 1 — Shared writing craft rules | User override: `~/.nexus42/rules/writing-craft.md`; in-repo default: `crates/nexus-orchestration/embedded-rules/writing-craft.md` | Immutable per version; user override may pin/replace | Cross-Work craft guidance for all `novel-writing` runs (五问 gate rationale) |
| Layer 2 — Per-work rules (agent-facing) | `Works/<work_ref>/AGENTS.md` | User-editable; `creator works rules reset` restores scaffold | Per-Work style constraints for agents/presets (POV, tense, tone) |

**Deprecated paths**: `Works/<work_ref>/Rules/novel-rules.md`, `novel-rules-history.md` — no migration; new scaffolds use `AGENTS.md`.

Rules are distinct from **SOUL** (creator voice) and **World KB** (fictional facts per [workflow-profile.md](workflow-profile.md) §3.5).

---

## 5. Logs structure (DF-66)

V1.39+ quality-loop work uses subdirectories under `Works/<work_ref>/Logs/`:

```text
Works/<work_ref>/Logs/
  brainstorm/    # brainstorm session outputs
  write/         # drafting process logs
  review/        # review outputs, findings notes, review-report.md (V1.48 producer)
  kb/            # V1.51: KB candidate audit trails (pending/rejected/missing)
  publish/       # reserved until platform publish (DF-59)
```

Write discipline:

1. Logs are process evidence — not canonical正文, World KB, or SOUL.
2. Promotion to findings, rules, or KB must be explicit.
3. `Logs/**` is **not** scanned by the chapter sync module ([sync-contract.md](sync-contract.md); [workflow-profile.md](workflow-profile.md) §7).

### 5.5 Missing-KB detection at finalize (V1.51 T-A P2)

When a `novel-writing` schedule completes and the active chapter transitions to `finalized`, the orchestration supervisor runs an advisory missing-KB scan over the finalized chapter prose. The scan reuses the same `nexus.llm.extract` pathway as review-time extraction (T-A P0), falling back to the capitalized-noun heuristic when no worker is available.

1. **Trigger**: `ScheduleSupervisor::on_schedule_terminal` for a completed `novel-writing` schedule.
2. **Input**: the finalized chapter body from `Works/<work_ref>/Stories/` (the chapter indicated by `works.current_chapter` after finalization).
3. **Diff**: extracted canonical names are compared against existing `confirmed` `KeyBlock` rows in the Work's World (`kb_key_blocks.canonical_name`). Candidates whose name is absent are classified as `missing`.
4. **Output**: an advisory log file written to
   `Works/<work_ref>/Logs/kb/missing/<YYYY-MM-DD>-ch<chapter>.md`.
   The file uses YAML frontmatter (`generated_at`, `world_id`, `work_id`, `work_ref`, `chapter`, `candidate_count`, `candidates`) and a human-readable Markdown body.
5. **Scope**: single chapter, single Work. Missing candidates are **not** written to `kb_extract_jobs`; they are surfaced only through the log file and the `creator world kb pending --missing-only` CLI view.
6. **Idempotency**: re-running finalize on the same day overwrites the same log file so repeated transitions do not accumulate duplicate entries.
7. **Best-effort**: errors are logged at `warn!` and do **not** fail the schedule terminal transition.

### 5.6 Auto-promote high-confidence KB candidates (Draft V1.52 overlay)

**Status**: Draft (V1.52 — body authored in plan `2026-06-19-v1.52-outline-five-q-and-auto-promote`)  
**Authoring plan**: `2026-06-19-v1.52-outline-five-q-and-auto-promote`  
**Promotes to Normative**: P-last of V1.52

The CLI command `creator world kb adopt --auto <world_ref>` promotes pending `kb_extract_jobs` rows to confirmed `KeyBlock`s without per-row author confirmation, provided every safety predicate holds:

| Predicate | Reason |
| --- | --- |
| `llm_confidence >= 0.95` | High LLM self-reported confidence only; heuristic rows (`llm_confidence IS NULL`) are skipped. |
| Non-empty `llm_source_quote` | Provenance-backed: the candidate carries a verbatim chapter excerpt. |
| `source_chapter_id IS NOT NULL` | Provenance-backed: the candidate is tied to a specific chapter. |
| `ValidationMode::Novel` passes | The generated `KeyBlock` body has a valid `novel_category` and canonical name. |
| No duplicate `canonical_name` in the world | The world's active KeyBlocks do not already contain the same name. |

Process:

1. List `promotion_status='pending'` candidates for the world ordered by creation time.
2. For each candidate, evaluate the predicates inside a dedicated transaction.
3. On success, insert a confirmed `KeyBlock` (`status='confirmed'`), flip the `kb_extract_jobs` row to `promotion_status='confirmed'`, and set `auto_promoted_at`, `auto_promoted_reason`, and `auto_promoted_by`.
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

## 8. Reflection-loop output contract (V1.47 Shipped)

Heading preserved for back-compat with existing inbound cross-refs; the canonical preset id is `novel-chapter-review` (renamed in V1.47 P0).

**Scope**: Normative behavior for the FL-E `review` preset after P0 implement. Applies to **both** auto-chain review stage and on-demand `creator run <preset_id>`.

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
3. Optional `rule_suggestion` text MAY be stored on the finding row; accepting a suggestion does **not** mutate `Works/<work_ref>/AGENTS.md` in V1.47.

### 8.3 Idempotency

Re-running review on the same chapter SHOULD avoid duplicate open findings with identical `body` hash within a 24h window (implementer may use content hash or finding kind+chapter dedupe — lock in P0 plan).

### 8.4 Auto-chain invariant

Finding creation MUST NOT fork or cancel the active FL-E auto-chain driver schedule.

---

*Shipped V1.47. Quality-loop normative SSOT for this domain; profile gates in [workflow-profile.md §5.3.3](workflow-profile.md#533-novel-chapter-review-gates-review-quality-pass).*

---

## 9. V1.48 Shipped — Findings maturity (folded from archived/knowledge/novel-findings-maturity.md overlay)

V1.48 closes the novel quality loop: durable findings enrich the writing prompt, accepted suggestions mutate the runtime Layer 2, and a retention policy prevents unbounded growth.

### 9.1 Producer (V1.48 P0)

- `novel-chapter-review` preset id hoisted to `crates/nexus-orchestration/src/preset_ids.rs::NOVEL_CHAPTER_REVIEW_PRESET_ID` (SSOT).
- `Works/<work_ref>/Logs/review/review-report.md` is parsed to populate `kind`/`severity`/`body`/optional `rule_suggestion` per finding row (per §2.1 vocabulary).
- Report file read is bounded (256 KiB cap); on miss/parse failure the placeholder synthesis is used and a `tracing::warn!` is emitted with `work_id`/`chapter`/`schedule_id`/`size_bytes`/`cap_bytes`.
- Findings are persisted in a single SQLite transaction (idempotent retries).
- R-V147P0-01 (review-report parsing), R-V147P0-05 (RVM schedule_id PK collision hotfix), R-V147P0-06 (preset-id SSOT) — **closed**.

### 9.2 Consumer (V1.48 P1)

- Open findings for the active `work_id` + chapter (or work-level with `chapter IS NULL`) are summarized into a `open_findings_block` and injected into `novel-writing` outline + draft prompts via the `{{open_findings_block}}` template variable.
- Cap: 8 findings max, 400 chars/body, 3200 chars total block (per overlay §2.2).
- Empty input → no block (no sentinel noise; `{{#if open_findings_block}}` guard in templates).

### 9.3 Rules runtime (V1.48 P2)

- Runtime `read_rules_layers` prefers `Works/<work_ref>/AGENTS.md` (Layer 2 per V1.47 normative); falls back to legacy `Rules/novel-rules.md` read-only when `AGENTS.md` absent.
- New scaffolds write `AGENTS.md` (not `Rules/novel-rules.md`).
- Accept path: `creator works findings accept <finding_id>` appends a structured entry to `AGENTS.md` (idempotent on `finding_id` marker, atomic temp+rename, timestamped).
- Reset path: `creator works rules reset [<work_id>]` restores the default scaffold (supports `--dry-run` for preview, `--yes`/`-y` to skip prompt; default prompts via `dialoguer`).
- R-V147P0-04 (AGENTS.md runtime + accept + reset) — **closed**.

### 9.4 Data hygiene (V1.48 P3)

- Retention: `prune_resolved_findings_older_than(pool, now_epoch, retention_seconds)` DAO removes `resolved` rows whose `updated_at` is older than `RETENTION_DEFAULT_DAYS` (default 90 days). Skips `open` and `wont_fix` rows. CLI command (e.g. `creator works findings prune`) is a future wiring item; the DAO is the seam.
- `FindingPatch.rule_suggestion` is tri-state `Option<Option<String>>`: `Some(Some(value))` sets, `Some(None)` clears to NULL, `None` leaves unchanged. Wire PATCH uses `deserialize_some` helper to accept explicit null.
- New composite index `idx_findings_status_updated_at` on `(status, updated_at)`.
- R-V147P0-02 (retention policy) and R-V147P0-03 (NULL clear) — **closed**.

### 9.5 Acceptance (V1.48)

- All P0–P3 acceptance criteria pass hermetically.
- All R-V147P0-* targets (6) closed in V1.48 (R-V147P0-01, 02, 03, 04, 05, 06).
- R-V147P1-01 (intake re-trigger on existing Work) — **deferred to V1.49** (per V1.48 P4 §8).

---

## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](../creator-run-preset-entry.md) (Shipped Master V1.45). The `novel-review-master` preset id + enqueue-only semantics + audit preset ids are now part of the canonical Master body.
