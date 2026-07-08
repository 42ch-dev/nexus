# DESIGN Unification — Merge Specification (V1.98)

**Status**: Normative (P-1 Prepare)  
**Owner**: `@architect`  
**Consumers**: P0 plan T1 (`apps/web` migration), `apps/design-studio`, `specs/design-studio.md`  
**Iteration compass**: [v1.98-design-studio-and-design-unification-compass-v1.md](../../v1.98-design-studio-and-design-unification-compass-v1.md)  
**Wire contracts**: `wire_contracts_changed: false` — documentation + frontend paths only

---

## 1. Architecture hierarchy (post-V1.98)

```
repo-root DESIGN.md + DESIGN.dark.md     ← sole normative SSOT (YAML + body)
        │
        ├── @42ch/nexus-ui               ← brand package only (logos, theme.css, brandColors)
        │
        ├── tooling/design-tokens        ← shared Tailwind preset + CSS var layers (P0 extract)
        │       │
        │       ├── apps/web             ← author product consumer (index.css imports tokens.css)
        │       └── apps/design-studio   ← dev gallery consumer (same CSS pipeline)
        │
        └── apps/web/src/components/ui/*  ← shadcn primitives (NOT in nexus-ui; studio imports transitively)
```

**Invariants:**

- Exactly **one** DESIGN pair on disk after P0 T1: `DESIGN.md` + `DESIGN.dark.md` at repo root.
- **No** `apps/web/DESIGN.md` or `apps/web/DESIGN.dark.md`.
- `nexus-ui` stays **brand-only** — no shadcn primitive migration.
- Studio is a **consumer** — no daemon, no `NexusClient`, no wire types.

---

## 2. Merge objective

Fold `apps/web/DESIGN*.md` **Production completeness** (frontmatter + body) into the root pair so:

1. Token **names** stay frozen (zero Tailwind class churn where values are unchanged).
2. Shipped `apps/web` **visual parity** is preserved for neutrals and accent scales already projected in `index.css`.
3. Root-only brand extensions (`brand-deep-blue-800` … `brand-cyan-alpha-200`) are retained in the unified SSOT.
4. All web-only domains (canvas, setup wizard, SOUL, memory, findings, reading chrome) live in root frontmatter + body sections.

**P-1 does not perform the file merge** — this spec is the implementor checklist for P0 T1.

---

## 3. Merge precedence (B1)

When root and `apps/web` frontmatter disagree, apply the rule for that **key category**. Document intentional value changes in the P0 PR description.

### 3.1 Scalar color tokens

| Key category | Keys (examples) | Winner | Notes |
| --- | --- | --- | --- |
| Brand VI core | `brand-deep-blue`, `brand-cyan`, `brand-white` | **Root** | Frozen VI; values already match |
| Brand extended | `brand-deep-blue-800` … `brand-cyan-alpha-200` | **Root** | Absent from apps/web; add to unified SSOT |
| Neutral surfaces | `background-*`, `gray-*`, `gray-alpha-*` | **apps/web** | Matches shipped `index.css`; preserves PS-7 parity |
| Interactive blue scale | `blue-700` … `blue-1000` | **apps/web** | Light theme maps to `brand-deep-blue` steps; CSS vars already wired |
| Semantic accent scales | `red/amber/green/teal/purple/pink-700` … `-1000` | **apps/web** | Four-step scales used by tailwind + badges |
| Root-only semantic | `red-700`, `red-800` (no `-900`) in root | **Union** | Keep web four-step; drop duplicate root-only pair if identical at 700/800 |

**Post-merge alias policy:** `blue-700` remains the **web primary interactive** token name in light theme (do **not** rename to `brand-deep-blue` in CSS vars — avoids sweeping `index.css` churn). Document in root body § Implementation Mapping that `blue-*` aliases brand-deep-blue in light and `brand-cyan` in dark.

### 3.2 Non-color frontmatter

| Key category | Winner | Notes |
| --- | --- | --- |
| `typography.*` (standard scale) | **Either** | Identical today |
| `typography.reading-prose-*` | **apps/web** | Author Reflection; theme-independent |
| `spacing.*`, `rounded.*` | **Either** | Identical today |
| `components.button` (+ sizes, disabled) | **apps/web** | Superset (tertiary, destructive, sizes) |
| `components.focus-ring` | **Root** if conflict; else **apps/web** | Two-layer ring spec |
| `components.input-select-textarea`, `card`, `dialog`, … | **apps/web** | Production component tables |
| `components.connection-setup`, `shell-nav`, `logo` | **Union** | Root has connection-setup; web has setup-wizard/footer-profile — keep all |
| Canvas / SOUL / memory / findings / reading-chrome tokens | **apps/web** | Only in web copy today |
| `version`, `name`, `description` | **Root** metadata, amended | Bump `version` to `0.3.0`; `name` → unified title; `description` mentions web + studio consumers |

### 3.3 Body markdown sections

| Section | Source | Target in root `DESIGN.md` |
| --- | --- | --- |
| Brand Colors, Neutrals, Typography, Spacing, Elevation, Motion, Focus, Logo | Root (keep) | Merge contrast tables; add web-only notes where needed |
| Voice & Content | **apps/web** | Append after Logo Usage |
| Component Primitives (Button, Input, …) | **apps/web** | New § after Voice |
| Background-driven contrast invariant | **apps/web** (expanded) | Replaces/supplements root short note |
| Canvas Surface, World KB, Outline/Timeline tokens | **apps/web** | Dedicated §§ |
| Setup Wizard (V1.95–V1.96), Footer Profile | **apps/web** | Dedicated §§ |
| SOUL, Memory, Findings, Reading chrome | **apps/web** | Dedicated §§ |
| Implementation Mapping (CSS vars, Tailwind, theme toggle) | **apps/web** | § Implementation Mapping — update paths to root |
| Web consumption mapping preamble | **apps/web** | **Delete** — root file is no longer a "mapping" |

`DESIGN.dark.md` body: keep root dark brand rules + apps/web dark-specific notes (blue→cyan mapping) in Implementation Mapping; duplicate rule-type prose from light where web had "see DESIGN.md".

### 3.4 Dark theme frontmatter

Same category rules as §3.1–3.2. Additional rule:

| Rule | Detail |
| --- | --- |
| `blue-700` in dark | **apps/web** value (`#25D1E0` / brand-cyan interactive scale) |
| Structural tokens | Identical names in both files; color-dependent values differ per theme |

---

## 4. Section-by-section content map (T4 implementor guide)

### 4.1 `DESIGN.md` (light)

| Block | Action |
| --- | --- |
| YAML `colors` | Start from **apps/web** neutrals + accents; **add** root `brand-*` extended steps; apply §3.1 table |
| YAML `typography` | apps/web superset (includes `reading-prose-*`) |
| YAML `spacing`, `rounded` | Copy either (identical) |
| YAML `components` | **Union** apps/web + root-only keys (`connection-setup`, etc.) |
| Body § Brand–Logo | Root base + web contrast invariant paragraphs |
| Body § Voice & Content | Copy from apps/web |
| Body § Component Primitives | Copy from apps/web |
| Body § Canvas / World KB / Outline | Copy from apps/web |
| Body § Setup / Footer / SOUL / Memory / Findings / Reading | Copy from apps/web |
| Body § Implementation Mapping | Copy from apps/web; replace path refs `apps/web/DESIGN.md` → `DESIGN.md` |

### 4.2 `DESIGN.dark.md` (dark)

| Block | Action |
| --- | --- |
| YAML frontmatter | apps/web `DESIGN.dark.md` frontmatter + root dark brand extensions |
| Body | Root dark brand prose + apps/web dark Implementation Mapping notes; cross-ref light body for shared rules |

---

## 5. Consumer update checklist

Execute in P0 T1 **before** deleting apps/web copies.

### 5.1 `apps/web`

- [ ] `src/index.css` — update header comments: SSOT = repo-root `DESIGN.md` / `DESIGN.dark.md` (values unchanged in parity pass unless §3.1 documents a fix)
- [ ] `tailwind.config.ts` — update comments; point to root SSOT; adopt shared preset (§6)
- [ ] `src/components/ui/index.ts` — **add** `export * from './tabs'` (B4 barrel gap)
- [ ] `AGENTS.md` — single SSOT path (root only); remove "Web consumption mapping" wording
- [ ] `README.md` — design token link → `../../DESIGN.md`
- [ ] Grep repo for `apps/web/DESIGN` and fix references
- [ ] `theme-provider.tsx` / tests — comment-only path updates if needed

### 5.2 `apps/design-studio` (P0 T2+)

- [ ] Import shared CSS pipeline (§6)
- [ ] Import ui primitives via `@web-ui/*` alias (§7)
- [ ] No imports from `apps/web/src/pages`, `components/layout`, `lib/nexus`

### 5.3 Docs / harness

- [ ] `specs/web-ui.md` §30 — DESIGN SSOT links confirmed (writing-specialist §5.3)
- [ ] `packages/nexus-ui` README — root DESIGN paths only (if cited)

### 5.4 Verification

- [ ] `pnpm --filter web test` + `pnpm --filter web run build`
- [ ] `pnpm --filter design-studio test` + build (after scaffold)
- [ ] File audit: `git ls-files '**/DESIGN*.md'` → exactly root pair
- [ ] Studio + web side-by-side spot-check (Tokens + Button, both themes)

---

## 6. CSS pipeline decision (B2)

**Decision: shared `tooling/design-tokens` workspace package** — not duplicate Tailwind configs, not a second manual `index.css` transcription.

| Artifact | Location | Owner |
| --- | --- | --- |
| Tailwind preset | `tooling/design-tokens/tailwind.preset.ts` | Extracted from `apps/web/tailwind.config.ts` `theme.extend` |
| CSS variable layers | `tooling/design-tokens/src/tokens.css` | Extracted from `apps/web/src/index.css` `:root` + `.dark` blocks |
| Brand import | top of `tokens.css` | `@import '@42ch/nexus-ui/theme.css'` (unchanged) |

**Consumer pattern:**

```css
/* apps/web/src/index.css and apps/design-studio/src/index.css */
@import '@nexus/design-tokens/tokens.css';
@tailwind base;
@tailwind components;
@tailwind utilities;
/* app-specific @layer rules only (focus ring utilities, reading prose, etc.) */
```

```ts
// apps/*/tailwind.config.ts
import preset from '@nexus/design-tokens/tailwind.preset';
export default { presets: [preset], content: [...] };
```

**Package name:** `@nexus/design-tokens` (private, `tooling/design-tokens/package.json`, `workspace:*`).

**Why not duplicate:** two Tailwind configs guaranteed drift on the next canvas token iteration.

**Why not studio `@import` web `index.css`:** pulls web-specific `@layer` utilities and couples gallery to product app stylesheet.

**P0 sequencing:** T1 may land preset extraction with merge; T2 scaffold consumes preset from day one.

**Transitional debt:** until extraction lands, design-studio may temporarily re-export `tokens.css` via relative import — remove before iteration close.

---

## 7. Import strategy (B3, B4)

### 7.1 UI primitives

**Decision: Vite + TypeScript path alias `@web-ui/*` → `apps/web/src/components/ui/*`**

| Import | Allowed |
| --- | --- |
| `@web-ui/button`, `@web-ui/dialog`, … | Yes — gallery matrices |
| `@web-ui` barrel (`index.ts`) | Yes **after** tabs export added (B4) |
| `@web-lib/utils` → `apps/web/src/lib/utils.ts` | Yes — `cn()` only |
| `@web-lib/*` (other) | **No** |
| `apps/web/src/pages/**` | **No** |
| `apps/web/src/lib/nexus/**` | **No** |
| `apps/web/src/components/layout/**` | **No** — Surfaces slice uses **studio-local fixtures** that compose `@web-ui/*` |

**`apps/design-studio/vite.config.ts`:**

```ts
resolve: {
  alias: {
    '@': path.resolve(__dirname, './src'),
    '@web-ui': path.resolve(__dirname, '../web/src/components/ui'),
    '@web-lib/utils': path.resolve(__dirname, '../web/src/lib/utils.ts'),
  },
},
```

Mirror in `tsconfig.json` `paths`.

**Peer dependencies:** `design-studio/package.json` declares the same Radix/CVA versions as `apps/web` for `dialog`, `select`, `slot` — studio bundles its own `node_modules`, not web's.

### 7.2 tabs.tsx barrel gap (B4)

- **File exists:** `apps/web/src/components/ui/tabs.tsx`
- **Barrel gap:** missing from `index.ts`
- **Fix owner:** P0 T1 (web migration) — add `export * from './tabs'`
- **Studio rule:** may import `@web-ui/tabs` directly until barrel lands; prefer barrel after T1

### 7.3 nexus-ui brand imports

```ts
import { NexusLogo, NexusMark } from '@42ch/nexus-ui';
import '@42ch/nexus-ui/theme.css'; // also pulled via tokens.css
```

---

## 8. Deletion steps (apps/web DESIGN copies)

Execute **only after** §5.1 checklist passes and merged root pair is committed.

1. Confirm `git diff` shows root `DESIGN.md` / `DESIGN.dark.md` contain all keys from apps/web frontmatter (scripted YAML key diff or manual audit).
2. Confirm `apps/web` builds against root paths.
3. `git rm apps/web/DESIGN.md apps/web/DESIGN.dark.md`
4. Re-run grep for `apps/web/DESIGN` — must be zero (except archived/historical docs if any).
5. Update any CI/doc link that pointed at deleted paths.

---

## 9. Intentional drift register (P0 PR)

Record any value that **changes** from pre-merge shipped web UI:

| Token | Pre-merge (apps/web) | Post-merge (root) | Reason |
| --- | --- | --- | --- |
| *(empty at spec freeze)* | — | — | Populate during P0 merge audit if §3.1 forces root refined neutrals |

**Default:** prefer apps/web neutrals → register should stay empty for V1.98 unless QA finds contrast fixes.

---

## 10. Downstream notes (P-1 close)

| Item | Owner | Status |
| --- | --- | --- |
| Studio route copy / Voice samples in IA guide §4.4 | `@writing-specialist` | **Done** — fixture strings from DESIGN § Voice & Content + shipped setup copy |
| Surfaces fixture copy in IA guide §4.5 | `@writing-specialist` | **Done** — Setup + shell chrome placeholder strings |
| `tooling/design-tokens` README | P0 `@frontend-dev` | Pending — dev commands only; no new knowledge spec |
| Barrel `tabs` export | P0 T1 `@fullstack-dev` | Pending — one line |
