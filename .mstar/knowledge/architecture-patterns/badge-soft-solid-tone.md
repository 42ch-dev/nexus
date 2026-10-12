---
module: packages/nexus-ui
date: 2026-10-12
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: low
tags:
  - badge
  - tone
  - cva
  - design-tokens
  - contrast
applies_when:
  - Adding a soft/solid emphasis axis to a shared component
---
# Badge soft / solid tone axis

**Status:** crystallized from V1.102  
**Source:** `badge-soft-solid-contract.md`

## Summary

`@42ch/nexus-ui` `Badge` supports `tone?: 'soft' | 'solid'` (default **soft**) via cva `compoundVariants`. Soft borders use stronger neutrals / ~50% semantic alpha. Solid uses locked semantic fills; dark solid semantic text uses `brand-deep-blue` on bright fills (Button Contrast Invariant), not white.

## When to use

- Soft: default status pills in dense UI
- Solid: high-emphasis status when contrast is required

## Non-goals

- Forced `StatusBadge` cutover to solid
- Independent finding-status / memory token trees

## Related

- DESIGN.md / DESIGN.dark.md `components.badge-status-pill`
- Design Studio `/components` Soft/Solid matrices
