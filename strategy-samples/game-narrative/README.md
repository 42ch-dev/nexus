# Game Narrative Strategies Sample

A forkable orchestration sample for AI-narrative games: a **trigger lane** turns
a game event into a brief plus worldview / character-sheet KnowledgeEntry drafts
and Relation hints, and a **scheduled lane** inventories new or changed source
documents on an interval, judges whether anything is new, and only then runs
extraction in a separate state. It declares capability routing and prompt
templates; your backend runs the LLM step and writes the drafts into a World
over Connect (see [`../README.md` §6](../README.md#6-fork-the-strategy-samples-and-validate)).

## Bundle layout

```
game-narrative/
├── workflow.json          # the authoring source: a WorkflowDocument (edit this)
├── preset.yaml            # generated from workflow.json — not the source
├── templates/             # the prompt templates `assets[]` hashes and carriers name
│   ├── lane-route.md              # lane-selector judge (GO/NOGO)
│   ├── trigger-game-event.md      # trigger-lane brief
│   ├── import-worldview.md        # worldview drafts
│   ├── import-character-sheet.md  # character drafts + Relation hints
│   ├── scheduled-sweep.md         # inventory prompt for the sweep
│   └── scheduled-sweep-exit.md    # sweep GO/NOGO judge
└── README.md              # this file
```

## Authoring

**Edit `workflow.json`.** It is the source of truth for the sample;
`preset.yaml` is its export and is not the file to author. CI re-exports
`workflow.json` and fails with the divergent YAML path when the two disagree:

```bash
cargo test -p nexus-orchestration --test sample_self_check
```

Canvas is not required for repository sample authoring — the JSON is edited
directly. The run payload (`preset.input.mode`, `game_event`, `watermark`) is
documented in
[`../../docs/strategy-authoring.md`](../../docs/strategy-authoring.md), with
per-state notes beside the states in `preset.yaml`.

`assets[]` is the one hand-maintained part: the export does not project it, so
each template a carrier references needs exactly one entry whose `content_hash`
is the BLAKE3 hex digest of that file's bytes. Recompute it with:

```bash
python3 -c 'import blake3,sys;print(blake3.blake3(open(sys.argv[1],"rb").read()).hexdigest())' \
  strategy-samples/game-narrative/templates/lane-route.md
```

Run that per template from the repository root and paste the digest into the
entry. The fuller authoring notes, including the round-trip normalisations, are
in [`../README.md` §6](../README.md#6-fork-the-strategy-samples-and-validate).

## Validating

```bash
./strategy-samples/validate.sh strategy-samples/game-narrative
```

Run from the nexus repository root — the bundle path is repo-root-relative. It
runs the real local validator core (`nexus42 preset validate <path>`), no
service host needed. Exit 0 = clean. The manifest format behind the export is
[`../../docs/strategy-authoring.md`](../../docs/strategy-authoring.md).
