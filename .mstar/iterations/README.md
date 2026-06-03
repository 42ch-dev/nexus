# Iteration Specs

Iteration-level specifications for Nexus delivery tracks.

This directory holds all iteration-scoped specs, including:

- `*-delivery-compass-*.md` — version delivery compasses (scope, milestones, acceptance, risk)
- Legacy `v1.*` compass artifacts (overview, matrix, program notes) with non-standard names

Implementation-detail SSOT that is not iteration-scoped stays in [`.mstar/knowledge/`](../knowledge/README.md).

## Index

### Delivery compasses

| Document | Version | Status |
| --- | --- | --- |
| [v1.2-delivery-compass-v1.md](v1.2-delivery-compass-v1.md) | V1.2 | Historical |
| [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md) | V1.3 | Historical |
| [v1.4-delivery-compass-v1.md](v1.4-delivery-compass-v1.md) | V1.4 | Historical |
| [v1.5-nexus-delivery-compass-v1.md](v1.5-nexus-delivery-compass-v1.md) | V1.5 | Historical |
| [v1.6-delivery-compass-v1.md](v1.6-delivery-compass-v1.md) | V1.6 | Historical |
| [v1.7-delivery-compass-v1.md](v1.7-delivery-compass-v1.md) | V1.7 | Historical |
| [v1.8-delivery-compass-v1.md](v1.8-delivery-compass-v1.md) | V1.8 | Historical |
| [v1.9-delivery-compass-v1.md](v1.9-delivery-compass-v1.md) | V1.9 | Historical |
| [v1.10-delivery-compass-v1.md](v1.10-delivery-compass-v1.md) | V1.10 | Historical |
| [v1.11-delivery-compass-v1.md](v1.11-delivery-compass-v1.md) | V1.11 | Historical |
| [v1.12-delivery-compass-v1.md](v1.12-delivery-compass-v1.md) | V1.12 | Historical |
| [v1.13-delivery-compass-v1.md](v1.13-delivery-compass-v1.md) | V1.13 | Historical |
| [v1.14-delivery-compass-v1.md](v1.14-delivery-compass-v1.md) | V1.14 | Historical |
| [v1.15-delivery-compass-v1.md](v1.15-delivery-compass-v1.md) | V1.15 | Historical |
| [v1.16-delivery-compass-v1.md](v1.16-delivery-compass-v1.md) | V1.16 | Historical (Shipped) — command IA big-bang |
| [v1.18-delivery-compass-v1.md](v1.18-delivery-compass-v1.md) | V1.18 | Shipped |
| [v1.19-delivery-compass-v1.md](v1.19-delivery-compass-v1.md) | V1.19 | Shipped |
| [v1.20-delivery-compass-v1.md](v1.20-delivery-compass-v1.md) | V1.20 | **Shipped** (2026-05-19) |
| [v1.21-local-platform-isolation-delivery-compass-v1.md](v1.21-local-platform-isolation-delivery-compass-v1.md) | V1.21 | **Shipped** (2026-05-21) — plan archived |
| [v1.22-cli-deprecation-cleanup-delivery-compass-v1.md](v1.22-cli-deprecation-cleanup-delivery-compass-v1.md) | V1.22 | **Shipped** (2026-05-21) |
| [v1.24-knowledge-crates-alignment-audit-compass-v1.md](v1.24-knowledge-crates-alignment-audit-compass-v1.md) | V1.24 audit | **Shipped** (2026-05-22) — doc refresh + KCA-002/003 |
| [v1.25-knowledge-crates-product-wiring-audit-compass-v1.md](v1.25-knowledge-crates-product-wiring-audit-compass-v1.md) | V1.25 audit | **Shipped (partial)** (2026-05-22) — in-memory wiring; superseded by V1.26 |
| [v1.26-local-persistence-delivery-compass-v1.md](v1.26-local-persistence-delivery-compass-v1.md) | V1.26 | **Shipped** (2026-05-23) — local SQLite persistence, reference MD layout, context productization |
| [v1.27-local-authoring-delivery-compass-v1.md](v1.27-local-authoring-delivery-compass-v1.md) | V1.27 | **Shipped** (2026-05-24) — CLI-first local authoring writes, User knowledge SQLite, context closure, API hygiene, `acp agent use` |
| [v1.28-context-and-agent-host-delivery-compass-v1.md](v1.28-context-and-agent-host-delivery-compass-v1.md) | V1.28 | **Shipped** (2026-05-25) — `assemble-moment` SSOT, structured KB query, Agent Host Batch 1 |
| [v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md](v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md) | V1.29 | **Shipped** (2026-05-26) — Author Intelligence (FL-A/B), Agent Host Batch 2, spec/tracker hygiene |
| [v1.30-residual-convergence-delivery-compass-v1.md](v1.30-residual-convergence-delivery-compass-v1.md) | V1.30 | **Shipped** (2026-05-26) — residual convergence (R5–R20) |
| [v1.31-agentic-design-patterns-delivery-compass-v1.md](v1.31-agentic-design-patterns-delivery-compass-v1.md) | V1.31 | **Shipped** (2026-05-30) — FL-D partial close: orchestration de-stub + 2 Agentic Design Pattern presets |
| [v1.32-preset-quality-gate-delivery-compass-v1.md](v1.32-preset-quality-gate-delivery-compass-v1.md) | V1.32 | **Shipped** (2026-06-03) — preset validator quality gate + `SEC-V131-01` |

### Reference compasses

| Document | Version | Status |
| --- | --- | --- |
| [v1.23-architecture-crate-wiring-reference-compass-v1.md](v1.23-architecture-crate-wiring-reference-compass-v1.md) | V1.23 reference | Reference — non-binding; Cargo target largely met; see V1.24/V1.25 audits for product gaps |

### Legacy `v1.*` iteration artifacts

| Document | Version | Description |
| --- | --- | --- |
| [v1.1-overview-v2.md](v1.1-overview-v2.md) | V1.1 | Program overview snapshot (status-aligned). |
| [v1.2-reclassification-matrix-v1.md](v1.2-reclassification-matrix-v1.md) | V1.2 | Cross-version reclassification matrix. |
