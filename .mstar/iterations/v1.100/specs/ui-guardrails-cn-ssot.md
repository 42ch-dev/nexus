# UI Guardrails and Class-Merge SSOT Contract

**Status:** Draft for V1.100 review chain
**Document class:** Iteration-scoped contract
**Coordinates with:** `@42ch/nexus-ui`, `apps/web`, `apps/design-studio`, `@nexus/design-tokens`

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

`@nexus/design-tokens` is rejected as the V1.100 class-merge authority because it already imports `@42ch/nexus-ui` theme assets; making `@42ch/nexus-ui` import Tailwind-merge config back from design-tokens would create a package cycle. A smaller shared module is not selected for this iteration because `@42ch/nexus-ui` already owns the promoted presentational primitives and their `cn` behavior, and both consumer apps may depend on it.

If implementation finds that exporting the package `cn` helper creates unacceptable public API or bundling pressure, the fallback is a narrowly scoped residual plus a sync test that proves Web/package class-group parity. Do not move the authority into `@nexus/design-tokens` without first inverting or removing its dependency on `@42ch/nexus-ui`.

## Guardrail Contract

- Web wrappers for promoted primitives must remain thin re-exports unless the plan documents a package-boundary exception.
- Guardrails must catch imports of app-local `cn`, `clsx`, `tailwind-merge`, or other local helper paths inside promoted wrapper files. Promoted wrappers may import the public primitive and, if needed, the public `cn` authority from `@42ch/nexus-ui`; they must not recreate merge configuration locally.
- Design Studio must not import Web pages, Web layout shells, daemon clients, contracts, app providers, product hooks, Tauri helpers, or promoted primitives through transitional `@web-ui/*` aliases. After P1, Design Studio may keep `@web-ui/*` only for unpromoted presentational primitives with an inline transitional annotation.
- Checks should run in CI or an equivalent repository gate, not only as reviewer instructions.
- The guardrail list must give future promotion tasks the files, import patterns, and update steps needed to add a new primitive without reverse-engineering P1 implementation details.

## SSOT Contract

The class-merge token group list has one V1.100 authority: `@42ch/nexus-ui`. The package owns the `extendTailwindMerge` configuration used by promoted primitives. `apps/web/src/lib/utils.ts` and Design Studio utility usage should either import/re-export the package authority or keep a guarded temporary parity test until the import migration is safe.

The chosen path must keep React-facing package code bundler-safe, avoid importing app code into packages, and avoid making `@nexus/design-tokens` a runtime dependency of `@42ch/nexus-ui`. Public API exposure for `cn` is allowed only as a documented package-level utility export; deep imports from `@42ch/nexus-ui/src/*` remain forbidden.

## Acceptance Hooks

- The three V1.99 hygiene residuals are closed or explicitly re-scoped with a named residual and dependency-cycle rationale.
- A representative wrapper violation and a representative Design Studio forbidden import are caught by the new gate or by a test that proves the gate behavior.
- The chosen class-merge authority is `@42ch/nexus-ui`, and Web/Studio consumers either import/re-export it or carry a named temporary parity residual with tests.
- P2 can cite the approved wrapper/direct-import strategy without reopening the architecture decision.

## Non-Goals

- No new visual design tokens unless SSOT consolidation requires moving existing token names.
- No broad ESLint rollout unless scoped and justified.
- No component promotion work; P2 owns Input/Label/Textarea.
- No removal of all `@web-ui/*` aliases; unpromoted primitives may remain transitional with annotations.
