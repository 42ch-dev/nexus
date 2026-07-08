# UI Guardrails and Class-Merge SSOT Contract

**Status:** Implementation-ready (P1 T1 architecture lock — 2026-07-08)
**Document class:** Iteration-scoped contract
**Coordinates with:** `@42ch/nexus-ui`, `apps/web`, `apps/design-studio`, `@nexus/design-tokens`
**Plan:** `2026-07-08-v1.100-ui-hygiene-guardrails`

## Problem

V1.99 proved the Studio → Package → Web promotion workflow but left three package hygiene residuals:

- `R-V199QC1-S001`: the package `cn` helper is byte-identical to `apps/web` `utils.ts` without a durable drift invariant.
- `R-V199QC1-S002`: Web re-export wrappers do not mechanically prevent reintroducing local `cn` or helper dependencies.
- `R-V199QC3-F001`: `extendTailwindMerge` configuration is duplicated between package and app.

The V1.99 retrospective also identified a manual-only Design Studio forbidden-import check.

The product risk is not just code duplication. Without an enforceable promotion path, each new primitive can reintroduce app-local helpers, unstable wrapper behavior, or Studio fixtures that depend on Web shell details. That would make P2 form fields and later promotions harder to trust.

## Locked Direction

Plan P1 owns both layers:

1. Add guardrails and CI checks so wrapper drift and Design Studio boundary violations are mechanically detected.
2. Consolidate the class-merge configuration behind `@42ch/nexus-ui` as the approved SSOT for V1.100.
3. Leave P2 with a clear wrapper/direct-import rule so `Input`, `Label`, and `Textarea` do not invent a second promotion pattern.

`@nexus/design-tokens` is rejected as the V1.100 class-merge authority because it already imports `@42ch/nexus-ui` theme assets (`workspace:*` in `tooling/design-tokens/package.json`); making `@42ch/nexus-ui` import Tailwind-merge config back from design-tokens would create a package cycle. A smaller shared module is not selected for this iteration because `@42ch/nexus-ui` already owns the promoted presentational primitives and their `cn` behavior, and both consumer apps may depend on it.

If implementation finds that exporting the package `cn` helper creates unacceptable public API or bundling pressure, the fallback is a narrowly scoped residual plus a sync test that proves Web/package class-group parity. Do not move the authority into `@nexus/design-tokens` without first inverting or removing its dependency on `@42ch/nexus-ui`.

## Implementation-Ready Mechanism Decision (T1 locked)

**Mechanism: Shell-script guardrails in `tooling/` + CI job.** This matches the existing repository pattern:

| Existing gate | Mechanism | Wired in CI |
|---------------|-----------|-------------|
| `verify-no-acp-in-daemon` | `rg` one-liner in ci.yml job step | Yes |
| `check-schema-drift.sh` | `tooling/*.sh` invoked by ci.yml job | Yes (`schema-consistency-check`) |

**Rationale (over alternatives considered):**

- **Shell scripts with `rg`/`grep`**: Zero new tooling dependencies, fast (<1s), works in CI and locally. Proven pattern in this repo (`check-schema-drift.sh` is 112 lines of grep-based invariants). CI job syntax is straightforward.
- **Scoped ESLint rule**: Requires `eslint-plugin-import` + custom rule config, per-package `.eslintrc` coordination, and would need a separate `pnpm` install step in CI. Overkill for a targeted set of 3–5 import-pattern invariants. Not justified for this scope (see Non-Goals: no broad ESLint rollout).
- **Vitest package/app tests**: Useful as a *secondary* parity check for the `cn` class-group sync (see cn-parity test below), but tests alone don't catch new violations on unmodified wrapper files. Shell scripts catch *unintended additions* without requiring a matching test file for every guarded file.

**Recommended CI job structure** (for T2 to implement):

```yaml
ui-guardrails:
  name: UI guardrails — wrapper drift & Studio boundaries
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Check promoted-wrapper forbidden imports
      run: bash tooling/check-ui-guardrails.sh
```

**Representative failure fixtures** (these must cause the guardrail to exit 1):

1. **Promoted wrapper re-adds local `cn`**: Insert `import { cn } from '@/lib/utils';` into `apps/web/src/components/ui/button.tsx`. The guardrail must fail.
2. **Design Studio imports web page**: Insert `import { DashboardPage } from '../web/src/pages/Dashboard';` into any file under `apps/design-studio/src/`. The guardrail must fail.

## Guardrail Contract — Forbidden Import Lists

### Promoted-wrapper forbidden imports

Files under `apps/web/src/components/ui/` that re-export from `@42ch/nexus-ui` (currently: `button.tsx`, `badge.tsx`, `card.tsx`; P2 will add `input.tsx`, `label.tsx`, `textarea.tsx`) **MUST NOT** import:

| Forbidden pattern | Example | Why |
|-------------------|---------|-----|
| `clsx` (direct) | `import { clsx } from 'clsx'` | Duplicates package `cn` dependency; wrapper must use the package authority |
| `class-variance-authority` (direct) | `import { cva } from 'class-variance-authority'` | Variants live in the package primitive |
| `tailwind-merge` (direct) | `import { twMerge } from 'tailwind-merge'` | Merge config is the package SSOT |
| `@/lib/utils` or `@/lib/*` | `import { cn } from '@/lib/utils'` | App-local helper; wrapper must not recreate merge logic |
| `../lib/utils` | `import { cn } from '../lib/utils'` | Same as above (relative path variant) |
| `@42ch/nexus-ui/src/*` | `import { cn } from '@42ch/nexus-ui/src/lib/cn'` | Deep import — must use public API only |

**Allowed in promoted wrappers**: `export { … } from '@42ch/nexus-ui'` (the re-export), plus `import type` for TypeScript re-export signatures, and app-specific behavior gate wrappers. Promoted wrappers are thin; they must not add `clsx`, `tailwind-merge`, or `cn` calls.

### Design Studio forbidden imports

Files under `apps/design-studio/src/**` **MUST NOT** import:

| Forbidden pattern | Example | Why |
|-------------------|---------|-----|
| `../web/src/pages/**` | `import { … } from '../../../web/src/pages/…'` | Product screens; studio is a gallery, not Control Room |
| `../web/src/components/layout/**` | `import { AppShell } from …` | Web layout shells; studio uses its own `Surfaces` fixtures |
| `../web/src/lib/nexus/**` | `import { NexusClient } from …` | Daemon transport/client; studio has no daemon |
| `../web/src/hooks/**` | `import { useNexusQuery } from …` | Product hooks |
| `../web/src/providers/**` or `../web/src/contexts/**` | `import { ThemeProvider } from …` (web's, not studio's own) | App providers |
| `@42ch/nexus-contracts` | `import { … } from '@42ch/nexus-contracts'` | Wire DTOs; studio is not an ACP consumer |
| `@42ch/nexus-ui/src/*` | `import { cn } from '@42ch/nexus-ui/src/lib/cn'` | Deep import — use public package API |
| `@tauri-apps/*` | `import { invoke } from '@tauri-apps/api'` | Desktop-only; studio is a browser SPA |

**Allowed in Design Studio**:
- `@42ch/nexus-ui` (public exports) — promoted primitives (Badge, Button, Card, and future Input/Label/Textarea)
- `@nexus/design-tokens` — shared CSS + Tailwind preset
- `@web-ui/*` — transitional, only for unpromoted presentational primitives with inline annotation
- `@web-lib/utils` — `cn()` only, transitional until T3 SSOT consolidation
- `@/*` — studio-local routes, fixtures, gallery layout

### Design Studio transitional `@web-ui/*` policy (locked)

After P1, `@web-ui/*` imports in `apps/design-studio/src/**` are **allowed only** when:

1. The imported primitive is **not yet promoted** to `@42ch/nexus-ui` (i.e., not one of: Button, Badge, Card; future: Input, Label, Textarea).
2. Every `@web-ui/*` import carries an **inline transitional annotation** identifying the blocking criteria for promotion. Format: `// @web-ui/<name> — transitional <until/because> <criteria>`.

**Current unpromoted primitives and their transitional rationale** (as of P1 T1 file audit):

| Import | Annotation | Promotion trigger |
|--------|------------|-------------------|
| `@web-ui/dialog` | `keep-web` | Radix portal/focus-trap beyond presentational scope; keep in web |
| `@web-ui/input` | `transitional until Form Field slice` | P2 promotes Input as part of form-field contract |
| `@web-ui/label` | `transitional until Form Field slice` | P2 promotes Label as part of form-field contract |
| `@web-ui/select` | `keep-web` | Native select wrapper; no cross-app demand proven yet |
| `@web-ui/states` | `keep-web` | lucide-react asset boundary; product copy & app-composition callbacks |
| `@web-ui/table` | `keep-web` | Data-aware table; not purely presentational |
| `@web-ui/tabs` | `keep-web` | Compound component owns selection state; not purely presentational |
| `@web-ui/textarea` | `transitional until Form Field slice` | P2 promotes Textarea as part of form-field contract |

The guardrail must **not** flag legitimately annotated `@web-ui/*` imports for unpromoted primitives. It must **flag** missing annotations, mistaken `@web-ui/*` imports of already-promoted primitives (e.g., `@web-ui/button`), and any `@web-ui/*` import not in the transitional-annotation format.

## SSOT Contract — cn Authority & Class-Merge Consolidation

### Authority

The class-merge token group list has one V1.100 authority: **`@42ch/nexus-ui/src/lib/cn.ts`**. This file owns:

1. The `clsx` + `tailwind-merge` composition (`customTwMerge` with DESIGN.md `font-size` class-group extension).
2. The `cn()` function signature (`(...inputs: ClassValue[]): string`).
3. The `extendTailwindMerge` configuration — the registry of custom text-token class groups.

### Consolidation strategy (T3 implements)

**Preferred path:** Export `cn` as a public API from `@42ch/nexus-ui`:

- Add `export { cn } from './lib/cn';` to `packages/nexus-ui/src/index.ts`.
- Add a `"./cn"` conditional export to `package.json` `exports` (or export directly from the root `"."` entry point).
- `apps/web/src/lib/utils.ts` becomes a thin re-export: `export { cn } from '@42ch/nexus-ui';` — preserving the `@/lib/utils` import for call-site compatibility.
- Design Studio transitions from `@web-lib/utils` to `@42ch/nexus-ui` for `cn`.

**Fallback** (if public `cn` export creates unacceptable bundling pressure):
- `cn` stays internal to the package. T2 writes a Vitest sync test under `packages/nexus-ui/__tests__/` that:
  1. Reads `packages/nexus-ui/src/lib/cn.ts` and `apps/web/src/lib/utils.ts`.
  2. Asserts byte-identical `extendTailwindMerge` class-group arrays.
  3. Asserts identical `cn` function body structure.
- A named residual records the drift risk and test location.
- Web and Studio keep their transitional `cn` import paths.

**Hard invariant:** The `extendTailwindMerge` class-group extension list must exist in exactly one place — `packages/nexus-ui/src/lib/cn.ts`. No second copy in Web, Studio, or design-tokens.

### cn-parity test (T2 writes regardless of export path)

A shell-script check in `tooling/check-ui-guardrails.sh` (or a sibling script) verifies byte-level parity between the package `cn.ts` and `apps/web/src/lib/utils.ts`. If the files differ, the check fails with a diff. This closes `R-V199QC1-S001`.

### Class-group extension list (frozen)

```typescript
'font-size': [
  'text-heading-32', 'text-heading-24', 'text-heading-20', 'text-heading-16',
  'text-label-14', 'text-label-12',
  'text-copy-16', 'text-copy-14', 'text-copy-13',
  'text-button-14', 'text-button-12',
  'text-label-12-mono', 'text-copy-13-mono',
]
```

Any new DESIGN.md `fontSize` token must be added here. The guardrail parity check ensures both copies stay synchronized.

## P2 Wrapper/Direct-Import Rule (locked)

This rule is **locked** by T1 so P2 cannot reopen the architecture decision:

1. **Web keeps thin re-export wrappers** under `apps/web/src/components/ui/` for every promoted primitive. Format: `export { Component, type ComponentProps } from '@42ch/nexus-ui';`. The wrappers exist to avoid call-site churn in `apps/web` — screens import from `@/components/ui` and don't need to know which primitives are promoted.
2. **Design Studio imports promoted primitives directly** from `@42ch/nexus-ui`. No `@web-ui/*` alias for promoted primitives — once promoted, the alias path is retired for that primitive.
3. **P2 Input/Label/Textarea** follow the exact same pattern as P1 Button/Badge/Card:
   - Promote presentational implementation to `packages/nexus-ui/src/components/`.
   - Create thin re-export wrappers in `apps/web/src/components/ui/`.
   - Design Studio switches from `@web-ui/input` → `@42ch/nexus-ui`.
4. **Promotion checklist** (for any future promotion task):
   - Move the component source + tests to `packages/nexus-ui/src/components/<name>.tsx`.
   - Add `export { Component, type ComponentProps }` to `packages/nexus-ui/src/index.ts`.
   - Replace web `apps/web/src/components/ui/<name>.tsx` with a thin re-export from `@42ch/nexus-ui`.
   - Update Design Studio imports from `@web-ui/<name>` to `@42ch/nexus-ui`.
   - Remove the transitional annotation comment in Studio.
   - Verify the guardrail passes (shell script + CI).
   - Update the forbidden-import lists and transitional table in this spec.

## Acceptance Hooks

- The three V1.99 hygiene residuals (`R-V199QC1-S001`, `R-V199QC1-S002`, `R-V199QC3-F001`) are closed or explicitly re-scoped with a named residual and dependency-cycle rationale.
- A representative wrapper violation (adding `import { cn } from '@/lib/utils'` to `button.tsx`) and a representative Design Studio forbidden import (importing a web page) are caught by `tooling/check-ui-guardrails.sh` with exit code 1.
- CI job `ui-guardrails` runs on PRs to `main` and fails on violations.
- The chosen class-merge authority is `@42ch/nexus-ui/src/lib/cn.ts`, and Web/Studio consumers either import/re-export it or carry a named temporary parity residual with a diff/sync test.
- P2 can cite the approved wrapper/direct-import strategy without reopening the architecture decision.
- `cn.ts` ↔ `utils.ts` byte-parity is checked mechanically (shell diff or test) — not by reviewer instruction.

## Non-Goals

- No new visual design tokens unless SSOT consolidation requires moving existing token names.
- No broad ESLint rollout unless scoped and justified.
- No component promotion work; P2 owns Input/Label/Textarea.
- No removal of all `@web-ui/*` aliases; unpromoted primitives may remain transitional with annotations.
- No changes to `apps/design-studio/vite.config.ts` alias resolution in P1 (aliases stay; guardrails enforce policy on import *content*, not alias configuration).

## Appendix A — Existing Gate Patterns (for T2 reference)

| File | Mechanism | Lines | Purpose |
|------|-----------|-------|---------|
| `.github/workflows/ci.yml` → `verify-no-acp-in-daemon` | Inline `rg` step | 10 lines | Prevent daemon linking ACP SDK |
| `tooling/check-schema-drift.sh` | `grep` + `rg` assertions | 112 lines | Schema version ownership & DDL drift |
| `tooling/check-wire-drift.sh` | `cargo test` wrapper | ~15 lines | Wire schema ↔ Rust struct parity |

T2 should create `tooling/check-ui-guardrails.sh` following the same `set -eu` + descriptive echo + `exit 1` on violation pattern.

## Appendix B — Residuals Addressed

| Residual | Severity | How P1 closes it | Owning task |
|----------|----------|------------------|-------------|
| `R-V199QC1-S001` | low | cn-parity diff check in `check-ui-guardrails.sh` or Vitest sync test | T2 |
| `R-V199QC1-S002` | low | Promoted-wrapper forbidden-import guardrail catches local `cn`/`clsx`/`tw-merge` | T2 |
| `R-V199QC3-F001` | low | T3 consolidates `extendTailwindMerge` behind `@42ch/nexus-ui`; T2 cn-parity check ensures no divergence until consolidation | T2 (parity) + T3 (consolidation) |
