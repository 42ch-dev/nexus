# Studio-First UI Workflow (V1.99 Draft)

**Status**: Draft (iteration-scoped guide)  
**Owner**: product-manager + architect during Phase 1 Review & Edit  
**Consumers**: V1.99 plans, `apps/design-studio`, `apps/web`, `packages/nexus-ui`, possible iteration-close knowledge

## Problem

UI work currently becomes expensive when agents must validate visual direction through the full `apps/web` / desktop path. `apps/design-studio` can shorten that loop because it runs as a daemon-independent Vite app and already consumes the same root DESIGN pair and `@nexus/design-tokens` pipeline.

The product risk is that Design Studio becomes either a pretty dead-end gallery or an accidental second app shell. V1.99 validates the middle path: use studio for fast View-level decisions, then promote or integrate only after the boundary is clear.

The workflow to validate in V1.99:

1. Compose pure View components and static fixtures in `apps/design-studio`.
2. Iterate visually and accessibly against light/dark themes without daemon/Tauri.
3. Promote stable presentational primitives into `@42ch/nexus-ui` when reusable.
4. Integrate accepted components into `apps/web` with real data, routing, and behavior.

## Workflow Contract

### Stage 1 — Studio Composition

- Build static, story-like fixtures in `apps/design-studio`.
- Use root `DESIGN.md` / `DESIGN.dark.md` tokens and `@nexus/design-tokens`.
- Use `@42ch/nexus-ui` for promoted primitives and brand pieces.
- Use `@web-ui/*` only for not-yet-promoted primitives, and track the dependency as transitional.
- Do not import `apps/web` pages, layout components, daemon clients, route definitions, app providers, product hooks, localStorage-backed product state, Tauri helpers, or `@42ch/nexus-contracts`.

### Stage 1A — Surface Fixture Boundary

`/surfaces` may look like setup or app shell UI, but its implementation remains a View fixture:

- Keep setup steppers, workspace rows, shell chrome, daemon status strips, and profile/footer mockups studio-local unless a smaller primitive beneath them is approved for package promotion.
- Use promoted package primitives for generic controls (`Button`, `Badge`, `Card` after P0) and keep unpromoted controls on `@web-ui/*` only while transitional.
- Use static product-shaped data; do not simulate daemon lifecycle, routing transitions, creator bootstrap, or persistence.
- For each major section, record the intended next home: `promoted primitive`, `studio-local fixture`, `web-only wrapper`, or `future web product component`.

### Stage 2 — Visual Acceptance

Acceptance before `apps/web` integration:

- Light and dark themes both look intentional.
- Keyboard focus and disabled states are visible.
- No unregistered Tailwind scale steps are introduced.
- Copy follows DESIGN.md Voice & Content rules.
- The surface communicates its job without live daemon data.
- A reviewer can name what would change, if anything, when the fixture enters `apps/web`.

### Stage 3 — Promotion Decision

For each stable component:

- If it is reusable and pure presentational, promote to `@42ch/nexus-ui`.
- If it is app-specific but reusable within web only, keep it in `apps/web`.
- If it is a one-off surface fixture, keep it in `apps/design-studio`.
- If missing tokens block consistency, update root DESIGN pair through the plan before implementing raw values.
- If the decision is deferred, record the owner, trigger, and missing evidence.

V1.99 draft recommendation:

- Promote first: `Button`, `Badge`, and `Card`.
- Defer by default: `Input`, `Label`, and `Textarea` until a Form Field slice locks label/control/helper/error composition across at least two Web consumers and one Studio fixture.
- Keep shells, steppers, setup rows, daemon status strips, nav groups, and page sections out of `@42ch/nexus-ui`.

### Stage 4 — Web Integration

After visual acceptance:

- Replace static fixture data with `apps/web` data and behavior.
- Keep app state, routing, and daemon transport in `apps/web`.
- Preserve the same primitive imports where promoted.
- Prefer thin app-local wrapper/re-export files when they reduce churn or attach app-specific behavior; wrappers must point inward to `@42ch/nexus-ui`, never the other way around.
- Add integration tests around behavior that Design Studio could not cover.
- Document any intentional divergence from the studio fixture so future contributors do not treat it as drift.

## Evidence Template

Use this template during P0/P1/P2 rather than relying on memory:

| Evidence | Required answer |
| --- | --- |
| Component or surface | What was composed in Design Studio? |
| Studio acceptance | What light/dark/focus/structure checks passed? |
| Promotion decision | `promote`, `keep-web`, `keep-studio`, or `defer` |
| Package path | If promoted, what public `@42ch/nexus-ui` import path is used? |
| Web integration | Which `apps/web` consumer adopted it, which wrapper/direct import was used, or why not? |
| Boundary check | Which forbidden imports were checked absent? |
| Remaining caveat | What must be revisited before treating this as a durable pattern? |

## Durable Landing Options

At iteration-close, choose the lightest durable surface that matches what V1.99 proves:

| Output | Use when |
| --- | --- |
| `apps/design-studio/AGENTS.md` / `apps/web/AGENTS.md` / `packages/nexus-ui/AGENTS.md` | The rule is repo/package-specific |
| `.mstar/knowledge/architecture-patterns/*.md` | The pattern is reusable across future Nexus UI iterations and is promoted during iteration-close |
| Possible skill proposal | The workflow has been validated, is reusable across projects/roles, and passes the `mstar-skill-authoring` purpose test |

Do not create a new skill just to memorialize this iteration. Skill work is a later conditional option, not an automatic output: the workflow must first prove reusable beyond this repo and not duplicate existing Morning Star behavior.

## V1.99 Validation Targets

- At least one promoted component follows the full Studio → Package → Web path.
- `/surfaces` demonstrates the studio-first visual iteration loop with setup and shell compositions.
- The final guidance identifies which rules belong in `AGENTS.md`, which become iteration-close knowledge input, and whether any skill proposal is justified.
- A negative skill decision is acceptable and preferred unless the workflow proves reusable beyond Nexus repo ownership boundaries.
