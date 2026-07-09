# V1.100 Iteration Workspace

Iteration-scoped contracts and guides for **V1.100 — UI Completion: Desktop First-Launch, Guardrails, Form Fields**.

## Product Throughline

V1.100 combines one author-visible reliability gap with two design-system maturity gaps. The desktop plan makes first launch complete for a clean local author. The guardrails plan makes UI promotion safe to repeat. The form-field plan proves the next promotion slice by resolving semantics first, then moving code.

## Files

| Path | Purpose |
|------|---------|
| `specs/desktop-first-launch-bootstrap.md` | Clean-state desktop bootstrap contract and non-substitutable interactive smoke gate |
| `specs/ui-guardrails-cn-ssot.md` | UI package guardrails, Design Studio boundary automation, and `cn` / Tailwind merge SSOT contract that enables P2 and later promotions |
| `specs/form-field-contract.md` | `Input` / `Label` / `Textarea` promotion contract focused on field accessibility semantics and app/package ownership |

## Boundaries

- This workspace holds V1.100 iteration-scoped contracts and review-chain edits.
- Long-lived normative updates belong in `.mstar/specs/` only when the review chain promotes and locks them.
- Reusable implementation knowledge is promoted to `.mstar/knowledge/` only during iteration-close compound.

## Prepare Readiness

- Product scope, non-goals, dependency order, and blocked_by relationships are documented in the compass and main plans.
- P0 is independent but cannot reach Done without interactive macOS clean-state and existing-install smoke evidence; its bootstrap path is desktop IPC before daemon start, not a daemon wire-contract expansion.
- P1 must lock guardrails and class-merge SSOT before P2 implements promoted wrappers. The V1.100 SSOT direction is `@42ch/nexus-ui`; `@nexus/design-tokens` is not the authority while it depends on the UI package.
- P2 must lock form-field semantics before package implementation; lift-and-shift alone is not acceptable, and no package-level `FormField` framework is in scope.
