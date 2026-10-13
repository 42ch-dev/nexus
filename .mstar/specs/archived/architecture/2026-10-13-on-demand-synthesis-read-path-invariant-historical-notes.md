# on demand synthesis read path invariant — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/on-demand-synthesis-read-path-invariant.md`](../../../knowledge/architecture-patterns/on-demand-synthesis-read-path-invariant.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 25

```markdown
V1.81 shipped `POST /v1/local/memory/soul/reflect` — an **on-demand** LLM
synthesis endpoint (Creator-SOUL Narrative). The web UI **polls** it every ~30s
with `force_regenerate: false` to render status (ungenerated / current / stale /
insufficient); the author explicitly opts into the expensive synthesis by
sending `force_regenerate: true` (the "Reflect on my SOUL" CTA). This on-demand
discipline is the same proportional-to-local-only model V1.80 established (no
background jobs; synchronous, user-triggered).

The Morning Star QC tri-review (3/3 Approve) and the headless QA gate (full
suite green) both **passed** this endpoint. Then the Greptile static-analysis
review (greploop) found a **critical** bug at confidence 3/5: the handler never
returned `state: "ungenerated"`, so a creator who passed the data gate but had
never generated a narrative fell straight through to **LLM synthesis on every
30s background poll** — silently, without user intent, unbounded. The frontend's
explicit "Reflect" CTA was unreachable.

This doc captures the two lessons: the read-path invariant that was violated,
and the verification gap that let QC + QA miss it.
```

### at pre-freeze line 46

```markdown
### The invariant (the part that was violated)
```

### at pre-freeze line 64

```markdown
The V1.81 bug: the `!force && !stale` block returned `current` only `if let
Some(cached)`; when `cached` was `None` (ungenerated) it **fell through** past
```

### at pre-freeze line 67

```markdown
A single missing `else → ungenerated` return made the poll path fire the LLM.
```

### at pre-freeze line 74

```markdown
### The verification gap (why QC + headless QA missed it)
```

### at pre-freeze line 81

```markdown
V1.81's QA report explicitly noted the scope limit: *"Live ACP synthesis
(`acp.prompt`) not exercisable headless without a configured worker. Seam, input
builder, cache, error paths, and gates are verified."* That limit is exactly
where the bug hid: the gate (force vs !force) was not verified end-to-end
because the synthesis side could not run.
```

### at pre-freeze line 93

```markdown
   reaches the registry lookup at all, the test fails. V1.81's G1 fix added
```

### at pre-freeze line 107

```markdown
### Why this matters beyond V1.81
```

### at pre-freeze line 117

```markdown
- **V1.81 `POST /soul/reflect`** — the bug itself. `force=false` poll fell
  through to `acp.prompt` for ungenerated creators. Fixed by gating synthesis
  behind `force=true` + adding the `ungenerated` return + the negative-path
  test. Caught by Greptile (greploop iteration 1, 3/5); missed by QC tri-review
  + headless QA.
```

### at pre-freeze line 136

```markdown
## Detection-history note
```

### at pre-freeze line 138

```markdown
This bug survived a full Morning Star QC tri-review (3/3 Approve) and a headless
QA gate (full suite green) before Greptile's static-analysis review flagged it.
The lesson for the QC process: for endpoints whose side effect (LLM/agent call)
is not CI-exercisable, add an explicit "read path does not reach the side-effect
site" assertion to the QC checklist — do not rely on the implementer's
self-check or headless tests alone.
```
