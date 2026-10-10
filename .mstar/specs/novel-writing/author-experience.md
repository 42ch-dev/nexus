# Novel Author Experience — Normative Supplement v1

**Status**: **Shipped (V1.49 P2 — author desk UX integrated)** — baseline Shipped (V1.43) + V1.45 CLI IA amendments + V1.46 Shipped; §8 intake re-trigger + reconcile preview merged from V1.49 P2 overlay  
**Document class**: Feature line (author experience supplement)  
**Created**: 2026-06-12  
**Last updated**: 2026-10-10 (v1.210 P3 — author journey re-narrated canvas-first; retired-command citations removed)  
**Scope**: End-user **ongoing serial** happy path — canvas-first author journey, normative CLI surfaces, remediation chains, and author visibility (spec-only SSOT; **no** `docs/novel-writing-quickstart.md` after P1)  
**Coordinates with**:

- [cli-spec.md](../cli/cli-spec.md) — §6.0B current command IA + §7 first-run UX principles
- [creator-run-preset-entry.md](../archived/creator-run-preset-entry.md) — historical V1.45 CLI IA (its generic preset runner was retired in v1.193 P2; kept for provenance)
- [creator-centric-entry-model.md](../archived/creator-centric-entry-model.md) — historical §3.1 local bootstrap (≤7 steps)
- [workflow-profile.md](workflow-profile.md) — artifact layout + completion §6
- [quality-loop.md](quality-loop.md) — findings + review visibility
- [creator-workflow.md](../creator/creator-workflow.md) — FL-E stage names in narrative

---

## 1. Purpose

V1.36–V1.45 implemented novel-writing **capabilities** across crates. V1.46 does **not** add a new profile or preset grammar. It:

1. **Embeds the author happy path** (formerly BL-10 quickstart) in this spec §3 — compact ~80 lines.
2. **Closes author-desk deltas** — `--json` `findings[]`, per-finding remediation, novel-only scope (P0).
3. **Retires duplicate end-user doc** — `docs/novel-writing-quickstart.md` deleted in P1; agents cite `.mstar/specs/` only.

**v1.210 P3 refresh**: §3's journey is re-narrated to the canvas-first reality — Work creation via the world-bound web canvas, ref-less reads as the default-outline degrade (v1.209 P3), ref-less writes as the typed D7 refusal (v1.210 P2) — and every retired-command citation is removed.

**Part II (optional)** — multi-work switch, multi-volume, inspiration pool — documentation pointers only; shipped in V1.41–V1.44.

---

## 2. Document map (V1.46)

| Section | Content | Owner plan |
| --- | --- | --- |
| §3 Author path | World + Work init → first chapter → serial → quality loop → completion | P1 (narrative); CLI detail in [cli-spec.md §6.0B](../cli/cli-spec.md#60b-v2-命令信息架构权威) |
| §4 Author visibility | Human + machine-readable status surfaces | P0 delta on V1.43 baseline |
| §4.1 `--json` contract | `findings[]` + optional `findings_stale` | P0 |
| §5 Residual pointer | local process (not clone SSOT) | P-last |

**Invariant**: Every command in §3 must exist in [cli-spec.md](../cli/cli-spec.md) at ship time; every canvas action narrated in §3 must be a shipped canvas surface.

---

## 3. Author path — ongoing serial (Part I)

> **CLI detail**: [cli-spec.md §6.0B](../cli/cli-spec.md#60b-v2-命令信息架构权威). This section is the **narrative** happy path only — Work creation runs through the world-bound web canvas.

### 3.1 Prerequisites and workspace setup

```bash
nexus42 system doctor
nexus42 creator register --name "Your Name"
nexus42 creator use <handle>
nexus42 creator workspace init workspace
```

The long-running local service is owned by the **desktop app**: launching the
app starts and supervises the service. The CLI daemon group is retired
(v1.193 P2) — there is no terminal command that starts the service.

### 3.2 World + Work init (canvas-first)

Create or pick a World first — the canvas Worlds page (**Create World**) or:

```bash
nexus42 creator world create --title "Neon River"   # → wld_…
```

Create the Work in the canvas: open the **Create** layout and use **Create
Work** — title, long-term goal, initial idea, the **Work profile** (`novel`),
and the **required World selector**. Work creation is world-bound: the runtime
refuses a Work without a World (400 `world_id_required`), and there is **no CLI
Work-create command** — canvas creation is the product path (the CLI
Work-creation entrance was retired in v1.193 P2).

A freshly created Work is **ref-less** (`work_ref`/`story_ref` not yet
assigned). The runtime stays honest about that state on both sides:

- **Reads render.** The Work Outline and Work Timeline canvases show the
  default outline instead of erroring (v1.209 P3) — the same "outline not yet
  written" presentation as any empty Work; the projection is in-memory and
  never persisted.
- **Writes refuse, typed.** Outline/timeline structure edits on a ref-less
  Work are refused with HTTP 400 `invalid_input` and the stable discriminator
  `error.details.field = "work_ref_missing"`; the message names the recovery
  step — assign the Work's `story_ref` (v1.210 P2, D7). The refusal surfaces
  through the canvas error toast; no in-canvas ref-assignment step exists yet
  (the D7-triggered next-iteration candidate). Wire contract:
  [canvas-strategy-surface.md §3.5](../surfaces/canvas-strategy-surface.md).

The `novel-project-init` scaffold flow then assigns the ref — it scaffolds
`Works/<work_ref>/` and seeds the chapter plan; until it completes, the
`novel-writing` chain is gated on the Work (`previous_preset:
novel-project-init`).

Gate/scaffold failures: remediation cites this spec §3.2 — **not** a quickstart file.

### 3.3 First chapter and serial production

First chapter: outline → draft → finalize via the `novel-writing` preset chain (auto-chain default **on**).

Steer the Work from its **Harness** canvas (Work page → **Open Harness**): the
Idea input's **Run / Steer / Resume** verbs start a preset run, append direction
to a running one, or continue a paused one
([canvas-strategy-surface.md §4.1](../surfaces/canvas-strategy-surface.md)).
From the terminal:

```bash
nexus42 creator works status    # current chapter, progress, next action
```

Serial chapter 2+: daemon auto-chain; inject direction:

```bash
nexus42 creator works inspire <work_id> --note "the partner is the informant"
```

On-disk chapter files: see [workflow-profile.md](workflow-profile.md); missing paths surfaced in status (P2 on-disk hints).

### 3.4 Quality loop — dual preset table (Grill #19)

> **V1.47 shipped**: Review preset produces findings per [quality-loop.md §8](quality-loop.md#8-reflection-loop-output-contract-v147-shipped) (P0). The preset is named `novel-chapter-review` (replaces the former generic `reflection-loop` demo).

| Intent | Preset id | When |
| --- | --- | --- |
| Generate / refresh findings | `novel-chapter-review` | After draft milestones; produces candidate findings |
| Master decision on open findings | `novel-review-master` | When findings need accept/reject/defer |

```bash
nexus42 creator works status                        # list open findings (human)
nexus42 creator works findings list [<work_id>]     # findings read (--status / --severity filters)
nexus42 creator works findings accept <finding_id>  # accept a rule suggestion into the Work's AGENTS.md
```

**Remediation (P0, Grill #7)**: `works status` uses **per-finding `routing_hint`** only — no blanket footer pointing only at `novel-chapter-review`; with **zero** open findings it reports the state alone (v1.193 P2 removed the runner the old master-review suggestion named). Review presets are enqueued as schedule runs carrying the Work's context; the 96h stale-findings auto-schedule is opt-in per Work.

96h master-review banner: visible on `creator works status` (V1.39 P4 baseline).

### 3.5 Completion

When all planned chapters finalized:

```bash
nexus42 creator works status    # COMPLETED marker
nexus42 creator works completion-lock release <work_id>   # optional: write more
nexus42 creator works reopen <work_id> --reason "epilogue"
```

Auto-chain stops on completion (`reject_produce_when_novel_complete` — V1.39+).

### 3.6 Part II appendix (optional, doc-only)

| Topic | Surface | Spec |
| --- | --- | --- |
| Multi-work desk | `creator works list/use/status` | [multi-work-lifecycle.md](multi-work-lifecycle.md) |
| Multi-volume | `volume` in status tables | [workflow-profile.md](workflow-profile.md) §multi-volume |
| Inspiration pool | `creator works pool …` | [work-pool.md](work-pool.md) |

---

## 4. Author visibility (P2 baseline + V1.46 delta)

Authors must answer without reading raw JSON APIs (human path). **Novel profile only** for findings (Grill #6) — generic `works status` does **not** fetch findings.

| Question | Surface (minimum) | Status |
| --- | --- | --- |
| Which chapter is active? | `creator works status` — `current_chapter` + chapter table | Shipped V1.43 P2 |
| Is the Work complete? | Completed banner + `COMPLETED` marker | Shipped V1.43 P2 |
| Open findings? | Count + severity; per-row hints | Shipped V1.43 P2; **remediation delta P0** |
| 96h master-review banner? | Stale banner on status path | Shipped V1.39 P4 |
| Run master review? | `novel-review-master` as a schedule run carrying the Work's context; 96h opt-in auto-schedule | Shipped V1.45; CLI runner retired v1.193 P2 |

Normative finding semantics: [quality-loop.md](quality-loop.md) §3.4.

### 4.1 Machine-readable status (`--json`, V1.46 P0)

For **`work_profile=novel`** only, `creator works status <work_id> --json` **extends** the daemon GET work payload:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| *(work fields)* | object | yes | Unchanged from daemon GET `/v1/daemon/works/{id}` |
| `findings` | array | conditional | Three-state: present-with-data when the findings endpoint is reachable; present-empty when reachable but no open findings; **omitted** when the daemon findings endpoint is unreachable (best-effort degradation). See §4.1 best-effort paragraph (W-1 reconcile) |
| `findings_truncated` | boolean | no | Present (and `true`) only when `findings[]` hit the fetch cap (`FINDINGS_FETCH_LIMIT = 50`); signals more open findings may exist beyond the fetched page. Omitted otherwise (qc3 F-003) |
| `findings_stale` | object | no | Present when 96h master-review stale banner would show (human parity). **Creator-global scope** (not work-scoped): the payload mirrors the human-path stale banner which is printed before the work block and spans all of the creator's works. A JSON consumer must not assume `findings_stale.stale_count` is scoped to the queried `work_id` (W-2 reconcile) |

Generic (non-novel) works: **omit** `findings` fetch; json output is work API only.

**Best-effort degradation**: `findings` is fetched via the daemon findings endpoint
with a short timeout (`FINDINGS_FETCH_TIMEOUT`, 5 s). When that endpoint is
unreachable, `findings` is **omitted** (rather than fabricated as an empty array)
so a JSON consumer can distinguish a genuinely findings-free Work from a
transient daemon fault. `findings_stale` follows the same novel-only,
best-effort contract and uses a matching short timeout (`STALE_FETCH_TIMEOUT`,
5 s; qc3 F-002) so neither subcall can block the JSON status command beyond ~5 s.
The two subcalls run concurrently (`tokio::join!`; qc3 F-001), bounding the
JSON-path fetch by the slower of the two rather than their sum.

---

## 5. CLI copy alignment (remediation SSOT)

When error/remediation conditions occur, user-visible output must include a **single-line next action** referencing:

- **CLI commands / preset ids** → [cli-spec.md §6.0B](../cli/cli-spec.md#60b-v2-命令信息架构权威)
- **Author narrative** → this document §3

| Condition | Minimum remediation |
| --- | --- |
| Local service not reachable | Open the desktop app (it owns the local service); cite §3.1 |
| `preset_gates_failed` | Name gate; cite §3.2 or §3.3 |
| Missing scaffold / intake incomplete | Cite §3.2 |
| Work completed (auto-chain stopped) | Cite §3.5 |
| Open findings (when shown) | Per-finding hint or §3.4 findings leaves |

**V1.46 P1**: remove all `docs/novel-writing-quickstart.md §N` runtime references.

---

## 6. P-last author-path tech-debt (pointer)

This spec does not duplicate local residual registers.

---

## 7. Promotion (iteration close)

At V1.46 P-last:

- [ ] Draft → **Shipped (V1.46)** header
- [ ] BL-10 archive supersede note in shipped tracker (Grill #15)
- [ ] Confirm zero runtime quickstart references

---

## 8. Author desk deltas (Shipped V1.49 P2)

> **Status**: Shipped (V1.49 P2) — P2 overlay merged into Master.  
> **Plan**: 
> **Cross-refs**: findings lifecycle → [quality-loop.md §2](quality-loop.md#2-findings-lifecycle) (6-state V1.49 P0); narrative indexes → [workflow-profile.md §4.6](workflow-profile.md#46-narrative-indexes--f--e-runtime-v149-p1) (V1.49 P1)

### 8.1 Intake re-trigger on existing Work (R-V147P1-01)

**Problem** (as recorded at V1.49): the V1.49-era Work-creation flow scheduled
`creative-brief-intake` for the new Work, but existing Works had no equivalent
intake re-trigger. That creation flow and the intake CLI leaf V1.49 P2 shipped
for this residual were both retired in v1.193 P2 (v1.193 P2-T1 removed the
remaining execution entrances).

**Present lane** (v1.210): `creative-brief-intake` is an embedded preset that
declares **no gates**, so it can be enqueued for an existing Work as a schedule
run — `POST /v1/daemon/orchestration/schedules` with
`preset_id="creative-brief-intake"` and the Work in the schedule input
(`input.work_id`). The enqueue creates no new Work row and does not replace the
Work's auto-chain driver (`driver_schedule_id` is untouched). There is no
dedicated CLI leaf for this since v1.193 P2.

### 8.2 Reconcile preview (R-V148P4-W2)

**Problem**: `creator works reconcile-chapters` mutates filesystem frontmatter and `work_chapters` without preview.

**Shipped CLI** (mirror `works rules reset` safety flags; V1.49 P2):

```bash
nexus42 creator works reconcile-chapters [<work_id>] [--dry-run] [--yes|-y] [--json]
```

| Flag | Requirement | Implementation (V1.49 P2) |
| --- | --- | --- |
| `--dry-run` | Compute `ReconcileReport` only; **no** filesystem or DB writes; **no** runtime lock acquire | Threads `?dry_run=true` query param; daemon skips `RuntimeLockGuard::acquire` and calls `reconcile_from_filesystem(..., dry_run=true)` which gates all writes behind `if !dry_run` while keeping counter increments accurate |
| `--yes` (or `-y`) | Skip interactive confirmation when not dry-run | CLI skips `confirm_reconcile_interactive`; daemon mutating path unchanged |
| default | Prompt before mutating when stdin is a TTY (same policy family as rules reset) | `confirm_reconcile_interactive` errors when stdin is non-TTY (scripted use must pass `--yes`); `--dry-run` takes precedence over `--yes` |

The daemon handler signature is now `POST /v1/daemon/works/{work_id}/reconcile-chapters?dry_run=true|false` (query param optional, defaults to the mutating path).

**Remediation copy** in `creator works status` must cite `reconcile-chapters --dry-run` when filesystem/DB drift detected.
