---
report_kind: qc-consolidated
plan_id: 2026-07-03-v1.87-nexus-ui-component-library
iteration: V1.87
reviewers: [qc-specialist, qc-specialist-2, qc-specialist-3]
verdict: Approve
generated_at: 2026-07-03
---

# V1.87 QC Consolidated — 3/3 Approve

## Scope (verbatim across three seats + QA)
- plan_id: `2026-07-03-v1.87-nexus-ui-component-library`
- Review range / Diff basis: `git diff main...iteration/v1.87` (merge-base `ffae19f9` → tip `60916911`; 18 files, +777/-84)
- Working branch (verified): `iteration/v1.87`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`

## Individual verdicts

| Seat | Reviewer | Focus | Critical | Warning | Suggestion | Verdict | Report |
|------|----------|-------|----------|---------|------------|---------|--------|
| 1 | qc-specialist | Architecture & maintainability | 0 | 0 | 1 | **Approve** | `qc1.md` (`a2dbe6f9`) |
| 2 | qc-specialist-2 | Security & correctness | 0 | 0 | 0 | **Approve** | `qc2.md` (`c62517d5`) |
| 3 | qc-specialist-3 | Performance & reliability | 0 | 0 | 2 | **Approve** | `qc3.md` (`4b740c65`) |

**Consolidated verdict: Approve (3/3).** No Critical, no Warning across any seat → no fix-wave required. Proceed to QA.

## Key verification (cross-seat)

- P0 package boundary: `@42ch/nexus-ui` source has **no `.svg` imports** (bundler-agnostic `src`-prop design confirmed by qc1), no imports from `apps/web`/routing/state. Component API well-typed and presentational.
- P0 supply chain: new deps (`react`/`react-dom` peer, `@types/*`, vitest, testing-library, jsdom) audited via `pnpm-lock.yaml` diff — all expected, trusted registries (qc2).
- P0 XSS: zero `dangerouslySetInnerHTML`; `<NexusMark>` is hand-authored safe JSX; `<NexusLogo>` `src` is build-time SVG URL only (qc2).
- P1 path-traversal closure: qc2 adversarial probing — sibling escape (`../workspace-evil/...`, name-extension `my-novel-evil`), `../../` parent traversals, absolute paths (`/etc/passwd`), null bytes, unicode normalization, symlinks, missing in-bounds file — **all rejected** by canonicalize + component-wise `Path::starts_with` (via `resolve_guarded_path`). No residual escape path found. The exact V1.86 residual case now correctly yields `invalid_input` instead of leaking to `FILE_READ_FAILED`.
- P1 error mapping: the implementer-flagged "ALL `BadRequest` → `InvalidInput`" is **intentional and correct** (qc1) — each handler maps `BadRequest` to its domain-appropriate envelope; this read path's data-corruption semantics match `InvalidInput`.
- Gates: `pnpm` build/typecheck/test (7 package + 387 web) green; `cargo test -p nexus-daemon-runtime` green (incl. 4 `manuscript_read_range_*` tests, 2 new regression); `cargo clippy -p nexus-daemon-runtime -- -D warnings` clean; `cargo +nightly-2026-06-26 fmt` clean.
- `wire_contracts_changed: false` confirmed by all three seats (no `schemas/`, `crates/nexus-contracts/src/generated/`, or `packages/nexus-contracts/` changes).

## Residuals to register (all Suggestion → `low`; decision: defer; target V1.88+)

| ID | Source | Title | Severity | Owner | Target |
|----|--------|-------|----------|-------|--------|
| `R-V187-QC1-S001` | qc1 S-001 | `Variant` (nexus-logo.tsx) and `LogoVariantName` (tokens.ts) duplicate the same 4-variant union — unify into one exported type identity | low | @frontend-dev | V1.88+ |
| `R-V187-QC3-P001` | qc3 F-001 | Migrate remaining sync `resolve_guarded_path` call sites (`host_tool_handlers.rs:1435,1502,2136,2280` + `outline.rs:159,239` + `chapters.rs:209,268`) to `resolve_guarded_path_async` to fully honor V1.86 T5 direction (`R-V156P0-M004`) | low | @fullstack-dev | V1.88+ (reliability roadmap) |
| `R-V187-QC3-P002` | qc3 F-002 | Optional `React.memo` on `<NexusMark>` if it lands in high-render surfaces (virtualized lists, motion loops) | low | @frontend-dev | V1.88+ (if observed) |

All three are non-blocking Suggestions registered for backlog tracking (per the Durable Roadmap Gate — not narrative-only). None blocks V1.87 ship or the `R-V186-QC1-S005` closure (which is confirmed closed).

## Residual lifecycle note

- `R-V186-QC1-S005` (the V1.86-deferred medium residual this iteration's P1 closes): lifecycle → `resolved`; resolution plan_id `2026-07-03-v1.87-nexus-ui-component-library`; commit `4eb26a7c`. Verified closed by qc2's adversarial probing.
