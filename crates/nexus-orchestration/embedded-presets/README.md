# Embedded Presets

This directory contains embedded presets compiled into the `nexus42` binary at build time. Every subdirectory with a `preset.yaml` is automatically discovered and validated by the loader at startup.

**P1 strict validation gate**: all embedded presets are validated against the shared semantic validator (`validate_preset_semantic` + `validate_path_safety`) at test time via the `all_embedded_presets_pass_strict_validation_gate` smoke test.

## Preset Catalog

| Preset | Pattern | States | Description |
|--------|---------|--------|-------------|
| `kb-extract` | Knowledge extraction | loading → extracting → done | Extract structured KeyBlocks from work-scope KB entries |
| `memory-augmented` | Memory recall + persist | recall → generate → persist → done | Recall memories, generate content, persist as new memory |
| `novel-writing` | Multi-phase writing | gathering → brainstorming → outlining → drafting → done | Multi-agent novel writing with roles (writer, reviewer) |
| `reflection-loop` | Self-reflection | draft → revise → summarize → done | Generate, critique, revise, and summarize with LLM judge |
| `research` | Research workflow | scanning → extracting → synthesizing → done | Scan references, extract content, produce structured reports |
| `soul-experience-refresh` | SOUL maintenance | aggregate → done | Aggregate long-term memories into SOUL Experience section |

## Manual Run

All presets are invoked via the daemon scheduler:

```bash
# Example: run reflection-loop
nexus42 daemon schedule add \
  --preset reflection-loop \
  --creator <creator-id> \
  --seed "Explain quantum computing in simple terms"

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

## Validation

All presets are embedded at compile time and validated by the loader at startup. The P1 strict validation gate runs at test time:

```bash
# Run the embedded preset smoke test (B1/B2)
cargo test -p nexus-orchestration -- all_embedded_presets_pass

# Run preset-specific tests
cargo test -p nexus-orchestration -- reflection_loop
cargo test -p nexus-orchestration -- memory_augmented
cargo test -p nexus-orchestration -- kb_extract

# Run full validation suite
cargo test -p nexus-orchestration
cargo clippy -p nexus-orchestration -- -D warnings
```

## Design Notes

- All presets are **linear state machines** with no conditional routing (`ConditionalNotYetSupported` remains enforced)
- Multi-agent presets (novel-writing, research) use the `roles` section; others are single-agent
- Prompt templates use Handlebars syntax (`{{preset.input.*}}`)
- The `context.summarize` capability in reflection-loop requires a worker at runtime; in standalone mode it returns `WorkerUnavailable`
- The `creator.read_memory` / `creator.write_memory` capabilities work in standalone mode (return stubs) and with a pool (real persistence)
- `exit_when: kind: rule` with no expression is the explicit always-true (immediate transition) form — the state advances as soon as its enter action completes
