# Findings Lifecycle — V1.77 UI Promotion and Provenance Historical Notes

**Provenance**: frozen from [`specs/active/contracts/findings-lifecycle.md`](../../active/contracts/findings-lifecycle.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-06-30 — V1.77 closed (`510e3e208`, 2026-06-30); the removed text is the V1.77 Phase 2b promotion provenance ("promoted from `novel-writing/findings-lifecycle.md` stub") and the V1.77 UI-remediation tags. Every rule the lines annotated is still current authority in the live spec; only the iteration provenance is frozen here.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (6 lines), anchored to the line number it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Normative — V1.77 Phase 2b (promoted from `novel-writing/findings-lifecycle.md` stub)  
```

### at pre-freeze line ~9

```markdown
- [web-ui.md](../../active/surfaces/web-ui.md) — Control-Room findings view and V1.77 remediation surface
```

### at pre-freeze line ~19

```markdown
The findings lifecycle is a cross-profile 6-state status machine that all Work profiles consume. The backend (V1.49+) enforces the lifecycle server-side on every `PATCH /v1/local/works/{work_id}/findings/{finding_id}`, rejecting illegal transitions with HTTP 422 `INVALID_TRANSITION`. The V1.77 UI promotion adds a remediation authoring surface that consumes this existing PATCH route.
```

### at pre-freeze line ~82

```markdown
## 4. UI remediation surface (V1.77)
```

### at pre-freeze line ~116

```markdown
## 5. Non-goals (V1.77)
```

### at pre-freeze line ~120

```markdown
- **No create/delete from the UI** — the V1.77 lead surface is update-only. `createFinding`/`deleteFinding` remain CLI/producer-only for now; the web app can add them in a follow-up if UX demands in-UI finding creation.
```
