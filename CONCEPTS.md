# Concepts — Nexus Domain Vocabulary

Core domain terms used across Nexus OSS documentation, plans, and code. Each entry is a concise definition of what the term means *in this project*.

---

## Three Pillars

The product thesis (canonized in the V1.122 pivot): **Nexus is the local-first creative-writing tool where a World's Timeline is the central instrument, AI agents are harnessed through Canvas, and Computable modules make worlds react.** Each pillar below names a product thesis, not a single crate. See `STRATEGY.md` § *Vision → Three pillars* for the crate/spec mapping.

### Harness
The **control-strategy pillar**: how an author *harnesses* AI agents to execute creative work — orchestration, capability routing, agent hosting, and presets. Maps to the orchestration engine + agent host + capability registry + presets (UI still labels this "Strategy / Preset"; product rename to "Harness" is deferred — `DF-V1122-HARNESS-RENAME`). Specs: [orchestration-engine.md](.mstar/specs/orchestration-engine.md), [agent-host.md](.mstar/specs/agent-host.md), [capability-registry.md](.mstar/specs/capability-registry.md).

### Computable
The **product-thesis pillar** that *worlds react* via WASM compute — combat resolution, dice, relationship-graph computation, user-authored modules. **Distinct from `Compute (Capability)` below**: *Computable* is the pillar (the product claim that worlds react); *Compute (Capability)* is the mechanism (the WASM execution unit an author/agent invokes). The pillar names the thesis; the capability names the implementation unit. Specs: [compute-module-abi.md](.mstar/specs/compute-module-abi.md), [wasm-host.md](.mstar/specs/wasm-host.md).

### Timeline-first World building
The **Canvas hero pattern** (V1.122, deepened V1.123): a World's Timeline is the primary Canvas surface for **World entry** — authors open a World and meet its *when* axis before its entity graph or chapter structure. `CanvasSurfaceKind = "timeline"` is a peer surface alongside Strategy / Outline (Timeline-companion) / World KB, and is the default **World-entry** surface. **Outline** remains the default for **Work entry** (V1.118, unchanged). From V1.123, Timeline is not a single flat event list: it is **three zoom layers** — [Brief](#brief), [Narrative](#narrative), and [Moment](#moment) — with domain-differentiated use (World: Brief+Narrative; Work: Narrative+Moment via peer `work-timeline`).

**Spine vs projection** (locked product model):
- **Spine:** World + Timeline + KnowledgeEntry + Fork — the truth of the narrative universe. Timeline is the World's *when* axis (three layers).
- **Projection:** Work + Outline + Manuscript — the authoring plan and prose bound to a World. Outline is the Work's structural projection (chapters / scenes); Work Timeline is a peer projection for Narrative+Moment.

Authors should feel: **World first for World building (Timeline, Brief-led); Work first for chapter writing (Outline), with Work Timeline reachable for scene precision.** Dual entry defaults encode that. Spec: [canvas-strategy-surface.md](.mstar/specs/canvas-strategy-surface.md). Iteration framing: [iterations/v1.122/specs/pillar-framing.md](.mstar/iterations/v1.122/specs/pillar-framing.md), [iterations/v1.123/specs/three-layer-product-spec.md](.mstar/iterations/v1.123/specs/three-layer-product-spec.md).

---

## Creative Writing Domain

### World
The core creative container — a narrative universe with its own knowledge base, timeline, and structured state. Worlds are the top-level organizational unit in Nexus. World history is **immutable**: changes go through Fork, never in-place mutation.

### KnowledgeEntry
The fundamental unit of structured knowledge in a world. KnowledgeEntries have typed attributes (character, location, event, concept, etc.), taxonomy labels, and an immutable identity. *Computable* KnowledgeEntries accumulate mutable state over WASM compute invocations.

### Lore Activation
The default-on mechanism (V1.149 / DF-74) that selects and orders World KB entries for a moment by their `modules.activation` fire-conditions (`keys` / `secondary_keys` / `logic` / `constant` / `priority` / `order` / `match`), with optional relation-hop expansion (≤2 hops) from firing entries. Worlds whose entries carry no activation module are assembled byte-identically to the pre-activation path. Applied during [Moment Context Assembly](#moment-context-assembly); dialect and contract: [`spoke-adapter-architecture.md`](.mstar/specs/spoke-adapter-architecture.md) §7.4.

### Moment Directive
A **short-horizon, author-written instruction** (V1.150 / DF-75) that injects into the assembled prompt as a distinct section **above lore, below system/personality** — the clean-room Author's-Note analogue. One paragraph, not a second system prompt; never silently generated. Scoped **per-Work with an optional World override** (closest scope wins; not creator-global). Lifetime is governed by **TTL** (`generations` — one injecting assemble = one count; `chapters` — one per chapter advance for novel Works) and optionally **clear-on-scene-change** (focused `event_id` change proxy). **Product-local only** — never a SPOKE object: not a `modules.*` entry, not a KnowledgeEntry, never on the spoke wire or in AssemblePacket traces (AC-I3). Author surface V1.150: CLI (`nexus42 creator moment-directive set|show|clear`); observation via `platform context assemble-moment` output. Not stage-gated — its TTL governs lifetime, not generation stage. Spec: [`fl-l-w5-prompt-control-plane.md`](.mstar/iterations/v1.150/specs/fl-l-w5-prompt-control-plane.md) §3 (tracked matrix: [`spoke-adapter-architecture.md`](.mstar/specs/spoke-adapter-architecture.md) §7.4). Cross-ref: [Lore Activation](#lore-activation), [Preset (Injection) Slot](#preset-injection-slot), [Moment Context Assembly](#moment-context-assembly).

### Preset (Injection) Slot
A **named, ordered region of the assembled prompt** (V1.150 / DF-75) filled by activated lore — the slot layer that shapes the `## World Knowledge Base` block after [Lore Activation](#lore-activation) decides what fires. Shipped slots: `world.before` (before-defs anchors), the default fallback (the V1.149 flat block — the byte-equivalence anchor), `world.after` (post-defs reminders), open `kb.outlet.<name>` (author-named outlets, sorted; unknown names are not errors), and `style.post_history` (the one reserved well-known outlet, tail of the lore block) — plus the `moment.directive` slot (top-level section, filled by the [Moment Directive](#moment-directive)). Emit order is locked (spec §2/Q5); within-slot order keeps the V1.149 priority-then-order. Slot filling is **generation-stage gated** (spec §4): `style.post_history` fills only for `produce`/`review`; `system_maintenance` runs no lore slots; direct-CLI `unspecified` keeps everything on. **Disambiguation:** distinct from [Preset](#preset) under Compute & AI Domain (a pre-configured bundle of compute capabilities) — this entry is the *injection* slot in the assembled prompt, not a preset manifest. Spec: [`fl-l-w5-prompt-control-plane.md`](.mstar/iterations/v1.150/specs/fl-l-w5-prompt-control-plane.md) §2 (tracked matrix: [`spoke-adapter-architecture.md`](.mstar/specs/spoke-adapter-architecture.md) §7.4).

### SourceAnchor
A reference that ties a KnowledgeEntry to its provenance — which artifact (manuscript chapter, outline node, etc.) produced it and at what position.

### Extensions (`extensions.nexus`)
A SPOKE-standard mechanism for carrying nexus-specific fields on spoke-schema objects. `extensions.nexus` is a typed namespace on spoke `KnowledgeEntry` (and other spoke types) that holds nexus-local identity, provenance, and lifecycle metadata — e.g., `world_id`, `created_from_command_id`, and provenance fields. Accessors live in `nexus-spoke-adapter::extensions`. This avoids requiring spoke to declare every nexus product field in its core schema. The `nexus-spoke-adapter` exposes a dual-surface API: **Surface A** (pure delegates, frozen) for consumers that manage their own storage, and **Surface B** (injection-orchestration via port traits) for consumers that want spoke to compose the full lifecycle. See [`spoke-adapter-architecture.md`](.mstar/specs/spoke-adapter-architecture.md) for the adoption guide.

### Manuscript
The structured prose output within a world — organized into chapters, scenes, and narrative flow. A world may have multiple manuscripts representing parallel storylines or drafts.

### Timeline
The ordered sequence of events and KnowledgeEntries in a world — the "when" axis of the narrative. Timeline entries are append-only; rewrites create Forks. From V1.123, a Timeline is experienced as **three zoom layers** — [Brief](#brief) (world-global shape), [Narrative](#narrative) (event-level), [Moment](#moment) (scene/beat-precise) — not a single flat event list. World Timeline leads with Brief+Narrative; Work Timeline (peer surface) leads with Narrative+Moment. See [Timeline-first World building](#timeline-first-world-building).

### Brief
Timeline's **world-global layer** — era / age / multi-decade markers and world-shape summary so an author can see the world's history at a glance. Brief is the **hero layer for World Timeline** (World entry defaults to Brief when Brief data exists, else Narrative with an honest empty-state). Time span is large by design; density is minimal (era landmarks, not every event). **Not** a separate container from Timeline, and **not** the same as a free-text World Summary / manifesto alone — Brief is a **when-axis projection**. The data carrier is `block_type=era` KnowledgeEntry, a **cross-profile world-shape marker** (not a profile-specific category like `novel_category` / `game_bible_category`). Work Brief is out of scope in V1.123 (Outline covers Work structure; tracker `DF-V1123-WORK-BRIEF`). Cross-ref: [Timeline](#timeline), [Timeline-first World building](#timeline-first-world-building), [Narrative](#narrative).

### Narrative
Timeline's **event-level layer** — human-paced events in order (days/weeks/years): battles, treaties, journeys. Shared by **World Timeline** and **Work Timeline**. This is the V1.122 Timeline surface reframed as one of three layers (balanced event axis + relationship edges + Context clusters). **Disambiguation:** not "narrative writing" (prose craft) and not a synonym for Manuscript — here *Narrative* means the **event-granularity Timeline zoom layer**. Cross-ref: [Timeline](#timeline), [Brief](#brief), [Moment](#moment), [KnowledgeEntry](#knowledgeentry).

### Moment
Timeline's **scene/beat-precise layer** — sub-scene time (minutes/hours within a scene), manuscript-anchored, so an author can scrutinize what happens in an exact scene. Moment is the **hero layer for Work Timeline** (peer `CanvasSurfaceKind = "work-timeline"`; Work **entry** stays Outline). World Timeline does not ship a Moment layer in V1.123 (`DF-V1123-WORLD-MOMENT`). **Disambiguation:** distinct from [Moment Context Assembly](#moment-context-assembly) (the session process that packs KnowledgeEntries + memory for an agent task). Scope hierarchy already includes Moment under `World > Timeline > Event > Moment` ([entity-scope-model.md](.mstar/specs/entity-scope-model.md)); V1.123 adds Canvas projection as a Timeline layer. Cross-ref: [Timeline](#timeline), [Narrative](#narrative), [Outline](#outline), [Manuscript](#manuscript).

### Fork
The only mechanism for changing world history. Creates a divergent branch from a point in the timeline. Original history is preserved. Forks are the structural equivalent of version control branches for narrative.

### Scope
A named selection of KnowledgeEntries for context assembly — defines which knowledge is visible during a specific creative moment (e.g., "current chapter scope", "scene scope").

### Narrative Profile
A world's structural type that determines which narrative tools and capabilities are available. Examples: `novel`, `essay`, `game-bible`.

### Outline
A structured, non-linear representation of a work's planned content — nodes representing chapters, scenes, beats, arcs, arranged on the infinite canvas. Outlines are editable and drive manuscript generation.

---

## Compute & AI Domain

### Compute (Capability)
A WASM-powered execution unit within a world — the *mechanism* an author or agent invokes. Examples: combat engine resolution, dice rolling, relationship graph computation. Compute modules are embedded (shipped with the binary) or user-authored. **Distinct from the [Computable](#computable) pillar** in *Three Pillars* above: this entry is the capability mechanism; *Computable* is the product thesis that worlds react via such capabilities.

### Run
One execution of a compute module against a World — the direct-lane (Control Room) product concept over the `compute_sessions` row. A Run moves `running → succeeded | failed`; a succeeded Run stays **Needs review** until the author explicitly **Accept**s (→ Applied) or **Discard**s (→ Discarded). The direct lane **never auto-applies** (review-then-apply); the preset `narrative.compute` path may auto-apply inside a Harness session. Run history is retained, and terminal Runs can be cleared per World (Clear history — never `running`/needs-review rows). Author-facing vocabulary (Run / Proposal / Accept / Discard): [computable-author-behavior.md](.mstar/iterations/v1.147/specs/computable-author-behavior.md). Cross-ref: [Compute (Capability)](#compute-capability), [Compute result](#compute-result), [Computable](#computable). Wire surface: [daemon-api-surface-conventions.md](.mstar/specs/daemon-api-surface-conventions.md) §12.3.

### Compute result
A Timeline node created when an author **accepts** a Run's proposals — `event_type: "compute_result"`, appended **canon** (never provisional) with `extensions.nexus.compute` provenance (module id/version, run id, `source_kind: "direct_invoke"`). Preset-path compute events share the same event family so accepted reactions speak one visual language; failed/discarded Runs never produce Timeline nodes (the Timeline stays narrative truth, not an ops log). Cross-ref: [Run](#run), [Timeline](#timeline), [Computable](#computable).

### Preset
A pre-configured bundle of compute capabilities with a YAML manifest. Presets define which capabilities are available, how they sequence, and what prompts/rules they use. Example: "combat-engine" preset. **Disambiguation:** distinct from the [Preset (Injection) Slot](#preset-injection-slot) under Creative Writing Domain (a named ordered region of the assembled prompt) — this entry is the capability bundle, not an assembly slot.

### System Preset (`_system.*`)
A built-in preset shipped with the app under `presets/_system/<name>/`, addressed by a qualified id with the `_system.` prefix (e.g. `_system.maintenance`). The on-disk directory is the **stripped** name — resolving an id to a path must strip the prefix first (see `.mstar/knowledge/conventions/system-preset-qualified-id-resolution.md`). System presets are read-only and hidden from author-facing management surfaces (e.g. Sessions list, preset Delete).

### Creator
The local user's identity aggregate — author profile, preferences, memories. A creator has one or more works and is the "self" that agents interact with.

### Creator Memory
The creator's persistent memory pipeline — a structured I/O system ("SOUL") that stores and retrieves personal context across sessions. This is *not* World KB; it's the author's own memory (writing preferences, character voice notes, etc.).

### Moment Context Assembly
The process of assembling the right set of KnowledgeEntries, timeline state, and creator memory for a given creative moment. Produces a "moment context" that an agent sees when performing a task (e.g., "write next chapter"). **Not** the Timeline [Moment](#moment) layer (scene/beat Canvas projection on Work Timeline) — this entry is the **session context-assembly** concept.

### Quality Loop
The iterative process: write → reflect → generate findings → human review → apply changes. Separates automated quality analysis from human decision-making.

### Knowledge Loop
The process: persist new text → extract structured knowledge → promote to World KB. Runs on a schedule, not inline with writing.

---

## Brand & Design

### Brand DESIGN SSOT
Repo-root `DESIGN.md` and `DESIGN.dark.md` — the cross-application source of truth for Nexus brand tokens, VI palette, logo usage rules, and accessibility intent. All app surfaces derive from this layer; they must not redefine shared brand values.

### Reading Chrome
The profile-specific, read-only typographic treatment applied to the manuscript reading surface in `apps/web`. Driven by `DESIGN.md` `reading-chrome-*` tokens and the Work's `work_profile` (`novel`, `essay`, `game-bible`, `script`). Reading Chrome is strictly presentational: it never mutates `body_path`, outline, or timeline state.

### Profile Switcher
The sidebar footer UI component that lists Creator avatar icons and switches the active `creator_id` for the SPA's data queries. The daemon already supports multi-creator at the API level; the profile switcher is the missing UI. Single-creator case: exactly one avatar plus a "+" affordance to add a new Creator.

### @42ch/nexus-ui
Publishable npm workspace package (`packages/nexus-ui`) exporting brand assets (SVG logos), token data, and CSS theme entry points. PNG logo sources are Git LFS–tracked for provenance; canonical SVGs are regular-git text. V1.83 ships assets/tokens/theme only — React component library deferred.

---

## Protocol & Infrastructure

### ACP (Agent Communication Protocol)
The standard protocol for agent-to-agent communication. Nexus is an **ACP client** (not an ACP agent/server). It sends requests to the user's local agents and receives structured responses.

### Agent Host
The adapter layer that translates between Nexus's internal capability model and external ACP agents. Allows Nexus to ask any ACP-compliant agent to perform tasks without being tied to a specific provider.

### Daemon Runtime
The local background process within `nexus42` that manages the World KB SQLite database, schedules quality/knowledge loops, serves the **Daemon API** HTTP surface (Axum), and coordinates with the agent host. Starts with `nexus42 daemon start`. The surface was historically called "Daemon API" before V1.90.

### Daemon API
The HTTP surface served by the Daemon Runtime, reachable under `/v1/daemon/*` (previously `/v1/local/*`). It exposes world/knowledge, creator, orchestration, and manuscript endpoints to the CLI, web SPA, and desktop shell. By default it binds to loopback; remote bind requires both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1`.

### Connect Host
The opt-in spoke-connect surface for **third-party narrative reasoning** (PD-09 / FL-R / DF-72 N-C0, V1.148): a separate OS process started by `nexus42 connect start` (Cargo feature `connect-host`, default off) that peers over spoke-connect with integrator runtimes and advertises an honest `HostCapabilityManifest` (installation `host_id` = `~/.nexus42/device-id`; roles `["data-store"]`; no `"reasoning-complete"` until N-C2). It is the third consumption surface **alongside Daemon HTTP** — Adapter-full, not Product-full: it never exposes Harness UI, ACP-as-server, or Canvas, and N-C0 refuses every inbound op (`op_unsupported`). Capability-token / world scoping must exist before multi-tenant exposure. See [Daemon API](#daemon-api) (creator UI SSOT) and the FL-R roadmap in [deferred-features-cross-version-tracker.md](.mstar/knowledge/deferred-features-cross-version-tracker.md). Spec: [spoke-adapter-architecture.md](.mstar/specs/spoke-adapter-architecture.md) §10.

### Setup Wizard
The first-launch 4-step flow (welcome + workspace → daemon ready → ACP agent detection → done) gated by a `setup_completed` marker in `~/.nexus42/config.toml`. Triggers again if the marker is cleared. Every app launch (not only first) verifies the daemon is running before entering the main UI.

### ACP Agent Detection
The combined registry-list + PATH-scan operation the Daemon API exposes at `POST /v1/daemon/agent-host/scan`, returning candidate agents annotated with local-install availability. The scan probes only registry-known binary names, with bounded concurrency and short `--version` timeouts (no user-supplied commands are executed during scan).

### Local Database
SQLite-based (via sqlx) persistent storage. Contains World KB tables, creator profiles, timeline data, and orchestration state. Single database per home directory.

### JSON Schema (Wire Contracts)
The single source of truth for all cross-language types. `schemas/` directory defines the JSON Schema, and codegen produces Rust types (`crates/nexus-contracts/`) and TypeScript types (`@42ch/nexus-contracts` npm package).

### Workspace (Canvas)
The infinite canvas surface that visually organizes creative material — worlds, manuscripts, outlines, KnowledgeEntries, and relationships — into a navigable spatial layout.

### Web UI
The local-first "Control Room + Setup" web interface (`apps/web`). A React SPA served by the daemon over HTTP (`127.0.0.1:8420`), providing the infinite canvas, workspace management, and structured writing tools. Reuses the `@42ch/nexus-contracts` TypeScript types — never hand-writes wire DTOs.

On Creator hub routes (`/works`, `/worlds`), the **content region** flips between two modes (V1.128): **Create page** (no World/Work selected — card CTAs to create) and **Controller Panel stub** (entity selected — placeholder + **Back** that clears selection). Mode SSOT is `CreatorEntitySelectionContext`, orthogonal to canvas routes under `/works/:workId/*`. See [creator-shell-content-mode-pattern.md](.mstar/knowledge/architecture-patterns/creator-shell-content-mode-pattern.md).

### Desktop Shell
The Tauri v2 native desktop client (`apps/desktop`). Wraps the web SPA (`apps/web/dist`) in a native window, adds OS-level capabilities (Open with…, Reveal in Finder, Copy Path, sidecar lifecycle management). Detects the Tauri runtime at startup and selects `TauriClient` over `BrowserClient` via capability detection.

---

## Cross-Reference

Paths are relative to the repo root. Each entry links the term to its authoritative spec doc under `.mstar/specs/`.

### Three Pillars

| Term | Related concepts | Spec doc |
|------|-----------------|----------|
| Harness | Orchestration Engine, Agent Host, Capability Registry, Preset | [orchestration-engine.md](.mstar/specs/orchestration-engine.md) |
| Computable (pillar) | Compute (Capability), WASM host, compute-module-abi | [compute-module-abi.md](.mstar/specs/compute-module-abi.md) |
| Timeline-first World building | World, Timeline, Brief, Narrative, Moment, Outline, Workspace (Canvas), Fork | [canvas-strategy-surface.md](.mstar/specs/canvas-strategy-surface.md) |

### Creative Writing Domain

| Term | Related concepts | Spec doc |
|------|-----------------|----------|
| World | Fork, Timeline, Manuscript, Scope | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| KnowledgeEntry | SourceAnchor, Taxonomy, Computable | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| SourceAnchor | KnowledgeEntry, Provenance | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| Manuscript | World, Timeline, Chapter, Moment (layer) | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| Timeline | World, KnowledgeEntry, Fork, Brief, Narrative, Moment | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| Brief | Timeline, Narrative, Timeline-first World building | [entity-scope-model.md](.mstar/specs/entity-scope-model.md); [three-layer-product-spec.md](.mstar/iterations/v1.123/specs/three-layer-product-spec.md) |
| Narrative (Timeline layer) | Timeline, Brief, Moment, KnowledgeEntry | [entity-scope-model.md](.mstar/specs/entity-scope-model.md); [three-layer-product-spec.md](.mstar/iterations/v1.123/specs/three-layer-product-spec.md) |
| Moment (Timeline layer) | Timeline, Narrative, Outline, Manuscript | [entity-scope-model.md](.mstar/specs/entity-scope-model.md); [three-layer-product-spec.md](.mstar/iterations/v1.123/specs/three-layer-product-spec.md) |
| Fork | World, Timeline | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| Scope | KnowledgeEntry, Moment Context Assembly | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| Narrative Profile | Novel, Essay, Game-Bible, Script | [novel-writing/workflow-profile.md](.mstar/specs/novel-writing/workflow-profile.md) |
| Outline | Workspace, Canvas, Manuscript, Moment (layer) | [canvas-strategy-surface.md](.mstar/specs/canvas-strategy-surface.md) |

### Compute & AI Domain

| Term | Related concepts | Spec doc |
|------|-----------------|----------|
| Compute | Preset, WASM module, Capability Registry | [compute-module-abi.md](.mstar/specs/compute-module-abi.md) |
| Run | Compute, Module, Accept, Discard, Compute result | [daemon-api-surface-conventions.md](.mstar/specs/daemon-api-surface-conventions.md) |
| Compute result | Run, Timeline, Accept, Computable | [entity-scope-model.md](.mstar/specs/entity-scope-model.md) |
| Preset | Compute, Orchestration, Capability | [orchestration-engine.md](.mstar/specs/orchestration-engine.md) |
| Creator | Creator Memory, Works | [creator-workflow.md](.mstar/specs/creator-workflow.md) |
| Creator Memory | Creator, SOUL I/O | [creator-workflow.md](.mstar/specs/creator-workflow.md) |
| Moment Context Assembly | Scope, KnowledgeEntry, Creator Memory (≠ Timeline Moment layer) | [local-runtime-boundary.md](.mstar/specs/local-runtime-boundary.md) |
| Quality Loop | Findings, Review, Knowledge Loop | [novel-writing/quality-loop.md](.mstar/specs/novel-writing/quality-loop.md) |
| Knowledge Loop | KnowledgeEntry, SourceAnchor, Quality Loop | [novel-writing/quality-loop.md](.mstar/specs/novel-writing/quality-loop.md) |

### Protocol & Infrastructure

| Term | Related concepts | Spec doc |
|------|-----------------|----------|
| ACP | Agent Host, Daemon Runtime | [acp-client-tech-spec.md](.mstar/specs/acp-client-tech-spec.md) |
| Agent Host | ACP, Capability, Daemon Runtime | [agent-host.md](.mstar/specs/agent-host.md) |
| Daemon Runtime | Local Database, Agent Host, Daemon API | [daemon-runtime.md](.mstar/specs/daemon-runtime.md) |
| Daemon API | Daemon Runtime, Web UI, CLI, JSON Schema | [daemon-api-surface-conventions.md](.mstar/specs/daemon-api-surface-conventions.md) |
| Connect Host | Daemon API, ACP, FL-R / DF-72 | [spoke-adapter-architecture.md](.mstar/specs/spoke-adapter-architecture.md) |
| Local Database | SQLite, World KB, Orchestration state | [local-db-schema.md](.mstar/specs/local-db-schema.md) |
| JSON Schema (Wire Contracts) | schemas/, codegen, nexus-contracts | [schemas-directory-layout.md](.mstar/specs/schemas-directory-layout.md) |
| Workspace (Canvas) | Canvas, Outline, Manuscript | [canvas-strategy-surface.md](.mstar/specs/canvas-strategy-surface.md) |
| Web UI | Desktop Shell, Daemon Runtime, NexusClient | [web-ui.md](.mstar/specs/web-ui.md) |
| Desktop Shell | Web UI, Sidecar, Tauri IPC | [desktop-shell.md](.mstar/specs/desktop-shell.md) |
| Setup Wizard | Desktop Shell, Daemon Runtime, ACP Agent Detection | [desktop-shell.md](.mstar/specs/desktop-shell.md) |
| ACP Agent Detection | Desktop Shell, Daemon API, ACP | [desktop-shell.md](.mstar/specs/desktop-shell.md) |
| Profile Switcher | Web UI, Creator | [web-ui.md](.mstar/specs/web-ui.md) |
