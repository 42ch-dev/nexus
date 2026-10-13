# Manuscript Audit — Draft-to-Shipped and CLI-Retirement Historical Notes

**Provenance**: frozen from [`specs/active/novel-writing/manuscript-audit.md`](../../active/novel-writing/manuscript-audit.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-06-13 — V1.44 shipped the feature (`**Status**: Shipped (V1.44 — 2026-06-13)`); the V1.45 supersession line and the v1.193 P2 CLI-retirement sketches are also frozen here. The live spec keeps the §1–§3/§5/§6 rules and the `Superseded by` pointer.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (36 lines), anchored to the line number it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Shipped (V1.44 — 2026-06-13)  
```

### at pre-freeze line ~6

```markdown
**Last updated**: 2026-06-13 (V1.44 P-last — promoted from Draft overlay to Shipped Feature line; DF-69 shipped)  
```

### at pre-freeze line ~13

```markdown
- [cli-spec.md](../../active/cli/cli-spec.md) — historical `creator run audit-chapter` IA (P0 implement; the runner entry was removed in v1.193 P2 — see §4)
```

### at pre-freeze line ~23

```markdown
V1.39–V1.43 shipped inline finalize `llm_judge`, FL-E `reflection-loop`, `novel-review-master` on **existing findings**, and queue-based `kb-extract`. Authors still lack a **single on-demand entry** to audit an already-written chapter body (`Works/<work_ref>/Stories/ch*.md`) outside the auto-chain driver.
```

### at pre-freeze line ~25

```markdown
V1.44 P0 implements **DF-69**: a dual-mode embedded preset (or preset pair) plus CLI entry.

V1.79's manuscript reading surface is distinct from this on-demand audit line: reading is a read-only consumption surface for authors, while manuscript audit remains an out-of-band quality/extract tool that may create reports, findings, or KnowledgeEntries.
```

### at pre-freeze line ~66

```markdown
- `Logs/review/audit-ch{nn}-v{vol}-{timestamp}.md` (or equivalent stable naming locked in P0 plan)
```

### at pre-freeze line ~85

```markdown
## 4. CLI entry (normative sketch — P0 locks single IA)

> **Entry retired (v1.193 P2).** Both surfaces sketched below were deleted with the v1.193 P2 command-surface cutover: the `creator run` runner and the whole `daemon` group are gone, and no current CLI audit entry is provided. The §3 modes and §5 preset contract are unchanged; audit admission goes through the core execution surface (schedule add via `POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`).

**Preferred** (compass grill-me 2026-06-13; historical):

```bash
nexus42 creator run audit-chapter <work_id> --mode review|extract [--chapter N] [--volume V]
```
```

### at pre-freeze line ~95

```markdown
**Alternatives** (document only; P0 plan picks one; both retired in v1.193 P2):

- `nexus42 daemon schedule add --preset novel-manuscript-audit --input '{"mode":"review",...}'`
- Subcommand under `creator run` with `audit-chapter` as a run intent
```

### at pre-freeze line ~102

```markdown
- Command must **not** set `fl_e_stage` driver fields or enqueue auto-chain continuation.
```

### at pre-freeze line ~111

```markdown
| `novel-manuscript-audit` | `review` + `extract` | Single preset with `mode` input **or** two thin presets; P0 plan decides |
```

### at pre-freeze line ~127

```markdown
5. CLI help cites [docs/README.md](../../../../docs/README.md) § novel-writing quickstart after P1 merge.


## 7. Promotion (iteration close)

At V1.44 P-last hygiene:

- [ ] Promote Status to **Shipped (V1.44)** or merge into `novel-writing/quality-loop.md` §3 if section stabilizes.


*Draft overlay for V1.44 P0. P0 implement plan is scope authority for preset YAML and exact CLI flags.*
```

### at pre-freeze line ~143

```markdown
## V1.45 supersession (P-last promotion)
```

### at pre-freeze line ~145

```markdown
**Superseded by**: [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (Shipped Master V1.45). The split preset ids (`novel-manuscript-audit-review` / `novel-manuscript-audit-extract`), DEPRECATED parent dir deletion, and `cli_args` declaration are now part of the canonical Master body.
```
