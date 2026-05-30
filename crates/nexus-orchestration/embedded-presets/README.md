# Embedded Presets — Agentic Design Pattern Demonstrators

This directory contains two embedded presets that demonstrate agentic design patterns, composing real capabilities delivered in v1.31 (P1: creator memory, P2: judge/summarize + worker IPC).

## reflection-loop — Self-Reflection Pattern

A linear state machine that generates, critiques, revises, and summarizes content using an LLM judge for quality gating.

### State Flow

```
draft → revise → summarize → done
```

| State     | Enter                    | Exit When                     | Next      |
|-----------|--------------------------|-------------------------------|-----------|
| draft     | `acp.prompt` (generate)  | `judge.llm` (quality check)   | revise    |
| revise    | `acp.prompt` (critique)  | `judge.llm` (quality check)   | summarize |
| summarize | `context.summarize`      | manual                        | done      |
| done      | *(terminal)*             | —                             | —         |

### Capabilities Required

- `acp.prompt` — LLM generation for draft and revision steps
- `judge.llm` — Quality evaluation at draft and revise exit gates
- `context.summarize` — Condense the final refined output

### Input Variables

| Variable | Required | Description                          |
|----------|----------|--------------------------------------|
| `topic`  | Yes      | The topic or task to draft about     |
| `content`| No       | Optional seed content for the draft  |

### Manual Run

```bash
nexus42 schedule add \
  --preset reflection-loop \
  --var topic "Explain quantum computing in simple terms" \
  --var content "Focus on superposition and entanglement"
```

### Prompt Templates

| File                            | Purpose                                |
|---------------------------------|----------------------------------------|
| `prompts/generate-draft.md`     | Initial draft generation prompt        |
| `prompts/draft-quality-check.md`| Judge prompt for draft evaluation      |
| `prompts/apply-critique.md`     | Revision prompt applying feedback      |
| `prompts/revise-quality-check.md`| Judge prompt for revision evaluation  |
| `prompts/summarize-output.md`   | Summary generation prompt              |

---

## memory-augmented — Memory Recall + Persist Pattern

A linear state machine that recalls relevant memories, generates new content informed by those memories, and persists the result as a new memory fragment.

### State Flow

```
recall → generate → persist → done
```

| State    | Enter                         | Exit When   | Next     |
|----------|-------------------------------|-------------|----------|
| recall   | `creator.read_memory`         | rule        | generate |
| generate | `acp.prompt` (with context)   | graph_complete | persist |
| persist  | `creator.write_memory`        | manual      | done     |
| done     | *(terminal)*                  | —           | —        |

### Capabilities Required

- `creator.read_memory` — Recall memories by keyword
- `acp.prompt` — Generate content using recalled memories as context
- `creator.write_memory` — Store the generated output as a new memory

### Input Variables

| Variable | Required | Description                          |
|----------|----------|--------------------------------------|
| `keyword`| Yes      | Keyword to filter memories for recall|
| `topic`  | Yes      | Topic for content generation         |

### Manual Run

```bash
nexus42 schedule add \
  --preset memory-augmented \
  --var keyword "character-development" \
  --var topic "Write a character arc for the antagonist"
```

### Prompt Templates

| File                               | Purpose                                  |
|------------------------------------|------------------------------------------|
| `prompts/recall-memory.md`         | Memory recall context description        |
| `prompts/generate-with-memory.md`  | Generation prompt with recalled memories |
| `prompts/persist-memory.md`        | Persistence step description             |

---

## Validation

Both presets are embedded at compile time and validated by the loader at startup.

```bash
# Run preset-specific tests
cargo test -p nexus-orchestration -- reflection_loop
cargo test -p nexus-orchestration -- memory_augmented

# Run full validation
cargo test -p nexus-orchestration
cargo clippy -p nexus-orchestration -- -D warnings
```

## Design Notes

- Both presets are **linear state machines** with no conditional routing (`ConditionalNotYetSupported` remains enforced)
- Neither preset uses multi-agent roles (single-agent mode)
- Prompt templates use Handlebars syntax (`{{preset.input.*}}`)
- The `context.summarize` capability in reflection-loop requires a worker at runtime; in standalone mode it returns `WorkerUnavailable`
- The `creator.read_memory` / `creator.write_memory` capabilities work in standalone mode (return stubs) and with a pool (real persistence)
