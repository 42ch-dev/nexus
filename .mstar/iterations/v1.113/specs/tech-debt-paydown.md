# Tech Debt Paydown Spec (V1.113 P2)

## User value

Indirect: green design-studio tests and a thinner residual slate reduce noise so
the next author-facing iteration can prioritize canvas/features instead of
hygiene false positives.

## Problem

V1.112 shipped (PR #142) but harness metadata may still lag. ~87 open residuals
accumulated across V1.91–V1.112. Design-studio has ~7 pre-existing test failures
(R-P0-006, per status.json; verify exact count at execution), likely
i18n-adjacent after V1.112.

## Scope

### Design-studio test fix (R-P0-006)

Failing tests in `apps/design-studio`. Root cause hypothesis: i18n provider
missing in test environment and/or locale-sensitive string assertions. Fix by
wrapping renders in `LocaleProvider` (or test setup) and adjusting assertions.

### Residual closures (target: ≥10)

Priority clusters (verify still applicable before coding):

1. V1.94 setup wizard residuals (6 low)
2. V1.96 path-context-menu / rounded-control (2 low)
3. V1.104 settings workspace (6 nit/low) — quick wins
4. V1.106 Studio alias doc / Toast maxToasts (2 nit)

Each residual: verify issue still exists → fix or accept with rationale → move
to archived. If a cluster is obsolete, pull the next applicable low/nit items
until ≥10.

### Status.json + tracker hygiene

1. Record V1.112 ship in `metadata.latest_ship`
2. Update `metadata.latest_active_iteration` / compass to V1.113
3. Archive V1.112 plans per Profile B if still hot
4. Refresh `metadata.tech_debt_summary` via rollup script (counts only)
5. Verify `wc -c .mstar/status.json` < 20,000 bytes
6. Update deferred-features tracker **quick-status line** to V1.113 (no DF list rewrite)
7. Update `iterations/README.md` V1.112 row to Shipped if needed

## Acceptance criteria

- [ ] Design-studio suite green for the previously failing App tests (or residual
  re-scoped with documented reason)
- [ ] ≥10 residuals closed or accepted with rationale + archived
- [ ] status.json ship/active/tech_debt_summary truthful and size-gated
- [ ] Deferred-features quick-status reflects V1.113

## Dependency

- **blocked_by:** P0 (i18n completion) — design-studio failures are i18n-adjacent;
  residual selection benefits from P0 landing first

## Non-goals

- Closing ALL ~87 residuals — floor is ≥10 high-value closures
- Restructuring status.json schema
- Full deferred-features archive pass (quick-status only)
- i18n product-page migration (P0) or DESIGN.md token work (P1)
