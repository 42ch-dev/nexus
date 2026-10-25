---
module: pnpm-workspace
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
date: 2026-10-25
last_updated: 2026-10-25
tags:
  - dependabot
  - security-alerts
  - pnpm-overrides
  - lockfile
  - supply-chain
  - transitive-dependencies
---

# Security-Alert Lockfile Sweep

Pattern for clearing Dependabot (or equivalent) alerts on transitive dependencies in a
pnpm workspace: scoped `overrides` in `pnpm-workspace.yaml`, a gate discipline that
decides keep-vs-rollback per package, and merge-time verification that the alerts
actually clear.

## Context

Transitive dependencies accumulate security advisories that no direct dependency bump
fixes. The naive responses both fail: upgrading the direct parent drags in unrelated
change, and a global override can break sibling consumers that need a different major
(v1.223's example: a global `undici` override would have downgraded jsdom's
`undici@8.12.3` to 7.x while fixing `@electron/get@5`'s 7.29.1).

## Decision

Drive each alert with the **narrowest override that clears its vulnerable range**, then
subject every override to the same gate: if any gate reds, roll back that line and keep
the alert open (disclosed) — never ship an unverified forced major.

## Implementation

**Override shape** (`pnpm-workspace.yaml` `overrides:` — the pnpm 11 SSOT, not
package.json):

- Floor-style keys pin the minimum: `fast-uri: '>=4.2.3'`, `markdown-it: '>=14.3.3 <15'`.
- **Parent-scoped keys** protect sibling consumers:
  `'@electron/get@5>undici': '>=7.30.1 <8'` — only that edge moves; jsdom's undici 8
  stays. Key by `<parent>@<major>><pkg>` so a future advisory on the other major needs
  its own key.
- A forced major (e.g. `postcss-selector-parser` 6→7 under Tailwind 3's `^6.1.2`) may be
  attempted under the D6-style gate, but first prove whether the override is even
  **loaded by the runtime app graph**. The edge is real in the lockfile —
  `tailwindcss@3.4.19` and `postcss-nested@6.2.0` both depend on
  `postcss-selector-parser@7.1.7`, and `tooling/design-tokens` depends on
  `tailwindcss@3.4.19` — but every one of those edges is tooling-time (no app or runtime
  gate loads `postcss-selector-parser` 7.1.7; apps resolve Tailwind 4, which has no
  dependency edges), so the override is inert for the shipped app. A standalone compile
  of the real consumer against the new major is then the honest evidence, not the app
  test suites (v1.223 QC found the app gates could not observe it).

**Gate discipline** (per override, in this order; all must be green or the line rolls
back):

1. Plain resolve: `pnpm install`.
2. Frozen-lockfile consistency: `pnpm install --frozen-lockfile`.
3. Shared component library: its test suite.
4. Primary app: typecheck (including whatever its `pretypecheck` rebuilds).
5. Primary app: test suite.
6. Primary app: production build.
7. Second shipped app: typecheck.
8. Second shipped app: test suite.
9. Second shipped app: production build (including its `prebuild` codegen).
10. The package that directly owns a forced edge: its build **and** codegen targets —
    the riskiest, so last among the consumers.
11. Remaining tooling consumers: their validate/lint scripts (if lint reports
    unresolved-import artifacts, run the pre-lint builds first).
12. Alert end state: open-alert list before and after (live API); alerts may resolve
    only after the merged lock is re-parsed.

Three rules sit across the whole list:

- The lock diff must touch **only the intended version pairs** (parse both revisions;
  integrity hashes must match the registry).
- A freshness gate (`minimumReleaseAge` or equivalent) is never lowered to admit the
  fix.
- Gate only what can actually load the package (trace the import graph). If no gate can
  observe it — a resolution-only edge — build that evidence (standalone consumer
  compile); "the app suite runs" is not evidence.

**Verification at merge**: record the resolved lock versions plus the pre-merge alert
list (live API) 1:1 against each GHSA's `first_patched_version`; the alerts clear when
Dependabot re-parses the merged lock. The merge-time expectation is "zero open alerts"
(or the named disclosed exceptions).

## Why this works

- **Narrowest blast radius**: parent-scoped keys fix the alerted edge without moving
  consumers on other majors.
- **Honest evidence**: the gate list forces the question "does anything actually load
  this?" instead of green-washing an inert override.
- **Reversible**: each override is one line; a red gate rolls back to the alert being
  open, which is the status quo ante.

## When to apply

- Transitive-dependency advisories in a pnpm workspace where direct-dependency bumps
  don't reach the vulnerable package.
- Any alert whose fix version is semver-compatible with all sibling consumers.

Do not use for: advisories needing a major jump that gates red (keep the alert open,
plan the real upgrade), or Rust/Cargo graphs (different override mechanism).

## References

- `pnpm-workspace.yaml` `overrides:` (v1.223: four keys incl. the scoped undici and the
  forced postcss-selector-parser major)
- GitHub advisory API `first_patched_version` — the alert-clear oracle
