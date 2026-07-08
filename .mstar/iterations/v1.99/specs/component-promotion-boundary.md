# Component Promotion Boundary (V1.99 Draft)

**Status**: Draft (iteration-scoped)  
**Owner**: product-manager + architect during Phase 1 Review & Edit  
**Consumers**: V1.99 P0 component promotion plan, `packages/nexus-ui/AGENTS.md`, `apps/design-studio/AGENTS.md`, `apps/web/AGENTS.md`

## Problem

V1.98 deliberately kept `@42ch/nexus-ui` brand-only while Design Studio imported `apps/web/src/components/ui/*` through `@web-ui/*`. That was safe for the first gallery, but it leaves reusable UI primitives owned by an app package and makes Design Studio depend on `apps/web` internals.

V1.99 reopens the boundary for a focused first batch of reusable UI primitives.

## Product Outcome

The package boundary should make future UI work easier without hiding app behavior inside a shared package. A promoted primitive is successful when Design Studio and Web can share its visual contract, while Web still owns data, routing, daemon integration, and product-specific composition.

This is a correction to V1.98's intentionally conservative boundary, not a reversal of the underlying rule: `@42ch/nexus-ui` must remain a reusable presentational package rather than a second app layer.

## Promotion Rules

A component may move to `@42ch/nexus-ui` only if all are true:

- It is pure presentational React and can render without daemon state, routing, `NexusClient`, localStorage, Tauri IPC, or app providers.
- It consumes shared design tokens through classes/CSS variables, not raw one-off values.
- It does not import from `apps/web`, `apps/design-studio`, or app-local aliases.
- It keeps React as peer dependency and remains compatible with the package's tsup/esbuild build.
- It has package-level tests that cover variants and accessibility-relevant class output or behavior.
- Consumers can import it through the public package export map, not internal source paths.

## Package Architecture Rules

These constraints are the implementation contract for V1.99 P0:

- **Package role:** `@42ch/nexus-ui` is a presentational React package. It may own primitive view components and brand assets; it must not become a form framework, app shell, routing layer, or daemon adapter.
- **Export map:** first-batch primitives should be named exports from `@42ch/nexus-ui` via `src/index.ts`. Do not add per-component deep public exports in V1.99 unless P-1/P0 explicitly locks them; named root exports keep the public API small while the package is pre-1.0.
- **Build entries:** keep tsup entries boring: `src/index.ts` and `src/tokens.ts` unless a new documented public subpath is required. Component files stay internal implementation details.
- **Runtime dependencies:** React and React DOM stay peer dependencies. Non-singleton implementation helpers such as `class-variance-authority`, `@radix-ui/react-slot`, `clsx`, and `tailwind-merge` may be package runtime dependencies only when the promoted primitive actually imports them. Do not make those peers unless the consumer must share an instance.
- **Class composition:** package code must own its own `cn` helper or equivalent local composition utility. It must not import `@web-lib/utils` or `apps/web/src/lib/utils.ts`.
- **Token consumption:** component class strings must reference tokens exposed by `@nexus/design-tokens` / root DESIGN, but the component package must not duplicate the token tables or require consumers to import package-owned component CSS beyond the existing brand `theme.css`.
- **Asset rules:** no `.svg`, `.png`, or app asset imports from component source. Asset-using components must keep the existing bundler-agnostic `src` prop or inline JSX pattern.
- **Consumer wrappers:** `apps/web/src/components/ui/*` may temporarily become thin re-export/wrapper files to avoid large call-site churn. Wrappers may add app behavior, labels, or data wiring only in `apps/web`; those additions must not move back into the package.

## Exclusions

Do not promote:

- Product screens, layout shells, sidebar/app chrome, page routes, or data-aware components.
- Components that require `NexusClient`, TanStack Query, React Router, Tauri commands, or daemon URLs.
- Components whose only reuse is a single one-off surface fixture.
- Components that import SVG/PNG assets directly from package source; asset-consuming components must follow the existing bundler-agnostic `src`-prop pattern.

## First-Batch Candidate Classes

The Phase 1 Review & Edit chain should validate the final list, but the initial candidate set is:

| Candidate | Rationale | Risk |
| --- | --- | --- |
| Button | Token-driven, broadly reused, existing tests can encode contrast invariants | Variant API becomes public package API |
| Badge | Pure status pill with semantic variants | Variant naming may be app-domain flavored |
| Card | Simple surface primitive needed by studio and web | Easy to overgrow into layout abstraction |
| Input / Label / Textarea | Presentational form primitives with shared token mapping | Need careful accessibility contract |

### Draft Architect Recommendation

Default the first batch to the smallest set that proves the loop:

- **Promote by default:** `Button`, `Badge`, and `Card`, if their APIs can remain visual/presentational and package dependencies are made explicit.
- **Defer by default:** `Input`, `Label`, and `Textarea`. They are technically pure enough to move, but promoting them alone would lock an incomplete form-field contract before helper text, error text, required/optional copy, and label/control association patterns are reviewed together.
- **Keep out of package:** any component whose main value comes from app copy, daemon status, route state, setup progression, or shell layout.

The final architect/product lock may narrow this list. Expanding it requires a clear cross-consumer use case, not just "this primitive exists in `apps/web`." The recommended revisit trigger for `Input` / `Label` / `Textarea` is a later Form Field slice that proves at least two Web consumers and one Studio fixture need the same accessible field composition.

## Consumer Pattern

- `packages/nexus-ui` owns promoted primitive implementation and exports it publicly.
- `apps/web` imports promoted primitives from `@42ch/nexus-ui`; app-specific wrappers stay in `apps/web` only when behavior/state is needed.
- `apps/design-studio` imports promoted primitives from `@42ch/nexus-ui`; remaining transitional primitives may still use `@web-ui/*` until later iterations.
- `tooling/design-tokens` remains the shared Tailwind/CSS pipeline; `@42ch/nexus-ui` must not duplicate the full root DESIGN contract.
- New Studio `/surfaces` compositions may use promoted primitives, but shell/setup fixture structure remains studio-local until Web integration proves reusable behavior.

## Open Decisions For Review Chain

- Does product/writing review accept deferring `Input` / `Label` / `Textarea` to a future Form Field slice?
- Should `@42ch/nexus-ui` expose style helpers such as `buttonVariants` and `badgeVariants`, or keep variant functions internal unless consumers already import them?
- Does package naming remain `@42ch/nexus-ui`, or should component primitives eventually split from brand assets?
- Which existing `packages/nexus-ui/AGENTS.md` prohibitions must be amended versus retained?

## Non-Promotion Record

For every reviewed component that does not move, record one of:

- `keep-web`: app-specific behavior, route/data coupling, or reuse only inside Web.
- `keep-studio`: one-off fixture or visual exploration with no reusable product contract.
- `defer`: likely reusable, but API, token, test, or accessibility contract is not stable enough for V1.99.

Deferred entries must name the owner/trigger for revisiting; otherwise they should be treated as intentionally out of scope.

## Acceptance Hooks

- Package boundary update is reflected in `packages/nexus-ui/AGENTS.md` and README.
- `apps/design-studio` no longer imports promoted first-batch components through `@web-ui/*`.
- `apps/web` uses the promoted first-batch primitives where parity is low risk.
- Package build/typecheck/test and both consumer tests/builds remain green.
