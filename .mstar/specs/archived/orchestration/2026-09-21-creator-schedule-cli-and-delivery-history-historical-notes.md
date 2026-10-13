# Creator Schedule — Retired CLI Group and V1.4/V1.5 Delivery History Historical Notes

**Provenance**: frozen from [`specs/active/orchestration/creator-schedule-and-core-context.md`](../../active/orchestration/creator-schedule-and-core-context.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 deleted the `nexus42 schedule` / `daemon schedule` CLI group and the daemon-crate HTTP handlers this spec recorded (§8, §12 Steps 4–5); the §11 V1.5 forward-compatibility preview was superseded earlier, when V1.5 WS-D shipped the clock poller and `context.summarize` (2026-04-19, commits `5c617cdf9`/`f611adc86`). Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed header lines

**Status**: Shipped (V1.4 WS7 → V1.34 agent-host + schedule wiring). The schedule/core_context primitives landed across V1.4–V1.34; the canonical SSOT for ongoing schedule work is now [`creator-schedule-and-core-context.md`](../../active/orchestration/creator-schedule-and-core-context.md) cross-referenced from `orchestration-engine.md`.

**Author**: @project-manager (2026-04-17 prep-phase spec); to be co-signed by @architect before WS7 implement.

**Wave-0 design inputs consumed**: (re-titled "Coordinates with" in the live doc; removed bullet: "program-level scope is historical (not clone SSOT)")

## §2 removed provenance line and version framings

From the 2026-04-17 brainstorming and its follow-up (item 3 of the 2026-04-17 PM reply):

- `**No priority / preemption in V1.4**` qualifier.
- `the `_system.maintenance` preset handles daemon housekeeping` (daemon host retired).
- `**`iterated_experience` in V1.4 is "preset `context_update` hook only" (Q1=D answer, 2026-04-17)** — the derivation trace enum **reserves** `kind: "llm_summarize"` but V1.4 does not emit that kind; **V1.5 implemented** a `context.summarize` capability … that writes this kind without schema migration.` (kept present-tense without the Q1/V1.4/V1.5 narration)
- `**Seed → first `core_context` semantics governed by preset** (Q2=C answer, 2026-04-17) … V1.4 built-in presets default to …` (kept present-tense without the Q2/V1.4 narration).
- `**Trigger model**: V1.4 supported on-demand triggers … **V1.5 WS-D added wall-clock / cron triggers** …` (kept present-tense).
- `**Auto-chain side-input (V1.39)** — inspiration append (`creator run continue --note`, …)` (entry name retired; live wording uses `creator works inspire --note`).

## §3/§5/§6/§7 removed version markers (statements kept)

- `scheduled_at` comment `// nullable; V1.4 ignored; V1.5 clock-trigger field`.
- `ParallelAny` comment `// escape hatch; V1.4 uses sparingly`.
- `CoreContextPayload` doc comments `(V1.4 minimum)` / `(V1.4 optional …)`.
- `LlmSummarize` comment `// V1.5+ only; V1.4 does not emit`.
- §4 rules: `(new module in `nexus-orchestration`; see §12)` pointer into the removed delivery section.
- §5.2: `one ACP worker subprocess in V1.4 MVP`.
- §6.2 heading `(explicitly implemented)` + `V1.4` qualifiers in bullets; §6.3 heading `(implemented in V1.5)` + `declared in the enum since V1.4. **V1.5 implemented** … zero schema migration was required`; `CLI history display` claim.
- §6.5: `via CLI (§8) or HTTP (§9)`; `(`schedule context-history <id> --show-content`)` retired CLI spelling; label suffix `(V1.5+)`.
- §6.6 heading `(V1.29)`; `Schedule a cron trigger via the daemon scheduler`.
- §7: `that WS7 introduces`; `existing V1.4-era presets`; `V1.4 built-in presets` (×3).

## §8 CLI Command Surface — removed whole section (retired `nexus42 schedule` group, deleted v1.193 P2)

All commands live under `nexus42 schedule`. V1.4 minimum:

### 8.1 `schedule add`

```
nexus42 schedule add --preset <preset-id> --creator <creator-id>
                     [--seed <text> | --seed-file <path>]
                     [--after <schedule-id> | --parallel-with <schedule-id> | --parallel-any]
                     [--label <name>]
                     [--start-now | --start-when-eligible]       # default: --start-when-eligible
```

Returns: new `ScheduleId` and the computed `core_context` v0 preview.

### 8.2 `schedule edit <id>`

```
nexus42 schedule edit <schedule-id> (
    --append <text> | --append-file <path>
  | --replace <text> | --replace-file <path>
  | --struct-merge-file <json-path>
  | --struct-remove <path>
)
```

Writes a new `core_context_versions` row with `kind: UserEdit`. Returns the new version number.

- `--replace` is **allowed** for UserEdit (contrast with PresetHook which is append-only). Users retain full control.
- Edits on `Completed` / `Cancelled` / `Failed` Schedules return an error — no edits after terminal status.
- Edits on `Running` Schedules succeed and take effect at the next state transition (see §3.3 and §6.4).

### 8.3 `schedule remove <id>`

Equivalent to `schedule cancel <id>` in V1.4 — only terminal statuses can be hard-removed (deletes the row). Non-terminal: cancel first, then remove is a separate call. V1.4 keeps it simple: remove is "cancel if needed, then delete row after a grace window". An admin flag `--force` allows immediate deletion.

### 8.4 Query commands

```
nexus42 schedule list [--creator <id>] [--status <status>]
nexus42 schedule inspect <schedule-id>
nexus42 schedule context <schedule-id>                        # current core_context content
nexus42 schedule context-history <schedule-id> [--show-content]
```

### 8.5 Control commands

```
nexus42 schedule start <schedule-id>              # force pending→running if eligible
nexus42 schedule pause <schedule-id>              # non-terminal only
nexus42 schedule resume <schedule-id>             # from paused
nexus42 schedule cancel <schedule-id>             # non-terminal only
nexus42 schedule advance <schedule-id>            # (existing, from orchestration-engine.md §10.4) — manual state advance on a running Schedule
```

### 8.6 Introspection

```
nexus42 schedule timeline --creator <id> [--days <N>]
```

Returns a human-readable summary of the creator's Schedule timeline (past N days + upcoming / queued). Design goal from user's original brief: "用户应该可以随时了解到 creator 之后的一段时间有什么规划，打算做什么事，这些事完成的情况怎么样了".

## §9 removed provenance and stale namespace

- Intro: `Following the pattern established in [acp-client-tech-spec.md] §4.3 (orchestration control endpoints), the new endpoints:` (§4.3 anchor no longer exists in the restructured acp-client spec).
- Heading/table namespace `/v1/local/orchestration/schedules/*` — the served family is `/v1/daemon/orchestration/schedules*` (`apps/nexus-service/src/execution.ts`; contract types `crates/nexus-contracts/src/local/schedule/http.rs`).
- Signal row `{signal: start|pause|resume|cancel|advance}` — the current contract's signal set is `pause|resume|cancel|continue`.

## §10 removed migration narration

**Migration safety**: Additive only; no existing tables touched. Running on a prior V1.4-pre-WS7 database just adds the three tables empty; prior `orchestration_sessions` rows remain and can be associated retroactively if needed (not required — the engine continues to function with no Schedules).

## §11 Forward Compatibility with V1.5 Iteration — removed whole section (preview superseded by the shipped capability; live stub retains the §11 anchor)

Per the 2026-04-17 user direction: "seed 是基础，但它需要不断迭代为完善的 core prompt，融合 user edits + 编排策略里 LLM 辅助下 edit 综合 summary 而成，它是动态的".

V1.4 ships the **data model + CRUD + preset hook** — everything except the automatic LLM-driven iteration. V1.5 completes the story by adding a `context.summarize` capability. The handover surface is:

### 11.1 What V1.5 adds (preview; **not** V1.4 scope)

- New capability `context.summarize` (`crates/nexus-orchestration/src/capabilities/context_summarize.rs`) that takes (current `core_context`, state execution trace, optional template) and writes a new `core_context_versions` row with `derivation_kind = 'llm_summarize'`.
- Preset YAML can declare `context_update.kind: llm_summarize` (new variant) with a template_file and capability binding — engine routes this to `context.summarize` instead of the simple text-append path.
- `schedule edit --allow-auto-merge` (optional user control for in-flight edits that would benefit from an automatic merge with the pending summary).

### 11.2 Why V1.5 needs zero V1.4 schema migration

- `DerivationStep::LlmSummarize` is already a variant in the Rust enum in V1.4 (marked `#[non_exhaustive]` but defined).
- `core_context_versions.derivation_kind` TEXT column already accepts `'llm_summarize'` — V1.5 just starts writing rows with that value.
- CLI `schedule context-history` already handles the label "LLM summarise" — the logic is "match on kind, fall through to a generic label"; no new code path for display.
- HTTP endpoints are the same — payload bytes change, envelope doesn't.

### 11.3 Why this satisfies the user's "dynamic iteration" intent

The user is not asking V1.4 to **implement** dynamic iteration — they are asking V1.4 not to **preclude** it. The concrete commitments:

- (1) core_context **is** versioned (not a single mutable string).
- (2) Every transition records **why** it happened (derivation trace).
- (3) User edits and preset actions can both contribute to derivation.
- (4) V1.5's LLM summarise is already a first-class variant, just not wired to any capability yet.

When V1.5 lights up the `context.summarize` capability and points one or two presets at it, the "dynamic iteration" becomes real — with no V1.4 data being stranded.

## §12 Migration / Implementation Steps — removed whole section (past delivery record)

> **Historical delivery record (v1.193 P2 supersession):** the steps below follow the V1.4 delivery compass WS7 ordering and name the pre-retirement host. The daemon-crate handler path in Step 5 and the `nexus42 schedule`/`daemon schedule` CLI group in Step 4 were deleted in v1.193 P2. Current owners: the HTTP endpoints are the retained `/v1/daemon/orchestration/schedules*` family served by `apps/nexus-service` (`src/execution.ts`) over `nexus-core`; the retained CLI scheduling surface is `creator works cron` declaration editing. Steps 1–3 (contracts, `nexus-orchestration` supervisor, core_context engine) name surviving crates.

Follows the V1.4 delivery compass WS7 ordering.

### Step 1 — Data-model crate

- Author `crates/nexus-contracts/src/local/schedule/` module per §3.
- Add SQLite migration per §10 to `crates/nexus-local-db/migrations/`.
- Unit tests: round-trip serialise each enum variant, FK constraint sanity.
- **Evidence**: `cargo test --workspace` clean; migration is forward-only (no rollback SQL in V1.4).

### Step 2 — Schedule supervisor module

- New module `crates/nexus-orchestration/src/schedule/`.
- `ScheduleSupervisor` trait + default impl backed by `SqliteSessionStorage` (extended or sibling table access).
- Admission rules from §5.1; single-ACP-worker coordination from §5.2.
- Integration with engine's `run_step`: supervisor notified on session terminal events.
- **Evidence**: unit tests for admission under every concurrency variant; multi-Schedule roundtrip test (serial two, then a parallel one, then a cancel).

### Step 3 — `core_context` derivation engine

- `CoreContextManager` struct that, given a Schedule and a `DerivationStep`, writes the next immutable version and updates the Schedule's `current_core_context_version`.
- Handlebars binding extension so templates can reference `{{core_context.*}}`.
- Preset loader additions for `initial_action` + per-state `context_update`.
- **Evidence**: unit test "write v0 from seed; edit appends v1; preset hook writes v2; template renders correct content at each state entry".

### Step 4 — CLI command group

- Implement `nexus42 schedule add/edit/list/inspect/context/context-history/start/pause/resume/cancel/advance/remove/timeline` per §8.
- Error surfaces for all validation (invalid EditOp kind against PresetHook-only constraint, terminal-status edits rejected, etc.).
- **Evidence**: cli integration tests against a mock daemon.

### Step 5 — HTTP endpoints

- Axum handlers under `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` (**historical** — deleted with the crate in v1.193 P2; the retained family is now served by `apps/nexus-service/src/execution.ts`).
- All endpoints per §9.
- **Evidence**: HTTP smoke tests (add → list → edit → signal → history).

### Step 6 — Preset YAML updates + built-in preset enablement

- Update `novel-writing` preset to use `initial_action: { kind: seed_direct }` (explicit; same behavior as default but sets convention).
- Add a `context_update` hook to one state of `novel-writing` as a demonstrator (e.g. `outlining` state appends its output to core_context so `drafting` sees it).
- Update `_system.maintenance` preset to use `initial_action: { kind: seed_direct }` (trivially; no seed meaningfully used for that preset).
- **Evidence**: E2E test (R11 below): `novel-writing` runs, user edits mid-outlining, drafting state sees edited core_context.

### Step 7 — Regression-gate tests

- New tests listed in V1.4 delivery compass §6 (R11–R14; refresh after landing this spec):
  - R11: `schedule add` → `schedule inspect` roundtrip; core_context v0 equals seed.
  - R12: mid-execution `schedule edit` creates v1; running state finishes on v0; next state sees v1.
  - R13: preset `context_update` hook fires at state exit, writes `PresetHook` version.
  - R14: dependency chain `A → B` auto-advances when A completes.

## §13 removed version phrasings (deferral statements kept)

- OQ-7/OQ-8 openers `Still V1.5+.`; `out of scope for V1.4`; `in V1.4`; `V1.4 Schedule admission`; `V1.5 either multiplexes one worker or spawns sibling workers`.
- OQ-6-extension: `V1.4 reserves the `scheduled_at` column; V1.5 adds a clock poller … Zero schema migration required. **(Shipped V1.5 WS-D — clock poller.)**`
- OQ-S2: `V1.4 enums are designed open so this hopefully doesn't happen soon`.

## §14 removed reference

- `daemon-lifecycle-api.md` — supervisor start/stop coupled to `Running`/`Stopping` (file does not exist at HEAD).
- `cli-spec.md` §10 trailing parenthetical `(this doc's endpoints extend the `/v1/local/*` namespace; no new v1-spec clause needed for V1.4 since endpoints are local-only)`.

## Other bare markers reworded in place (no independent content)

- `// crates/nexus-contracts/src/local/schedule/mod.rs (WS7-new)` and `core_context.rs (WS7-new)` module comments.
- `PresetSeedExpansion … (Q2=C; rare)` comment tag.
- CLI-spelling rewords where the retired `nexus42 schedule` surface was named and a concept wording is now used: `a user schedule edit <id>` → core-context edit (§2 in-flight bullet, §3.3, §6.2 UserEdit); `schedule add` timing → "when the Schedule is added" (§6.2 Seed / PresetSeedExpansion); `schedule add B --after A` / `--after-or-manual` → `depends_on` + `concurrency` declarations (§5.3); `--parallel-with` → `concurrency: ParallelWith` (§2); `before schedule add creates the row` → "before the Schedule row is created" (§7.3); `schedule context-history … --show-content` dropped (§6.5).
- §6.1 statement kept: "one creator has one ACP worker subprocess" (V1.4 MVP qualifier dropped).
