# V1.47 Novel Quality Loop Closure — Delivery Compass v1

**Version**: V1.47 delivery  
**Created**: 2026-06-15  
**Status**: **Shipped** (2026-06-15, P-last closeout) — 6 plans all Done (P-1 + P0–P-last); [PR #60](https://github.com/42ch-dev/nexus/pull/60) merged to `main` at `8f4f9f2b` (2026-06-15); `iteration/v1.47` integration branch retired  
**Scope**: `nexus` OSS — **Novel quality loop closure** (`reflection-loop` → findings), **gate/remediation audit**, **serial completion hardening**, **spec reconcile**; bounded P-last hygiene (clippy + author-blocking residuals).  
**Predecessor**: [v1.46/delivery-compass.md](v1.46/delivery-compass.md) (Shipped 2026-06-15, PR #59 merged to `main`)  
**Successor**: TBD (V1.48+)  
**Human map**: [novel-writing/quality-loop.md](../specs/novel-writing/quality-loop.md) · [novel-writing/workflow-profile.md](../specs/novel-writing/workflow-profile.md) · [novel-writing/author-experience.md](../specs/novel-writing/author-experience.md)

**Normative specs** (wave 0 + iteration):

| Document | Role |
| --- | --- |
| [novel-writing/quality-loop.md](../specs/novel-writing/quality-loop.md) | **Shipped (V1.47)** — wave 0; §8 `novel-chapter-review` output contract + `rule_suggestion` metadata |
| [novel-writing/workflow-profile.md](../specs/novel-writing/workflow-profile.md) | **Shipped (V1.47)** — §5.5.4 two-layer rules (`AGENTS.md`); §5.5.6 normative reflection→findings |
| [novel-writing/author-experience.md](../specs/novel-writing/author-experience.md) | **Shipped (V1.46)** — §3.4 reconciled in P3 (V1.47) |
| [creator-workflow.md](../specs/creator-workflow.md) | FL-E `review` stage preset mapping (P0 updates preset id if renamed) |
| [creator-run-preset-entry.md](../specs/creator-run-preset-entry.md) | Remediation targets for P1 |

**Implementation plans** (registered under `.mstar/plans/`):

| Plan ID | Theme |
| --- | --- |
| `2026-06-15-v1.47-harness-docs-prepare` | P-1 — docs-only compass + plans + spec overlays + tracker/status |
| `2026-06-15-v1.47-reflection-loop-findings` | P0 — novel review preset → findings + `rule_suggestion` metadata |
| `2026-06-15-v1.47-gate-remediation-audit` | P1 — remediation + gate chain audit |
| `2026-06-15-v1.47-serial-completion-hardening` | P2 — §4.5.7 tests #1–#3 + completion edge |
| `2026-06-15-v1.47-quality-loop-spec-reconcile` | P3 — author §3.4 ↔ workflow §5.5.6 reconcile (docs-only) |
| `2026-06-15-v1.47-hygiene-and-closeout` | P-last — clippy + bounded residual triage + spec promotion |

**Branch policy** ([`.mstar/AGENTS.md`](../AGENTS.md) § Multi-plan iteration branches):

| Role | Branch |
| --- | --- |
| **Iteration integration branch** | `iteration/v1.47` |
| **Final merge target** | `main` |
| **Pre-implement base** | `main` **must include** V1.46 merge (PR #59, `72eba21b` on `main` as of 2026-06-15) |

| Plan ID | Topic branch | Merges into |
| --- | --- | --- |
| `2026-06-15-v1.47-harness-docs-prepare` | `iteration/v1.47` | `iteration/v1.47` |
| `2026-06-15-v1.47-reflection-loop-findings` | `feature/v1.47-reflection-loop-findings` | `iteration/v1.47` |
| `2026-06-15-v1.47-gate-remediation-audit` | `feature/v1.47-gate-remediation-audit` | `iteration/v1.47` |
| `2026-06-15-v1.47-serial-completion-hardening` | `feature/v1.47-serial-completion-hardening` | `iteration/v1.47` |
| `2026-06-15-v1.47-quality-loop-spec-reconcile` | `feature/v1.47-quality-loop-spec-reconcile` | `iteration/v1.47` |
| `2026-06-15-v1.47-hygiene-and-closeout` | `feature/v1.47-hygiene-and-closeout` | `iteration/v1.47` |

**Worktree policy**:

| Stream | Path | Branch | Notes |
| --- | --- | --- | --- |
| Harness prepare | repo root on `iteration/v1.47` | `iteration/v1.47` | P-1 docs-only (branch checked out at repo root; dedicated worktree optional) |
| Implement | per plan | `feature/v1.47-*` | See dispatch order §0.1 |

**Dispatch order (implement)**:

1. **P0** → merge integration  
2. **P1** → merge integration (depends on P0 findings shape)  
3. **P2** → merge integration  
4. **P3** → merge integration (may use parallel worktree with P2; merge after P2)  
5. **P-last**

QC tri-review and QA use **`Working branch: iteration/v1.47`** at integrated `HEAD`.

---

## 0. Position

V1.46 shipped author desk `--json` findings, spec hygiene, and runtime UX edges. The **largest remaining product gap** is the quality loop **producer** path: `novel-writing/author-experience.md` §3.4 states `reflection-loop` generates findings, but the shipped preset is still a **generic topic demo** and V1.39 plan T3 (wire review → findings) was never completed.

V1.47 closes that gap and hardens the author unblockers carried from V1.46:

1. **P0** — Transform FL-E `review` preset into a novel/work/chapter-aware review that persists findings (and `rule_suggestion` metadata only).
2. **P1** — Fix author-blocking remediation (`R-V146P1-QC3-S1/S4`) and audit gate chains.
3. **P2** — Serial completion test pack (§4.5.7 #1–#3); formal close `R-V138P1-01` if baseline confirms V1.39 P5 guard is sufficient.
4. **P3** — Reconcile specs; promote Draft overlays at P-last.
5. **P-last** — `nexus-orchestration` clippy + bounded residual triage.

North star: **auto-chain and on-demand review both produce durable findings** authors see on `creator works status`.

When this compass conflicts with a later locked implementation plan, the active plan wins for delivery batching only. Normative behavior changes still require spec updates before implementation.

---

## 0.1 Locked Decisions (grill-me 2026-06-15)

| # | Decision | Choice |
| --- | --- | --- |
| 1 | Primary axis | **Novel Quality Loop Closure** — reflection-loop → findings + remediation/gate audit + serial completion hardening |
| 2 | Plan slice | **4 implement + P-last** (+ P-1 harness prepare) |
| 3 | Wave-0 spec | **Amend existing** — `novel-writing/quality-loop.md` Draft V1.47 + `novel-writing/workflow-profile.md` §5.5.x overlays |
| 4 | Out of scope | **Strict novel-only** — DF-59, DF-46/56 full, foreshadowing/event-index, GoNogo preset, platform sync, findings→draft prompt (§5.5.2) → V1.48+ |
| 5 | Residual strategy | **P-last bounded triage** — fix author-blocking + high clippy; defer other lows to V1.48 |
| 6 | P0 integration | **Transform and rename** `reflection-loop` — novel/work/chapter aware + `from-review` findings; **no** parallel generic demo preset |
| 7 | Rule suggestion | **Semi IN** — `rule_suggestion` on finding metadata only; **no** file write in P0 |
| 8 | Findings trigger | **Dual path** — auto-chain review stage + on-demand `creator run <preset_id>` |
| 9 | Layer 2 rules | **`Works/<work_ref>/AGENTS.md`** replaces `Rules/novel-rules.md`; **no** history file; **no** migration of existing Works |
| 10 | P2 R-V138P1-01 | Baseline verify V1.39 P5 `reject_produce_when_novel_complete`; formal close in P2 if sufficient |
| 11 | P2 tests | §4.5.7 acceptance tests **#1–#3 only** (#4 reconcile, #5 resume → V1.48) |
| 12 | P3 | Independent **docs-only** plan; P-last promotes Draft → Shipped |
| 13 | P-last whitelist | `R-V145-PRE-CLIPPY-001`, `R-V146P1-QC3-S1`, `R-V146P1-QC3-S4`; optional `R-V145B2-001/002` |
| 14 | P0 contracts | **No** new wire schema; reuse `POST .../findings:from-review` / `ReviewVerdictFinding` |
| 15 | Runtime rules path | V1.47 **spec normative only** for `AGENTS.md`; `read_rules_layers` / scaffold migration **OUT** (follow-up or V1.48) |

---

## 0.2 Implementation baseline (code review 2026-06-15)

Ground truth on `main` at `72eba21b` (post V1.46 merge).

| Surface | Shipped? | V1.47 action |
| --- | --- | --- |
| `findings` table + CRUD API | **Yes** (V1.39 P1) | P0 consumes via `from-review` |
| `create_finding_from_review` handler | **Yes** (daemon) | P0 wires preset terminal / supervisor hook |
| `reflection-loop` preset | **Generic demo** (`topic`/`content`; `embedded-presets/reflection-loop/`) | **P0 replace** — novel/work/chapter inputs |
| `novel-brainstorm` / `novel-review-master` | **Yes** (V1.39 P2) — consume findings | Unchanged |
| Auto-chain review stage | **Yes** (FL-E; preset id `reflection-loop` today) | P0 must write findings on terminal |
| `reject_produce_when_novel_complete` | **Yes** (V1.39 P5; `run.rs`) | P2 baseline → likely **close** R-V138P1-01 |
| `creator works status --json` findings | **Yes** (V1.46 P0) | P1 remediation audit |
| Layer 2 rules reader | **Yes** — reads `Rules/novel-rules.md` | **OUT** V1.47 — spec declares `AGENTS.md`; runtime follow-up |
| Intake gate wrong remediation | **Open** R-V146P1-QC3-S1 | **P1** |

**P0 baseline**: V1.39 shipped findings **infrastructure** but not review **producer**. P0 completes V1.39 T3 debt; do not re-implement findings CRUD.

---

## 0.3 Grill closure

All V1.47 implement contracts are locked in §0.1 and linked plans/spec overlays. **No open grill items** remain for P0–P-last Execute after P-1 signoff.

---

## 1. Scope Lock

### 1.1 In Scope

| # | Deliverable | Plan | Effort |
| --- | --- | --- | --- |
| 1 | Compass, P-1 + P0–P-last plans, status/tracker/README | P-1 | S |
| 2 | Novel review preset → findings (+ metadata `rule_suggestion`) | P0 | L |
| 3 | Gate/remediation audit + executable remediation tests | P1 | M |
| 4 | §4.5.7 tests #1–#3 + R-V138P1-01 disposition | P2 | M |
| 5 | Spec reconcile author ↔ workflow | P3 | S |
| 6 | Clippy + bounded residual triage + spec promotion | P-last | M |

**Happy path acceptance (implement wave)**:

1. Auto-chain **review** stage and `creator run <review_preset_id>` create at least one `findings` row per successful review pass.
2. `creator works status` lists new findings with `routing_hint` / `target_executor`.
3. Gate failure remediation cites **executable** commands (P1 fixes R-V146P1-QC3-S1).
4. Completed novel Works do not enqueue empty produce schedules (P2 confirms/closes R-V138P1-01).
5. Specs agree: review preset produces findings; Layer 2 normative path is `Works/<work_ref>/AGENTS.md`.

### 1.2 Out of Scope

| Item | Tracker / notes |
| --- | --- |
| DF-59 platform publish | Backlog |
| DF-46 / DF-56 full | Post-V1.42 / FL-D |
| Foreshadowing / event-index depth | V1.48+ |
| GoNogo preset adoption | V1.48+ |
| `creator run rules reset` CLI | V1.48+ |
| Findings → draft prompt enrichment (§5.5.2) | V1.48+ |
| `read_rules_layers` / scaffold → `AGENTS.md` runtime | Spec only V1.47; implement follow-up |
| P0 writing `AGENTS.md` files | Grill #7 metadata-only |
| `Rules/novel-rules.md` migration | No migration per grill #9 |
| New wire schemas in `schemas/` | Grill #14 |

### 1.3 P-last residual whitelist

| ID | Disposition |
| --- | --- |
| `R-V145-PRE-CLIPPY-001` | **Fix** (high) |
| `R-V146P1-QC3-S1` | **Fix** (author-blocking) |
| `R-V146P1-QC3-S4` | **Fix** (author-facing paths) |
| `R-V145B2-001`, `R-V145B2-002` | **Fix or close** if superseded by P3 |
| All other V1.46 open lows | **Defer** `target: V1.48` in P-last |

### 1.4 Platform Integration Policy

Unchanged: `metadata.platform_integration` = `paused — local-only until further notice`.

### 1.5 Success Criteria (prepare wave)

1. V1.47 active in `status.json`, `iterations/README.md`, deferred tracker.  
2. P0–P-last registered `Todo`; P-1 `Done` after PM signoff.  
3. Draft overlays for `novel-quality-loop` + `novel-workflow-profile` §5.5.x.  
4. `pre_implement_gate` → GO after P-1.  
5. No product code in prepare session.

---

## 2. Supersession notes

| Prior artifact | Disposition |
| --- | --- |
| `metadata.v1_47_iteration_placeholder` | Replaced by this compass + active `plans[]` |
| V1.39 plan T3 (wire reflection-loop → findings) | **Superseded by V1.47 P0** |
| `Rules/novel-rules.md` normative authority | **Superseded in spec** by `Works/<work_ref>/AGENTS.md` (runtime deferred) |
