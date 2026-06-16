# Novel Narrative Indexes — Draft Overlay v1.49

**Status**: Draft (V1.49)  
**Document class**: Draft overlay  
**Created**: 2026-06-17  
**Last updated**: 2026-06-17  
**Scope**: Runtime contract for `Works/<work_ref>/Outlines/foreshadowing.md` (F###) and `event-index.md` (E###) — supersedes empty-stub-only behavior from V1.36 scaffold  
**Coordinates with**:

- [novel-workflow-profile.md](novel-workflow-profile.md) §4.2 (outline F### section), §3.1 layout
- [orchestration-engine.md](orchestration-engine.md) — `novel-writing` preset hooks

**Iteration compass**: [v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md](../../iterations/v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md)

---

## 1. Purpose

V1.36 scaffolded index files with table headers. Prompts require outline "Foreshadowing Touched" sections referencing F### ids, but no runtime allocates ids or persists promotions from outline text into `foreshadowing.md`.

V1.49 P1 implements the **minimum viable index runtime** (file-first SSOT).

---

## 2. SSOT and boundaries

| Path | SSOT | Notes |
| --- | --- | --- |
| `Works/<work_ref>/Outlines/foreshadowing.md` | **File** | F### rows; not scanned as chapter by sync module |
| `Works/<work_ref>/Outlines/event-index.md` | **File** | E### rows; P1 may ship read-only + stub writer |
| `work_chapters` table | DB | Unchanged; no index mirror table in V1.49 |

[`sync_module.rs`](../../../crates/nexus-orchestration/src/sync_module.rs) `SKIP_FILES` must continue to exclude index files from chapter discovery.

---

## 3. F### row schema (normative minimum)

Markdown table columns (aligned with embedded template):

| Column | Required | Description |
| --- | --- | --- |
| `id` | yes | `F` + three-digit zero-padded integer (`F001`) |
| `description` | yes | Short human label |
| `status` | yes | `planned` \| `buried` \| `paid_off` |
| `chapters` | no | Comma-separated chapter numbers where touched |

### 3.1 Id allocation

- Next id = `max(existing numeric suffix) + 1` for the Work.
- Inline outline declaration `F001: description` creates row if id absent.
- Duplicate id with conflicting description → fail outline step with remediation citing reconcile or manual edit.

---

## 4. Outline promotion flow (P1)

After `outline_chapter` writes `Outlines/chapters/ch<nn>-outline.md`:

1. Parse "Foreshadowing Touched" section for F### references and inline new items.
2. Upsert rows into `foreshadowing.md` (atomic temp + rename).
3. Inject summary block into subsequent draft prompt (optional P1 stretch; minimum: file updated).

Event-index (E###): **P1 minimum** — preserve scaffold; read existing rows for prompt if present. Full E### promotion deferred to V1.50 unless P1 capacity allows.

---

## 5. World KB boundary

Explicit promotion from index rows to World KB remains **manual/opt-in** per [novel-workflow-profile.md](novel-workflow-profile.md) §3.5.1.5. V1.49 does not auto-sync index → World KB.

---

## 6. P-last merge

Fold into [novel-workflow-profile.md](novel-workflow-profile.md) new **§4.6 Narrative indexes (V1.49)**. Archive this overlay with supersede stub.
