# Novel Quality Loop — Version History and Runner Retirement Historical Notes

**Provenance**: frozen from [`specs/active/novel-writing/quality-loop.md`](../../active/novel-writing/quality-loop.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 (2026-09-21) retired the `creator run` preset runner; the frozen text spans the V1.45/V1.47/V1.48/V1.49 lifecycle ledger and the removed runner CLI surface (the v1.166 lock pointer is kept live). Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (58 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Normative — V1.51 Shipped (findings lifecycle F6 + KB closure overwrites/supersedes integrated)  
```

### at pre-freeze line ~6

```markdown
**Last updated**: 2026-09-29 (authority routing aligned with the cross-profile findings Master; V1.49 lifecycle history retained)  
```

### at pre-freeze line ~14

```markdown
- [manuscript-audit.md](../../active/novel-writing/manuscript-audit.md) — DF-69 on-demand audit (V1.44 P0)
- [author-experience.md](../../active/novel-writing/author-experience.md) — quickstart §5 cross-refs (V1.43 shipped)
```

### at pre-freeze line ~21

```markdown
V1.36 shipped inline `llm_judge` 五问 on finalize. V1.39 implements a durable quality-loop backplane: findings lifecycle, auxiliary review presets, three-layer rules, process logs, and 96h master-decision escalation — all local DB + daemon + CLI, no Redis or platform workers.
```

### at pre-freeze line ~36

```markdown
| `status` | TEXT | **V1.49 P0**: 6-state — `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate` (supersedes V1.39 three-state `open` / `resolved` / `wont_fix`) |
```

### at pre-freeze line ~39

```markdown
| `rule_suggestion` | TEXT NULL | **V1.47** — optional prose suggestion for Layer 2 rules; persisted on finding row only (no `AGENTS.md` write in P0) |
```

### at pre-freeze line ~42

```markdown
Indexes: `(work_id, status)`, `(work_id, chapter, status)`. **V1.49 P0**: added composite index `idx_findings_status_updated_at` on `(status, updated_at)`.
```

### at pre-freeze line ~55

```markdown
### 2.3 Extended status enum (V1.49 P0 — F6 lifecycle)
```

### at pre-freeze line ~57

```markdown
> **Authority (§2.3–§2.4 and executor routing):** The root [Findings Lifecycle — Cross-Profile Master](../../active/contracts/findings-lifecycle.md) owns the cross-profile six-state lifecycle, `target_executor` routing semantics, and UI remediation surface. The V1.49 enum and transition graph below are retained as historical context, not a second normative lifecycle authority. This spec retains only novel-specific producer-side responsibilities: review verdicts, finding creation, orchestration hooks, produce-prompt selection, and retention/prune.
```

### at pre-freeze line ~59

```markdown
V1.49 P0 extends the V1.39 three-state model (`open` / `resolved` / `wont_fix`) to a 6-state lifecycle:
```

### at pre-freeze line ~85

```markdown
Findings with status ∈ `{ open, triaged }` are included in `open_findings_block` (V1.48 naming preserved). Status `in_review` is **excluded** from produce prompts unless a future spec amends.
```

### at pre-freeze line ~91

```markdown
### 2.7 API error classification (V1.49 P0)
```

### at pre-freeze line ~104

```markdown
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer); **V1.47 P0** shipped. Named `novel-chapter-review` (replaces the former generic `reflection-loop` demo). See §8 for output contract + [workflow-profile.md §5.3.3](../../active/novel-writing/workflow-profile.md#533-novel-chapter-review-gates-review-quality-pass) for preset gates. |
| `novel-brainstorm` | Ideation from open findings (V1.39 P2) |
| `novel-review-master` | Master decision surface (V1.39 P2) |
| `novel-manuscript-audit` | On-demand chapter audit — review and/or extract (V1.44 P0; see [manuscript-audit.md](../../active/novel-writing/manuscript-audit.md)) |
```

### at pre-freeze line ~109

```markdown
### 3.4 Review-master CLI surface (V1.45 P0–P2 — generic preset dispatch; **runner retired in v1.193 P2**)
```

### at pre-freeze line ~111

```markdown
V1.44 shipped a dedicated `review-master` subcommand. V1.45 replaced it with the generic `creator run <preset_id>` entry — `creator run novel-review-master` was the preset-id form. **Both entries were removed in v1.193 P2**: no CLI preset-dispatch entrance exists today ([creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) is the historical Master). Preset admission goes through the core execution surface (schedule add via `POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`); the retained CLI scheduling surface is `creator works cron` declaration editing. Findings listing lives on `creator works status`.
```

### at pre-freeze line ~113

````markdown
**Normative CLI (Shipped V1.45 — historical record, entry removed in v1.193 P2):**

```bash
nexus42 creator run novel-review-master [<work_id>] [--finding-id <id>] [--auto-schedule]
```
````

### at pre-freeze line ~121

```markdown
| `--finding-id` | Runs or enqueues `novel-review-master` preset scoped to one finding |
| `--auto-schedule` | Opt-in: enqueue `novel-review-master` when 96h stale findings exist (mirrors DF-67 Work setting) |
```

### at pre-freeze line ~128

```markdown
- Quickstart §5 updated to cite `creator run novel-review-master` as primary path (V1.45 P3 — historical; that cite was retired with the runner in v1.193 P2)
- On empty findings: `creator works status [<work_id>]` surfaces a clear "no findings yet" message (rendered as `none open`); the former suggestion to `creator run novel-review-master` went with the removed runner in v1.193 P2 — master-decision review admission is via the execution surface (schedule add), not a CLI entry

**Residual**: R-V143P0-002 — resolved V1.44 P1; close in P-last hygiene.

---
```

### at pre-freeze line ~137

```markdown
**V1.47 Shipped (normative)** — two-layer model (Layer 3 history removed). **Shipped runtime** (through V1.48) prefers `Works/<work_ref>/AGENTS.md` for Layer 2; legacy `Rules/novel-rules.md` read-only when absent.
```

### at pre-freeze line ~152

```markdown
V1.39+ quality-loop work uses subdirectories under `Works/<work_ref>/Logs/`:
```

### at pre-freeze line ~158

```markdown
  review/        # review outputs, findings notes, review-report.md (V1.48 producer)
  kb/            # V1.51: KB candidate audit trails (pending/rejected/missing)
```

### at pre-freeze line ~169

```markdown
### 5.5 Missing-KB detection at finalize (V1.51 T-A P2)
```

### at pre-freeze line ~183

```markdown
### 5.6 Auto-promote high-confidence KB candidates (V1.52 T-A P0 — Normative)
```

### at pre-freeze line ~224

```markdown
## 8. Reflection-loop output contract (V1.47 Shipped)
```

### at pre-freeze line ~226

```markdown
Heading preserved for back-compat with existing inbound cross-refs; the canonical preset id is `novel-chapter-review` (renamed in V1.47 P0).
```

### at pre-freeze line ~228

```markdown
**Scope**: Normative behavior for the FL-E `review` preset after P0 implement. Applies to **both** auto-chain review stage and on-demand preset admission (the on-demand CLI entry was `creator run <preset_id>`, retired in v1.193 P2).
```

### at pre-freeze line ~243

```markdown
3. Optional `rule_suggestion` text MAY be stored on the finding row; accepting a suggestion does **not** mutate `Works/<work_ref>/AGENTS.md` in V1.47.
```

### at pre-freeze line ~255

```markdown
*Shipped V1.47. Quality-loop normative SSOT for this domain; profile gates in [workflow-profile.md §5.3.3](../../active/novel-writing/workflow-profile.md#533-novel-chapter-review-gates-review-quality-pass).*
```

### at pre-freeze line ~259

```markdown
## 9. V1.48 Shipped — Findings maturity (folded from the novel-findings-maturity overlay)
```

### at pre-freeze line ~261

```markdown
V1.48 closes the novel quality loop: durable findings enrich the writing prompt, accepted suggestions mutate the runtime Layer 2, and a retention policy prevents unbounded growth.
```

### at pre-freeze line ~263

```markdown
### 9.1 Producer (V1.48 P0)
```

### at pre-freeze line ~271

```markdown
### 9.2 Consumer (V1.48 P1)
```

### at pre-freeze line ~274

```markdown
- Cap: 8 findings max, 400 chars/body, 3200 chars total block (per overlay §2.2).
```

### at pre-freeze line ~277

```markdown
### 9.3 Rules runtime (V1.48 P2)
```

### at pre-freeze line ~279

```markdown
- Runtime `read_rules_layers` prefers `Works/<work_ref>/AGENTS.md` (Layer 2 per V1.47 normative); falls back to legacy `Rules/novel-rules.md` read-only when `AGENTS.md` absent.
```

### at pre-freeze line ~285

```markdown
> **Forward pointer (V1.166, DR-64):** §9.3 is the work-scoped three-layer rules runtime (`read_rules_layers`); it is distinct from the daemon check callback — post-V1.164 `POST /v1/daemon/check` runs the frozen mental checker pair (`stale_belief_drift` / `dramatic_irony_asymmetry`), and V1.166 adds the deterministic rule evaluator over world-scoped `spoke_rules` (four constraint families; empty `rule_refs` auto-includes `status=active`; foreign-world ids fail closed). Iteration locks: `v1.166-quality-loop-locks.md`.
```

### at pre-freeze line ~287

```markdown
### 9.4 Data hygiene (V1.48 P3)
```

### at pre-freeze line ~294

```markdown
### 9.5 Acceptance (V1.48)
```

### at pre-freeze line ~297

```markdown
- All R-V147P0-* targets (6) closed in V1.48 (R-V147P0-01, 02, 03, 04, 05, 06).
- R-V147P1-01 (intake re-trigger on existing Work) — **deferred to V1.49** (per V1.48 P4 §8).
```

### at pre-freeze line ~302

```markdown
## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (Shipped Master V1.45). The `novel-review-master` preset id + enqueue-only semantics + audit preset ids are now part of the canonical Master body.
```
