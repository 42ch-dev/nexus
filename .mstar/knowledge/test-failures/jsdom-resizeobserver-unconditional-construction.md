---
module: apps/design-studio
date: 2026-09-10
problem_type: test_failure
category: test-failures
severity: medium
symptoms:
  - "pnpm --filter design-studio test failed 181 App tests with ReferenceError: ResizeObserver is not defined"
  - "The failures appeared only in the jsdom environment; the browser behavior was correct"
root_cause: "A mount effect constructed `new ResizeObserver(...)` unconditionally to keep a measured CSS variable in sync. jsdom implements no ResizeObserver, so every test that rendered the shell threw during commit."
resolution_type: code_fix
tags:
  - design-studio
  - jsdom
  - resizeobserver
  - environment-parity
  - guard
  - sticky-header
---

# jsdom regressions: a browser-only API constructed unconditionally

## Problem

The sticky-header offset fix (measure `header.offsetHeight`, publish it as a CSS variable, observe resizes) added `new ResizeObserver(...)` in a React effect. The browser behavior was correct, but the same code runs in the Studio test environment, where `ResizeObserver` does not exist — 181 `App.test.tsx` tests failed at once.

## Symptoms

- `ReferenceError: ResizeObserver is not defined` from the shell's mount effect; the entire App suite collapsed rather than a single focused test failing.
- The defect was invisible in the real browser check that accompanied the change — the environment that ran the code was not the environment that ran the tests.

## What Didn't Work

- **Adding a test-only global stub.** It would hide the real hazard (the feature silently losing resize updates in any environment that lacks the API) instead of making the component degrade honestly.
- **Guarding the whole effect.** Returning early before the initial measurement means the CSS variable never gets its first value in observer-less environments, so focus offsets silently fall back to a default.

## Solution

Measure first, then guard only the construction:

```tsx
const syncStickyHeaderOffset = () => {
  document.documentElement.style.setProperty(
    '--studio-sticky-header-offset',
    `${header.offsetHeight}px`,
  );
};
syncStickyHeaderOffset();                       // always: initial value is correct

if (typeof ResizeObserver === 'undefined') {    // jsdom / older runtimes
  return;                                       // no observation, no crash
}
const observer = new ResizeObserver(syncStickyHeaderOffset);
observer.observe(header);
return () => observer.disconnect();
```

## Why This Works

The measured value is produced by a plain read of `offsetHeight`, which exists everywhere; only the *continuous* observation depends on a browser API. Splitting "read once" from "observe" keeps the feature correct in every environment and confines the capability check to the part that needs it. The cleanup path is unchanged when the observer exists.

## Prevention

- When a change introduces an environment-dependent API, run the affected suite in the environment the code runs in — and check the *browser* behavior too; a green build in one environment proves nothing about the other.
- Keep the first measurement outside capability guards; guard only the construction/observation.
- Do not paper over missing browser APIs with test-environment stubs on shared chrome; fix the component so it degrades honestly.
- Treat a mass test failure (dozens/hundreds of tests from one effect) as one root cause, not as flakiness.
- Related: [studio-sticky-header-wrapped-overlap.md](../ui-bugs/studio-sticky-header-wrapped-overlap.md) (the change that introduced the observer).
