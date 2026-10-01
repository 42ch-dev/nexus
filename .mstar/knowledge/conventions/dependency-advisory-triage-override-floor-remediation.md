---
module: tooling
date: 2026-10-01
problem_type: convention
category: conventions
severity: medium
plan_id: 2026-10-01-v1.203-p1-dependency-alert-triage
tags:
  - dependabot
  - supply-chain
  - pnpm-overrides
  - security-floor
  - attribution
  - disposition-record
  - lockfile
  - advisory-triage
applies_when:
  - Dependabot (or any npm advisory source) reports open alerts against this workspace
  - remediating a vulnerable transitive npm dependency
  - closing an alert-triage round (disposition records, register rows, post-merge re-scan)
---

# Dependency-Advisory Triage & Override-Floor Remediation

## Context

Dependabot alerts key on the **lockfile**, not on reachability — an alert says "a vulnerable version is selected", never "shipped code can reach the vulnerable path". Treating the lockfile line as the vulnerability leads to two failure modes: dismissal without evidence ("probably build-time only") and over-remediation (unqualified major jumps that break consumer ranges). This procedure is the pnpm-side complement to the cargo-side lesson in [cargo-lockfile-feature-independent-dependabot.md](../workflow-patterns/cargo-lockfile-feature-independent-dependabot.md); the toolchain pin and `minimumReleaseAge` policy it references live in [pnpm-toolchain-pin-and-supply-chain-age.md](pnpm-toolchain-pin-and-supply-chain-age.md) (not duplicated here).

## Guidance (the procedure, per alert batch)

1. **Attribution before action.** Per alert: fetch the advisory metadata (`gh api repos/<owner>/<repo>/dependabot/alerts/<n>` — vulnerable range, first-patched version, manifest, scope); run the inverse graph (`pnpm why -r <pkg>`); read the selected version from `pnpm-lock.yaml`; and record **input provenance** — who controls the input that reaches the vulnerable code (e.g. a glob library whose only patterns in the shipped graph are compile-time repo constants; a URI parser fed only repo-controlled schemas; a markdown path whose vulnerable feature the sole consumer configures off). No alert closes as "unreachable" without the inverse-graph + input-provenance evidence; "not exploitable" asserted without it is not a disposition.
2. **Remediate with floors, not pins.** Raise or add a `pnpm-workspace.yaml` `overrides` **floor** (`pkg: '>=<first-patched>'`), never an exact-version pin, unless no compatible release within the consumers' ranges provably exists — that exception is documented per-alert with the proof. Check consumer range admissibility before choosing the floor (the direct consumers' declared ranges must admit the patched version). Add an upper cap when a floor could drift across a major on a future regeneration (e.g. `'>=14.3.1 <15'`). Extend the adjacent overrides comment with the new GHSA ids, matching the existing security-floor comment convention.
3. **Verify the lockfile mirror.** Regenerate with `pnpm install`; the lockfile `overrides:` block must byte-mirror the workspace floors, show exactly one selected record per package satisfying every patched bound, carry integrity, and leave no stale vulnerable record or re-pointed incoming edge behind.
4. **Run the consumer-mapped verification set, not a full suite.** Map each remediated package to the checks that exercise its consumers: `pnpm install --frozen-lockfile` green (the gate) plus, per inverse-graph consumer, its scoped deterministic check (schema validator run, codegen tests, the editor round-trip suite, the desktop packaging contract test). Record commands + observed outputs verbatim.
5. **Respect `minimumReleaseAge`.** Check publish age of the target versions before remediating; a patched release published inside the window re-arms it. On `ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`: hold the remediation — never relax the repo policy to force an alert closed (policy details and the CLI-only local override live in the pin doc).
6. **Record one disposition per alert** (format below) in the plan's durable summary; the gate summaries cite the section instead of duplicating rows.
7. **Zero register rows is the expected clean closure.** A `deferred` action REQUIRES a residual-register row with a named re-check trigger, and the disposition record cites the register id. An all-`remediated` round therefore changes the register not at all — any row added without a deferred alert is a smell.
8. **Post-merge re-scan is the real closure.** Dependabot re-scans the default branch when the remediated lockfile merges. Plan-level proof stops at "floors satisfy every patched bound + frozen install green"; the acceptance claim (`gh api …/dependabot/alerts?state=open` → 0) is post-merge evidence, recorded when the iteration PR lands.

## Disposition record format

One record per alert. Fields:

| Field | Content |
|---|---|
| `alert` | Alert number + GHSA id + severity |
| `package` | Package name + vulnerable range (from the advisory metadata) |
| `selected_pre` | Version selected in `pnpm-lock.yaml` before remediation |
| `reachability` | Attributed consumer path (from `pnpm why -r`) + input provenance, or "no shipped consumer" with the inverse-graph evidence |
| `action` | `remediated` (name the override floor applied) or `deferred` (name the register row id) |
| `selected_post` | Version selected after remediation — read from the regenerated lockfile, never from the intended floor |
| `evidence` | Pointers to the captured command outputs and lockfile diff |
| `closure` | The post-merge re-scan expectation, or the carried residual row |

Rules: every record carries its reachability row (rule 1); `deferred` implies a register row (rule 7); `selected_post` is lockfile truth (rule 3 evidence, restated because the intended floor and the selected version can legitimately differ, e.g. floor `>=4.1.5` selecting `4.2.1`).

## Why This Matters

Attribution-first converts an unbounded "6 open alerts" anxiety into a bounded per-alert decision, and floor-not-pin keeps remediation inside the existing supply-chain policy surface instead of accumulating exact-version pins that fight future upgrades. The disposition record is what makes the round re-checkable: when the next advisory batch arrives, the previous round's reachability evidence either still holds (reuse) or visibly drifted (re-attribute) — without it, every batch starts from zero.

## When to Apply

- Any Dependabot/npm-advisory triage round on this workspace.
- Reviewing a remediation PR: the checks are floor-vs-pin shape, mirror byte-match, per-alert reachability evidence, and a consumer-mapped verification set.
- The cargo-side analog of the same lockfile-vs-reachability lesson is the workflow doc linked above.

## Examples

- v1.203 P1 round (6 alerts: brace-expansion ×3, fast-uri ×2, markdown-it ×1): all attributed — brace-expansion reachable only through packaging-time `@electron/packager` and build-time codegen with repo-controlled glob constants; fast-uri only through the repo-schema validator; markdown-it through the one shipped-runtime consumer with the vulnerable `linkify` path configured off. Remediated with three floors (`brace-expansion >=5.0.12`, `fast-uri >=4.1.5`, `markdown-it >=14.3.1 <15`) — the same-major check held everywhere (selected majors already matched the patched majors, so no unqualified jump was needed); zero register rows; post-merge re-scan carried as the closure evidence.
