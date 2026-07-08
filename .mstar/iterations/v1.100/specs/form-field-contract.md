# Form Field Component Promotion Contract

**Status:** Locked (T1 architecture lock — 2026-07-08)
**Document class:** Iteration-scoped contract
**Coordinates with:** `.mstar/iterations/v1.99/specs/component-promotion-boundary.md`, `@42ch/nexus-ui` (`packages/nexus-ui`), `apps/web`, `apps/design-studio`

## Problem

V1.99 intentionally deferred `Input`, `Label`, and `Textarea` because form controls need a focused accessibility and form-state boundary. The promotion mechanics are now proven by `Button`, `Badge`, and `Card`, but field semantics need to be explicit before more primitives move into `@42ch/nexus-ui`.

This slice exists to solve the deferral reason, not to lift app-local files into a package. Success means contributors can tell which semantics the package owns, which semantics the app composes, and how an accessible field is demonstrated in Design Studio.

## Locked Direction

Promote only `Input`, `Label`, and `Textarea` in this slice.

The package owns pure presentational primitives. Apps own composition, copy, validation state, daemon data, routing, submit behavior, and product-specific field groups.

Plan P2 consumes the P1-locked wrapper/direct-import strategy (`ui-guardrails-cn-ssot.md` § "P2 Wrapper/Direct-Import Rule") — it does not reopen the architecture decision:

- Web: thin re-export wrappers under `apps/web/src/components/ui/*` → `export { Component, type ComponentProps } from '@42ch/nexus-ui'`.
- Design Studio: direct `import { Component } from '@42ch/nexus-ui'`; removes `@web-ui/input`, `@web-ui/label`, `@web-ui/textarea`.
- Promoted wrappers auto-enter the forbidden-import guard set (already listed in `tooling/check-ui-guardrails.sh` `WRAPPER_CANDIDATES`).
- Web call-site churn is out of scope — screens keep importing from `@/components/ui` unchanged.

## Field Semantics — Locked Decisions (T1)

These decisions are **locked** by T1 architecture review. T2 (package controls), T3 (Studio consumers), and T4 (Web wrappers) implement against these — not against app-local shadcn conventions or undocumented assumptions.

### 1. Label/control association

- `Label` is a presentational `<label>` element that forwards standard `LabelHTMLAttributes` including `htmlFor`. It does **not** generate, cache, or own IDs.
- `Input` and `Textarea` receive their control ID via the standard `id` prop (inherited from `InputHTMLAttributes` / `TextareaHTMLAttributes`).
- **ID ownership: app.** The app (or fixture) generates a stable ID and passes it as `id` to the control and `htmlFor` to the label. The package is pure pass-through — it neither invents IDs nor validates the association.
- Nesting (`<label><input /></label>`) is allowed by HTML spec but is not the package-enforced pattern. The default composition pattern in Studio and Web uses explicit `htmlFor` + `id` association; the package does not dictate nesting.

**Evidence:** Current `apps/web/src/components/ui/label.tsx` is a thin `forwardRef<HTMLLabelElement, LabelHTMLAttributes<HTMLLabelElement>>` that passes `htmlFor` through — no ID generation logic exists in the source. The promotion preserves this.

### 2. Helper/error/required semantics

- `Input` and `Textarea` are **presentational controls only**. They render the native `<input>` / `<textarea>` element with DESIGN.md token-based styling and forward all standard HTML attributes.
- **Helper text and error text are app-owned composition.** The app renders helper/error elements in its composition layer alongside the control; the package does not export a helper-text or error-text component in V1.100.
- The typical composition pattern the Studio fixture must demonstrate:
  ```tsx
  <Label htmlFor="field-id">Field Name</Label>
  <Input id="field-id" invalid={hasError} aria-describedby="field-id-helper field-id-error" />
  <p id="field-id-helper">Must be between 3 and 50 characters.</p>   {/* app-owned */}
  {hasError && <p id="field-id-error" role="alert">Name is required.</p>}  {/* app-owned */}
  ```
- The package does **not** own message ordering, conditional display logic, or the semantic relationship between helper and error text.

### 3. `aria-invalid` ownership

- `Input` and `Textarea` provide an explicit `invalid?: boolean` prop:
  - `invalid={true}` → applies the visual error border class (e.g., `border-red-700`) **and** renders `aria-invalid="true"`.
  - `invalid={false}` or omitted → renders **no** `aria-invalid` attribute (passes `undefined` to React, which omits the attribute from the DOM).
  - This `undefined`-coercion pattern (matching the current `aria-invalid={invalid || undefined}` implementation) allows consumers to pass native `aria-invalid="false"` explicitly when needed without the package prop interfering.
- Consumers may **also** pass the native `aria-invalid` attribute directly for advanced cases (e.g., `aria-invalid="grammar"`). The native attribute overrides the `invalid` prop because it appears later in the spread (`{...props}` after `aria-invalid={invalid || undefined}`).

**Evidence:** Current `apps/web/src/components/ui/input.tsx:21` implements exactly this pattern: `aria-invalid={invalid || undefined}`.

### 4. `aria-describedby` ownership

- `aria-describedby` is **app-owned wiring**. The app:
  1. Generates stable IDs for helper and error elements.
  2. Concatenates them into a space-separated string.
  3. Passes the combined string as `aria-describedby` to the control.
- The package does **not**:
  - Generate description IDs.
  - Internally wire `aria-describedby` to any child element.
  - Own the ordering or presence of describedby IDs.
  - Export a helper-text or error-text primitive that auto-wires `aria-describedby`.
- When both helper and error are present, the app is responsible for the concatenation order (typically `"helper-id error-id"`).

### 5. Required/optional copy

- Required/optional **indicators** (text like `"(required)"`, `"*"`, `"(optional)"`) are **app-owned copy**.
- Package controls pass through the native `required` boolean attribute (inherited from standard `InputHTMLAttributes` / `TextareaHTMLAttributes`) and, if provided by the consumer, `aria-required`.
- Package controls do **not** render required/optional text as children, tooltips, post-label ornaments, or pseudo-elements. No asterisk, no "(required)" span.
- The Studio accessible-field fixture must demonstrate the app-owned pattern: label text + optional indicator (e.g., `Field Name (optional)`) rendered by the app, not by the package.

### 6. Wrapper strategy (consume P1 rule — do not reopen)

This contract consumes the P1-locked P2 Wrapper/Direct-Import Rule (`ui-guardrails-cn-ssot.md` § "P2 Wrapper/Direct-Import Rule") verbatim:

| Surface | Strategy | Rationale |
|---------|----------|-----------|
| **Web** (`apps/web`) | Thin re-export wrappers: `export { Input, type InputProps } from '@42ch/nexus-ui'` | Avoids call-site churn; screens keep `@/components/ui/input` imports |
| **Design Studio** (`apps/design-studio`) | Direct `@42ch/nexus-ui` import; remove `@web-ui/input`, `@web-ui/label`, `@web-ui/textarea` | Studio imports primitives from the canonical package; no transitional aliases for promoted controls |
| **Guardrail** (`tooling/check-ui-guardrails.sh`) | Promoted wrappers auto-enter the forbidden-import guard set (already listed in `WRAPPER_CANDIDATES`) | Mechanically prevents wrapper drift |

The P1 rule is **not reopened** by this contract. If a future plan needs a different wrapper strategy for form controls, it must amend `ui-guardrails-cn-ssot.md` through its own spec/clarify/plan cycle.

### 7. `Select` — out of scope

`Select` (`apps/web/src/components/ui/select.tsx`) is **not promoted** in V1.100. It retains its V1.99 `keep-web` classification (native `<select>` wrapper; no cross-app demand proven yet). Any future promotion requires a separate specification and plan.

### 8. No stateful `FormField` wrapper

The package **will not** export a stateful `FormField` component in V1.100. Specifically:

- No `FormField` component that manages error visibility, helper text rendering, or `aria-describedby` wiring.
- No form context provider, validation adapter, or schema-binding layer.
- No `useFormField` hook or equivalent internal state management.

The app owns state, validation, error messages, helper text, and all composition logic. If a `FormField` composition primitive is ever needed, it requires a **separate plan/spec** because it changes the package from primitive controls to form composition (see `Package Boundary` below).

### 9. Disabled and focus-visible styling

- Disabled styling is presentational and DESIGN.md token-based — the controls apply `disabled:bg-gray-100 disabled:text-gray-700 disabled:cursor-not-allowed`.
- Focus-visible styling remains tied to the global focus-visible ring (defined in `apps/web/src/index.css` / `@nexus/design-tokens`) — the controls apply `focus-visible:border-blue-700`.
- No validation state machine or form context is introduced by the package. The controls remain pure presentational surfaces.

### Summary for implementers

| Concern | Owned by | Mechanism |
|---------|----------|-----------|
| Control rendering + visual styling | Package | Native element + DESIGN.md tokens + `className` merge |
| `id` generation | App | Standard `id` prop on Input/Textarea, `htmlFor` on Label |
| `aria-invalid` | Package (from `invalid` prop) + App (native override) | `invalid` prop → visual class + `aria-invalid`; native `aria-invalid` in `{...props}` overrides |
| `aria-describedby` wiring | App | App concatenates helper/error IDs → passes as prop |
| Helper text / error text | App | App renders `<p>` elements with generated IDs |
| Required/optional indicators | App | App renders text in label or adjacent element |
| `cn` class merging | Package (`@42ch/nexus-ui/src/lib/cn.ts` SSOT) | Package-local `cn`; Web wrapper re-exports from package |
| Composition layout | App | App arranges Label + Input/Textarea + helper/error in DOM order |

## Package Boundary

`@42ch/nexus-ui` must not export form state, validation adapters, schema bindings, product copy helpers, daemon-aware fields, route-aware fields, or app-specific field groups. Any future `FormField` composition primitive requires a separate plan/spec because it changes the package from primitive controls to form composition.

## Product Acceptance Standard

- A promoted primitive without documented field semantics is not acceptable.
- A Studio fixture that only shows isolated controls is not enough; it must also show label, helper, error, invalid, disabled, and required/optional composition.
- Web compatibility should preserve existing `@/components/ui/*` imports unless the review chain explicitly approves direct package imports.
- No package API should imply ownership of validation messages, submit behavior, daemon data, routing, or product copy.

## Implementation Boundaries

In scope:

- Package-owned `Input`, `Label`, and `Textarea` implementation and tests.
- Package root named exports.
- Web thin re-export wrappers or direct imports as approved by P1 guardrails.
- Design Studio imports through `@42ch/nexus-ui` for promoted controls.
- Documentation updates for package and app ownership rules.

Out of scope:

- `Select`, `Dialog`, `Tabs`, tables, app chrome, or daemon-aware controls.
- A package-level form framework, validation library, or state machine.
- Broad refactor of every Web form composition.
- Wire schema changes.

## Acceptance Hooks

- The promoted controls match the locked batch with no surprise additions.
- Studio demonstrates label/control/helper/error composition and invalid/disabled states.
- Web keeps compatibility for existing `@/components/ui/*` imports unless the review chain approves direct package imports.
- Package tests cover class merging, invalid state, ref forwarding, and key accessibility attributes.
- No app code is imported into `@42ch/nexus-ui`.
- The V1.99 deferral rationale is visibly closed: helper/error/required semantics and app/package ownership are documented before Done.
