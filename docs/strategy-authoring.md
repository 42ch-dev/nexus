# Strategy Authoring

A **strategy** is a preset bundle that declares *capability routing and prompt
templates* for an orchestration loop: which lane a run enters, which prompts
run in which order, and what the run payload looks like. It is a plain
directory — `preset.yaml` + a `templates/` directory — consumed through the
same preset bundle format as embedded presets. Nothing in a strategy is
compiled into any binary. For a guided authoring reference, use the
`strategy-author` skill in the external
[`42ch-dev/agent-toolkit`](https://github.com/42ch-dev/agent-toolkit)
repository (no agent skill ships in this repo). The authoring reference
below stays the canonical contract.

> **Guided authoring:** use the `strategy-author` skill from the
> [`42ch-dev/agent-toolkit`](https://github.com/42ch-dev/agent-toolkit)
> repository (external — **no** agent skill ships in this repo) for the
> agent-side on-ramp: bundle layout, the two execution surfaces, the
> signals-not-delivered trap, and validator verification. This doc
> remains the human authoring SSOT.

**A strategy does not execute on the runtime.** It declares the routing and
the prompts; your backend runs the LLM step (using the templates), performs
settlement (the `compute` op over Connect, for turn strategies), and writes
the resulting KnowledgeEntry drafts into the World over Connect. The runtime
owns the World KB, the write/read ops, and host-local compute modules — see
[`nexus-runtime.md`](nexus-runtime.md). The division of labor is spelled out in
the [integrator walkthrough](../strategy-samples/README.md) — read that first
for the end-to-end loop; this doc is the authoring reference.

> **Execution surfaces.** A strategy bundle is consumed on two surfaces:
> 1. **Integrator path (primary)** — your backend runs the LLM step using the
>    templates and writes KnowledgeEntry drafts into the World over Connect
>    (the E2 loop in the
>    [integrator walkthrough](../strategy-samples/README.md)). The runtime has
>    no daemon HTTP router and no schedule supervision.
> 2. **Creator-daemon path (optional)** — install the bundle under
>    `~/.nexus42/presets/<id>/` and drive it through the daemon's schedule
>    API; preset lanes then execute on the creator-facing daemon's
>    orchestration engine. Snippets marked *daemon path* below apply only
>    there.

Two forkable bundles ship as worked examples (details in
[Worked examples](#worked-examples)):

- [`game-narrative/`](../strategy-samples/game-narrative/) — lore import:
  trigger (game event) + scheduled (interval sweep) lanes extracting
  worldview / character-sheet KnowledgeEntry drafts + Relation hints.
- [`react-trpg-turn/`](../strategy-samples/react-trpg-turn/) — TRPG turn
  loop: mechanical-op lane (settle-first) + natural-language-turn lane
  (propose → settle → narrate), plus the browse-guard contract in its README.

## Bundle layout

```
my-strategy/
├── preset.yaml            # manifest: lanes, states, inner graphs, run payload
└── templates/             # prompt templates, referenced by relative path
    ├── lane-route.md              # lane selector judge (GO/NOGO)
    ├── <lane-step>.md             # one template per prompt step
    └── ...
```

The bundle directory name is part of the contract: the validator enforces
`preset.id` == bundle directory name (`check_bundle_id_vs_directory`).

## preset.yaml — manifest format

Top-level keys (subset of `PresetManifest` in
`crates/nexus-contracts/src/local/orchestration/preset.rs`):

| Key | Meaning |
|-----|---------|
| `preset.id` | Identifier; `/^[a-z][a-z0-9._-]*$/` and **must equal the bundle directory name**. There is no separate `name` field — `id` is the identifier and `description` is the human label. |
| `preset.version` | Schema version (`>= 1`; bump on breaking changes to this preset). |
| `preset.kind` | `creator` (user-facing) or `system` (internal). External strategies use `creator`. |
| `preset.description` | Human-readable description (<= 240 chars). |
| `preset.requires_capabilities` | Capabilities the strategy needs (`acp.prompt`, `creator.inject_prompt`, `judge.llm`, …). Loader rejects a preset whose capabilities are missing from the registry. |
| `preset.run_intents` | Declared run intents (e.g. `knowledge_ingest`, `work_continue`); used for discovery/filtering. |
| `preset.initial` / `preset.terminal` | IDs of the initial and terminal states; both must exist in `states[].id`. |
| `preset.initial_action` | Optional action on run start (e.g. `kind: seed_direct` stores the schedule seed/input verbatim as `core_context` v0). |
| `signals` | Optional declared external-event bindings (schema-valid, parsed by the loader — but **no runtime component delivers declared signals yet**; the daemon's `/schedules/{id}/signal` endpoint only handles lifecycle signals `start|pause|resume|cancel|advance` and rejects preset-declared names). Treat signal delivery as a host customization point; the schedule-input path is the primary lane entry. |
| `states` | Ordered state list — the outer state machine (see below). |
| `inner_graphs` | Optional named node graphs referenced by `enter.kind: inner_graph` (see below). |

### states

Each state has:

| Field | Meaning |
|-------|---------|
| `id` | Unique state id within the preset. |
| `description` | Optional human description. |
| `enter` | Optional actions on entry: `kind: inner_graph` (run a named `inner_graphs` entry), `kind: capability` (invoke a capability, e.g. `creator.inject_prompt` with `prompt_file`). |
| `exit_when` | Exit condition: `kind: llm_judge` (judge prompt in `template_file`, optional `min_interval` throttle), `kind: graph_complete` (inner graph finished), `kind: manual` (park — `NextAction::WaitForInput`). |
| `next` | Edge(s) to the next state: linear target, `go`/`nogo` targets (only valid on `llm_judge` states), or `branches` with `when: "<expr>"` + `default`. |
| `terminal` | `true` for the terminal state. |

### inner_graphs

Named node graphs entered from a state. Nodes are sequenced with
`depends_on` (the example below is the `game-narrative` trigger lane's
`trigger_graph` verbatim):

```yaml
inner_graphs:
  trigger_graph:
    nodes:
      - id: trigger_parse
        kind: acp_prompt
        template_file: templates/trigger-game-event.md
        tool_policy: auto_grant_read_only
      - id: extract_worldview
        kind: acp_prompt
        depends_on: [trigger_parse]
        template_file: templates/import-worldview.md
        tool_policy: auto_grant_read_only
      - id: extract_characters
        kind: acp_prompt
        depends_on: [extract_worldview]
        template_file: templates/import-character-sheet.md
        tool_policy: auto_grant_read_only
    output_binding: extract_characters.text
```

Node kinds: `acp_prompt` (run a prompt template). `output_binding` exports one
node's output under a key for the caller. Each node template is
self-contained (reads `preset.input`) per the embedded-preset convention.

## Run payload (`preset.input`)

The run payload is the strategy's input contract. It is supplied by your
backend. On the **daemon path** (optional — the creator app's schedule API),
it is provided when creating the schedule:

```text
POST /v1/daemon/orchestration/schedules
{ "creator_id": "<creator-id>", "preset_id": "<strategy-id>",
  "input": { ... run payload ... } }
```

The `input` field maps to `preset.input.*` at schedule start. On the daemon
path, the CLI's `nexus42 daemon schedule add --preset <id> --creator <id>
--seed <text>` only stores seed text as `core_context` v0 — it does **not**
populate `preset.input.*`.

**Template rendering vs expression resolution.** Prompt templates render
dotted keys — `{{preset.input.mode}}` works inside a template. The expression
engine that evaluates branch conditions resolves `_context.<path>` against a
**flat** context map, so `_context.preset.input.mode` can never be read
(context keys are stored literally as `"preset.input.mode"`, and the grammar
has no bracket syntax). The only runtime-populated discriminator expressions
can see is `_context._judge_result`, written by an `llm_judge` exit **before**
branch evaluation. Consequently:

> Lane selection is always a tiny judge (`llm_judge` exit whose prompt reads
> `preset.input.<selector>` and answers GO/NOGO), never a branch expression.

Both samples follow this pattern (`game-narrative` routes on
`preset.input.mode`, `react-trpg-turn` on `preset.input.trigger_type`).

## Trigger lanes

A trigger lane runs when an external event arrives (a game event, a player
action, a mechanical op request). Two shapes, both in the samples:

- **game-narrative trigger lane** — a game event from the game runtime is
  turned into an extraction/assembly task: `trigger-game-event.md` emits a
  task brief, then the worldview / character-sheet extraction templates emit
  KnowledgeEntry drafts + Relation hints (see
  [Worked examples](#worked-examples)).
- **react-trpg-turn lanes** — the partner's turn contract has three trigger
  types, modeled as **two preset lanes + one README contract**:
  - *Mechanical-op lane* (trigger type 2): an explicit mechanical action
    (attack, cast, shield block, check shortcut) carries a stable
    `operationId` + `params`; the caller settles **first** via the Connect
    `compute` op (host-local WASM — read-only over Connect), with no AI step
    in the request path; the AI then confirms the receipt
    (`settle-receipt.md`) and narrates from the confirmed receipt only
    (`receipt-narration.md`).
  - *Natural-language-turn lane* (trigger type 3): the AI parses the player's
    free-text intent (`natural-language-intent.md`), **proposes** an op
    request + params without pre-announcing outcomes
    (`natural-language-op-request.md`), the caller settles, and the AI
    narrates the confirmed receipt only, then parks at `wait_for_player`
    (`ExitWhen::Manual`).
  - *Browse guard* (trigger type 1): pure-UI operations (view sheet, switch
    page, expand spell, open inventory) are **not** a preset lane — no AI
    call, no world-time advance, no state mutation. Governed by the README
    contract in `react-trpg-turn/README.md`.

## Scheduled lanes

A scheduled lane runs on an interval/cron-style sweep rather than an event.
`game-narrative` shows the pattern: the `scheduled_sweep` state runs an
inventory prompt (`creator.inject_prompt` with `scheduled-sweep.md`) that
lists new/changed source documents, then an `llm_judge` exit
(`scheduled-sweep-exit.md`) decides GO (something new to import) or NOGO
(nothing new — the state parks and is re-evaluated at the next interval).
Extraction runs only in the separate `sweep_extract` state after a GO, so a
parked sweep never re-runs the LLM extraction work.

Two mechanisms:

- **`min_interval`** — judge cadence throttle (ISO-8601 duration, e.g.
  `"PT1H"`): the lane re-evaluates at most once per interval.
- **Watermark ownership** — the engine does **not** persist the sweep
  watermark. Nothing writes `core_context.*` session keys at runtime (the
  schedule seed/input is stored verbatim as core_context v0, never parsed
  into template context), and the sweep's `next_watermark` is not written
  back anywhere. Your backend owns the watermark: capture `next_watermark`
  from the sweep brief, persist it, and feed it back as
  `preset.input.watermark` on the next run. With an empty watermark every
  sweep treats all documents as new.

## Prompt templates (`templates/`)

Templates are markdown files with a YAML frontmatter header (e.g.
`max_tokens`). They are referenced from the manifest by **relative path**:

| Reference | Where |
|-----------|-------|
| `template_file` | `exit_when.kind: llm_judge` (judge prompt) and inner-graph `acp_prompt` nodes |
| `prompt_file` | `enter` capability args (e.g. `creator.inject_prompt`) |
| `system_prompt_file` | optional multi-agent role definitions |

Templates read the run payload via `{{preset.input.<key>}}` (dotted keys
render fine in templates). Each template is self-contained — it reads what it
needs from `preset.input` rather than relying on shared session context.

## Validator

`strategy-samples/validate.sh` runs the **real validator core** in-process
with no daemon required:

```bash
bash strategy-samples/validate.sh                        # bundled game-narrative sample
bash strategy-samples/validate.sh strategy-samples/react-trpg-turn   # TRPG turn sample
bash strategy-samples/validate.sh my-strategy            # your fork
```

Requires the `nexus42` CLI on `PATH` (build it once from the repo root:
`cargo build --bin nexus42`). Exit status: `0` when the strategy validates
clean, non-zero otherwise (`127` when the CLI is missing).

The script delegates to `nexus42 system preset validate --offline <path>`.
`--offline` mirrors the daemon's `POST /v1/daemon/presets:validate`
composition in-process: `loader_validate_manifest_compat` +
`validate_path_safety` + `validate_preset_semantic` +
`validate_assets_in_bundle`. `--json` prints the machine-readable verdict:

```bash
nexus42 system preset validate --offline --json strategy-samples/react-trpg-turn
# pretty-printed JSON on stdout:
# {
#   "errors": [],
#   "id": "react-trpg-turn",
#   "state_count": 5,
#   "valid": true,
#   "version": 1
# }
```

Without `--offline` the command delegates to a running creator daemon; the
daemon-backed mode cannot take a directory argument (pass the `preset.yaml`
file path there). `validate.sh` always uses `--offline`.

### What the validator proves

- **Manifest compatibility** (loader §7.6) — required capabilities exist in
  the registry; `initial` / `terminal` and every `next` target reference
  existing states; `go`/`nogo` only on `llm_judge` states.
- **Semantic checks** (A2/A4/A5/A6) — initial→terminal reachability,
  terminal-marker consistency, bundle-id match, inner-graph references,
  labeled-edge duplicates, merge-node integrity, capability-arg
  compatibility, run intents, CLI args.
- **Path safety (A3)** — every `template_file` / `prompt_file` /
  `system_prompt_file` reference is a safe relative path (no `..`, no
  absolute paths, no backslashes, no null bytes / control characters).
- **Assets in bundle (A3)** — `preset.id` equals the bundle directory name;
  every referenced template file exists inside the bundle; no symlink
  escapes out of the bundle.
- **Size limits** — YAML files are capped at 1 MiB and depth 10
  (`DEFAULT_MAX_YAML_SIZE` / `DEFAULT_MAX_YAML_DEPTH`).

The path argument may be a bundle directory (resolves `<dir>` →
`<dir>/preset.yaml`), a `preset.yaml` file (asset checks run against its
parent), or a standalone YAML file (standalone skips asset checks). On
oversize / parse / depth failures the offline path exits non-zero with no
JSON body (error on stderr) — treat non-zero as "could not validate".

**What it does not prove.** The validator proves schema, graph shape, and
referenced assets. Idempotency guarantees (uniqueness of `turnId` /
`operationId`, retry dedup, atomic commit, client unlock) belong to the
client/runtime boundary: your backend generates stable ids, persists raw
input + receipts + final output in its own turn ledger, and applies the
world-aware CAS rules when committing. YAML alone does not provide a global
idempotency ledger.

### Adding a sample

1. Add a bundle directory under `strategy-samples/` (`preset.yaml` +
   `templates/`).
2. Ensure `preset.id` equals the directory name.
3. Validate: `bash strategy-samples/validate.sh strategy-samples/<id>`.

## Fork flow

1. Copy a bundle anywhere you own — it is a plain directory; nothing under
   `strategy-samples/` is embedded or required at build time:
   ```bash
   cp -R strategy-samples/react-trpg-turn my-strategy
   ```
2. Edit `preset.id: my-strategy` (and the `description`) in
   `my-strategy/preset.yaml` — the validator enforces
   `preset.id` == directory name, so an unedited copy fails validation.
3. Edit prompts, triggers, and cadence freely; keep the manifest schema
   shape intact.
4. Validate: `bash strategy-samples/validate.sh my-strategy`.

Strategies can also be installed for daemon-side runs via the 3-tier preset
resolution: `~/.nexus42/presets/<id>/` overrides embedded presets with the
same id.

## Editing an installed strategy (`preset patch`)

Once a strategy is installed under `~/.nexus42/presets/<id>/`, the **write
path is the daemon's strategy canvas API** via the CLI leaves
(`nexus42 preset patch state|transition|prompt`, V1.175 P1 — see
[cli-spec §6.2G.4](../.mstar/specs/cli-spec.md)):

```bash
# Patch a state node (rename via --label, or update --description).
nexus42 preset patch state my-strategy start --base-revision 1 --description "New description"

# Rewire a transition (create or update).
nexus42 preset patch transition my-strategy --base-revision 1 --source-state start \
  --op update --old-target end --new-target done

# Patch a state's prompt template (--file <path> or '-' for stdin).
nexus42 preset patch prompt my-strategy start --base-revision 1 \
  --template-ref prompts/start.md --file prompts/start.md
```

Every write is **CAS-guarded**: pass `--base-revision` (the revision
observed on the last canonical read, e.g. `nexus42 preset show
my-strategy`). A stale revision returns 409 `strategy_conflict` naming the
current revision, the conflicting path, and a recovery hint — re-read the
Strategy and reapply with the new revision. Embedded/system presets are
read-only; only user bundles under `~/.nexus42/presets/<id>/` are patchable
(the daemon surfaces the rejection as a 400 `bad_request` — its public
`error_code()` allowlist does not passthrough the internal
`strategy_update_forbidden` code).

## Worked examples

- [`react-trpg-turn/`](../strategy-samples/react-trpg-turn/) — README
  carries the full turn contract: browse guard, the two lanes, the
  idempotency/completion contract, and the distillation mapping to the
  partner's turn contract. `preset.yaml` shows both lane graphs; templates/
  shows the stepwise prompt chain (`intent → op request → settle receipt →
  narration`).
- [`game-narrative/`](../strategy-samples/game-narrative/) — `preset.yaml`
  shows the trigger + scheduled lane split and the lane-selector judge;
  templates/ shows the extraction → KnowledgeEntry/Relation wire contract
  (the exact draft shape your backend writes over Connect).
- [`strategy-samples/README.md`](../strategy-samples/README.md) — the
  end-to-end integrator walkthrough (runtime boot, Connect SDK, N-C1/N-C2
  ops, compute, fork + validate).

## Where the strategy runs

On the **daemon path** (optional — see the *Execution surfaces* callout at
the top of this doc), preset lanes execute on the
creator-facing daemon's orchestration engine via its schedule API;
`nexus-runtime` intentionally ships without the daemon HTTP router and
without schedule supervision. On the **integrator path**, your backend
drives the strategy side (its own timer/event loop + LLM step using the
templates) and writes results into the World over Connect — see
[`nexus-runtime.md`](nexus-runtime.md) for the runtime surface and the
[integrator walkthrough](../strategy-samples/README.md) for the wire
patterns.

## Next steps

- [Module authoring](module-authoring.md) — WASM ABI, `manifest.json` (incl.
  `wasm_sha256`), `module_scope` allowlist, operator install (turn strategies
  depend on the Connect `compute` op).
- [Runtime usage](nexus-runtime.md) — install/run, allowlist + `module_scope`
  setup, home layout.
- [Integrator walkthrough](../strategy-samples/README.md) — worked example
  end to end.
- [Docs index](README.md) — all docs.
