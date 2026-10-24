---
module: apps/nexus-service
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
date: 2026-10-25
last_updated: 2026-10-25
tags:
  - evidence-and-close
  - issue-closeout
  - boot-test
  - tokenized-url
  - regression-test
  - verification
---

# Entrance Evidence-and-Closeout Pattern

Pattern for closing "already implemented but unproven" issues: verify the shipped
behavior, add only the genuinely missing regression tests, record replays, and close
with named evidence — never rewriting a working implementation to make it testable.

## Context

Long-lived issue trackers accumulate items whose fix landed in an unrelated commit but
whose acceptance was never recorded. Rewriting those surfaces "to close the issue"
duplicates working code and risks regressions. The v1.222 entrance-closeout plan closed
four such issues (I-000059/60/61/62) whose implementations had shipped in `93b2043a4`
weeks earlier, by treating the plan as an *evidence-and-close* delivery: acceptance
first, gap-fill only where acceptance fails (and re-plan if a real gap appears).

## Decision

Split the closeout into four evidence classes, each with a pinned recipe:

1. **Boot-level regression test** (the launch surface no route-level test reaches).
2. **Pure-function gate matrix** (policy functions with no socket).
3. **Acceptance runs on existing suites** (what already covers the claim).
4. **Recorded replays** (the end-to-end claim no automated suite makes).

## Implementation

**Boot regression test with an observable opener** (`apps/nexus-service/tests/boot-bootstrap-url.test.mjs`):

- Pick a free port explicitly (`--port 0` is rejected by config) and point `--home` at
  an **existing empty** temp dir — an uninitialized shell still mints a `bootstrapUrl`,
  so the test needs no seeded home.
- Prepend a temp dir to `PATH` holding an executable `open`/`xdg-open` **shim** that
  appends its argv to a marker file: this makes auto-open observable without launching a
  browser, and PATH-shadowing keeps the host untouched.
- Assert the **stdout contract** by property, not substring: exactly one non-empty line
  matching `^NEXUS_SERVICE_READY \{` and no `[nexus-service]` prefix — a leaked human
  log line on stdout must fail.
- For absence assertions (opener never ran), **stop the service first, then poll the
  marker across a 3 s quiescence window** (25 ms interval). A fixed short sleep races
  the launcher's ~400 ms spawn and misses real regressions (measured: a fixed 500 ms
  window stayed green in 3 of 6 runs against a reverted build).
- Every platform-relevant case also GETs the printed URL and asserts `302` +
  `Set-Cookie` — that is the proof the printed token is the one the store minted.
- Skip shim-observed cases on `win32` (`cmd /c start` needs a shell).
- Red-on-revert is demonstrated by rebuilding `dist` with the production line
  temporarily removed, running the case repeatedly (≥3×), and restoring byte-for-byte
  (`cmp`).

**Pure-function gate matrix** (`apps/nexus-service/tests/i-000061-gate-matrix.test.mjs`):
both policy gates (`validateStartupBind`, `validateBindPolicy`) are pure — test the full
truth table with **no socket and no non-loopback listener**: unix exempt, loopback
admitted keyless, non-loopback refused without the opt-in flag (403) or the key (401),
cleartext refused **only while unauthenticated**, and the always-authenticated bootstrap
route admitted. Include host-classification near-misses (`0.0.0.0`, `::`,
`127.0.0.1.evil`) so the loopback predicate cannot be loosened silently.

**Recorded replays**: boot the real service once against a LAN-style bind
(`--allow-remote` + key, plain HTTP) and record per claim the exact command line, the
origin used, and the observed outcome (status + response body shape) into a review
artifact. A curl call with an explicit `Origin` header stands in for a fresh browser
origin; drive an actual browser only when the claim is about rendered UI.

**Closure notes**: cite test paths, replay files, and commits; record PM adjudications
inline (e.g. the 403-vs-401 split: a keyed-entry recovery surface triggers on 401, while
a credential-less visitor on a plain-HTTP LAN bind gets the I-000061 gate's 403 and is
guided by the tokenized URL). Keep wording exactly as strong as the evidence —
unasserted claims ("renders a db reason") are narrowed or dropped.

## Why this works

- **No rewrite pressure**: the plan's only product-safe outcome is evidence; a failed
  acceptance stops and re-plans instead of patching.
- **Determinism**: shim + polling window + property assertions remove the three classic
  flake sources (real browser, fixed sleeps, substring output matching).
- **Auditability**: each issue's closure references artifacts a reviewer can re-run
  (`node --test <file>`) without a LAN.

## When to apply

- Issues whose fix predates their acceptance and whose surface already works.
- Launch/CLI surfaces (stdout contracts, auto-open) unreachable by route-level tests.
- Policy gates that are pure functions.

Do not use for: claims about rendered visuals (use fixture/browser evidence), or issues
whose acceptance genuinely requires new product behavior (that is a build plan, not an
evidence plan).

## References

- `apps/nexus-service/tests/boot-bootstrap-url.test.mjs` (I-000062)
- `apps/nexus-service/tests/i-000061-gate-matrix.test.mjs` (I-000061)
- the iteration's closure evidence package (evidence assembly + adjudication
  recording; assembled locally from the evidence plan, not tracked)
- `.mstar/knowledge/architecture-patterns/daemon-api-remote-bind-gate.md` (the gate
  being evidenced)
