# VI Chronos confirm + Logo upgrade

> **SSOT status:** `.mstar/status.json`  
> **VI reference (Cursor canvas):** `~/.cursor/projects/Users-bibi-workspace-organizations-42ch-nexus/canvases/v1-130-vi-atmosphere-palettes.canvas.tsx`  
> **Token SSOT:** root `DESIGN.md` / `DESIGN.dark.md` (V1.130 P4 Chronos lock)  
> **Logo provenance:** `packages/nexus-ui/assets/logos/*.png` (new mark + wordmark + theme variants)  
> **Standalone plan** — not part of V1.130 iteration; merge target `main`

**Goal:** Align the full design system + Design Studio to **T1 Chronos Light/Dark Shell** philosophy (Cursor canvas), unify cyan brand signal (including primary Button light = dark), replace the old N-network mark with the new timeline logo system, and adopt it across product/Studio surfaces.

**Architecture:** Chronos tokens stay compile-time SSOT (no runtime atmosphere switcher). Interactive chrome uses a **dual-role** model (Ink structure vs Cyan signal) so Light and Dark share the same brand accent language. Logo geometry is a wide five-node timeline mark (+ optional wordmark). Package owns SVG assets + presentational brand components; apps resolve SVG URLs via bundler. Theme variants (elegant/nature/parchment/scifi) are Studio-only specimens.

**Tech Stack:** `@42ch/nexus-ui`, `apps/web`, `apps/design-studio`, root DESIGN pair, `@nexus/design-tokens`, optional `apps/desktop` icon sources.

**Execution:** mstar-sdd

### Wire contracts

- **Verdict:** `wire_contracts_changed: false`

## Prepare gates

- specify: [done]
- clarify: [done] — Chronos canvas = Cursor `/canvas`; primary Button light=dark cyan; full DS+Studio review required
- plan: [locked] 2026-07-22

## Intent gate

| Item | Statement |
| --- | --- |
| Real goal | Product + Studio read as one Chronos identity (青绿墨渊 · 生机/神秘/时间流): cyan brand signal consistent across shells; timeline logo; no office-navy / deep-as-primary-CTA on light |
| Success criteria | DESIGN pair documents dual-role philosophy; Light/Dark primary Button identical (cyan fill + deep text); interactive signal affordances cyan in both shells; text links on light stay deep (AA); Studio Tokens/Components/Brand/Surfaces/shell fixtures pass light+dark Chronos check; new logo adopted; no old N-network mark in runtime |
| Non-goals | Runtime multi-theme switcher; locking T2 Umbra / T3 Aurora; variant SVGs; re-litigating Chronos base hexes (`#0D2B3E` / `#25D1E0`) |

## Clarify decisions (locked)

### VI — T1 Chronos (from canvas)

Source: [v1-130-vi-atmosphere-palettes.canvas.tsx](/Users/bibi/.cursor/projects/Users-bibi-workspace-organizations-42ch-nexus/canvases/v1-130-vi-atmosphere-palettes.canvas.tsx) · status **Locked · shipped**.

| Token role | Hex | Light Shell | Dark Shell (ink) |
| --- | --- | --- | --- |
| `brand-deep-blue` | `#0D2B3E` | Titlebar fill, **text links**, structural chrome tint | Fill guard only — **never** deep fills on dark chrome; titlebar fill still deep |
| deep steps | `#0A2333` / `#071A28` / `#04121C` | Deep hover (structure only) | Dark cast source |
| `brand-cyan` | `#25D1E0` (invariant) | **Brand signal** (active bar, primary CTA fill, focus, selection) | Same signal + titlebar label text on deep titlebar |
| Light bg / surface | `#FFFFFF` / `#FAF8F4` | Page bg + warm-paper panels | — |
| Dark bg / surface | `#08141C` / `#0D1B26` | — | Ink chamber page + panels |

**MiniShell component philosophy (canvas `MiniShell`):**

1. **Titlebar** sits on `deep`. Light: white label text. Dark: **cyan** label text (not white).
2. **功能区 (sidebar strip):** surface fill; active rail uses **cyan**; quiet rows use deep at low opacity.
3. **内容区:** page bg; cards/panels use surface + deep-alpha border.
4. **Footer 创作|编排:** active pill = cyan fill + **deep text** (both shells); inactive = deep @ ~0.55 opacity.
5. **Cyan invariant:** exact `#25D1E0` everywhere.
6. **Buttons (author lock):** Light primary === Dark primary — **cyan fill + deep text**. No deep-blue fill + white text for primary CTAs on light.
7. **Dark interactive guard:** cyan fills use deep text (6.2:1 AA); no deep-blue fills on dark chrome.

### Chronos dual-role model (PM review lock — DESIGN / Studio alignment)

Today Light and Dark diverge because light `blue-700` = **deep** while dark `blue-700` = **cyan**. Canvas + author intent require **cyan as the shared brand signal**. Body text / links on light **cannot** be cyan (AA fail ~1.9:1 on white). Therefore lock a **dual-role** model and implement it across DESIGN, tokens, package primitives, App, and Studio:

| Role | Color | Light | Dark | Use for |
| --- | --- | --- | --- | --- |
| **Ink structure** | deep `#0D2B3E` | deep | deep (fill only as guard / titlebar fill) | Titlebar fill, text **links** on light, connection-setup deep washes, logo structure on light |
| **Cyan signal** | cyan `#25D1E0` | cyan (same as dark) | cyan | Primary Button fill, active nav bar, focus-ring outer, spinner/progress fills, selection strokes, timeline accents, setup active step, footer active pill, checked/selected control chrome |
| **Surfaces** | paper / ink | `#FFFFFF` / `#FAF8F4` / `#F5F2EC` | `#08141C` / `#0D1B26` / `#132635` | Page, panels, hover — already Chronos-aligned |

**Token strategy (locked):**

1. Flip light **interactive scale** `blue-700/800/900/1000` to the **cyan scale** (mirror dark role; use `brand-cyan` / `brand-cyan-800` / `brand-cyan-900` / `brand-cyan-1000` already in DESIGN light, or equivalent light-tuned cyan steps).
2. Primary Button light = `bg brand-cyan` + `text brand-deep-blue` (remove `dark:` fork for primary fill/text).
3. Retarget **textual links / retry text / list links** that currently use `text-blue-700` on light to **`text-brand-deep-blue`** (dark keeps bright cyan via `dark:text-blue-700` or `dark:text-brand-cyan`). Document semantic pair: light link = deep, dark link = cyan.
4. Do **not** use cyan as paragraph/body text on light surfaces.
5. Graphical accent icons on light: brand-accent indicators may use cyan; if a control needs AA graphical ≥3:1 on white and cyan fails, pair cyan with deep (icon on cyan pill) or use deep for the icon — document in DESIGN.

### Gap matrix (PM review — DESIGN + Studio + App)

| Area | Current drift | Target (Chronos) | Priority |
| --- | --- | --- | --- |
| `components.button.primary` (DESIGN light) | deep fill + white text | cyan fill + deep text (= dark) | Must |
| `@42ch/nexus-ui` Button CVA | `bg-blue-700 text-white` + `dark:bg-brand-cyan` | single cyan+deep recipe both themes | Must |
| Light `blue-700` token | `#0D2B3E` | cyan scale (signal) | Must |
| Light focus-ring outer | deep via `blue-700` | cyan | Must |
| Sidebar `activeBarColor` / chrome | `bg-blue-700` (deep on light) | cyan both shells | Must |
| Spinner / progress / setup active step / footer avatar active | `blue-700` (deep on light) | cyan signal | Must |
| Canvas selection / timeline / WorldKB focus | `blue-700` (deep on light) | cyan signal (timeline = Chronos) | Must |
| Text links (`text-blue-700` in lists, retry, agent-picker) | would become cyan if only token-flip | migrate light links → `brand-deep-blue` | Must |
| Empty-create / hub icons (`text-blue-700`) | deep on light | cyan accent icon OK if brand signal; else deep | Should |
| Connection-setup security note (deep alpha washes) | deep tint — structure role | keep deep (ink structure) | Keep |
| Shell titlebar label color (if presentational extract) | may use white both | Light white / Dark cyan on deep titlebar | Must if chrome exists |
| Footer 创作\|编排 active pill | verify matches cyan+deep | cyan fill + deep text | Must |
| DESIGN prose (Semantic Mapping, Button Contrast, Cyan usage rule) | light primary = deep | rewrite for dual-role + unified primary | Must |
| AA contrast tables | light primary pairings outdated | recompute cyan-fill+deep-text; link pairings stay deep | Must |
| Studio Tokens gallery | shows blue-* as deep on light | reflect cyan interactive scale; annotate link vs signal | Must |
| Studio Components Button matrix | light primary deep | light+dark primary identical cyan | Must |
| Studio Brand page | old N-network logos | new mark + text + variants; Chronos shell context | Must |
| Studio Surfaces / shell fixtures | may show deep active bars | cyan active + warm-paper / ink surfaces | Must |
| Studio home / voice | may not mention Chronos dual-role | short Chronos identity note if gallery intro exists | Should |
| App ad-hoc `bg-blue-700 text-white` CTAs (timeline canvases) | light deep CTA | cyan fill + deep text (or use `<Button variant="primary">`) | Must |
| Desktop icons | old N mark | new timeline mark | Should |

**VI work in this plan:** full DESIGN + tokens + package + App + Studio alignment per gap matrix (not a token-hex re-litigation of Chronos deep/cyan bases). T2/T3 Umbra/Aurora stay Studio-only comparison (canvas already marks them Studio).

### Logo — new mark system

Provenance PNGs (keep as LFS): `logo-primary.png`, `logo-white-bg.png`, `logo-mono.png`, `logo-text.png`, `logo-variants-*.png`. Delete legacy `logo_dark.png` / `logo_light.png` / `logo_white.png` already staged.

**Geometry:** horizontal five-node timeline (ring · ring · **solid center** · ring · ring) on one axis line. Gradient left→right (deep→cyan for color marks; light→dark gray for mono). Wide aspect (not the old 1:1 N-network square).

**Canonical SVGs (transparent background — shell provides fill):**

| File | Role | Maps from |
| --- | --- | --- |
| `logo-primary.svg` | Light nav / light shell header | `logo-white-bg.png` mark (deep→cyan gradient) |
| `logo-color.svg` | Dark nav / dark shell header | `logo-primary.png` mark (brighter gradient for dark surfaces) |
| `logo-white.svg` | Dark hero / high-contrast panels | white/near-white monochrome mark |
| `logo-mono.svg` | Inline UI; `currentColor` | `logo-mono.png` geometry (no baked gradient in mono path — inherit color) |
| `logo-text.svg` | Wordmark `nexus` (geometric sans, lowercase) | `logo-text.png` |

Public `logoVariants` keys stay `primary | color | white | mono`; add `text` for wordmark.

**Product placement:**

- Shell sidebar/header: **mark only**, theme-aware (`primary` light / `color` dark) — existing `apps/web` thin wrapper pattern.
- Studio Brand + optional lockup: mark + `logo-text` composition.
- `<NexusMark>`: hand-authored wide timeline geometry (replace old N-network JSX); `currentColor`.

**Theme variants (no SVG files):**

- Presentational component(s) e.g. `<NexusLogoVariant theme="elegant|nature|parchment|scifi" />` (or equivalent) driven by palette props derived from PNG mood — gold / nature / parchment / scifi.
- Studio Brand gallery only; **not** wired into product runtime; **not** a theme switcher.
- Record promotion in `packages/nexus-ui/AGENTS.md` promotion list.

**Desktop icons:** update `apps/desktop` icon source composition to the new mark (same Chronos deep/cyan) when icon sources still use the old N-network mark.

## Global Constraints

- Cyan hex remains exactly `#25D1E0`; deep remains `#0D2B3E`.
- No runtime atmosphere id / preference / switcher.
- Dual-role: cyan = signal (shared light/dark); deep = ink structure + light text links.
- No `.svg` imports inside `packages/nexus-ui` component source (NexusLogo uses consumer `src`; NexusMark is inline JSX).
- PNG = provenance only; SVG = runtime canonical.
- Studio-first: Tokens / Components / Brand / Surfaces fixtures updated before claiming App done.
- UI component policy: brand + promoted primitives in `@42ch/nexus-ui`; shell chrome stays `@web-layout` / app-local.

## Tasks

### Task 1: Canonical SVGs + DESIGN logo section

- [x] **T1 · Must**
  **Files:** `packages/nexus-ui/assets/logos/{logo-primary,logo-color,logo-white,logo-mono,logo-text}.svg`; `packages/nexus-ui/package.json` exports; `DESIGN.md` / `DESIGN.dark.md` Logo Usage; remove obsolete `logo_*` PNGs from tree if still present.
  **Interfaces:** transparent wide-viewBox SVGs; public asset paths unchanged for existing keys + new `logo-text.svg`.
  **DoD:** SVGs match new geometry; no old N-network paths remain in logo SVG files; DESIGN documents Light/Dark placement + wordmark rules; package exports `logo-text.svg`.

### Task 2: Package brand components + tokens

- [x] **T2 · Must**
  **Files:** `packages/nexus-ui/src/tokens.ts`, `nexus-logo.tsx`, `nexus-mark.tsx` (+ tests), `index.ts`, package README/AGENTS promotion row for any new variant component.
  **Interfaces:** `logoVariants` includes `text`; `NexusMark` renders timeline mark; optional `NexusLogoVariant` for theme specimens (palette props, no asset import).
  **DoD:** unit tests pass; Mark is wide aspect (`w-auto` friendly); variants render in isolation without SVG files.

### Task 3: Logo adoption (App + Studio Brand)

- [x] **T3 · Must**
  **Files:** `apps/web/src/components/brand/nexus-logo.tsx` (+ tests); Studio `brand.tsx` / fixtures using logos; any remaining old-asset imports.
  **Interfaces:** theme-aware mark in shell; Brand page shows primary/color/white/mono/text + Chronos Light/Dark mini context + theme variants gallery.
  **DoD:** no references to deleted `logo_*` PNGs; shell uses new mark; Studio Brand gallery complete light+dark.

### Task 4: DESIGN dual-role + interactive token flip

- [x] **T4 · Must**
  **Files:** `DESIGN.md`, `DESIGN.dark.md` (button primary, focus-ring, semantic mapping, cyan usage rule, contrast tables); `tooling/design-tokens/src/tokens.css` light `blue-*` scale; package `theme.css` if mirrored.
  **Interfaces:** light `blue-700…1000` = cyan signal scale (same role as dark); light primary button = cyan fill + deep text; document link = `brand-deep-blue` on light / cyan on dark.
  **DoD:** DESIGN prose states Chronos dual-role; light primary button tokens match dark; AA tables updated; no `#1E3A5F`; cyan remains `#25D1E0`.

### Task 5: Primitives + shell chrome signal unification

- [x] **T5 · Must**
  **Files:** `packages/nexus-ui` Button (+ tests); shell sidebar active bar; focus ring CSS; spinner/states; setup wizard active step; footer profiles / mode switch; any presentational titlebar.
  **Interfaces:** consume unified tokens; primary Button has no light/dark fill fork.
  **DoD:** Light and Dark primary buttons pixel-same recipe; active bars/focus/spinners use cyan signal; titlebar label rule (white light / cyan dark) if chrome present; unit tests updated.

### Task 6: Text-link migration + ad-hoc CTA cleanup

- [x] **T6 · Must**
  **Files:** App/Studio call sites using `text-blue-700` for **links/retry** (lists, states, agent-picker, global-timeline, etc.); ad-hoc `bg-blue-700 text-white` CTAs (timeline canvases) → primary Button or cyan+deep.
  **Interfaces:** light links → `text-brand-deep-blue` (+ dark cyan/blue-700); do not leave body links on cyan.
  **DoD:** grep for light-theme link contrast violations; no light primary CTA still using deep fill + white text.

### Task 7: Design Studio full Chronos review pass

- [x] **T7 · Must**
  **Files:** `apps/design-studio/src/pages/{tokens,components,brand,surfaces,home}.tsx`; shell/settings/setup fixtures; canvas surface fixtures as needed for selection/timeline accents.
  **Interfaces:** gallery is the acceptance surface for dual-role + logo.
  **DoD:** Tokens show cyan interactive scale on light; Components Button matrix shows identical primary light/dark; Brand shows new logos + Chronos note; Surfaces/shell fixtures show cyan active affordances + warm-paper/ink surfaces; light+dark toggle smoke green.

### Task 8: Desktop icon source refresh

- [x] **T8 · Should**
  **Files:** `apps/desktop` icon source assets if they still embed the old N mark.
  **DoD:** desktop icon source uses new timeline mark on Chronos deep; regenerated icons if pipeline requires, or source SVG updated with note for raster regen.

## Roadmap / deferred

| Item | Owner | Trigger |
| --- | --- | --- |
| Runtime Umbra/Aurora switcher evaluation | product-manager | Future VI iteration |
| Full desktop icon marketing set / store assets | frontend-dev | Release packaging if T8 source-only |
| Optional `text-link` utility token in Tailwind preset | frontend-dev | If link migration is error-prone without a named utility |

## PM Task Board

| ID | Owner | Priority | Depends |
| --- | --- | --- | --- |
| T1 | frontend-dev | Must | — |
| T2 | frontend-dev | Must | T1 |
| T3 | frontend-dev | Must | T2 |
| T4 | frontend-dev | Must | — (can start parallel with T1) |
| T5 | frontend-dev | Must | T4 |
| T6 | frontend-dev | Must | T4 |
| T7 | frontend-dev | Must | T3, T5, T6 |
| T8 | frontend-dev | Should | T1 |

**Route:** Medium visual feature · `frontend-dev` (SDD sticky, **implementer: Grok 4.5 / `cursor-grok-4.5-high`**; **reviewers: composer-2.5**) → QC tri (composer-2.5) → `qa-engineer` (mandatory, UI)

**Branch policy:**

| Field | Value |
| --- | --- |
| Working branch | `plan/vi-logo-upgrade` |
| Base | `main` |
| Merge target | `main` |
| Worktree | waived (single-stream) |

**QA gate:** `mandatory` — UI-observable brand + Chronos shell/chrome + Studio galleries.

## Plan self-review (PM)

1. Spec coverage: canvas MiniShell + author Button lock + full DS/Studio gap matrix map to T1–T8.
2. Placeholder scan: no TBD.
3. Type consistency: logo variants `primary|whiteBg|white|mono|text` (no `color`); theme variant ids `elegant|nature|parchment|scifi`; dual-role Ink vs Cyan.

## Review Gate Summary

> Raw reports: `.mstar/sdd/2026-07-22-vi-logo-upgrade/review/qc1.md` … `qc3.md` + `qc-consolidated.md` + `qc2.md` Revalidation. Open residuals in `.mstar/status.json`.

| gate | verdict | date | notes |
|------|---------|------|-------|
| QC1 | **Approve** | 2026-07-22 | 0 Critical / 0 Warning |
| QC2 | **Request Changes** → **Approve** | 2026-07-22 | W-001 connection-setup security notes used cyan `blue-700` washes; fixed `492b5b8e` to deep-ink `brand-deep-blue`; targeted revalidation Approve |
| QC3 | **Approve** | 2026-07-22 | 0 Critical / 0 Warning |
| Consolidated | **Approve** | 2026-07-22 | Post-range logo corrections (primary plate, remove `logo-color`, shell primary-only, wordmark refit) on branch through `492b5b8e` |
| PR security automation | **No findings** | 2026-07-22 | PR #167 — no medium/high/critical |
| PR Bugbot | **Approve** (nits only) | 2026-07-22 | PR #167 @ `342476ea` — R-VI-007…011 registered |

## QA Gate Summary

> Raw report: `.mstar/sdd/2026-07-22-vi-logo-upgrade/review/qa.md`.

| gate | verdict | date | notes |
|------|---------|------|-------|
| QA (mandatory — brand / Chronos / Studio / desktop compose) | **Accept with residuals** | 2026-07-22 | Must DoD themes pass; 216 scoped tests + `icons:compose`; Dock runtime smoke deferred |

**Residuals (open):** R-VI-001…004 (QA, low) + R-VI-007…011 (Bugbot PR #167, nit — stale JSDoc/comments, dead ternary, design-studio logoVariants count). **Closed in compound:** R-VI-005/006 → `.mstar/archived/residuals/2026-07-22-vi-logo-upgrade.json`.

**Plan verdict: Done.** Squash-merged via PR #167 → `main`.
