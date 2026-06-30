# Concepts — Nexus Domain Vocabulary

Core domain terms used across Nexus OSS documentation, plans, and code. Each entry is a concise definition of what the term means *in this project*.

---

## Creative Writing Domain

### World
The core creative container — a narrative universe with its own knowledge base, timeline, and structured state. Worlds are the top-level organizational unit in Nexus. World history is **immutable**: changes go through Fork, never in-place mutation.

### KeyBlock
The fundamental unit of structured knowledge in a world. KeyBlocks have typed attributes (character, location, event, concept, etc.), taxonomy labels, and an immutable identity. *Computable* KeyBlocks accumulate mutable state over WASM compute invocations.

### SourceAnchor
A reference that ties a KeyBlock to its provenance — which artifact (manuscript chapter, outline node, etc.) produced it and at what position.

### Manuscript
The structured prose output within a world — organized into chapters, scenes, and narrative flow. A world may have multiple manuscripts representing parallel storylines or drafts.

### Timeline
The ordered sequence of events and KeyBlocks in a world. The "when" axis of the narrative. Timeline entries are append-only; rewrites create Forks.

### Fork
The only mechanism for changing world history. Creates a divergent branch from a point in the timeline. Original history is preserved. Forks are the structural equivalent of version control branches for narrative.

### Scope
A named selection of KeyBlocks for context assembly — defines which knowledge is visible during a specific creative moment (e.g., "current chapter scope", "scene scope").

### Narrative Profile
A world's structural type that determines which narrative tools and capabilities are available. Examples: `novel`, `essay`, `game-bible`.

### Outline
A structured, non-linear representation of a work's planned content — nodes representing chapters, scenes, beats, arcs, arranged on the infinite canvas. Outlines are editable and drive manuscript generation.

---

## Compute & AI Domain

### Compute (Capability)
A WASM-powered execution unit within a world. Examples: combat engine resolution, dice rolling, relationship graph computation. Compute modules are embedded (shipped with the binary) or user-authored.

### Preset
A pre-configured bundle of compute capabilities with a YAML manifest. Presets define which capabilities are available, how they sequence, and what prompts/rules they use. Example: "combat-engine" preset.

### Creator
The local user's identity aggregate — author profile, preferences, memories. A creator has one or more works and is the "self" that agents interact with.

### Creator Memory
The creator's persistent memory pipeline — a structured I/O system ("SOUL") that stores and retrieves personal context across sessions. This is *not* World KB; it's the author's own memory (writing preferences, character voice notes, etc.).

### Moment Context Assembly
The process of assembling the right set of KeyBlocks, timeline state, and creator memory for a given creative moment. Produces a "moment context" that an agent sees when performing a task (e.g., "write next chapter").

### Quality Loop
The iterative process: write → reflect → generate findings → human review → apply changes. Separates automated quality analysis from human decision-making.

### Knowledge Loop
The process: persist new text → extract structured knowledge → promote to World KB. Runs on a schedule, not inline with writing.

---

## Protocol & Infrastructure

### ACP (Agent Communication Protocol)
The standard protocol for agent-to-agent communication. Nexus is an **ACP client** (not an ACP agent/server). It sends requests to the user's local agents and receives structured responses.

### Agent Host
The adapter layer that translates between Nexus's internal capability model and external ACP agents. Allows Nexus to ask any ACP-compliant agent to perform tasks without being tied to a specific provider.

### Daemon Runtime
The local background process within `nexus42` that manages the World KB SQLite database, schedules quality/knowledge loops, serves the local HTTP API (Axum), and coordinates with the agent host. Starts with `nexus42 daemon start`.

### Local Database
SQLite-based (via sqlx) persistent storage. Contains World KB tables, creator profiles, timeline data, and orchestration state. Single database per home directory.

### JSON Schema (Wire Contracts)
The single source of truth for all cross-language types. `schemas/` directory defines the JSON Schema, and codegen produces Rust types (`crates/nexus-contracts/`) and TypeScript types (`@42ch/nexus-contracts` npm package).

### Workspace (Canvas)
The infinite canvas surface that visually organizes creative material — worlds, manuscripts, outlines, KeyBlocks, and relationships — into a navigable spatial layout.

### Web UI
The local-first "Control Room + Setup" web interface (`apps/web`). A React SPA served by the daemon over HTTP (`127.0.0.1:8420`), providing the infinite canvas, workspace management, and structured writing tools. Reuses the `@42ch/nexus-contracts` TypeScript types — never hand-writes wire DTOs.

### Desktop Shell
The Tauri v2 native desktop client (`apps/desktop`). Wraps the web SPA (`apps/web/dist`) in a native window, adds OS-level capabilities (Open with…, Reveal in Finder, Copy Path, sidecar lifecycle management). Detects the Tauri runtime at startup and selects `TauriClient` over `BrowserClient` via capability detection.

---

## Cross-Reference

| Term | Related concepts | Spec doc |
|------|-----------------|----------|
| World | Fork, Timeline, Manuscript, Scope | entity-scope-model.md |
| KeyBlock | SourceAnchor, Taxonomy, Computable | entity-scope-model.md |
| Creator | Creator Memory, Works | creator-workflow.md |
| Compute | Preset, WASM module, Capability Registry | compute-module-abi.md |
| ACP | Agent Host, Daemon Runtime | acp-client-tech-spec.md |
| Workspace | Canvas, Outline, Manuscript | canvas-strategy-surface.md |
| Web UI | Desktop Shell, Daemon Runtime, NexusClient | web-ui.md |
| Desktop Shell | Web UI, Sidecar, Tauri IPC | desktop-shell.md |
