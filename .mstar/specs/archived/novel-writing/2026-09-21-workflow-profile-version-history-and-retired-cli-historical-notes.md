# Novel Writing Workflow Profile — Version History and Retired CLI Historical Notes

**Provenance**: frozen from [`specs/active/novel-writing/workflow-profile.md`](../../active/novel-writing/workflow-profile.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 (2026-09-21) retired the `creator run`/`creator bootstrap` CLI entries; the frozen text spans the V1.36→V1.50 version/change-control ledger and the retired-entry tables. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (176 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Shipped (V1.36 — 2026-06-07); V1.37–V1.48 extensions; **V1.49** narrative indexes folded into §4.6; **V1.50** cron staggering + auto-chronology folded into §11 (all from folded V1.49/V1.50 overlays)  
```

### at pre-freeze line ~6

```markdown
**Last updated**: 2026-06-18 (V1.50 P-last — cron + auto-chronology folded into §11)
```

### at pre-freeze line ~40

```markdown
### 2.1 `work_profile` field (V1.36 extension)
```

### at pre-freeze line ~50

```markdown
| `world_id` | string (FK) | yes for new V1.40 novel Works | Bind to a World (per [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §5.4). Required for V1.40 Work creation/init; legacy `NULL` is allowed only when reading V1.39-and-earlier worldless Works. `novel-project-init` grill-me asks whether to create a new World or bind an existing one (see §3.5). |
```

### at pre-freeze line ~65

```markdown
      README.md                 # human overview; includes bound world_id for V1.40 Works; legacy worldless Works may include brief world setting notes
```

### at pre-freeze line ~67

```markdown
        volume-outline.md       # optional in V1.36 MVP
```

### at pre-freeze line ~70

```markdown
        foreshadowing.md        # empty stub V1.36 (F### rows; future V1.37+ scaffold)
        event-index.md          # empty stub V1.36 (E### rows; future V1.37+ scaffold)
```

### at pre-freeze line ~81

```markdown
| `Works/<work_ref>/README.md` | **No** | Human overview; V1.40 Works include bound `world_id`; legacy worldless Works may include world setting notes here |
```

### at pre-freeze line ~85

```markdown
| `Works/<work_ref>/Outlines/volume-outline.md` | **No** | Volume-level outline (optional V1.36) |
```

### at pre-freeze line ~109

```markdown
- **No per-Work `Worldbuilding/` subtree** in V1.36. The reference-system pattern of `{作品目录}/世界设定/` is intentionally rejected for OSS core; world content is the World's job, not the Work's.
- **`world_id` is the mandatory binding for new V1.40 novel Works**. New Works must be **World-bound** (`work.world_id` set) at creation/init time.
  - **World-bound Work** (`world_id != NULL`): characters, locations, society, rules, events, timelines come from World KB. Chapter body may reference World KB items by id via `world_refs: [char_xxx, loc_yyy]` frontmatter; V1.40 validates per §3.5.1.4.
  - **Legacy worldless Work** (`world_id == NULL`, V1.39 and earlier only): no cross-Work continuity. Existing data remains readable/operable; V1.40 creation/init must not produce this state.
- **`novel-project-init` asks the binding question** (grill-me). Two valid V1.40 options: bind to existing `world_id` (user picks from `nexus42 creator world list`) / create new World (calls `creator world create --title "..."`, narrative kind implicit). There is no V1.40 "stay worldless" creation option.
- **Work → World KB promotion** is the **long-term** path: as chapters finalize, `kb-extract` preset (existing, per [creator-workflow.md](../../active/creator/creator-workflow.md) `persist` stage) can extract entities / events / rules from chapter body into World KB items. V1.36 documents this path; enforcement is V1.37+.
```

### at pre-freeze line ~116

```markdown
### 3.5.1 World KB continuity implement contract (V1.37 P2 roadmap → V1.40 implement)
```

### at pre-freeze line ~118

```markdown
**Scope of this extension**: V1.37 P2 locked the roadmap; **V1.40** implements it across plans P0–P3. The `novel-writing` `world_binding` mode is `required` for V1.40 new Work creation/init. Legacy worldless Works from V1.39 and earlier continue to read/operate without a World context block, but no V1.40 init path may create a worldless Work. Implement slices: P0 world create + validation; P1 taxonomy; P2 prompt context block; P3 kb-extract binding.
```

### at pre-freeze line ~122

```markdown
**V1.40 implement** — CLI contract:
```

### at pre-freeze line ~131

```markdown
The init grill-me has exactly two valid V1.40 binding paths:
```

### at pre-freeze line ~143

```markdown
**V1.40 P0** ships this path; until then behavior remains stubbed.

```

### at pre-freeze line ~147

```markdown
When a novel Work is created/init-scaffolded in V1.40, `world_id` MUST reference an existing World visible under the active `creator_id` + `workspace_slug` context. Missing or omitted `world_id` values return a structured `preset_gates_failed`-style error with remediation. Legacy V1.39-and-earlier Works with `world_id == NULL` remain readable, but that legacy state is not creatable by V1.40 init paths.
```

### at pre-freeze line ~157

```markdown
The P0 gate evaluator already supports the `world_id` gate for `novel-writing`; the preset-level toggle is `world_binding: required` for V1.40 (§5.3.4). Implementations may keep the `optional` mode internally for back-compat/legacy reads, but new `novel-project-init` creation must fail closed without a valid `world_id`.
```

### at pre-freeze line ~183

```markdown
`characters_in_chapter` and `locations_referenced` are selected from `world_refs` when available, then from outline/body heuristics if needed. `active_rules` includes high-priority `foundation` and `rules` category items that constrain the scene. For legacy worldless Works (`world_id == NULL`, V1.39 and earlier), the block is omitted and prompts use only `Works/<work_ref>/README.md` setting notes, preserving read-time compatibility.
```

### at pre-freeze line ~201

```markdown
For V1.40 World-bound Works, `world_refs` is required by the validation contract: it may be empty only when the chapter truly references no World KB items, and any non-empty entry must validate under `work.world_id`. For legacy worldless Works (`world_id == NULL`, V1.39 and earlier), `world_refs` is allowed but unused; if present, implementations should warn but not fail.
```

### at pre-freeze line ~208

```markdown
- **Legacy worldless Work** (V1.39 and earlier): extraction is skipped or remains local Work scope; it MUST NOT silently create a new World or promote content into an arbitrary World.
```

### at pre-freeze line ~211

```markdown
**V1.40 acceptance** (per plan P0–P3): hermetic tests for valid/invalid/missing `world_id`, prompt block presence for new World-bound Works, legacy worldless read compatibility, `world_refs` warning/error timing, and `kb-extract` target selection. On ship, DF-63 is closed (V1.40).
```

### at pre-freeze line ~219

```markdown
DF-63 (World KB cross-Work unification) shipped V1.40.
```

### at pre-freeze line ~227

```markdown
V1.36 chapter state lives in a **new DB table** `work_chapters` in `state.db` (owned by `nexus-local-db`). The legacy `Works/<work_ref>/work-status.md` file is **removed** — chapter state is no longer author-edited as markdown; the orchestration engine owns the truth (CLI reads it via `creator works status`).
```

### at pre-freeze line ~229

```markdown
#### 4.1.1 `work_chapters` table DDL (V1.36)
```

### at pre-freeze line ~235

```markdown
  volume               INTEGER,                  -- nullable; V1.36 single-chapter MVP leaves NULL; V1.37+ multi-volume uses 1..N
```

### at pre-freeze line ~252

```markdown
**V1.37 P1 roadmap decision**: V1.37 does **not** migrate this primary key. The PK stays `(work_id, chapter)` for V1.37 single-volume Works; `volume` remains nullable and defaults to `NULL` for V1.36/V1.37 single-volume rows. A future V1.37+ multi-volume implementation may drop this PK and add a unique index on `(work_id, volume, chapter)` after it ships volume-aware migrations and backfill rules (§4.5.4).

```

### at pre-freeze line ~258

```markdown
- **Reconciliation**: a manual `creator works reconcile-chapters <work_id>` walks `Works/<work_ref>/Stories/` and rebuilds `work_chapters` rows from filesystem for missing rows/files (V1.45 P2 command path; migrated from the retired `creator run reconcile-chapters`). For status disagreements, V1.37 narrows the rule: the DB row is the source of truth and the next run re-syncs chapter frontmatter via a single transition (§4.5.3). For V1.36 this is a manual command, not an automatic job.
```

### at pre-freeze line ~265

```markdown
- For **legacy worldless** Works (V1.39 and earlier): a brief world setting note (1–2 paragraphs) so the LLM has context
```

### at pre-freeze line ~273

```markdown
Structured outline before正文. **Required headings**: opening scene, core conflict, turning point, climax, ending hook, character state change. **Foreshadowing is required**: the outline must list every foreshadowing item touched (buried or paid-off) in this chapter, referencing `Outlines/foreshadowing.md` F### ids. If the foreshadowing file does not yet exist for a V1.36 single-chapter MVP, the outline body may declare new F### items inline and the next outline is responsible for promoting them to the index.
```

### at pre-freeze line ~281

```markdown
volume: integer             # optional; V1.36 single-chapter MVP leaves it empty. Reserved for V1.37+ multi-volume.
status: draft | finalized   # published reserved; not set by OSS core in V1.36; mirror of work_chapters.status
```

### at pre-freeze line ~284

```markdown
world_refs: [string]        # optional; list of World KB item ids referenced in this chapter (V1.36 advisory, not validated)
```

### at pre-freeze line ~288

```markdown
`volume` is **forward-compatible** for V1.37+ multi-volume expansion. When present, the chapter must also appear in `Outlines/volume-outline.md` chapter range. Sync module does **not** validate cross-reference in V1.36 (intentional — V1.36 single-volume). `world_refs` is a soft hint; for **World-bound** Works (§3.5) it should list World KB item ids (e.g. `char_lin_xia`, `loc_neon_city`). V1.36 does not enforce; V1.37+ may validate. Body text only (title in frontmatter). Status transitions:
```

### at pre-freeze line ~294

```markdown
| → `finalized` (manual override) | user explicit advance on NOGO with logged audit reason; V1.36 escape hatch only |
```

### at pre-freeze line ~298

```markdown
V1.36 implement wave ships template stubs under `crates/nexus-orchestration/embedded-presets/novel-writing/templates/` (or documented equivalent). Prompts reference `Works/{{work_ref}}/…` variables.
```

### at pre-freeze line ~300

```markdown
### 4.5 Multi-chapter and multi-volume semantics (V1.37 extension)
```

### at pre-freeze line ~302

```markdown
**Scope of this extension**: V1.37 P1 is **roadmap-only**. It locks the semantics future implementers must follow, but it does not claim a code, schema, preset, or migration implementation in this plan. V1.36 single-chapter behavior is a strict subset: when `total_planned_chapters == 1`, chapter 1 is selected, drafted, finalized, and completes the Work exactly as in the shipped V1.36 MVP.
```

### at pre-freeze line ~306

```markdown
The V1.37 multi-chapter roadmap keeps the V1.36 chapter status state machine:
```

### at pre-freeze line ~343

```markdown
**V1.37–V1.41 (shipped):** PK `(work_id, chapter)`; `volume` nullable; single-volume Works use unique chapter numbers across the Work.

**V1.42 P1 (implement — grill-me locked):**
```

### at pre-freeze line ~354

```markdown
**V1.42 P1 (Implemented):** Volume-outline scaffold delivered. `novel-project-init` grill-me captures volume count + per-volume chapter totals and seeds `Works/<work_ref>/Outlines/volume-{n}-outline.md` per the YAML structure below. Cross-volume chapter numbers may repeat across volumes (per §4.5.4 PK migration).
```

### at pre-freeze line ~356

```markdown
When `volume != NULL`, the chapter must appear in `Outlines/volume-outline.md` under that volume's chapter range. The minimum V1.37+ multi-volume outline structure is:
```

### at pre-freeze line ~371

```markdown
The sync module (per [sync-contract.md](../../active/novel-writing/sync-contract.md)) does **not** validate `volume-outline.md` cross-references in V1.37. A post-roadmap V1.37+ implementation may enforce that every non-NULL `work_chapters.volume` row falls inside exactly one declared volume range.
```

### at pre-freeze line ~377

```markdown
The V1.36 `novel-writing` preset and templates may still bind or imply `chapter: 1` in places. Future implementation must replace hard-coded chapter-1 bindings with a scheduler/gate-evaluator injected value such as `{{work_chapters.next_chapter}}` (or an equivalent preset input variable). The value must be derived from `next_chapter(work_id)` above after the P0 gates have passed (`intake_status == complete`, scaffold exists, `previous_preset: novel-project-init` complete).
```

### at pre-freeze line ~383

```markdown
**V1.42 P1 (Implemented subset):** Test #6 (Future multi-volume migration: the `(work_id, chapter)` → `(work_id, volume, chapter)` migration is idempotent and preserves row data) is implemented. Verified by `w01_v142_migration_idempotent` test in `crates/nexus-local-db/tests/v142_migration_fixes.rs`. Index coverage for `next_chapter_volume_aware` query is also verified by `w02_volume_aware_index_coverage` test.
**v1.179 (DR-67 landed — plan `2026-08-27-v1.179-p2-reliability-convergence`):** Tests #1–#5 are implemented in `crates/nexus-local-db/tests/v179_dr67_acceptance.rs`: `w179_dr67_01_next_chapter_selects_lowest_eligible` (#1, §4.5.2), `w179_dr67_02_current_chapter_advances_only_on_finalize` (#2, §4.5.2 work-level invariant), `w179_dr67_03_completion_requires_all_rows_finalized_count_match_and_intake` (#3, V1.44 row-count contract — see the #3 note below), `w179_dr67_04_resume_reuses_draft_row_without_creating_new` (#4, §4.5.2 resume + §5.4.5), and `w179_dr67_05_reconcile_rebuilds_missing_rows_with_db_status_ssot` (#5, §4.5.3) — mirroring the #6 annotation style above.
```

### at pre-freeze line ~386

```markdown
The DR-67 acceptance tests pin the following invariants (**Durable roadmap:** DR-67 — #1–#5 are implemented as of the v1.179 landed note above; #6 is implemented as of the V1.42 note above; the list below is the normative description of what each pin covers):
```

### at pre-freeze line ~390

```markdown
3. **Novel completion**: completion fires only when the row set covers `total_planned_chapters`, every row is `finalized`, and `intake_status == complete` (V1.44 F-002 volume-aware row-count contract implemented by `is_work_completed`). **Spec-pointer gap (historical):** §6.1's `current_chapter >= total_planned_chapters` conjunct is NOT enforced by `is_work_completed` today — the V1.44 row-count contract superseded it, and the v1.179 test pins the implemented contract rather than the unenforced pointer.
```

### at pre-freeze line ~395

```markdown
### 4.6 Narrative indexes — F### / E### runtime (V1.49 P1)
```

### at pre-freeze line ~397

```markdown
V1.36 scaffolded `Outlines/foreshadowing.md` and `Outlines/event-index.md` as empty table stubs. V1.49 P1 implements the **minimum viable index runtime** — file-first SSOT (overlay folded here at P-last).
```

### at pre-freeze line ~405

```markdown
| `work_chapters` table | DB | Unchanged; no index mirror table in V1.49 |
```

### at pre-freeze line ~453

```markdown
Event-index (`Outlines/event-index.md`): **P1 minimum** — preserve scaffold; read existing rows for prompt if present. Full E### promotion (write + upsert) deferred to V1.50 — **Durable roadmap:** DR-66 (E### event-index write+upsert promotion).
```

### at pre-freeze line ~457

```markdown
Explicit promotion from index rows to World KB remains **manual/opt-in** per §3.5.1.5. V1.49 does not auto-sync index → World KB.
```

### at pre-freeze line ~468

```markdown
| `creative-brief-intake` | Structured brief on Work | FL-E `intake` / `creator bootstrap` | (generic; out of novel overlay) |
```

### at pre-freeze line ~470

```markdown
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review that produces findings (V1.47); replaces the former generic `reflection-loop` demo | FL-E `review` | §5.3.3 |
```

### at pre-freeze line ~472

```markdown
**Separation rule**: `novel-project-init` is **not** auto-chained inside `novel-writing`. The author explicitly schedules it when starting a new novel Work — since v1.193 P2 there is no CLI preset runner (the `creator run` entry is retired); preset admission goes through the core execution surface (schedule add via `POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`). The engine enforces this via the `previous_preset: novel-project-init` gate on `novel-writing` (§5.3.2).
```

### at pre-freeze line ~474

```markdown
### 5.1 V1.36 chapter finalize quality gate
```

### at pre-freeze line ~480

```markdown
- **GO override on NOGO** → (historical entry, retired in v1.193 P2) the CLI override was `creator run novel-writing <work_id> --force-gates --reason "<text>"` with audit-logged reason, flipping both rows to `finalized` regardless; the runner was removed in v1.193 P2 and no current CLI gate-override surface is provided.
```

### at pre-freeze line ~484

```markdown
### 5.1.1 Outline 五问 gate (V1.52 T-A P0 — Normative)
```

### at pre-freeze line ~506

```markdown
- `world_refs: [char_xxx, loc_yyy]` frontmatter is filled by the agent based on what the chapter references. V1.36 does not validate; V1.37+ may validate.
```

### at pre-freeze line ~508

```markdown
For **legacy worldless** Works (`world_id == NULL`, V1.39 and earlier): no World context block; LLM uses `README.md` setting notes as the only world context.
```

### at pre-freeze line ~510

```markdown
### 5.3 V1.36 novel preset gates (Draft overlay on orchestration-engine.md §7.9)
```

### at pre-freeze line ~598

```markdown
- **`required`** (V1.40 default for `novel-writing`): the `world_id` gate is active; new V1.40 Works must have `world_id` set before scheduling. Legacy worldless Works (`world_id == NULL`, V1.39 and earlier) may continue to run only through explicit back-compat handling or audited override paths; V1.40 creation/init must not create them.
- **`optional`**: retained only as an internal/back-compat mode for legacy reads or older manifests; new V1.40 `novel-project-init` behavior treats it as a no-op and still requires a valid `world_id` at creation.
```

### at pre-freeze line ~601

```markdown
Recommendation: keep the toggle in manifests for compatibility with older presets, but default and enforce `required` for V1.40 novel Work creation/init.
```

### at pre-freeze line ~605

```markdown
When a `novel-writing` gate fails (e.g. user runs `creator bootstrap --idea "..."` without first running `novel-project-init`), the CLI surfaces:
```

### at pre-freeze line ~613

```markdown
        ↳ Run `creator bootstrap --init-preset novel-project-init` to scaffold the Work.
```

### at pre-freeze line ~615

```markdown
        ↳ Complete intake via `creator bootstrap --preset creative-brief-intake`.
  override: pass --force-gates with --reason "<text>" to bypass (audit-logged)
```

### at pre-freeze line ~621

```markdown
#### 5.3.6 Implementation note (V1.36)
```

### at pre-freeze line ~625

```markdown
- Engine logs gate failures to the structured log with `preset_id`, `work_id`, `failed_gates` array. Failed-gate rate by preset is a future observability dashboard (out of V1.36 scope).
```

### at pre-freeze line ~638

```markdown
      volume-outline.md                    # copy from embedded template; V1.36 single-volume: still created (placeholder)
```

### at pre-freeze line ~643

```markdown
    Logs/                                  # mkdir (empty; structure OUT V1.36 per DF-66)
```

### at pre-freeze line ~659

```markdown
For V1.40 Works, `README.md` is rendered with a one-line `world_id: <uuid>` header and links to the World KB items the Work will reference most. Legacy worldless Works from V1.39 and earlier may retain README inline world setting notes, but V1.40 scaffold rendering does not create new worldless README variants.
```

### at pre-freeze line ~667

```markdown
| `work_id` | from grill-me / `creator bootstrap` |
```

### at pre-freeze line ~669

```markdown
| `volume` | `NULL` (V1.36 single-volume) |
| `slug` | user-provided per chapter from grill-me (or auto-derived from `chNN` for V1.36 MVP) |
| `planned_word_count` | default `4000` (single V1.36 value; user may override per chapter in grill-me) |
```

### at pre-freeze line ~687

```markdown
| `work_profile` | `'novel'` (was `null` or previously set) |
```

### at pre-freeze line ~704

```markdown
The grill-me offers an "overwrite templates" option for users who want to re-render the README/Outlines/ from latest embedded templates (useful after a toolchain update). V1.36 default is **preserve**; "overwrite" is the explicit user opt-in.
```

### at pre-freeze line ~710

```markdown
**V1.49 shipped**: 6-state findings lifecycle → [quality-loop.md §2](../../active/novel-writing/quality-loop.md#2-findings-lifecycle) (Shipped V1.49 P0; §5.5.1 three-state `open` / `resolved` / `wont_fix` paragraph superseded).
```

### at pre-freeze line ~719

```markdown
| Producer/consumer contract | [quality-loop.md §8–§9](../../active/novel-writing/quality-loop.md#8-reflection-loop-output-contract-v147-shipped) |
```

### at pre-freeze line ~721

```markdown
Historical V1.37 P3 §5.5 roadmap text was folded into `quality-loop.md` at V1.39–V1.48. Do not duplicate normative findings detail here.
```

### at pre-freeze line ~727

```markdown
### 6.1 Completion criteria (V1.36 MVP; V1.37 multi-chapter extension)
```

### at pre-freeze line ~746

```markdown
The V1.36 single-chapter case (`total_planned_chapters == 1`) is a strict subset of this rule: chapter 1 finalizes, `current_chapter` becomes `1`, all rows are finalized, and the Work completes.
```

### at pre-freeze line ~754

```markdown
**Note**: V1.36 single-chapter MVP completion means `ch01` reached `finalized` after the `llm_judge` GO (§5.1). V1.37 supersedes the chapter-1-only interpretation by extending the same rule across all seeded `work_chapters` rows; it does not change behavior for a one-chapter Work.
```

### at pre-freeze line ~756

```markdown
### 6.3 Explicit non-goals (through V1.40)
```

### at pre-freeze line ~758

```markdown
- No automatic creation of next novel Work **without** explicit switch/pool commands (V1.41 adds opt-in ceremony — §6.4)
```

### at pre-freeze line ~762

```markdown
### 6.4 Multi-work completion extension (Shipped V1.41)
```

### at pre-freeze line ~764

```markdown
[multi-work-lifecycle.md](../../active/novel-writing/multi-work-lifecycle.md) shipped V1.41 (PR #53):
```

### at pre-freeze line ~769

```markdown
4. Next Work via `creator works use`, `creator works pool promote`, or `creator bootstrap --from-work` — not implicit scaffold.
5. **Reopen** same Work: `creator works reopen <work_id> --reason "<text>"` (V1.45 P2 — one core operation that releases the completion lock; migrated from the retired `creator run resume --reopen`); distinct from `--from-work` new Work.
```

### at pre-freeze line ~773

```markdown
**V1.42 P1**: multi-volume PK (§4.5.4) — .
```

### at pre-freeze line ~794

```markdown
| ~~`creator bootstrap --idea "..."`~~ | **Retired in v1.193 P2** (no CLI bootstrap/scaffold entry; historical V1.40 contract) — default `work_profile: novel` when `--preset novel-writing` or default produce path; V1.40 creation/init must obtain a `world_id` via create-new or bind-existing before scaffold completes |
| ~~`creator bootstrap --idea "..." --world-id <world_id>`~~ | **Retired in v1.193 P2** (historical) — bound the new Work to an existing World (per §3.5); World KB is injected as context in `novel-writing` prompts |
| ~~`creator bootstrap --idea "..." --init-preset novel-project-init`~~ | **Retired in v1.193 P2** (historical) — ran the `novel-project-init` grill-me (scaffold dirs + mandatory World binding question + `work_chapters` seed rows) before intake |
| `creator works status [<work_id>]` | **V1.41** — migrated from `creator run status` (runner retired in v1.193 P2); reads `work_chapters`; shows `work_ref`, chapter list, completion; **V1.39** fields + completion/runtime lock per [cli-spec.md](../../active/cli/cli-spec.md) §6.2H |
| `creator works list` | **V1.41** — migrated from `creator run list` |
| ~~`creator run resume <work_id>`~~ | **V1.39** — resumed checkpointed auto-chain after daemon restart. **Retired in v1.193 P2** with the runner; no current CLI resume entry — schedule admission is via the execution surface (`POST /v1/daemon/orchestration/schedules`), and the retained scheduling CLI surface is `creator works cron` declaration editing |
| `creator works inspire [<work_id>] --note "..."` | **V1.45 P2** — migrated from `creator run continue`; appends inspiration; does not advance chapter index |
| `creator works reconcile-chapters [<work_id>] [--dry-run] [--yes]` | **V1.45 P2** — migrated from `creator run reconcile-chapters`; (V1.36 manual semantics) rebuilds `work_chapters` rows from `Works/<work_ref>/Stories/` filesystem state; per §4.1.2 reconciliation rules |
```

### at pre-freeze line ~803

```markdown
First-run path adds a mandatory World binding step for novel Work creation: create a new World or pick one from `creator world list`. New V1.40 Works print a one-line `world: <name> (<world_id>)` in the run summary.
```

### at pre-freeze line ~805

```markdown
### 8.1 Multi-chapter `creator works status` UX (V1.37 extension; V1.41 command path)
```

### at pre-freeze line ~824

```markdown
Next action: Chapter 3 is not started; the next `novel-writing` preset pass drafts it (preset admission via the execution surface — the `creator run` CLI verb was retired in v1.193 P2).
```

### at pre-freeze line ~838

```markdown
Next action: Chapter 2 is in draft; the next `novel-writing` preset pass resumes it (preset admission via the execution surface — the `creator run` CLI verb was retired in v1.193 P2).
```

### at pre-freeze line ~869

```markdown
3. Chapter state SSOT is **`work_chapters` table in `state.db`** (§4.1.1); `work-status.md` file is **removed** in V1.36.
```

### at pre-freeze line ~872

```markdown
6. `novel-project-init` asks the mandatory World binding question (§3.5, §5) with only create-new / bind-existing V1.40 paths.
7. Compass demo path §2 in v1.36 compass achievable after P1–P3 implement.

---

## 10. Change control

- **Authority**: Active V1.37 compass > V1.37 extension sections in this spec > shipped V1.36 baseline in this spec > generic Work spec for novel-specific rules. V1.37 delivery batching does not retroactively claim implementation until a plan ships.
- **Promotion**: On iteration ship, Status → `Shipped (V1.36)`; merge overlay sections into work-experience-model §profile extension if appropriate.
- **Reference distill**: Internal novels-system patterns informed §3.1/§4/§5; **§3.5 World integration** is a **Nexus-architectural choice** (cross-Work content belongs to World, not per-Work), explicitly **rejecting** the reference-system's per-Work `世界设定/` shape. No external repo paths in normative text.
- **V1.36 architecture deltas vs earlier draft** (recorded 2026-06-07):
  - `work_chapters` table replaces `work-status.md` file (§4.1)
  - `Works/<work_ref>/Worldbuilding/` subtree **removed**; world content lives in World KB (§3.5)
  - `world_id` becomes the cross-Work binding (§2.1, §3.5, §5, §8)
  - **Preset gates mechanism added** (orchestration-engine.md §7.9 Master + novel-specific gate values in this spec §5.3; new `world_binding: required | optional` toggle in §5.3.4). Replaces the implicit "novel-writing should already have scaffold" assumption with explicit enqueue-time enforcement.
  - **`novel-project-init` scaffold protocol enumerated** (§5.4): explicit file list, template sources, `work_chapters` row seeding, atomicity, idempotency, re-init handling. Replaces the high-level "mkdir scaffold + write template stubs" P1 T2 with a P1-implementer checklist.
- **V1.37 P1 multi-chapter roadmap deltas** (recorded 2026-06-08):
  - **Roadmap-only** decision recorded (§4.5): no code/schema/preset implementation claimed in P1.
  - `next_chapter(work_id)` algorithm defined (§4.5.2): lowest `not_started`, then lowest `draft` resume, otherwise novel-completion.
  - `works.current_chapter` clarified as latest finalized chapter only (§4.5.2, §6.1).
  - `work_chapters` PK migration deferred: V1.37 keeps `(work_id, chapter)` and reserves `(work_id, volume, chapter)` unique index for post-V1.37 multi-volume support (§4.5.4).
  - `Outlines/volume-outline.md` minimum structure and `world_refs` placement documented (§4.5.5).
  - Multi-chapter `creator run status` output made testable (§8.1) — command name is the V1.37-era record; it migrated to `creator works status` in V1.41 and `creator run` was retired in v1.193 P2.
- **V1.37 P2 World KB roadmap deltas** (recorded 2026-06-08):
  - **Roadmap-only** decision recorded (§3.5.1): no CLI/API/schema/prompt runtime/validator/`kb-extract` implementation claimed in P2.
  - Future `creator world create --title ... --description ...` contract defined for the init "create new World" path (§3.5.1.1); `--name` remains an alias and narrative kind is implicit.
  - `world_id` existence validation, legacy `world_binding: optional` V1.37 posture, and `preset_gates_failed` remediation documented (§3.5.1.2).
  - Prompt-time World context block shape documented for World-bound Works, with legacy worldless Works preserving README-only context (§3.5.1.3).
  - **V1.40 amendment** (recorded 2026-06-10): new novel Work creation/init requires `world_id`; legacy V1.39-and-earlier worldless Works remain readable but cannot be newly created (§2.1, §3.5, §3.5.1, §5.3.4, §5.4).
  - `world_refs` canonicalization and warning/error timing documented (§3.5.1.4).
  - Chapter → World KB extraction and explicit event/foreshadowing promotion path documented (§3.5.1.5).
- **V1.37 P3 quality-loop roadmap deltas** (recorded 2026-06-08):
  - **Roadmap-only** decision recorded (§5.5): no findings migration, new presets, daemon scheduled task, CLI subcommands, prompt templates, or file writers claimed in P3.
  - Findings lifecycle, severity mapping, and future `findings` table sketch documented (§5.5.1).
  - Reference executor concepts mapped to Nexus presets / CLI surfaces (`novel-writing`, future `novel-brainstorm`, future `novel-review-master`) (§5.5.2).
  - 96h master-decision timeout mapped to local DB + daemon scheduled task + `creator run status` banner with opt-in escalation (§5.5.3) — V1.37-era record; the daemon scheduled task host and the `creator run` runner were both retired in v1.193 P2 (the banner surface is now `creator works status`).
  - Three-layer rules architecture and SOUL / World KB boundaries documented (§5.5.4).
  - `Logs/brainstorm|write|review|publish` roadmap structure documented while reaffirming `Logs/**` sync exclusion (§5.5.5).
  - `reflection-loop` → findings / rule-suggestion integration documented as future implementation (§5.5.6 — **superseded V1.47 P0**; §5.5.6 is now normative).
- **V1.47 P-last promotion** (recorded 2026-06-15):
  - §5.5.4 two-layer rules (`AGENTS.md`) promoted Draft → Shipped.
  - §5.5.6 `novel-chapter-review` findings integration promoted Draft → Shipped.
  - Status banner updated to `V1.47 Shipped`.
- **V1.50 P-last promotion** (recorded 2026-06-18):
  - §11 cron staggering (V1.50): per-Work cron configuration + three-role defaults promoted Draft → Shipped (folded from the cron-staggering overlay).
  - §11.5 auto-chronology (V1.50): per-Work opt-in volume auto-advance promoted Draft → Shipped (folded from the auto-chronology overlay).
  - Status banner updated to `V1.50 Shipped` (pending P-last + PR).
```

### at pre-freeze line ~922

```markdown
## 11. Cron staggering and auto-chronology (V1.50)
```

### at pre-freeze line ~947

```markdown
CLI surface (V1.50 T-A P0):
```

### at pre-freeze line ~964

```markdown
The daemon `cron_supervisor` (V1.50 T-A P1) runs on the 1-min tick supervisor loop and:
```

### at pre-freeze line ~970

```markdown
5. Enqueues a `Schedule` with `preset_id = <role-preset>` and `work_ref` from the source Work. The `enqueue_cron_schedule` path is uniform across all three roles (no role-specific branch). The `CRON` prefix + `cron:<role>:<work_id>` label preserves provenance vs V1.39 SLA-launched schedules.
```

### at pre-freeze line ~974

```markdown
Cron-fired schedules are out-of-band: they do NOT touch the FL-E `driver_schedule_id`. This mirrors `enqueue_review_master_schedule` (V1.39). The existing supervisor `tick()` admits; the existing terminal pipeline handles completion. This avoids disrupting an in-progress FL-E chain.
```

### at pre-freeze line ~1007

```markdown
#### 11.5.3 CLI surface (V1.50 T-A P3)
```

### at pre-freeze line ~1017

```markdown
### 11.7 真完本 UX (Draft V1.52 overlay)
```

### at pre-freeze line ~1019

```markdown
**Status**: Draft (V1.52 — body authored  unless pulled earlier by T-A P2)  
```

### at pre-freeze line ~1021

```markdown
**Promotes to Normative**: P-last of V1.52
```

### at pre-freeze line ~1023

```markdown
Draft overlay placeholder: define the user-visible "true completion" ceremony after all chapters/volumes are finalized, including status copy, no-auto-switch behavior, and next-Work guidance.
```

### at pre-freeze line ~1029

```markdown
- `nexus-orchestration::auto_chronology` — advance implementation (V1.50 T-A P3)
- `nexus-orchestration::cron_supervisor` — fire evaluator (V1.50 T-A P1)
- `nexus-local-db::works.schedule_json` column — storage (V1.50 T-A P0)
```

### at pre-freeze line ~1035

```markdown
*Shipped V1.36 baseline with V1.37 P1/P2/P3 roadmap extensions. Implement V1.37 multi-chapter, World KB continuity, or quality-loop behavior only via a future locked implementation plan.*
```
