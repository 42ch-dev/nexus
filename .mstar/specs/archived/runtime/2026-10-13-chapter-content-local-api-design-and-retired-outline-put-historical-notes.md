# Chapter Content Daemon API — V1.65 Design and Retired Outline PUT — Historical Notes

**Provenance**: frozen from [`specs/active/runtime/chapter-content-local-api.md`](../../active/runtime/chapter-content-local-api.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text names the V1.65/V1.75/V1.192 iterations but no uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here. §5.4 (the retired whole-document outline PUT) was deleted as a whole section and the following sections were renumbered — no inbound anchor link referenced the old numbering.

### at pre-freeze line ~5 (Status)

```markdown
| **Status** | Shipped — V1.65 chapter surface; V1.75 retired whole-document outline PUT in favor of the canvas patch route |
```

### at pre-freeze line ~15 (§1 Purpose and boundary)

```markdown
V1.65 exposes the existing `work_chapters` metadata and file-backed chapter content to the bundled local Web UI so authors can plan and restructure a Work without turning the browser SPA into a manuscript co-writer.

Route and schema names in this document are current. Conditional “P0” wording
inside DTO/implementation discussions is retained as V1.65 design history and
does not override the shipped source or schema tree.
```

### at pre-freeze line ~24 (§1 — surface split)

```markdown
- **Structure and outline are writable** in V1.65.
- **Body markdown is read-only** — the AI owns prose writing via orchestration through the host-tool path; there is **no manual body editor** (the body-editor direction was rejected 2026-06-26 — Nexus is an AI-autonomous executor; see [canvas-strategy-surface.md](../../active/surfaces/canvas-strategy-surface.md)). Any future human body interaction is a V1.68 canvas concern (structured/node-granular, no-raw-file-editing), not a per-chapter manual write route.
```

### at pre-freeze line ~44 (§3 Endpoint summary — retired route row)

```markdown
| ~~`PUT`~~ | ~~`/v1/local/works/{work_id}/chapters/{n}/outline`~~ | **Removed in V1.75** (historical route spelling; canvas pivot). Outline prose writes now go through `POST /v1/daemon/works/{work_id}/chapters/{chapter_id}/patch` with `set.content` + `base_revision` (`outline_revision` CAS). See [canvas-strategy-surface.md](../../active/surfaces/canvas-strategy-surface.md) §3.5. | — | — |
```

### at pre-freeze line ~48 (§3)

```markdown
`{n}` is the chapter number within a volume. V1.65 MUST support `volume` as an optional query parameter on detail/content/patch routes; when omitted it defaults to `1` to preserve existing single-volume behavior.
```

### at pre-freeze line ~64 (§4.2 Chapter status enum)

```markdown
V1.65 uses the existing chapter status vocabulary:
```

### at pre-freeze line ~101 (§4.4 Chapter detail DTO)

```markdown
| `body_read_only` | boolean | yes | Always `true` in V1.65. |
```

### at pre-freeze line ~136 (§5.1 list query)

```markdown
| `status` | chapter status | absent | Optional filter; recommended for P2 table filtering. |
```

### at pre-freeze line ~204 (§5.4 — whole retired section)

The entire section was removed (heading + body). Its historical content:

```markdown
### 5.4 `PUT /v1/local/works/{work_id}/chapters/{n}/outline` — removed in V1.75

**Removed in V1.75** (canvas-pivot). The historical V1.65 whole-document outline PUT route, its `put-chapter-outline-request` DTO, and schema file were retired; outline prose writes now go through the canvas patch route `POST /v1/daemon/works/{work_id}/chapters/{chapter_id}/patch` with `set.content` + `base_revision` (`outline_revision` CAS). The atomic-write / RuntimeLockGuard / body-ownership invariants that this section used to describe are re-anchored to that PATCH content path in [daemon-api-surface-conventions.md](../../active/runtime/daemon-api-surface-conventions.md) §6.2 and §7 (V1.75 amendment); see also [canvas-strategy-surface.md](../../active/surfaces/canvas-strategy-surface.md) §3.5. The outline *read* (`GET …/outline`, §5.3) is unchanged.
```

### at pre-freeze line ~285 (§5.6 body read)

```markdown
- Body is read-only in V1.65: no PUT/PATCH body route is introduced.
```

### at pre-freeze line ~310 (§6 Work profile classification decision)

```markdown
1. It preserves V1.64 UI behavior where omitted profile values continue to let the daemon apply its default/profile inference.
...
2. It lets the V1.65 UI select a profile explicitly without forcing every existing consumer to send the field.
```

### at pre-freeze line ~332 (§8 heading)

```markdown
## 8. Host-agnostic frontend boundary (V1.65 title: "native-desktop-ready frontend boundary" — the native host was retired in v1.192)
```

### at pre-freeze line ~342 (§8 body)

```markdown
The contract deliberately returns markdown strings and relative workspace paths; it does not require browser filesystem access. Browser-only APIs such as clipboard writes are UI affordances around `Copy path`, not transport semantics. The V1.66 plan implemented `DesktopClient` with the same method signatures plus native `open with` / `reveal` affordances outside this API (historical: the native shell was retired in v1.192; the desktop host is Electron, which consumes this surface through the same `NexusClient` boundary).
```
