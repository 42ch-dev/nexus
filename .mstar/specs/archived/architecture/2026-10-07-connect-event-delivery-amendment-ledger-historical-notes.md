# Connect Event Delivery & Operation Receipts — Delivery Ledger Historical Notes

**Provenance**: frozen from [`specs/active/architecture/connect-event-delivery-and-operation-receipts.md`](../../active/architecture/connect-event-delivery-and-operation-receipts.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-10-07 — the removed text itself records the amendment/freeze date ("Frozen 2026-10-07", four "Amended 2026-10-07" entries); the ledger was the v1.207 delivery record and the branch anchors are the v1.207 P3 delivery provenance. Nothing here is current authority; the wire contract in the live spec is.
**Freeze date**: 2026-10-13

---

## Original title

"# v1.207 Spec — Connect Replay/Negotiation Wire + Operation Receipts"

## Original Status / provenance block

> **Status:** shipped (v1.207, PR #367) — the Connect event-delivery + durable
> operation-receipt surface. Frozen 2026-10-07 (Phase 1 Review & Edit pass 2/3,
> architect) with four in-flight amendments during delivery. Landed in tracked
> specs at v1.207 close; the gitignored iteration-package copy remains the
> working record of the amendment rounds.
> **Document class:** Master (Connect event surface + operation-receipt store:
> the durable contract for WS-lane event delivery, gap/reconcile, and the
> receipt-first recover handshake).
> **Coordinates with:** [spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md)
> (the spoke-connect transport + manifest seam), `schemas/core/`
> (subscribe/gap/receipt schemas), `knowledge/architecture-patterns/connect-event-delivery-and-operation-receipts.md`
> (the distilled reusable shape).
> Resolves compass Q1 and the P2/P3 architect markers; referenced by the
> v1.207 iteration plans (P2 §A/§C, P3 §B/§C). Field names are named **once**
> here (§C) — the plans point at this file instead of restating them.
>
> **Promoted to**: `.mstar/knowledge/architecture-patterns/connect-event-delivery-and-operation-receipts.md`
> (v1.207 §3.2 iteration-close, structured rewrite of the durable shape).
>
> Source survey basis: HEAD 2026-10-07, read-only. Anchor citations at the
> bottom; every placement claim below names the module it lands in.

> Amended 2026-10-07 (Phase 2, architect): §A.2a added — the WS

## Amendment ledger (removed)

> (the P2 Task 1 report);
> §C gained the §A.2a field rows. §A.1–§A.5 and §B are otherwise unchanged.

> Amended 2026-10-07 (Phase 2, second pass, architect): §A.2a(f) added —
> the composition-seam decision resolving the P2 Task 1 BLOCKED round
> (same report); the §A.2 subscribe bullet is re-pointed to the WS
> accept/responder lane; §A.2a(d)'s manifest-lockstep sentence is restated
> (subscribe enters no node-lane set); §C gained the
> `tools.nexus.subscribe` row. The §A.2a(a)–(e) wire contract, §A.3, §A.4,
> §A.5 and §B are otherwise unchanged.

> Amended 2026-10-07 (Phase 2, third pass, architect): §B.2 writer
> admission ruled cooperative — the Connect receipt-writer path,
> resolving the P3 Task 3 blocker
> (the P3 Task 3 report);
> §B.1 compute-Run `session` derivation confirmed run-scoped for the
> session-less daemon lane. §A and §C are unchanged (both rulings are
> P3-internal).

> Amended 2026-10-07 (Phase 2, fourth pass, architect): §A.2a(f)4
> ruled — the WS-lane host hello names `tools.nexus.deliver_events` as a
> **reverse-use** capability, resolving the P2 Task 2 BLOCKED round
> (the P2 Task 2 report):
> the pinned consumer-side reverse-invoke dispatch gate
> (`remote_adapter.rs:1051-1063`) reads the both-hello intersection
> (`remote_adapter.rs:1707-1710`), so consumer-side advertisement alone
> can never authorize the push. §A.2a(a)/(d)/(f)7 restated; the §C
> `tools.nexus.deliver_events` row updated; source anchors gained the
> gate/intersection citations. The id stays consumer-served, the
> node-lane prohibition stands, and §A beyond §A.2a(a)/(d)/(f) and §B

## v1.207 P3 branch anchors (removed from Source anchors tail)

- v1.207 P3 branch anchors for the §B.1/§B.2 rulings (Task-3 report
  HEAD `412989b23`, branch `feat/v1.207-p3-op-receipts`):
  `crates/nexus-local-db/migrations/20260919000001_operation_receipts.sql:37-57`
  — table + payload CHECK; `:67-140` — `guard_operation_receipts_*`
  (admission ruled cooperative, §B.2); `:150-164` — terminal
  immutability; `:168-190` — outbox family.
  `crates/nexus-local-db/src/operation_receipts.rs:262` / `:334` /
  `:385` / `:410` — `begin_operation` / `settle_operation` /
  `classify_recovery` / `recover` (admission-agnostic store).
  `crates/nexus-local-db/src/writer_protocol.rs:65-69` — `WriterMode`;
  `:807-811` — Direct holds the shared migration lock, never the
  exclusive engine lock; `:921-963` — migration quiescence of
  cooperative pools.
  `crates/nexus-spoke-adapter/src/adapter/computable_port.rs:934-937`
  and `apps/nexus42/src/commands/connect/invoke.rs:2190-2193` — the
  Connect host's no-engine-ownership lock (untouched by the ruling);
  `invoke.rs:432-435` / `:451` / `:898-975` — the Connect write routes
  (the `LiveWrite` dispatch the receipt lifecycle wires into).
  `crates/nexus-core/src/execution/compute.rs:254-263` — receipt begun
  before the effect; `:1287-1303` — `compute_run_operation_id`
  (`session = run_id`); `:1357-1397` — `recover_stuck_compute_runs`
  asking by `subject_id`; `capabilities.rs:91` —
  `ToolExecuteRequest.session_id`.

---

## In-text amendment tags removed (rulings retained in the live spec)

- §A heading tag "(P2)"; §B heading tag "(P3)"; §C heading "cross-plan … (P2 transport ↔ P3 semantics)" framing.
- §A.1: "**out of P2 scope** (plan boundary; v1.188/v1.195). P2 therefore adds …" → reworded to "out of scope here; this spec defines …".
- §A.2: "(second amendment — §A.2a(f))" → "(§A.2a(f))".
- §A.2a heading tag "(Phase 2 amendment)"; (f) heading "(Phase 2, second amendment).** Decided on the P2 Task 1 BLOCKED evidence (the P2 Task 1 report):" — provenance dropped.
- §A.2a item 4: "(fourth amendment; the P2 Task 2 BLOCKED ruling)" dropped; "the pre-amendment text erred in naming it the only consumer-side check;" dropped.
- §A.2a item 6: "**Lane address disclosure (v1.210 P3, architect-locked in the Phase 1 Review & Edit chain, 2026-10-10; ships with v1.210).**" → "**Lane address disclosure.**"
- §A.2a item 7: "**WS-lane-vs-node-lane distinction (fourth amendment):**" → tag dropped; "so this amendment changes no external protocol field" → "so this changes no external protocol field".
- §B.1: "(confirmed 2026-10-07, third amendment)" dropped; "Dedupe scope (third amendment):" → "Dedupe scope:".
- §B.2: "(**ruled 2026-10-07, third amendment** — resolves the P3 Task 3 Connect-writer blocker)" → "(ruled cooperative 2026-10-07)".
- Source anchors: heading "(HEAD 2026-10-07)" → "(surveyed 2026-10-07)"; "(reverse-invoke authorization, fourth amendment)" → "(reverse-invoke authorization)".
