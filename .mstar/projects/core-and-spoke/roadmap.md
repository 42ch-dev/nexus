---
project_id: core-and-spoke
title: Core and Spoke — daemon reliability, wire contracts, SPOKE adoption
status: active
created_at: 2026-08-25
residuals_ref: residuals.json
---

# Core and Spoke — daemon reliability, wire contracts, SPOKE adoption

## Direction

Module epic for **daemon core reliability**, **wire contracts**, and **SPOKE
adoption** (lockstep upgrades, spoke-adapter, connect-host serving posture,
yamux/hickory-gated upgrade line). SPOKE-aligned contracts remain the
integration dialect; next spoke bump is blocked on upstream libp2p ≥0.57
(R1/R2 stay in the `_default` register — not migrated).

Historical residuals stay in `_default/residuals.json`. Closed work is
append-only in [_default/shipped-features-tracker.md](../_default/shipped-features-tracker.md).

Migrated from `_default` on 2026-08-25. Row IDs unchanged except the new
SPOKE-adoption tracking row (not a DF/DR id).

**Close protocol:** delete the row here; append the same ID to the shipped archive.

## Open features

| ID | Pillar | Feature | Target | Effort | Notes |
|----|--------|---------|--------|--------|-------|
| DF-V1127-COMPOSITE-PERF | Cross-cutting | Composite-endpoint performance round | V1.128+ | M | Scale; not user-visible at <100 worlds. |
| DF-V1127-NIT-CLOSEOUT | Cross-cutting | V1.126 nit close-out (22) | V1.128+ | S | |
| BL-02 | Cross-cutting | Local Shadow Read / staged change full chain | L | |
| BL-04 | Cross-cutting | Long-running task checkpoint (product-level) | M | |
| DF-V1177-SPEC-STALE | Cross-cutting | Sibling outbox-era spec staleness — daemon-runtime.md:513, local-db-schema.md:310, orchestration-engine.md:392 + drop-migration "V1.159→V1.59" comment typo | next docs/hygiene iteration | S | V1.177 P1 bounded-corpus record-only defers (writable-set constraint; QC Approved). Also fold in 6 legacy `#[allow]` sites in connect invoke/interop (v1.177 qc3 F-001). |

## Spec durable roadmaps (DR-*)

Targets are guidance. Linked `R-*` lifecycle stays in `_default/residuals.json`.

| ID | Item | Target | Track | Linked |
|----|------|--------|-------|--------|
| DR-01 | Daemon retry jitter range expansion | — | Reliability | — |
| DR-02 | Capability-layer metrics overhead benchmarking | — | Reliability | — |
| DR-03 | Daemon no-Profile: background subsystem attach (H2) | follow-up | Reliability | R-V1118-era H2 |
| DR-04 | Desktop clean-home CI leg | — | Reliability | — |
| DR-05 | Secret-store hardening | future | Reliability | — |
| DR-06 | Converge `wait_for_all` timeout enforcement | — | Reliability | — |
| DR-07 | Reference refresh caps + CLI + E2E + OCC | P3 | Reliability | DF-44 shipped core |
| DR-23 | Tool-bridge contract-gap codegen | P4/P5+ | Contract | — |
| DR-24 | ACP wire full JSON Schema drafts | — | Contract | — |
| DR-25 | Capability→ACP schema map + quotas + timeouts | — | Contract | — |
| DR-42 | Holistic route-path review | dedicated | Cross-cutting | — |
| DR-43 | MCA read-path cutover evaluation | V1.146+ | Cross-cutting | — |
| DR-44 | Spoke fork-model participation | — | Cross-cutting | — |
| DR-45 | `order_timeline_events_by_precedes` adapter | stretch | Cross-cutting | — |
| DR-48 | Relationship promotion state machine | post-1.0 | Cross-cutting | — |
| DR-52 | User/global knowledge entry surface | — | Cross-cutting | — |
| DR-53 | Top-level `sync` hard-delete (alias retirement) | — | Cross-cutting | — |
| DR-55 | Registry-integration open items | — | Cross-cutting | — |
| DR-56 | DB-backed revisions table | — | Contract | — |
| DR-57 | ACP sessions/operations/events SSE DTO promotion | — | Contract | — |
| DR-63 | Additive moment-directive response schema + codegen | — | Contract | R-V1151P2-002 |
| DR-67 | workflow-profile §4.5.7 acceptance tests #1–5 | — | Reliability | — |

## SPOKE adoption

No committed target. Blockers R1/R2 remain in `_default/residuals.json`
(`v1.153-dependabot-security`).

| ID | Item | Target | Track | Linked |
|----|------|--------|-------|--------|
| SPOKE-LOCKSTEP | spoke lockstep upgrade line — blocked by yamux 0.12.1 / hickory-proto advisories via libp2p ≥0.57 (R1/R2 in this project's register); next spoke bump evaluation | — | SPOKE | `_default` residuals `v1.153-dependabot-security` R1/R2 |
