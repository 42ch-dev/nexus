# Novel Quality Loop — Normative Specification v1

**Status**: Shipped (V1.47); V1.48 extensions folded (§9)  
**Document class**: Feature line (quality-loop supplement)  
**Created**: 2026-06-09  
**Last updated**: 2026-06-15 (V1.47 P-last — spec promotion Draft → Shipped)  
**Scope**: Local-first quality loop for `work_profile: novel` — findings, review routing, rules, logs, 96h escalation, on-demand audit cross-refs  
**Coordinates with**:

- [novel-workflow-profile.md](novel-workflow-profile.md) — §5.5 roadmap promoted to implement contract here for V1.39
- [creator-workflow.md](creator-workflow.md) — FL-E `review` stage and auto-chain
- [orchestration-engine.md](orchestration-engine.md) — presets, daemon scheduled tasks
- [cli-spec.md](cli-spec.md) — status/banner surfaces
- [novel-manuscript-audit.md](novel-manuscript-audit.md) — DF-69 on-demand audit (V1.44 P0)
- [novel-author-experience.md](novel-author-experience.md) — quickstart §5 cross-refs (V1.43 shipped)

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
| `status` | TEXT | `open`, `resolved`, `wont_fix` |
| `target_executor` | TEXT | `write`, `brainstorm`, `none`, `master` |
| `body` | TEXT | Human-readable finding |
| `rule_suggestion` | TEXT NULL | **V1.47** — optional prose suggestion for Layer 2 rules; persisted on finding row only (no `AGENTS.md` write in P0) |
| `created_at` / `updated_at` | INTEGER | Unix epoch |

Indexes: `(work_id, status)`, `(work_id, chapter, status)`.

### 2.2 Executor routing

| `target_executor` | Preset / action |
| --- | --- |
| `write` | Re-run or continue `novel-writing` (`produce`) |
| `brainstorm` | `novel-brainstorm` |
| `none` | User resolves manually |
| `master` | `novel-review-master` |

Auto-chain must not fork driver when routing spawns auxiliary schedules; at most one active FL-E driver per Work remains invariant.

---

## 3. Presets

| Preset ID | Role |
| --- | --- |
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer); **V1.47 P0** shipped. Named `novel-chapter-review` (replaces the former generic `reflection-loop` demo). See §8 for output contract + [novel-workflow-profile.md §5.5.6](novel-workflow-profile.md#556-novel-chapter-review-feeding-findings-v147-normative) for preset gates. |
| `novel-brainstorm` | Ideation from open findings (V1.39 P2) |
| `novel-review-master` | Master decision surface (V1.39 P2) |
| `novel-manuscript-audit` | On-demand chapter audit — review and/or extract (V1.44 P0; see [novel-manuscript-audit.md](novel-manuscript-audit.md)) |

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

See [novel-workflow-profile.md §5.5.4](novel-workflow-profile.md#554-two-layer-rules-architecture-v147) — V1.47 normative: Layer 2 = `Works/<work_ref>/AGENTS.md` (runtime reader migration deferred).

---

## 5. Logs structure (DF-66)

See [novel-workflow-profile.md §5.5.5](novel-workflow-profile.md#555-logs-structure-and-write-discipline) — V1.39 creates subdirs and write discipline.

---

## 6. Master-decision timeout (DF-67)

1. Daemon task every 24h queries open findings with `created_at < now - 96h`.
2. `creator works status` banner lists stale count + `novel-review-master` hint.
3. Automatic `novel-review-master` schedule: **opt-in only** (Work setting or CLI flag).

---

## 7. Acceptance (spec-level)

1. Findings CRUD isolated per creator.
2. Review stage in auto-chain can create findings without canceling driver.
3. Rules and Logs paths match novel-workflow-profile layout.
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

*Shipped V1.47. Normative text reconciled to [novel-workflow-profile.md §5.5](novel-workflow-profile.md).*

---

## 9. V1.48 Shipped — Findings maturity (folded from novel-findings-maturity.md overlay)

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

**Superseded by**: [creator-run-preset-entry.md](creator-run-preset-entry.md) (Shipped Master V1.45). The `novel-review-master` preset id + enqueue-only semantics + audit preset ids are now part of the canonical Master body.

