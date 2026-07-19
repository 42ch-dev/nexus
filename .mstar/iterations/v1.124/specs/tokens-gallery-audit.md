# Tokens Gallery Audit (V1.124 architect contract)

**Status:** Locked (iteration-scoped) — architect seat 2  
**Document class:** Iteration package working spec  
**Audience:** P1 implementers, QC, recurrence gate consumers  
**Authority:** Root `AGENTS.md` § "Tokens need a gallery"; compass AC-V1124-3 / NG-9  
**SSOT for values:** `tooling/design-tokens/src/tokens.css` (derived from root DESIGN pair)  
**Gallery surface:** `apps/design-studio/src/pages/tokens.tsx` (`CANVAS_TOKEN_GROUPS` + future Soul Viz group)

---

## 1. Purpose

1. Enumerate every V1.122/V1.123 Timeline / Layer / Outline-timeline-pin / Soul-viz-timeline token in CSS.
2. Compare to Studio Tokens page registration.
3. Lock **gallery IA** (subsection names + grouping) so P1 Task 2 does not invent layout.
4. Define the **recurrence gate** (P1 Task 3 fills enforcement text; skeleton locked here).

---

## 2. Audit method

- Grep `tooling/design-tokens/src/tokens.css` for:
  - `--color-canvas-timeline-*`
  - `--color-canvas-layer-*`
  - `--color-canvas-outline-timeline-*`
  - `--color-soul-viz-timeline-*`
- Grep `apps/design-studio/src/pages/tokens.tsx` for matching `label` / `varName` entries.
- Light values from `:root` block; dark from `.dark` block (both verified present for every row below).

**Scope boundary (NG-9 / AC-V1124-3):** P1 **must** close the nine rows in §3. Broader canvas gaps (worldkb entity cards, write-state, outline scene fills, etc.) are **out of AC-V1124-3** — listed in §5 as optional residual candidates, not silent gallery invention and not required for P1 Done.

---

## 3. Delta table — V1.124 Must set

| Token (CSS custom property) | Light (`:root`) | Dark (`.dark`) | In Studio gallery (pre-P1) | Action |
|-----------------------------|-----------------|----------------|----------------------------|--------|
| `--color-canvas-timeline-accent` | `var(--color-blue-700)` | `var(--color-blue-700)` | **No** | **add** → group **Canvas — Timeline accent spine** |
| `--color-canvas-layer-brief-accent` | `var(--color-amber-700)` | `var(--color-amber-700)` | **No** | **add** → group **Canvas — Layer accents** |
| `--color-canvas-layer-narrative-accent` | `var(--color-blue-700)` | `var(--color-blue-700)` | **No** | **add** → group **Canvas — Layer accents** |
| `--color-canvas-layer-moment-accent` | `var(--color-gray-900)` | `var(--color-gray-900)` | **No** | **add** → group **Canvas — Layer accents** |
| `--color-canvas-outline-timeline-event-pin` | `var(--color-amber-700)` | `var(--color-amber-700)` | **No** | **add** → group **Canvas — Outline Timeline pins** |
| `--color-canvas-outline-timeline-marker` | `var(--color-teal-700)` | `var(--color-teal-700)` | **No** | **add** → group **Canvas — Outline Timeline pins** |
| `--color-soul-viz-timeline-axis-line` | `rgba(0, 0, 0, 0.12)` | `rgba(255, 255, 255, 0.16)` | **No** | **add** → group **Soul Viz — Timeline axes** |
| `--color-soul-viz-timeline-axis-tick` | `#c7c7c7` | `#737373` | **No** | **add** → group **Soul Viz — Timeline axes** |
| `--color-soul-viz-timeline-axis-label` | `#8a8a8a` | `#a3a3a3` | **No** | **add** → group **Soul Viz — Timeline axes** |

**Already in gallery (leave — surface spines, not V1.124 gap):**

| Token | Gallery group today | Action |
|-------|---------------------|--------|
| `--color-canvas-strategy-accent` | Accent spines | **leave** |
| `--color-canvas-outline-accent` | Accent spines | **leave** |
| `--color-canvas-worldkb-accent` | Accent spines | **leave** |

---

## 4. Gallery IA (locked)

### 4.1 Placement rules

1. **Do not dump** new tokens at the end of an unrelated group.
2. Prefer extending the existing `CANVAS_TOKEN_GROUPS: CanvasTokenGroup[]` array with **new group objects** (same `title` / `hint` / `tokens[]` shape).
3. **Soul Viz** is not currently a Tokens page section — introduce a **sibling block** under the Canvas section (same page, new `data-testid` group), not a separate route.
4. Group titles are **product-readable** (contributor scanning Tokens page), not internal plan IDs.

### 4.2 Locked group titles + membership

| Order (relative) | Group `title` (exact string) | Tokens | Hint (intent) |
|------------------|------------------------------|--------|---------------|
| After existing **Accent spines** | **Canvas — Timeline accent spine** | `canvas-timeline-accent` | Surface-level Timeline identity (blue-700). Distinct from per-layer accents. |
| Next | **Canvas — Layer accents** | `canvas-layer-brief-accent`, `canvas-layer-narrative-accent`, `canvas-layer-moment-accent` | Intra-surface Brief / Narrative / Moment feel (V1.123 P4). Used on Timeline node icons and badges — see `studio-timeline-fixture-boundaries.md`. |
| Next | **Canvas — Outline Timeline pins** | `canvas-outline-timeline-event-pin`, `canvas-outline-timeline-marker` | Outline canvas when-axis pins/markers (not World Timeline card chrome). |
| After Canvas groups (or immediately after Outline pins) | **Soul Viz — Timeline axes** | `soul-viz-timeline-axis-line`, `soul-viz-timeline-axis-tick`, `soul-viz-timeline-axis-label` | Soul visualization timeline axis geometry colors (light/dark differ). |

### 4.3 Optional IA note (do not block P1)

Existing **Accent spines** group may gain a one-line hint update: "Surface spines (strategy / outline / worldkb). Timeline surface accent lives in **Canvas — Timeline accent spine**; layer feel lives in **Canvas — Layer accents**." Do not move the three existing spine tokens into the new groups.

### 4.4 Entry shape (unchanged)

```ts
{ label: 'canvas-layer-brief-accent', varName: '--color-canvas-layer-brief-accent' }
```

Labels omit the `--color-` prefix (match existing gallery convention).

---

## 5. Out-of-scope canvas tokens (not AC-V1124-3)

These exist in `tokens.css` and may lack gallery rows. **Do not** expand P1 to full canvas completeness unless PM re-scopes. Candidates for residual / future gallery sweep:

- `--color-canvas-write-*` (dirty / conflict / success / stale-bg)
- Full `--color-canvas-worldkb-*` entity/relationship/promotion family
- Full `--color-canvas-outline-*` volume/chapter/scene/beat family (except the two timeline pin tokens in §3)
- Ambient / node chrome / edges already partially gallery-covered — leave

If implementer discovers a **Timeline/Layer/Soul-viz-timeline** token missed by §3, **add it to §3 table + gallery in the same PR** (audit is living during P1 Execute).

---

## 6. Recurrence gate (normative skeleton — P1 Task 3 finalizes copy)

Cross-link: root `AGENTS.md` § "Tokens need a gallery". Gallery completeness is both **product policy** and **this iteration's exit evidence** (AC-V1124-3 + this gate).

**Checklist (copy-pasteable into QC / PR description):**

1. Any PR that adds a gallery-projected `--color-*` token to `tooling/design-tokens/src/tokens.css` **must** register it in `apps/design-studio/src/pages/tokens.tsx` in the **same PR**.
2. Light + dark values must both resolve (theme toggle check or equivalent test).
3. If the PR cannot add a gallery entry in the same change → **file a residual** (`residual_findings`) with severity ≥ important, naming the token and the Tokens page path — do not merge silent CSS-only tokens.
4. P1 is the **last catch-up sweep** for the V1.122/V1.123 Timeline / Layer / Outline-timeline-pin / Soul-viz-timeline families; future iterations treat CSS-only tokens in these families as **defects**, not backlog flavor.

P1 Task 3 appends any process notes; do not weaken the four bullets above.

---

## 7. Verification (P1 Done)

| Check | Evidence |
|-------|----------|
| All nine §3 tokens appear in DOM | Smoke test asserts labels / varNames |
| Theme toggle | Manual or test: light + dark computed values non-empty |
| No invented tokens | Diff only adds gallery entries for CSS-existing vars |
| IA matches §4.2 titles | Code review |

---

## 8. Relationship

| Doc | Role |
|-----|------|
| This file | CSS ↔ gallery delta + IA + recurrence skeleton |
| `studio-timeline-fixture-boundaries.md` | Which fixtures **consume** layer tokens (P0) |
| `studio-fixture-acceptance-criteria.md` | F6 token-true on fixtures (orthogonal to gallery completeness) |
