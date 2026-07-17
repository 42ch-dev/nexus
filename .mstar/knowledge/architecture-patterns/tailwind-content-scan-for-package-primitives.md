---
module: apps/web
date: 2026-07-17
problem_type: build-config
category: architecture-patterns
severity: low
plan_id: 2026-07-17-v1.120-shell-form-polish
tags: [tailwind, content-scan, jit, nexus-ui, css, monorepo]
applies_when: an app's Tailwind build consumes class utilities that only appear in `packages/nexus-ui/src` (shared primitives)
---

# Tailwind `content` scan must cover `packages/nexus-ui/src`

## Context

Tailwind JIT emits only the utilities it **sees** in scanned files. When shared primitives live in `packages/nexus-ui/src` but the app's `tailwind.config.ts` `content` array only scans `apps/<app>/src`, utilities used **exclusively** inside package components (e.g. `appearance-none`, `ps-3`, `pe-8`) are never emitted — silently. Symptom: broken primitive rendering with **no build error** (V1.120 P1: native `<select>` showed its UA arrow next to the custom overlay → duplicate chevron).

## Guidance

- Every app consuming `@42ch/nexus-ui` components must include `../../packages/nexus-ui/src/**/*.{ts,tsx}` in `content` (relative depth per app location). `apps/design-studio` already had it; `apps/web` was missing it until V1.120 P1.
- When a shared-primitive style "disappears", suspect content-scan coverage **before** touching component CSS — verify by grepping the built stylesheet for the utility (`.appearance-none{` present?).
- Sibling failure mode (theme-key routing): see `architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md` — both are "Tailwind silently emits nothing" but with different root causes (scan coverage vs theme key placement).

## Why This Matters

Silent non-emission wastes diagnosis time on component code that was never broken; the fix is one config line. Any future app (or renamed package path) re-introduces the gap if the scan list is copied from an incomplete config.

## When to Apply

- Creating a new app in the monorepo that renders `@42ch/nexus-ui` primitives.
- Debugging "utility class has no effect" for a class defined only in package code.

## Examples

```ts
// apps/web/tailwind.config.ts
content: [
  "./index.html",
  "./src/**/*.{ts,tsx}",
  "../../packages/nexus-ui/src/**/*.{ts,tsx}", // required — package primitives
],
```

Verification (V1.120 P1 T2): after the fix, built CSS emits `.appearance-none`, `ps-3`, `pe-8` (count 0 → 1+); `create-work-dialog` chevron test asserts single chevron inside control boundary.
