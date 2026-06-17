# Embedded Presets

This directory contains embedded presets compiled into the `nexus42` binary at build time. Every subdirectory with a `preset.yaml` is automatically discovered and validated by the loader at startup.

**P1 strict validation gate**: all embedded presets are validated against the shared semantic validator (`validate_preset_semantic` + `validate_path_safety`) at test time via the `all_embedded_presets_pass_strict_validation_gate` smoke test.

## Preset Catalog

| Preset | Pattern | States | Description |
|--------|---------|--------|-------------|
| `creative-brief-intake` | Grill-me intake | clarifying → synthesizing → persisting → done | Multi-turn ACP clarification to produce a structured creative brief |
| `kb-extract` | Knowledge extraction | loading → extracting → done | Extract structured KeyBlocks from work-scope KB entries |
| `memory-augmented` | Memory recall + persist | recall → generate → persist → done | Recall memories, generate content, persist as new memory |
| `novel-writing` | Chapter-scoped pipeline | outline_chapter → draft_chapter → finalize → done | Single-chapter outline→draft→finalize with llm_judge 五问 gate (V1.36) |
| `novel-chapter-review` | FL-E review stage (V1.47) | load_chapter → review → done | Novel/work/chapter-aware 五問 review; persists ≥1 finding per pass via the supervisor terminal hook (V1.47 P0; replaces the generic `reflection-loop` demo) |
| `research` | Research workflow | scanning → extracting → synthesizing → done | Scan references, extract content, produce structured reports |
| `soul-experience-refresh` | SOUL maintenance | aggregate → done | Aggregate long-term memories into SOUL Experience section |
| `novel-brainstorm` | Quality loop ideation | gather → synthesize → done | Consume open findings (target_executor=brainstorm), generate ideation prompts (V1.39 P2) |
| `novel-review-master` | Human-in-loop review | present → await_decision → done | Surface findings (target_executor=master) for approval/reject/wont_fix decisions (V1.39 P2) |

## Manual Run

All presets are invoked via the daemon scheduler:

```bash
# Example: run novel-chapter-review (the FL-E `review` stage producer)
nexus42 creator run novel-chapter-review <work_id>

# Example: run memory-augmented
nexus42 daemon schedule add \
  --preset memory-augmented \
  --creator <creator-id> \
  --seed "Write a character arc for the antagonist"

# Example: run kb-extract
nexus42 daemon schedule add --preset kb-extract --creator <creator-id>

# Example: run soul-experience-refresh
nexus42 daemon schedule add --preset soul-experience-refresh --creator <creator-id>
# Or use the one-shot CLI command:
nexus42 creator soul refresh-experience
```

## Intake → Production Chaining

The `creative-brief-intake` preset is designed to be chained with `novel-project-init` and `novel-writing`:

```bash
# Via creator run (recommended — chains intake automatically):
nexus42 creator run start --idea "A sci-fi thriller about AI consciousness"

# Manual chaining:
nexus42 daemon schedule add --preset creative-brief-intake --creator <creator-id> --seed "<idea>"
# After intake completes, initialize the novel scaffold:
nexus42 daemon schedule add --preset novel-project-init --creator <creator-id> --seed "<topic from brief>"
# After scaffold completes, start production:
nexus42 daemon schedule add --preset novel-writing --creator <creator-id> --seed "<topic>"
```

### Novel Project Init → Novel Writing (P1 → P3)

The `novel-writing` preset requires the `novel-project-init` preset to have completed first (enforced by §5.3.2 gates). The init preset:

1. Runs an interactive grill-me to collect `work_ref`, `total_planned_chapters`, and World binding
2. Creates the `Works/<work_ref>/` directory tree with templates
3. Seeds `work_chapters` rows in `state.db`

Then `novel-writing` drives the chapter pipeline:

1. **`outline_chapter`** — writes `Works/<work_ref>/Outlines/chapters/ch01-outline.md` (with required F### foreshadowing section)
2. **`draft_chapter`** — writes `Works/<work_ref>/Stories/ch01-<slug>.md` (frontmatter `status: draft`); advances `current_chapter`
3. **`finalize`** — `llm_judge` 五问 quality gate; GO flips `status: finalized` in both DB and frontmatter; NOGO → WaitForInput

See `.mstar/knowledge/specs/novel-writing/workflow-profile.md` §3 (layout), §4.1 (work_chapters), §5.1 (五问 gate).

## Quality Loop Presets (V1.39 P2)

Auxiliary presets invoked by the quality loop when findings are routed to specific executors. These are NOT FL-E stages — they are triggered from findings routing (novel-quality-loop §2.2).

### `novel-brainstorm`

Consumes open findings with `target_executor: brainstorm` and generates structured ideation prompts for downstream `novel-writing`. Auto-chain compatible (uses `llm_judge` exit).

```bash
# Trigger brainstorm via daemon schedule (findings-driven routing):
nexus42 daemon schedule add \
  --preset novel-brainstorm \
  --creator <creator-id> \
  --seed '{"work_id":"<work_id>","work_ref":"<work_ref>"}'
```

### `novel-review-master`

Interactive human-in-loop preset that surfaces open findings with `target_executor: master` for approval/reject/wont_fix decisions. Uses `manual` exit (waits for human input).

```bash
# Trigger review-master via daemon schedule (findings-driven routing):
nexus42 daemon schedule add \
  --preset novel-review-master \
  --creator <creator-id> \
  --seed '{"work_id":"<work_id>","work_ref":"<work_ref>"}'
```

## Validation

All presets are embedded at compile time and validated by the loader at startup. The P1 strict validation gate runs at test time:

```bash
# Run the embedded preset smoke test (B1/B2)
cargo test -p nexus-orchestration -- all_embedded_presets_pass

# Run preset-specific tests
cargo test -p nexus-orchestration -- novel_chapter_review
cargo test -p nexus-orchestration -- memory_augmented
cargo test -p nexus-orchestration -- kb_extract
cargo test -p nexus-orchestration -- novel_brainstorm
cargo test -p nexus-orchestration -- novel_review_master

# Run full validation suite
cargo test -p nexus-orchestration
cargo clippy -p nexus-orchestration -- -D warnings
```

## Design Notes

- All presets are **linear state machines** with no conditional routing (`ConditionalNotYetSupported` remains enforced)
- Multi-agent presets (novel-writing, research) use the `roles` section; others are single-agent
- Prompt templates use Handlebars syntax (`{{preset.input.*}}`)
- The `creator.read_memory` / `creator.write_memory` capabilities work in standalone mode (return stubs) and with a pool (real persistence)
- `exit_when: kind: rule` with no expression is the explicit always-true (immediate transition) form — the state advances as soon as its enter action completes

## FL-E Stage Chain (V1.34)

V1.34 introduces the FL-E (Full Lifecycle Experience) stage chain — an explicit, linear progression through preset-driven stages for each Work:

```text
intake → research → produce → review → persist
```

### Stage → Preset Mapping

| Stage | Default Preset | `--force` | `run_intents` |
|-------|---------------|-----------|---------------|
| `intake` | `creative-brief-intake` | N/A (first stage) | `work_init` |
| `research` | `research` | Skips research gate | `work_continue` |
| `produce` | `novel-writing` | Skips produce gate | `work_continue` |
| `review` | `novel-chapter-review` | Skips review gate | `work_continue` |
| `persist` | `kb-extract` | Skips persist gate | `knowledge_ingest` |

### Stage Advance Flow

Each `creator run stage advance --stage <id>` triggers:

1. **Gate validation** — checks linear order, current stage completion, no active schedule (shared by CLI and daemon).
2. **Work PATCH** — sets `current_stage` and `stage_status = active`.
3. **Schedule create** — enqueues a schedule for the default preset with `presetInput` containing `work_id`, `fl_e_stage`, `creative_brief`, and `inspiration_log`.

### `--force` Semantics

`--force` bypasses all gate checks (wrong order, incomplete current stage, active schedule). Every forced advance is audit-logged with target `fl_e.audit`.

### Preset Input Variables

All stage schedules receive these preset input fields from the Work entity:

| Variable | Source | Used by |
|----------|--------|---------|
| `work_id` | Work entity ID | All stages |
| `fl_e_stage` | Target stage name | All stages |
| `creative_brief` | `works.creative_brief` | `research`, `novel-writing` |
| `inspiration_log` | `works.inspiration_log` (JSON array) | `research`, `novel-writing` |

### Manual Demo

```bash
# Full FL-E chain on a demo Work:
nexus42 creator run start --idea "A sci-fi thriller about AI consciousness"
# After intake completes:
nexus42 creator run stage advance <work_id> --stage research
nexus42 creator run stage advance <work_id> --stage produce
nexus42 creator run stage advance <work_id> --stage review
nexus42 creator run stage advance <work_id> --stage persist
```

### Test Coverage

```bash
# Run the full FL-E chain integration test
cargo test -p nexus-orchestration -- fl_e_chain

# Run stage gate unit tests
cargo test -p nexus-orchestration -- stage_gates
```
