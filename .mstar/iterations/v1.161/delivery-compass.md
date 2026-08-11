---
iteration_id: V1.161
start_date: 2026-08-12
status: locked
iteration_base_branch: main
target_branch: main
plans:
  - 2026-08-12-v1.161-p1-rust-dependency-refresh
  - 2026-08-12-v1.161-p2-tech-debt-documentation-cleanup
---

# V1.161 Delivery Compass

## Scope

Maintenance iteration — one coherent hygiene unit: **Rust dependency lockfile refresh** + **tech-debt / residual SSOT cleanup**. No feature work; no wire contract changes; no product residual burn-down.

Keeps the dependency tree fresh (≈182 stale patch/minor `Cargo.lock` versions since V1.158) and brings `status.json` residual archival + `tech_debt_summary` back in sync after V1.159/V1.160 closures.

### Autonomous direction lock rationale

Code-first survey of `status.json` residuals, dependabot alerts, `Cargo.lock`, `pnpm-lock.yaml`, and clippy baseline found:

- **≈182 Cargo.lock updates available** (`cargo update --dry-run`) — all non-libp2p / non-spoke patch/minor versions within existing `Cargo.toml` ranges. Lockfile last refreshed around V1.158. Highest-value mechanical cleanup: reduces future audit noise, picks up upstream bug fixes, keeps the tree close to upstream.
- **Frontend deps are essentially current** — only `oxlint` 1.75→1.78 is pending. Included in P1 as a **tiny lockfile-only** frontend hygiene bump (same maintenance unit), not a frontend feature track.
- **3 dependabot / RUSTSEC-class alerts remain open** (yamux 0.12.1 **high** + hickory-proto/resolver **medium/high** surface) — all blocked by `libp2p =0.56.0` pin (upstream 0.57 not released). Documented residuals, not actionable upgrades this iteration. P1 records a fresh `cargo audit` baseline; P2 refreshes residual notes with a dated upstream check.
- **Clippy is clean** (`0` warnings `--all-targets`); historical clippy-debt residual is already `lifecycle: closed`.
- **Residual ledger (2026-08-12 read of `status.json`)**:
  - **6 open** residuals: `R1` yamux (**high**, deferred/blocked), `R2` hickory (**medium**, deferred/blocked; lockfile-only honesty applies), `R-V1148P2-001` run_checker (**medium**, product defer), `R-V1148P1-001` spoke_rules (**low**), `R-V1151P2-001` AC-I5 inspector (**low**), `R-V1151P2-004` acp-host flake (**low**).
  - **9 closed** residuals still sitting in open `residual_findings` buckets (check-wire-drift, scan-probe flake, clippy debt, schedule flake, peer_id, MomentDirective schema, hop-edge cap, R-V1159P1-001, R-V1159P1-002) — **archive targets** for P2 (move to `.mstar/archived/residuals/`, do not delete).
- **`metadata.tech_debt_summary`** is stale relative to the ledger (`updated_at: 2026-08-10`, `total_open: 13` while only **6** rows are actually open; closed-but-unarchived rows inflate the rollup).

**Scale = M** (2 business plans):

- **P1**: Rust dependency lockfile refresh (`cargo update` + workspace verification + oxlint lockfile bump + cargo-audit baseline)
- **P2**: Tech-debt documentation cleanup (archive closed residuals per Profile B procedure, refresh `tech_debt_summary`, dated security residual freshness notes, tracker quick-status touch)

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-08-12-v1.161-p1-rust-dependency-refresh | Rust dependency lockfile refresh + cargo-audit baseline | Todo | ops track; lockfile-only `cargo update`; oxlint lockfile bump; CI-equivalent local gates green; no `Cargo.toml` manifest range change |
| 2026-08-12-v1.161-p2-tech-debt-documentation-cleanup | Tech-debt documentation cleanup + residual archival | Todo | docs track; archive **closed** residuals to `.mstar/archived/residuals/`; refresh `tech_debt_summary`; dated yamux/hickory freshness; **no** open product residual implementation |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain) | 2026-08-12 | pending |
| Dev complete | 2026-08-12 | pending |
| QC complete | 2026-08-12 | pending |
| Iteration close + PR | 2026-08-12 | pending |

## Acceptance Criteria

Iteration-level Done definition (all must be machine- or artifact-checkable):

1. **Cargo.lock refreshed** — workspace `cargo update` applied within existing `Cargo.toml` ranges; **no** workspace `Cargo.toml` version-range / pin edits except documented emergency pin-backs of a single breaking transitive crate via `cargo update -p <crate> --precise <old>` (commit body explains why).
2. **Local CI-equivalent green on refreshed lockfile** — with `SQLX_OFFLINE=true` and `CARGO_TARGET_DIR=$HOME/.cache/nexus-target`:
   - `cargo build --workspace` exit 0
   - `cargo test --workspace` exit 0
   - `cargo clippy --all-targets -- -D warnings` exit 0
   - `cargo +nightly fmt --all -- --check` exit 0
   - After oxlint bump: web unit tests used by the plan (`cd apps/web && pnpm test -- --run`) exit 0
3. **cargo-audit baseline** — `cargo audit` captured for the iteration; known advisories limited to the libp2p-blocked set (yamux / hickory family); **no new actionable** advisory that is fixable without forbidden bumps. Report path or commit body holds the summary.
4. **oxlint lockfile bump** — `oxlint` moves 1.75→1.78 (or current latest within existing range) in `pnpm-lock.yaml` only; no app feature changes.
5. **Residual archival** — every residual with `lifecycle`/`status` ∈ {`closed`,`resolved`} is **moved** out of hot `residual_findings` into `.mstar/archived/residuals/<plan-id>.json` (Profile B: read-merge-write, never overwrite; preserve `closure_note` / `closed_at`). **Open** residuals stay open. `metadata.tech_debt_summary` refreshed so `total_open` equals the count of truly open rows and `by_severity` / `by_target` / `by_plan` reconcile.
6. **Security residual freshness** — open `R1` (yamux) and `R2` (hickory) notes updated with **verified_date: 2026-08-12** (or execute day), crates.io `libp2p` latest observed, still-blocked rationale; R2 keeps lockfile-only honesty if still not in shipped feature graph. Lifecycles remain open/deferred.

## Non-Goals

- **Cargo.toml manifest version-range bumps** (major upgrades such as axum 0.7→0.8, or widening pins) — separate iteration; risk assessment required.
- **libp2p 0.57 / yamux / hickory forced upgrades** — upstream libp2p ≥0.57 not released; do not unsafe-force.
- **spoke version bump** — spoke remains pinned at `=0.9.2`; bumping requires spoke-repo coordination.
- **Feature work** — no DF-*/BL-* implementation (including **DF-V1122-FORK-UI**).
- **Open product residual implementation** — do **not** implement run_checker evaluator, spoke_rules CRUD, AC-I5 inspector body, or flake fixes; P2 is ledger/docs only for those rows.
- **CI workflow restructuring** — no `.github/workflows/` changes.
- **Wire / schema / daemon contract changes** — `wire_contracts_changed: false` on both plans.
- **Knowledge-dir product specs** — no new `{SPECS_DIR}` or `{KNOWLEDGE_DIR}` feature specs in Phase 1 start chain beyond this maintenance package.

## Roadmap Position

- **Previous (V1.160)**: ERA-TAXONOMY completion — World KB entity create-on-absent + World Brief era create UI + Work Brief time-band inheritance + tracker hygiene (shipped / completed).
- **Current iteration (V1.161)**: Maintenance sweep between feature iterations — dependency lockfile refresh + residual/tech-debt SSOT cleanup. Foundation hygiene only.
- **Next iteration (candidate, not a commit)**: **DF-V1122-FORK-UI** (Fork creation UI; spine data ready) **or** user-directed feature work; alternate candidates remain multi-timeline / other open product residuals when triggered. **Owner:** product-manager. **Trigger:** V1.161 ship + dogfood or explicit product direction.
- **Long-term**: three pillars each complete and self-consistent; short maintenance iterations like this keep lockfiles and residual SSOT from rotting between feature drops.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | main |
| `spec_integration_branch` | iteration/v1.161 |
| `target_branch` | main |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Patch/minor update breaks compilation or tests | Low | Med | Full workspace build/test/clippy/fmt after `cargo update`; pin back only the offending crate with `--precise` + commit note; no range edits |
| Test flake blamed on deps | Low | Low | Rerun; open flake residuals (`R-V1151P2-004` etc.) stay documented — do not “fix” flakes in this iteration unless dep-caused and proven |
| cargo-audit reports a **new** actionable advisory | Low | Med | Document; if fixable within lockfile-only constraints, apply; if requires forbidden bump → residual + roadmap, do not expand scope |
| Residual archival overwrite loses history | Med | Med | Profile B procedure: read existing `archived/residuals/<plan-id>.json`, merge, write; never delete closed evidence |
| tech_debt_summary recount drift | Med | Low | Recompute from open-filter only (`lifecycle/status == open` or defer-without-closed); assert `total_open == sum(by_severity)` |

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-08-12-v1.161-p1-rust-dependency-refresh | TBD | pm-acceptance (ops/lockfile; local CI-equivalent evidence) | none expected | TBD |
| 2026-08-12-v1.161-p2-tech-debt-documentation-cleanup | TBD | pm-acceptance (docs/ledger) | open product/security rows unchanged except freshness notes | TBD |

**Findings cleanup (default):** `zero-residual` for plan-introduced issues. Pre-existing open product/security residuals are **in-scope only as documentation/archival**, not as implementation burn-down.

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
