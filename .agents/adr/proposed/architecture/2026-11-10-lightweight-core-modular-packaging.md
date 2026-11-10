# ADR: Lightweight Core and Modular Packaging (“Steady Fast”)

Status: proposed — not implement authority.
Date: 2026-11-10
Scope: Crate-graph and publish strategy to reduce whole-workspace rebuilds, ship stable feature packages incrementally, and keep the user-facing layer on TypeScript while Rust owns narrative/orchestration truth.
Source: migrated from the retired tracked specs corpus

## Problem

Whole-workspace rebuild cost is high and packaging is coupled: `nexus-core` (~167k LOC), `nexus-contracts` (~156k, generated-heavy) and `nexus-orchestration` (~87k, fanning in path deps across the workspace) dominate builds, while most crates share one version and cannot ship stable feature packages incrementally. Scoped `cargo -p <crate>` runs are the daily workaround, and `modules/nexus-module-sdk` is the existing standalone-publish precedent. The user-facing layer must stay TypeScript while Rust keeps narrative/orchestration truth.

## Proposal

### 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. |
| **Document class** | Exploration |
| **Scope** | Crate-graph and publish strategy to reduce whole-workspace rebuilds, ship stable feature packages incrementally, and keep the user-facing layer on TypeScript while Rust owns narrative/orchestration truth. |
| **Coordinates with** | [docs/specs/architecture/overview.md](../../../../docs/specs/architecture/overview.md), [rust-core-service-boundary.md](../../../../docs/specs/architecture/rust-core-service-boundary.md), [dynamic-workflow-and-simulation.md](2026-11-10-dynamic-workflow-and-simulation.md), modules/nexus-module-sdk (existing standalone publish precedent) |

---

### 1. Locked product decisions

1. **User-facing layer stays TypeScript.** `apps/nexus-service`, Electron host lifecycle, React Canvas / Control Room, and high-churn provider SDK glue remain TS. Rust owns domain and execution truth. This is a product lock, not a temporary split.
2. **No greenfield “rewrite everything in Rust.”** The monorepo is already Rust-led for core. Work is **thin the spine and publish packages**, not replace the stack.
3. **No second runtime / second hand-written DTO set** ([STRATEGY.md](../../../../STRATEGY.md)).

---

### 2. Evidence of current pressure

Observed on `main` @ `3263aa71f` (local analysis, Asia/Shanghai 2026-11-10):

- Large crates (approx LOC): `nexus-core` ~167k, `nexus-contracts` ~156k (generated-heavy), `nexus-orchestration` ~87k, `nexus-local-db` ~62k; `nexus-narrative` ~4.2k is comparatively thin.
- `nexus-orchestration` fans in path deps across contracts, preset, knowledge, spoke-adapter, local-db, narrative, creator-memory, `nexus-home-layout`, and `nexus-wasm-host`.
- Root `AGENTS.md` Development Policy treats running CI-scope `cargo clippy --all` / `cargo test --all` on every small edit as an anti-pattern (scoped `-p <crate>` is the daily default); `pnpm run dev:backend:refresh` and the `build:cli` / `build:cli:release` scripts are the routine Cargo/codegen DX paths — full rebuild cost is already treated as a constraint.
- `modules/nexus-module-sdk` is STANDALONE with `publish = true` — the right packaging precedent; most workspace crates still share `0.1.0`.

---

### 3. Target layering

| Layer | Contents | Shape | Release |
| --- | --- | --- | --- |
| Contracts | JSON Schema → codegen | `schemas/` + generated Rust/TS | Lockstep semver (Spoke-like) |
| Lightweight core | World/Timeline/KE/Fork, gateway traits, storage ports; **no** engine/WASM/agent by default | Small Rust libs | Stabilize first |
| Workflow IR | Shared universal IR for generate/compose/validate/simulate/UI/patterns | Thin Rust crate + schema | Stable API; see companion proposal |
| Pattern packages | Versioned reusable subgraph / template packages consumed by Compose and generators | IR + manifest packages (independent semver) | Opt-in content; pin at compose/apply |
| Feature packages | orchestration runtime, **simulator**, agent-host, wasm-host | Cargo features / independent version tracks | Opt-in; simulator omitable from prod graphs |
| Compute modules | Deterministic settle logic | `wasm32` + module-sdk | Operator install; no host rebuild |
| Surfaces | CLI / Connect / TS HTTP / UI (incl. Canvas compose) | Rust bins + **TS apps** | Product versioning |

---

### 4. Rust modularization practices to apply

| Practice | Reference | Application |
| --- | --- | --- |
| Workspace crate splits | Cargo workspaces as recompilation units; keep entry points thin ([InfoWorld](https://www.infoworld.com/article/4050654/organize-rust-projects-for-faster-compilation-with-cargo-workspaces.html); rs4ts compile-time guidance) | Split mega-crates along change frequency; `cargo build -p` daily |
| Feature unification | [RFC 3692](https://github.com/rust-lang/rfcs/blob/master/text/3692-feature-unification.md) `resolver.feature-unification = "workspace"` | Evaluate for cohort switching (`execution` / `compute` / `connect-host`) to avoid rebuild thrash |
| Bevy-style features/plugins | [Bevy cargo features](https://github.com/bevyengine/bevy/blob/latest/docs/cargo_features.md); plugin modularity | `default-features = false`; optional deps behind features; align with existing core cohorts |
| Caution: over-splitting | Bevy community notes crate overhead can dominate when parallelism gains are small | Split on measured fan-out and publish boundaries, not file count |
| Wasmtime plugins / WIT | [WASI P2 plugins](https://docs.wasmtime.dev/wasip2-plugins.html); Component Model | Keep V1 compute ABI; optional WIT as ABI V2 — MUST NOT block V1 packaging |
| Spoke lockstep publish | `spoke-schemas` / `spoke-operations` / `spoke-connect` exact pins | Narrative spine, workflow-ir, module-sdk follow the same ops model |

---

### 5. Relationship to dynamic workflow

The companion proposal [dynamic-workflow-and-simulation.md](2026-11-10-dynamic-workflow-and-simulation.md) depends on this packaging:

- **Workflow IR** is a stable crate so generators, **Canvas compose**, validators, and **pattern runtime** do not link the full orchestration graph.
- **Pattern packages** version independently and are consumed by Compose (end-user nest/combine) and by generators as building blocks.
- **Simulator** is a feature package for review graphs (including nested effect rollup).
- Generation / compose churn stays out of `nexus-core`’s default cohort.

Dual-track roadmap lives in that companion §7 (packaging A + generate/compose/Canvas nest B).

---

### 7. References

- [docs/specs/architecture/overview.md](../../../../docs/specs/architecture/overview.md)
- [rust-core-service-boundary.md](../../../../docs/specs/architecture/rust-core-service-boundary.md)
- Root [AGENTS.md](../../../../AGENTS.md) (build hygiene)
- [modules/README.md](../../../../modules/README.md) (standalone SDK)
- Spoke lockstep packages (crates.io / npm `@42ch/spoke-*`)

## Alternatives considered

Not stated as a separate comparison in the migrated proposal; where the body names a rejected direction, that record stays in the proposal.

## Acceptance criteria

Not stated as a separate acceptance list; where the proposal defines a landing or promotion sequence, it stays in the proposal.

## Risks

- Migrating user-facing TS surfaces into a single Rust binary “for purity.”
- Mandatory canvas drag-edit from scratch as the only strategy authoring path.
- Treating Workflow IR as a **developer-only** tool or forbidding end-user nest/compose.
- Reducing Canvas to a **pure read-only** inspection of Agent output and discarding composition UX.
- Publishing every internal helper crate without a consumer stability story.
- Enabling WIT/Component work as a gate on thinning core or shipping IR v1.

---
