---
iteration_id: v1.177
start_date: 2026-08-27
status: completed
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.177
enforcement: hard
plans:
  - 2026-08-27-v1.177-p0-connect-host-clippy-gate
  - 2026-08-27-v1.177-p1-spec-hygiene-sweep
end_date: 2026-08-27
---

# v1.177 Delivery Compass

## Scope

Post-close-out **stabilization & gate-honesty sweep** — the V1.80 /
V1.93 / V1.157 / V1.158 house pattern after pivot close-out (V1.176
shipped #231 with a tail-free PD-11 roadmap). **No new product surface.**
Two verified self-triggering honesty gaps, both already registered in
the core-and-spoke residual register.

**Who:** (1) developers and integrator partners who ship the third
consumption end — the distributed `nexus-runtime` artifact (`connect-host`,
PD-09 / FL-R); (2) implementers and QC who treat `{SPECS_DIR}` as SSOT
for daemon-sync state.

**Real problem (two verified gaps):**

1. **Connect-host shippers have no failing CI check.** The distributed
   `nexus-runtime` artifact builds with
   `--no-default-features --features connect-host`
   (`apps/nexus42/Cargo.toml` `[[bin]]` rows). Default-graph clippy in
   `ci.yml` never sees this graph (`connect-host` is opt-in; spec §10);
   `runtime-build.yml` only `cargo build`s it. Residual identity:
   `2026-08-19-v1.169-p0-spoke-011-upgrade` **R1** (connect-host clippy
   debt — **not** the yamux dependabot R1 under
   `v1.153-dependabot-security`). Original residual cited 64 *errors*
   in `invoke.rs` / `interop.rs`; verified 2026-08-27 those errors are
   gone and **70 distinct warning sites** remain — 68 in
   `apps/nexus42/src/commands/connect/{invoke,interop}.rs` plus 2 in
   the connect-host-gated example `examples/trpg_raw_bridge.rs`
   (`items_after_statements` ×23, doc-markdown ×17, large-Err-variant
   ×11, over-long fn ×12 (11 in-scope + 1 off-path), `redundant_clone`
   ×2, `_`-prefixed binding ×3, async-no-await ×1, const-fn ×1;
   per-file inventory and dedupe rule in
   lock spec §1 / AR-100). Lint counts are **evidence**, not extra
   product scope. Honest close of that R1 = remaining warnings
   remediated mechanically **and** a durable `-D warnings` gate so the
   class cannot regrow silently.
2. **Spec readers are still told a dropped table is live.**
   `.mstar/specs/outbox-consolidation.md` §2.3 + §6.2/§6.3 still
   document the daemon `outbox` table as "sole access point" and a
   phased removal plan "ending at V1.61+ drop" — but V1.163 P2 dropped
   the table (`migrations/20260812_drop_legacy_outbox.sql`) and removed
   enforcement. Residual **R-V1163P2QC1-001** (open; original target
   "iteration-close spec hygiene"). This iteration **is** that named
   hygiene pass after V1.176 close-out — not a silent trigger bypass.

**Direction lock mode: autonomous** (`/iteration-loop`, no direction
arg, scale `M`). Code-first research 2026-08-27: pivot roadmap tail-free;
all next-iteration triggers from v1.176 compass re-checked against
evidence — DF-87 (no named network-MCP demand), phase E / DF-73 (no
partner signal), DF-88/91/92 (no operator demand), SPOKE lockstep
(libp2p still blocked upstream), DF-62 (no author demand). Stabilization
candidates require no external trigger: both residuals are
self-triggered by already-shipped code/docs drift.

**Scale budget: M** → 2 business plans:

| Plan | Scope | Why separate |
|------|-------|--------------|
| P0 connect-host clippy debt → 0-warning + CI gate | `apps/nexus42/src/commands/connect/*` + `ci.yml` gate | Rust implementation; risk = touching shipped-artifact graph |
| P1 outbox spec hygiene sweep | `.mstar/specs/outbox-consolidation.md` | Docs-only; different executor + review lens |

P0 does **not** absorb a workspace-wide or default-graph warning sweep.
Warnings the connect-host invocation surfaces outside `commands/connect/*`
are per-item `#[expect]` allowances (PL-2), never drive-by refactored.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-08-27-v1.177-p0-connect-host-clippy-gate | Connect-host clippy debt → gate | Done | closes v1.169 connect-host clippy R1 (not yamux R1); merge 2193e37d; QC Approve 3/3; QA PASS |
| 2026-08-27-v1.177-p1-spec-hygiene-sweep | Outbox spec hygiene sweep | Done | closes R-V1163P2QC1-001; merge edd9c595; QC Approve (single); QA PASS 6/6 |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (review chain + PM lock) | 2026-08-27 | done |
| P0 merged to integration | 2026-08-27 | done (2193e37d) |
| P1 merged to integration | 2026-08-27 | done (edd9c595) |
| Iteration close + PR | 2026-08-27 | done |

## Acceptance Criteria

Product-first journeys (architect may add technical clauses; do not weaken).

- **AC-V177-1 — A connect-host change cannot merge with clippy warnings.**
  Journey: a developer (or CI) touching the shipped `nexus-runtime` /
  `connect-host` graph sees a **failing check**, not a silent warning
  dump. Contract (all required):
  - Exact invocation `cargo clippy -p nexus42 --features connect-host --all-targets -- -D warnings` exits 0.
  - In-scope source edits: `apps/nexus42/src/commands/connect/*` only, mechanical per PL-2.
  - Warnings that same invocation surfaces **outside** that directory: per-item `#[expect(…)]` (or equivalent) with written justification — **no** drive-by cleanup, extraction, or refactor of unrelated modules; never a crate-level `#![allow]`.
  - New CI job or step **beside** the existing Rust fmt & clippy job, reusing that job's cache/toolchain pattern (no second matrix). The job fails on any warning. A cleanup PR **without** this gate does **not** satisfy the AC.
  - Runtime behavior unchanged: no public-signature, wire-DTO, or semantic change.
- **AC-V177-2 — A spec reader of daemon outbox state sees shipped reality.**
  Journey: opening `.mstar/specs/outbox-consolidation.md` §2.3 / §6.2 /
  §6.3 does not lead anyone to follow a live "sole access point" or
  "V1.61+ drop" plan. The file states: daemon `outbox` table dropped via
  `20260812_drop_legacy_outbox.sql`; enforcement removed; cloud-line
  `outbox_entries` (legacy-sync, feature-gated) is the **distinct**
  surviving schema. Bounded corpus grep for other *live daemon `outbox`
  table/enforcement* claims in `{SPECS_DIR}`: hits in this file family
  fixed in P1; hits elsewhere recorded with defer rationale. **No writes
  under `{KNOWLEDGE_DIR}`.**
- **AC-V177-3 — Registers close the intended rows only.**
  `2026-08-19-v1.169-p0-spoke-011-upgrade` connect-host clippy **R1**
  closes when AC-V177-1 verifies; **R-V1163P2QC1-001** closes when
  AC-V177-2 verifies. Yamux/hickory R1/R2 stay open (upstream-blocked).
  No new open R# under `Findings cleanup: zero-residual`.

## Non-Goals

- **No libp2p/SPOKE bump** — upstream still gated (libp2p latest
  stable 0.56.0 < 0.57); yamux/hickory dependabot R1/R2 stay open.
- **No flake-hunt** for `nexus-acp-host --lib`
  (R-V1151P2-004): trigger is "flake reproduces"; it did not.
- **No blanket pedantic/nursery enablement** of workspace clippy lints
  (the 2026-04-29 cleanup was scoped; adopting `-W clippy::pedantic`
  repo-wide is a separate decision).
- **No workspace-wide or default-graph clippy sweep** riding P0. P0's
  only extra-module work is per-item expect/allow for warnings the
  connect-host invocation surfaces outside `commands/connect/*`.
- **No `--features connect-host` test-suite expansion** beyond the
  clippy gate (compile-check is the contract; existing crate tests may
  be **run** as non-regression evidence; do not add new behavioral
  tests).
- **No renames/refactors** of connect modules; mechanical lint fixes +
  spec prose only. If a lint fix would change public signatures → STOP
  + escalate.
- **No `{KNOWLEDGE_DIR}` writes** this iteration (Phase 1 §1.6 HARD;
  P1 may grep knowledge for evidence, record-only).
- **No trigger-gated features**: DF-87/88/91/92, DF-73 phase E, DF-62,
  DF-41, DF-81 resume stay untouched.

## Roadmap Position

- **Current iteration (v1.177)**: **DELIVERED** — post-pivot stabilization
  sweep: connect-host clippy debt eliminated (70 → 0 warning sites) with a
  durable CI gate (`rust-clippy-connect-host`, exact AR-99 invocation); core-and-spoke
  R1 + R-V1163P2QC1-001 closed; `outbox-consolidation.md` describes V1.163+
  reality. Carry-forwards recorded below (non-blocking, explicit).
  No new product surface.
- **Next iteration (owner: PM; trigger-based)**, unchanged from the
  v1.176 ladder: (a) pivot phase E (DF-73) **if** partner demand lands;
  (b) DF-87 HTTP MCP transport **if** network-exposed MCP demand is
  named; (c) developer-DX remainder (DF-88 / DF-91 / DF-92 / DF-46 as
  their triggers fire); (d) core-and-spoke / SPOKE lockstep re-check
  when libp2p ≥0.57 ships. Candidate hygiene closeouts
  (DF-V1127-NIT-CLOSEOUT, DF-V1122-V1121-RES) need PM prioritization
  **before** they enter a later compass — they are not a fifth ladder
  rung.
  - **v1.177 carry-forwards (explicit; (i)+(ii) non-blocking, (iii)
    deferred by plan design)**: (i) ~~workspace fmt drift on
    `crates/nexus-orchestration/src/capability/watch.rs`~~ **resolved
    on integration** — standalone fmt commit `3d8666b2` applied the
    pinned-nightly formatting after QC confirmed it would block
    rust-checks' `fmt --all` gate on this PR (drift inherited from the
    V1.176 merge ab87a107); watch tests re-run green (19/19). (ii) 6
    legacy `#[allow]` sites outside the P0 set remain (qc3 F-001;
    hygiene candidate). (iii) sibling spec staleness (daemon-runtime.md:513,
    local-db-schema.md:310, orchestration-engine.md:392) + drop-migration
    "V1.159→V1.59" typo comment (P1 corpus record-only defers per plan's
    writable-set constraint; Durable Roadmap row to be added by PM).
- **Long-term Done:** PD-11 developer-first pivot remains tail-free —
  every consumption end (CLI / daemon API / MCP / `nexus-runtime`+Connect)
  stays honest; residual registers converge toward empty except
  upstream-blocked rows.

## Product locks (this pass — product-manager)

Phase 1 pass 1/3, renumbered pass 2/3 (architect). Normative copy also
lives in `specs/v1.177-stabilization-lock.md` (PL-1..3). Architect
continues the corpus-global **AR-*** sequence and never reuses PL-* for
architecture records.

**ID-space convention (resolved pass 2/3, architect):** PL IDs are
**per-iteration scoped and restart at PL-1** each iteration (v1.173,
v1.174, v1.175, v1.176 all run PL-1..N); cross-iteration references must
be qualified — "V1.176 PL-10" ("no riders") is a different lock from
v1.177 PL-1 below. AR IDs are **one corpus-global continuing sequence**
(v1.176 ended at AR-98), so this iteration's architecture records are
AR-99..AR-105. Pass 1's draft numbering (PL-10..12 / AR-98..104, which
collided with v1.176's AR-98 and implied a false PL continuation) was
renumbered accordingly; v1.176's IDs are untouched.

| ID | Lock |
|----|------|
| PL-1 | P0 Done = a **failing CI check**, not a one-time cleanup. Test: the workflow YAML contains the exact connect-host clippy invocation with `-D warnings`; deleting the cleanup while keeping the job would fail CI; a cleanup PR without the job fails AC-V177-1. |
| PL-2 | Lint remediation is **mechanical-only** in `commands/connect/*`. Test: P0 `git diff` source hunks are that directory + `ci.yml` (+ per-item `#[expect]` at the named off-path sites, verified 2026-08-27: 2 in `examples/trpg_raw_bridge.rs`); no public signature / wire DTO / runtime behavior change; STOP + escalate if a fix would. Crate-level `#![allow]` forbidden; allowances are `#[expect]` only, enumerated in the SDD ledger. |
| PL-3 | Spec edits describe **shipped reality as of V1.163+**. Test: §2.3 / §6.2 / §6.3 match migration `20260812_drop_legacy_outbox.sql`; they do not present live daemon-`outbox` enforcement or a future V1.61+ drop plan; `outbox_entries` is named as the distinct surviving cloud-line schema. No `{KNOWLEDGE_DIR}` writes. |

## Quality Gate Summary

| Plan | QC | QA gate | Residuals |
|------|----|---------|-----------|
| P0 connect-host clippy gate | Approve 3/3 (qc1 architecture / qc2 security-correctness / qc3 perf-reliability; 0 Critical, 0 Warning; suggestions informational) | PASS mandatory — legs A1–A5 green (`clippy` connect-host + default both exit 0, fmt `-p nexus42` green, 1456/1456 tests); A6 DoD cross-check pass with the Phase-5 CI-run check explicitly deferred by design | zero |
| P1 outbox spec hygiene | Approve (single-seat inline; 2 record-only suggestions) | PASS mandatory — legs B1–B6 green (facts vs migrations verified, corpus disposition matches, diff scope exactly 2 spec files) | zero |

Consolidated reports: `.mstar/sdd/2026-08-27-v1.177-p0-connect-host-clippy-gate/review/qc-consolidated.md`, `.mstar/sdd/2026-08-27-v1.177-p1-spec-hygiene-sweep/review/{qc.md,qa-report.md}`. Raw per-seat reports in the same `review/` dirs.

## Compound Round Summary

Package inventory (`v1.177/`): only the lock spec (`specs/v1.177-stabilization-lock.md`) — **Keep snapshot** (iteration-scoped AR/PL records; already tracked on the integration branch; no cross-iteration process knowledge beyond what plan artifacts carry). Plan-round candidates screened via Q1–Q8:

- "Cache-pass emits no clippy messages — inventories need `cargo clean -p`" → Skip (≤2 Yes; caveat recorded in task-1-report.md + qa-report.md).
- "AR global sequence / PL per-iteration restart ID convention" → Skip (recorded normatively in Product locks section above; self-documenting).

No new knowledge docs; no `{KNOWLEDGE_DIR}` changes; CONCEPTS.md unchanged.

## Iteration Retrospective (minimal)

- 做得好的：autonomous lock 两候选均带实测证据（本地 clippy 计数 + CI workflow 缺口核实）；P0 implementer 单轮 fix wave 清零 70 站点；QC tri 三席同消息并行零冲突；继承的 fmt drift 在 close 阶段被拦下并以独立 commit 处置，未污染 P0 的 mechanical-only 范围。
- 待改进：feature-graph 的 fmt 门与 CI 冷启动验证在 Phase 5 才能闭环；P1 类"已知 defer"应在锁定时同步写 Durable Roadmap 行，而非依赖 QC 提醒。


## Delivery Branch Policy

| Field | Value |
|-------|-------|
| iteration_base_branch | main |
| spec_integration_branch | iteration/v1.177 |
| target_branch | main |

House pattern (STRATEGY V1.39 "integration branches via PR only"):
base `main` → integration `iteration/v1.177` → PR back to `main`.
