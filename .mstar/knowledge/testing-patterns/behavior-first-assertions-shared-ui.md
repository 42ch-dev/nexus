---
module: packages/nexus-ui + apps/design-studio
date: 2026-09-10
problem_type: testing_pattern
category: testing-patterns
severity: medium
applies_when:
  - "Writing or migrating tests for shared presentational components whose classes/copy are expected to change"
  - "Reviewing a fix round whose tests were rewritten alongside a visual change"
  - "Auditing cross-package test pins after a shared component's emitted classes change"
tags:
  - testing
  - assertions
  - implementation-pins
  - nexus-ui
  - design-studio
  - jsdom
---

# Behavior-first assertions for shared UI (and when a class pin is legitimate)

**Track**: Knowledge (distilled from the v1.187 design-system iteration, where implementation-string assertions caused fix rounds in every plan).

## Context

The v1.187 overhaul restyled shared primitives and the Studio galleries. Tests that had been written against the previous visual contract broke in bulk — not because behavior regressed, but because they asserted *how the UI was encoded* rather than *what a consumer observes*. The recurring classes:

- **Exact copy assertions** (`getByText('2 sections')` vs the component announcing `2 sections.`; a page-wide `/Read-only/` matcher that also matched new Home copy) — the test then forces the product copy to freeze, and any copy edit becomes a "failure".
- **Implementation class pins** on shared components (rounded/colour class strings, exact DOM wrappers) — a legitimate restyle strands the pin, and because the pin is usually updated in the *changing* package only, sibling packages' fixtures/tests fail elsewhere (cross-package pin drift).
- **Dead wording/source-string tests** kept as a proxy for a gate that no longer needs them (e.g. asserting a generated CSS file still contains a literal instead of asserting the projection behaviour).
- **Tautologies** introduced while "fixing" the above (asserting an element exists without any observable consequence) — green forever, catches nothing.

## Guidance

**Assert the consumer-visible contract, and only that.**

- Prefer role/name/state effects: `getByRole('contentinfo')` + a scoped text match, `aria-live` announcements, `aria-controls` targets existing, `tabIndex`/focus destination, `aria-invalid`, opened/closed overlays, keyboard paths (Arrow/Enter/Escape), and count *semantics* ("reports the numeric count", "announces a no-results state") rather than exact wording.
- Scope queries to the landmark/region under test when copy can legitimately appear twice; do not delete the other surface's content to make a global matcher pass.
- One assertion should fail for a plausible bug. If no realistic defect could turn it red, delete it rather than keeping it "for coverage".

**A class pin is legitimate only when the class name *is* the contract.**

- Shared components that resolve through projected tokens (`bg-nexus-ui-badge-soft-running-bg …`) may pin the semantic token classes — but the pin moves with the token, in the same change.
- Raw arbitrary/utility class strings (e.g. a `color-mix(...)` arbitrary) are implementation detail: assert the computed/observable result or migrate the value into a token first, then pin the token name.
- When a shared component's emitted classes change, audit **all** packages that pin them before merging — `grep -rn "<old-class>"` across apps/packages, not just the component's own suite.

**Keep gates and tests honest about what they prove.**

- A byte-compare/parity gate (projection artifacts) should not be backed by "the file still contains this string" tests; the gate already fails closed.
- When a test and a component disagree about observable copy, decide which is the contract and change the other; never re-pin the test to the new string by reflex.

## Why This Matters

- **Fixed cost, not fixed tests.** Every fix round that rewrites assertions consumes review capacity without changing shipped behavior; behavior-first tests survive restyles by design.
- **Pins that outlive their value become traps.** An assertion on last revision's classes fails exactly when the design system does its job (intentional restyle).
- **Cross-package drift is invisible locally.** A shared component's class change breaks sibling packages' suites, which is where the drift is finally noticed — usually mid-iteration.
- **Tautologies hide the absence of coverage.** They pass during the defect and afterwards, so they are worse than no test.

## When to Apply

- Writing new tests for shared primitives, gallery fixtures, or anything driven by design tokens.
- Migrating tests alongside a restyle or copy change (always in the same commit).
- Reviewing a diff where test expectations changed: ask "what behavior would fail this test?" and "is this string part of any contract?".

## Examples

```tsx
// before — page-wide copy match; a new Home hint also matches
expect(screen.getByText(/Read-only/)).toBeInTheDocument();

// after — scoped to the footer landmark, plus a meaningful artifact assertion
const footer = screen.getByRole('contentinfo');
expect(within(footer).getByText(/Read-only/)).toBeInTheDocument();
expect(within(footer).getByText('DESIGN.md')).toBeInTheDocument();
```

```tsx
// before — exact wording is now the contract
expect(status.textContent).toBe('2 sections');

// after — observable semantics: a polite announcement reporting the count
expect(status).toHaveAttribute('aria-live', 'polite');
expect(status.textContent).toMatch(/\b2\b/);
```

## Pitfalls

- **"Do not weaken the test" is not the same as "pin the copy".** Strengthen by asserting the observable consequence (announcement, focus, state), not by freezing strings.
- **Deleting an assertion to make a suite pass** — replace it with a behavioral equivalent; a removed assertion with no replacement is lost coverage.
- **Fixing a pin in one package only** — shared visual changes ripple; sweep every pin site.
- **Snapshot-style class assertions on shared primitives** — they reproduce the same failure mode with more churn; assert computed/observable state or the semantic token.
