# P2 Spec — Dogfood-visible nit closeout (V1.126–V1.128 residuals)

> **Iteration:** `.mstar/iterations/v1.129/delivery-compass.md`
> **Status:** product-reviewed, architect-locked, writing-hygiene done (2026-07-21)
> **Plan:** `.mstar/plans/2026-07-21-v1.129-p2-dogfood-nit-closeout.md`
> **Cross-references:** P0 spec (`profile-create-reliability.md`) — transport classification; P1 spec (`transport-error-ux.md`) — toast error surface if delete fails
> **SSOT:** `.mstar/status.json` `residual_findings` + `metadata.tech_debt_summary`

## Problem statement (user value)

**Symptom a manual tester recognizes while dogfooding:** small, repeated "wait — that's wrong" moments that are not the Profile-create blob but still break trust:

1. **R-V1126P0-T2-001 (anchored):** Open shell selection submenu on a Work or World → actions include Timeline / outline-or-KB / Agent / Rename — **no Delete**. Author cannot remove the item from the place the UI just taught them to manage it.
2. **R-P1-001 (anchored):** Switch UI locale to `zh-CN` → walk Works / Schedule / Sessions / Strategies / Capabilities (and their dialogs + content editor) → **English-only chrome** mixed into an otherwise localized shell (~25 catalog gaps left from V1.112).
3. **Other open rows** from V1.126–V1.128 may also be user-visible (keyboard dead-ends, missing affordances, contrast fails). Those enter scope only after **visible-symptom triage** against `status.json`; they are not a blank check to clear all 40 residuals.

P2 closes the **dogfood-visible** subset so the author path stops accumulating unfinished chrome. Pure code-quality nits (e.g. `R-V1126P0-QC-S-001` two-source identity) **stay deferred** with explicit rationale — that discipline is part of the product promise, not optional.

## Scope (in)

- **Visible-symptom triage (gate for everything else):** read open `residual_findings` for plans  
  `2026-07-20-v1.126-p0-shell-selection-submenu`,  
  `2026-07-20-v1.126-p1-canvas-directed-axis`,  
  `2026-07-20-v1.126-p2-composite-timeline-endpoint`,  
  `2026-07-20-v1.126-p3-status-compaction-residual-cleanup`,  
  `2026-07-20-v1.127-p0-control-room-author-loop-fixes`,  
  `2026-07-20-v1.128-p1-nle-timeline-canvas`,  
  `2026-07-20-v1.128-p2-creator-create-controller-shell`,  
  `2026-07-12-v1.112-i18n-ui-migration`.  
  Classify each row **`visible`** (tester can reproduce a UI/UX symptom without reading code) vs **`quality-only`** (no user-facing symptom). Anchors **must** classify visible: `R-V1126P0-T2-001`, `R-P1-001`.
- **Delete on selection submenu (`R-V1126P0-T2-001`):** author can delete Work and World from the submenu after confirm; item leaves the list. Daemon contract shape (hard `DELETE` vs soft `PATCH`) is architect-locked — product only requires the author outcome.
- **Secondary-page i18n (`R-P1-001`):** catalogued strings on the flagged pages render in `zh-CN` (and remain correct in `en`). No new namespaces.
- **Other triage=`visible` rows:** minimal fix **or** explicit `decision: defer` with user-facing rationale in residual note. No silent drops.
- **Deferral discipline for quality-only:** every triage=`quality-only` row stays open; note records "deferred V1.129 P2 — no user-visible symptom."

## Scope (out)

- **Clearing the entire residual backlog** — only dogfood-visible rows; ~40 open does not mean ~40 fixes.
- **Performance** (`DF-V1127-COMPOSITE-PERF`) — scale, not usability.
- **Pure code-quality nits** — no user-observable symptom → deferred, not "fixed while we are here."
- **Re-opening `lifecycle: closed` / archived residuals.**
- **Visual redesign / design-system elevation** — symptom gone, not restyled (`DF-V1122-V1121-RES` owns elevation).
- **P0/P1 transport work** — do not re-fix create/classification here; only consume P1 toast path if delete fails.

## Interfaces

### DELETE routes + cascade (locked Seat 2)

**Decision: hard delete with confirm dialog.** Soft delete (`PATCH {status:'deleted'}`) adds a `status` column to `works` + `worlds` tables and a filter on every list query — the blast radius is larger than the user-visible benefit warrants for V1. Confirm dialog is the safety net; undo is not a V1 goal. Authoring-tool expectation: DELETE in a submenu with a confirm step is the standard destructive-action pattern.

```http
DELETE /v1/daemon/works/{work_id}
DELETE /v1/daemon/worlds/{world_id}
```

**Response 204** on success; canonical `NexusApiError` envelope for failures (`NotFound` for unknown id; `Internal` for DB failures; 401 without API key handled by middleware).

### Cascade rules (locked Seat 2)

Principle: cascade-delete **child** entities that are meaningless without their parent; preserve **referencing** entities that are independently meaningful.

| Deleted entity | Cascade effect | Rationale |
|---|---|---|
| **Work** | Cascade-delete: Manuscripts, Outlines, Timelines, KB entries under this Work | These are wholly owned by the Work — no independent existence |
| **World** | Cascade-delete: KB entries, Timelines directly under this World | These are wholly owned by the World |
| **World** | Set `world_id` to NULL on Works that reference this World | Works are independently meaningful — do NOT cascade-delete them |

Implementer (T2): verify the actual SQLite foreign key constraints on the `works`, `manuscripts`, `outlines`, `timelines`, and `kb_entries` tables. If foreign keys with `ON DELETE CASCADE` / `ON DELETE SET NULL` exist, the handler can rely on them. If not, add manual cascade queries. Prefer explicit cascade queries in the handler for visibility — do not rely solely on silent FK cascades that may surprise future maintainers.

### Confirm dialog contract (locked Seat 2)

Per Seat 1 concern #1: the confirm dialog must **name what will be removed**. Product copy (polished Seat 3):

| Entity | Dialog title | Body |
|--------|-------------|------|
| Work | Delete "<work_title>" | This will permanently remove this Work and all of its manuscripts, outlines, and timeline entries. This cannot be undone. |
| World | Delete "<world_name>" | This will permanently remove this World and its knowledge base entries. Works referencing this World will remain but will no longer be associated with it. This cannot be undone. |
| — | — | **CTA:** Delete (destructive primary) / Cancel (secondary) |

> **`wire_contracts_changed` verdict: `true` (locked Seat 2).** Two new DELETE daemon routes. No new JSON Schema types needed (both handlers return 204 No Content). The route entries are the only contract delta.

### i18n keys

Add to `apps/web/src/locales/{en,zh-CN}/*.json` per the nine-namespace table in `apps/web/AGENTS.md`. New keys follow the dot-separated convention. No new namespaces.

## Acceptance criteria

- **AC-V1129-P2-1 (Delete on submenu — R-V1126P0-T2-001):** Open selection submenu on a Work **and** on a World. **Pass:** Delete is present; confirm; row leaves the list and does not reappear after reload (hard or soft delete per architect lock is invisible to the author as long as the item is gone from normal lists). Residual closed + archived. **Fail:** Delete missing, confirm skipped with instant destroy, or item returns on reload.
- **AC-V1129-P2-2 (i18n — R-P1-001):** Locale `zh-CN`; walk Works / Schedule / Sessions / Strategies / Capabilities + dialogs + content editor. **Pass:** every string catalogued under R-P1-001 is Chinese (no leftover English chrome for those keys); `en` still correct. Residual closed + archived. **Fail:** any flagged surface still shows hardcoded English for a catalogued key.
- **AC-V1129-P2-3 (triage complete + discipline):** Triage table lists every scanned residual as `visible` or `quality-only`. **Pass:** each `visible` row is fixed (with regression evidence) **or** `decision: defer` with rationale; each `quality-only` row stays open with "no user-visible symptom" deferral note. **Fail:** visible row silently ignored, or quality-only row closed without a user-facing fix (scope bleed).
- **AC-V1129-P2-4 (no regression):** Existing suites pass; DELETE paths covered by tests; at least one assertion per secondary page namespace that new keys resolve under `zh-CN`.

## Test strategy

- **Daemon DELETE routes:** axum integration tests (success 204, 404 for unknown id, 401 without API key, pool-attach pattern preserved).
- **Web submenu:** component test for DELETE presence + list refresh; integration test for full delete-and-reload cycle.
- **i18n:** snapshot or visual diff under both locales for the affected pages; smoke test that runs `t()` over the new keys to confirm no missing-key warnings in the console.

## Risks / open questions (architect Seat 2 — locked)

1. ~~Hard delete vs soft delete (`R-V1126P0-T2-001`):~~ **Locked: hard delete with confirm dialog.** Rationale above (§ Interfaces). Confirm dialog + naming what is removed addresses author safety.
2. ~~DELETE cascade:~~ **Locked.** See cascade rules table above (§ Interfaces). Implementer must verify actual foreign key constraints and prefer explicit cascade queries.
3. ~~i18n nuance (machine translation vs human review):~~ **Locked: ship best-effort zh-CN.** Mark machine-translated strings for human review in a follow-up residual note. Writing-specialist (Seat 3) pressure-tests EN tone but does not gate ship on zh-CN perfection.

## References

- Residual SSOT: `.mstar/status.json` `residual_findings` + `metadata.tech_debt_summary`
- `R-V1126P0-T2-001` original note: `.mstar/plans/2026-07-20-v1.126-p0-shell-selection-submenu.md#Review Gate Summary`
- `R-P1-001` original note: `.mstar/plans/2026-07-12-v1.112-i18n-ui-migration.md`
- i18n rules: `apps/web/AGENTS.md` § i18n
- Error envelope rule: `crates/nexus-daemon-runtime/AGENTS.md`
- DF tracker alignment: `.mstar/knowledge/deferred-features-cross-version-tracker.md` (`DF-V1127-NIT-CLOSEOUT`)
