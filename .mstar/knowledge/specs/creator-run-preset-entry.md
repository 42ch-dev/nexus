# Creator Run Preset Entry — Normative Specification v1

**Status**: Shipped (V1.45 — 2026-06-13)  
**Document class**: Master (wave 0 for V1.45 CLI IA)  
**Created**: 2026-06-13  
**Last updated**: 2026-06-14 (P-last promotion Draft → Shipped)  
**Scope**: Author-facing **`nexus42 creator run <preset_id> [<work_id>]`** — generic orchestration preset dispatch; relationship to `creator bootstrap`, atomic `creator works`, and `daemon schedule`  
**Coordinates with**:

- [cli-spec.md](cli-spec.md) — per-flag detail (§6.2D implement amendment)
- [cli-command-ia.md](cli-command-ia.md) — three-plane IA
- [orchestration-engine.md](orchestration-engine.md) — presets, gates, `run_intents`
- [work-experience-model.md](work-experience-model.md) — Work lifecycle
- [creator-workflow.md](creator-workflow.md) — FL-E stage ↔ preset mapping
- [novel-work-pool.md](novel-work-pool.md) — pool `active` default

**Iteration compass**: [v1.45-creator-run-preset-unification-delivery-compass-v1.md](../../iterations/v1.45-creator-run-preset-unification-delivery-compass-v1.md)  
**Tracker**: BL-12

---

## 1. Purpose

Pre-V1.45, `creator run` accumulated **hardcoded subcommands** (`start`, `continue`, `stage`, `audit-chapter`, `review-master`, …). Each new embedded preset required editing `RunCommand` in Rust.

V1.45 defines a **single product entry** for running any orchestration preset:

```text
nexus42 creator run <preset_id> [<work_id>] [global flags] [preset cli_args…]
```

Adding a preset ships **YAML + optional docs**, not CLI enum variants.

---

## 2. Three-plane CLI model

| Plane | Command | Responsibility |
| --- | --- | --- |
| Composite onboarding | `creator bootstrap` | Create Work + schedule intake/init/produce chain |
| Atomic Work ops | `creator works <sub>` | One business function per subcommand (inspire, reopen, …) |
| Strategy execution | **`creator run <preset_id>`** | Enqueue orchestration preset (state machine + ACP) |

Presets may invoke daemon capabilities that perform atomic Work ops during execution. Atomic ops **must not** appear as `creator run` subcommands.

---

## 3. Command grammar

### 3.1 Syntax

```text
nexus42 creator run <preset_id> [<work_id>] [--json] [--force-gates --reason "<text>"] [<preset-specific flags>]
```

| Token | Required | Semantics |
| --- | --- | --- |
| `<preset_id>` | Yes | Resolved via orchestration preset registry (embedded, user, `_system.*`) |
| `<work_id>` | No | Optional **positional** `wrk_…`. Omitted → pool **`active`** Work (same resolution as `creator works status`) |
| `--json` | No | Machine-readable schedule response |
| `--force-gates --reason` | No | Audited gate bypass ([orchestration-engine.md](orchestration-engine.md) §7.9). `--reason` required when `--force-gates` set |

**Not supported on `creator run`:** `--work-id` flag (use positional); `stage advance --force` (Removed in V1.45; see changelog).

### 3.2 Preset discovery

The CLI **must not** maintain a hardcoded preset allowlist. Any id that resolves through the orchestration loader is valid at the CLI surface. Validation errors (`run_intents`, `gates`, missing Work) are returned by **daemon/orchestration** at schedule creation time.

Documented FL-E defaults (quickstart only):

| FL-E stage | Default preset id |
| --- | --- |
| `research` | `research` |
| `produce` | `novel-writing` |
| `review` | `novel-chapter-review` |
| `persist` | `kb-extract` |

`intake` is triggered only via **`creator bootstrap`**, not manual `creator run`.

### 3.3 Preset-specific arguments (`cli_args`)

Preset directories declare optional CLI flags in `preset.yaml`:

```yaml
cli_args:
  - name: chapter
    type: integer
    required: true
    description: "1-based chapter number"
  - name: volume
    type: integer
    required: false
    default: 1
```

V1.45 P0 minimal schema fields: `name`, `type` (`string` \| `integer` \| `boolean`), `required`, `default`, `description`.

The generic runner maps parsed flags to `AddScheduleRequest.input`. P0 ships `cli_args` for:

- `novel-manuscript-audit-review`, `novel-manuscript-audit-extract`
- `novel-review-master` (`finding_id`, `auto_schedule`)

---

## 4. Execution flow

1. Resolve `<preset_id>` (fail if unknown).
2. Resolve `<work_id>`: positional arg or pool `active`; if none, fail with remediation → `creator bootstrap` or `creator works use`.
3. Build schedule request (preset input from `cli_args`, Work-derived context from daemon).
4. For FL-E default presets (`research`, `novel-writing`, `novel-chapter-review`, `kb-extract`), apply **stage advance** semantics before enqueue: validate stage gates, PATCH Work stage fields, then create schedule. These semantics are **live behavior** of the generic runner — the standalone `creator run stage advance` **subcommand** was removed in V1.45 (replaced by this runner's built-in stage path); see the V1.45 changelog.
5. `POST /v1/local/orchestration/schedules` — orchestration validates `run_intents` and `gates`.
6. Print schedule id (human or JSON).

The CLI **does not** filter presets by `run_intents` subcommand (`start`/`continue` mapping removed).

---

## 5. Relationship to other entries

| Entry | When to use |
| --- | --- |
| **`creator bootstrap`** | First-time Work creation + intake/produce chain |
| **`creator run <preset_id>`** | Run any preset on an existing Work |
| **`creator works inspire`** | Append inspiration note only (not a preset) |
| **`daemon schedule add`** | Power user / automation; same underlying API |
| **`nexus42 system preset list`** | Discover preset ids + `run_intents` |

---

## 6. Migration (V1.44 → V1.45)

See compass [migration appendix](../../iterations/v1.45-creator-run-preset-unification-delivery-compass-v1.md) §2.

Hard delete legacy subcommands — **no deprecated aliases** (pre-release).

---

## 7. Acceptance (implement wave)

1. `creator run novel-brainstorm` works without new Rust subcommand.
2. `creator run novel-manuscript-audit-review wrk_… --chapter 3` replaces `audit-chapter --mode review`.
3. `creator run novel-review-master` enqueues only (no findings list).
4. `creator run --help` lists preset id syntax, not legacy subcommands.

---

## 8. V1.45 supersession notes (P-last promotion)

This Master **supersedes** the V1.44 cli-spec §6.2D/E bespoke subcommand tables and the V1.45 Draft overlay sections in the following specs:

- [creator-workflow.md](creator-workflow.md) (FL-E CLI overlay)
- [novel-quality-loop.md](novel-quality-loop.md) (preset-id commands overlay; applied P3)
- [novel-manuscript-audit.md](novel-manuscript-audit.md) (CLI entry overlay; split presets)
- [work-experience-model.md](work-experience-model.md) (side-input + run_intents overlay)
- [orchestration-engine.md](orchestration-engine.md) (`run_intents` dispatch overlay)
- [cli-spec.md](cli-spec.md) (creator run preset entry overlay)

**Promotion date**: 2026-06-14 (P-last closeout)
