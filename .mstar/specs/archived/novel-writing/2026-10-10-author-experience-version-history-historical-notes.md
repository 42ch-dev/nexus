# Author Experience — Version History Historical Notes

**Provenance**: frozen from [`specs/active/novel-writing/author-experience.md`](../../active/novel-writing/author-experience.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-10-10 — the removed header carried `**Last updated**: 2026-10-10 (v1.210 P3 …)`; the frozen lines span V1.36–V1.46 delivery framing, the V1.49 P2 author-desk overlay, and the v1.193 P2 runner retirement. Every live rule (journey, `--json` contract, remediation chains, Grill/W/qc3 decision IDs) is retained present-tense in the live spec.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (72 lines), anchored to the line number it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: **Shipped (V1.49 P2 — author desk UX integrated)** — baseline Shipped (V1.43) + V1.45 CLI IA amendments + V1.46 Shipped; §8 intake re-trigger + reconcile preview merged from V1.49 P2 overlay  
```

### at pre-freeze line ~6

```markdown
**Last updated**: 2026-10-10 (v1.210 P3 — author journey re-narrated canvas-first; retired-command citations removed)  
**Scope**: End-user **ongoing serial** happy path — canvas-first author journey, normative CLI surfaces, remediation chains, and author visibility (spec-only SSOT; **no** `docs/novel-writing-quickstart.md` after P1)  
```

### at pre-freeze line ~11

```markdown
- [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) — historical V1.45 CLI IA (its generic preset runner was retired in v1.193 P2; kept for provenance)
- [creator-centric-entry-model.md](../creator/2026-06-07-creator-centric-entry-model.md) — historical §3.1 local bootstrap (≤7 steps)
```

### at pre-freeze line ~21

```markdown
V1.36–V1.45 implemented novel-writing **capabilities** across crates. V1.46 does **not** add a new profile or preset grammar. It:
```

### at pre-freeze line ~23

```markdown
1. **Embeds the author happy path** (formerly BL-10 quickstart) in this spec §3 — compact ~80 lines.
2. **Closes author-desk deltas** — `--json` `findings[]`, per-finding remediation, novel-only scope (P0).
3. **Retires duplicate end-user doc** — `docs/novel-writing-quickstart.md` deleted in P1; agents cite `.mstar/specs/` only.
```

### at pre-freeze line ~27

```markdown
**v1.210 P3 refresh**: §3's journey is re-narrated to the canvas-first reality — Work creation via the world-bound web canvas, ref-less reads as the default-outline degrade (v1.209 P3), ref-less writes as the typed D7 refusal (v1.210 P2) — and every retired-command citation is removed.

**Part II (optional)** — multi-work switch, multi-volume, inspiration pool — documentation pointers only; shipped in V1.41–V1.44.
```

### at pre-freeze line ~33

```markdown
## 2. Document map (V1.46)
```

### at pre-freeze line ~35

```markdown
| Section | Content | Owner plan |
| --- | --- | --- |
| §3 Author path | World + Work init → first chapter → serial → quality loop → completion | P1 (narrative); CLI detail in [cli-spec.md §6.0B](../../active/cli/cli-spec.md#60b-v2-命令信息架构权威) |
| §4 Author visibility | Human + machine-readable status surfaces | P0 delta on V1.43 baseline |
| §4.1 `--json` contract | `findings[]` + optional `findings_stale` | P0 |
| §5 Residual pointer | local process (not clone SSOT) | P-last |
```

### at pre-freeze line ~42

```markdown
**Invariant**: Every command in §3 must exist in [cli-spec.md](../../active/cli/cli-spec.md) at ship time; every canvas action narrated in §3 must be a shipped canvas surface.
```

### at pre-freeze line ~60

```markdown
app starts and supervises the service. The CLI daemon group is retired
(v1.193 P2) — there is no terminal command that starts the service.
```

### at pre-freeze line ~75

```markdown
Work-create command** — canvas creation is the product path (the CLI
Work-creation entrance was retired in v1.193 P2).
```

### at pre-freeze line ~82

```markdown
  default outline instead of erroring (v1.209 P3) — the same "outline not yet
```

### at pre-freeze line ~88

```markdown
  step — assign the Work's `story_ref` (v1.210 P2, D7). The refusal surfaces
```

### at pre-freeze line ~120

```markdown
On-disk chapter files: see [workflow-profile.md](../../active/novel-writing/workflow-profile.md); missing paths surfaced in status (P2 on-disk hints).
```

### at pre-freeze line ~124

```markdown
> **V1.47 shipped**: Review preset produces findings per [quality-loop.md §8](../../active/novel-writing/quality-loop.md#8-reflection-loop-output-contract) (P0). The preset is named `novel-chapter-review` (replaces the former generic `reflection-loop` demo).
```

### at pre-freeze line ~137

```markdown
**Remediation (P0, Grill #7)**: `works status` uses **per-finding `routing_hint`** only — no blanket footer pointing only at `novel-chapter-review`; with **zero** open findings it reports the state alone (v1.193 P2 removed the runner the old master-review suggestion named). Review presets are enqueued as schedule runs carrying the Work's context; the 96h stale-findings auto-schedule is opt-in per Work.
```

### at pre-freeze line ~139

```markdown
96h master-review banner: visible on `creator works status` (V1.39 P4 baseline).
```

### at pre-freeze line ~151

```markdown
Auto-chain stops on completion (`reject_produce_when_novel_complete` — V1.39+).
```

### at pre-freeze line ~163

```markdown
## 4. Author visibility (P2 baseline + V1.46 delta)
```

### at pre-freeze line ~167

```markdown
| Question | Surface (minimum) | Status |
| --- | --- | --- |
| Which chapter is active? | `creator works status` — `current_chapter` + chapter table | Shipped V1.43 P2 |
| Is the Work complete? | Completed banner + `COMPLETED` marker | Shipped V1.43 P2 |
| Open findings? | Count + severity; per-row hints | Shipped V1.43 P2; **remediation delta P0** |
| 96h master-review banner? | Stale banner on status path | Shipped V1.39 P4 |
| Run master review? | `novel-review-master` as a schedule run carrying the Work's context; 96h opt-in auto-schedule | Shipped V1.45; CLI runner retired v1.193 P2 |
```

### at pre-freeze line ~177

```markdown
### 4.1 Machine-readable status (`--json`, V1.46 P0)
```

### at pre-freeze line ~217

```markdown
**V1.46 P1**: remove all `docs/novel-writing-quickstart.md §N` runtime references.

```

### at pre-freeze line ~221

```markdown
## 6. P-last author-path tech-debt (pointer)
```

### at pre-freeze line ~227

```markdown
## 7. Promotion (iteration close)

At V1.46 P-last:

- [ ] Draft → **Shipped (V1.46)** header
- [ ] BL-10 archive supersede note in shipped tracker (Grill #15)
- [ ] Confirm zero runtime quickstart references


## 8. Author desk deltas (Shipped V1.49 P2)
```

### at pre-freeze line ~239

```markdown
> **Status**: Shipped (V1.49 P2) — P2 overlay merged into Master.  
> **Plan**: 
> **Cross-refs**: findings lifecycle → [quality-loop.md §2](../../active/novel-writing/quality-loop.md#2-findings-lifecycle) (6-state V1.49 P0); narrative indexes → [workflow-profile.md §4.6](../../active/novel-writing/workflow-profile.md#46-narrative-indexes--f--e-runtime-v149-p1) (V1.49 P1)
```

### at pre-freeze line ~243

```markdown
### 8.1 Intake re-trigger on existing Work (R-V147P1-01)

**Problem** (as recorded at V1.49): the V1.49-era Work-creation flow scheduled
`creative-brief-intake` for the new Work, but existing Works had no equivalent
intake re-trigger. That creation flow and the intake CLI leaf V1.49 P2 shipped
for this residual were both retired in v1.193 P2 (v1.193 P2-T1 removed the
remaining execution entrances).

**Present lane** (v1.210): `creative-brief-intake` is an embedded preset that
```

### at pre-freeze line ~257

```markdown
dedicated CLI leaf for this since v1.193 P2.

### 8.2 Reconcile preview (R-V148P4-W2)
```

### at pre-freeze line ~261

```markdown
**Problem**: `creator works reconcile-chapters` mutates filesystem frontmatter and `work_chapters` without preview.
```

### at pre-freeze line ~263

```markdown
**Shipped CLI** (mirror `works rules reset` safety flags; V1.49 P2):
```

### at pre-freeze line ~269

```markdown
| Flag | Requirement | Implementation (V1.49 P2) |
```

### at pre-freeze line ~275

```markdown
The daemon handler signature is now `POST /v1/daemon/works/{work_id}/reconcile-chapters?dry_run=true|false` (query param optional, defaults to the mutating path).
```
