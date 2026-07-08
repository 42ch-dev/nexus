# Form Field Component Promotion Contract

**Status:** Draft for V1.100 review chain
**Document class:** Iteration-scoped contract
**Coordinates with:** `.mstar/iterations/v1.99/specs/component-promotion-boundary.md`, `@42ch/nexus-ui` (`packages/nexus-ui`), `apps/web`, `apps/design-studio`

## Problem

V1.99 intentionally deferred `Input`, `Label`, and `Textarea` because form controls need a focused accessibility and form-state boundary. The promotion mechanics are now proven by `Button`, `Badge`, and `Card`, but field semantics need to be explicit before more primitives move into `@42ch/nexus-ui`.

This slice exists to solve the deferral reason, not to lift app-local files into a package. Success means contributors can tell which semantics the package owns, which semantics the app composes, and how an accessible field is demonstrated in Design Studio.

## Locked Direction

Promote only `Input`, `Label`, and `Textarea` in this slice.

The package owns pure presentational primitives. Apps own composition, copy, validation state, daemon data, routing, submit behavior, and product-specific field groups.

Plan P2 is blocked by the P1 wrapper guardrail contract. It must consume the approved wrapper/direct-import strategy instead of creating a local exception for form controls.

The selected Web strategy is compatibility-preserving thin wrappers under `apps/web/src/components/ui/*` that import the promoted primitives from `@42ch/nexus-ui`. Direct package imports are required for Design Studio promoted controls; Web call-site churn is out of scope unless P1 explicitly changes the wrapper rule.

## Field Semantics

The review chain locks these decisions for package implementation:

- `Label` is a presentational `<label>` that forwards standard label props, including `htmlFor`. It does not generate ids.
- `Input` and `Textarea` forward refs and native attributes. If they keep the existing `invalid?: boolean` prop, it maps only to visual invalid classes and `aria-invalid={true}`; consumers may also pass native `aria-invalid`.
- Helper text, error text, and `aria-describedby` ids are composed by the app or fixture owner. The package does not generate description ids or own message ordering.
- Required/optional copy is app-owned text. Package primitives may pass through native `required`, but they do not render required/optional indicators.
- Disabled and focus-visible styling stays presentational and token-class based; no validation state machine or form context is introduced.
- Web consumers use P1-approved thin wrappers for compatibility; Design Studio imports promoted primitives directly from `@42ch/nexus-ui`.
- Studio fixture acceptance requires both isolated primitive examples and one accessible field composition showing label/control/helper/error, invalid/disabled, and required/optional states.

The default recommendation is:

- `Input`, `Label`, and `Textarea` are package exports.
- Apps compose labels, helper text, error text, and required/optional copy.
- The package does not export a `FormField` stateful wrapper in this iteration.
- Design Studio demonstrates at least one accessible field composition fixture in addition to isolated primitive examples.

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
