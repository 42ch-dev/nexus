# Canvas Strategy Surface — Specification

| Attribute | Value |
| --- | --- |
| **Status** | **Exploration (V1.67)** — design-only; no implement authority until a future compass promotes Draft→implement (target V1.68+) |
| **Document class** | Exploration |
| **Scope** | Product vision + architecture for the human-facing **Canvas** control surfaces: Strategy (Preset) orchestration graph, Work outline + timeline graph, World KB graph; React Flow rendering; the "AI owns prose, human steers via Canvas" thesis; the no-raw-file-editing write boundary |
| **Coordinates with** | [orchestration-engine.md](orchestration-engine.md) (strategy = graph-of-graphs), [web-ui.md](web-ui.md) (§15 V1.67 stage + V1.68 canvas roadmap), [local-api-surface-conventions.md](local-api-surface-conventions.md), [chapter-content-local-api.md](chapter-content-local-api.md), [daemon-runtime.md](daemon-runtime.md) |
| **Supersedes** | [body-editor.md](body-editor.md) (archived: [../../archived/knowledge/body-editor.md](../../archived/knowledge/body-editor.md)) |
| **Authored** | V1.67 Phase 2b re-discussion — **@architect** (architecture + React Flow feasibility + DAG↔canvas mapping + write boundary) + **@product-manager** (product thesis + canvas UX + Strategy terminology); PM-scaffolded stub pending authoring |

> **STUB.** This Exploration spec was scaffolded by PM during the 2026-06-26 V1.67 re-discussion. The full body (sections below) is to be authored by `@architect` + `@product-manager`. The product thesis and architectural principle are locked from the user's re-discussion; the technical design (React Flow feasibility, node/edge model, write boundary, surface boundaries) is the authoring work.

## 1. Product thesis (LOCKED from user re-discussion, 2026-06-26)

Nexus is an **AI-autonomous creative executor** (in the spirit of Codex / a design tool): the human **inputs an Idea** and **steers** the work; the **AI owns the prose writing and execution**. Nexus is **not** a manual editor where the human writes chapter bodies by hand.

The human steers through three **Canvas (infinite-canvas) surfaces**, not document editors:

1. **Strategy (Preset) orchestration canvas** — visualize and edit the preset/strategy that drives the creative workflow. Conceptual rename: **"Preset" → "策略 (Strategy)"** — it is the workflow that drives the creative work (this is already the orchestration engine's mental model: a strategy is a hierarchical state-machine of inner DAGs — graph-of-graphs; `orchestration-engine.md` §3).
2. **Work outline + timeline canvas** — compile and steer the Work's outline and timeline as a graph, not a linear rich-text document.
3. **World KB canvas** — browse and steer the World Knowledge Base (entities, events, rules, relationships) as a graph.

**Renderer**: [React Flow](https://reactflow.dev/learn) (`@xyflow/react`) — chosen because a Strategy **is** already a graph/DAG at runtime (states + edges + converge merge points), so React Flow's node/edge model is a natural projection, not a forced fit.

## 2. Core architectural principle (LOCKED)

> **Visualization products must not edit raw files directly.** All edits are structured / node-granular operations through the canvas, to avoid accidentally corrupting file structure. Rich-text (TipTap) survives as an **in-node** editing capability (editing the content of a single canvas node), **not** as a whole-document editor.

Implications:
- The shipped V1.65 whole-document outline rich-text editor (TipTap over `outline_path`) is itself a **canvas-pivot candidate** (V1.68+ target, recorded here; **not** changed in V1.67 — no regression to shipped surface).
- The Local API write boundary for canvas surfaces is **structured/node-granular**, not whole-file PUT. (V1.68 design; this Exploration records the principle.)

## 3. Architecture + feasibility (TO BE AUTHORED by @architect)

*Authoring targets:*
- React Flow feasibility in the Tauri v2 webview (macOS WKWebView) + the browser-tab flow; bundle/perf considerations.
- Strategy-DAG ↔ React Flow node/edge mapping: state-machine states → nodes; transitions/edges → edges; converge merge points → join nodes; inner DAG (per-state prompt/tool chain) → nested/sub-flow representation. Reuse the runtime strategy model in `orchestration-engine.md` §3 as the data source.
- The three canvas surfaces: shared canvas shell vs. per-surface node types; data binding to the Local API.
- Write boundary: structured/node-granular edit operations + how they flush to preset YAML / outline / KB without raw-file mutation; relationship to the existing atomic-write + path-guard patterns.
- "AI owns prose" execution model: how a canvas steer (e.g., node edit, edge rewiring, Idea input) triggers orchestration execution; the boundary between human-steer and AI-execute.
- Relationship to the V1.64–V1.67 Local API surface (the canvas is a heavy consumer — the convergence work in V1.67 P0 is the foundation).

## 4. Product / UX (TO BE AUTHORED by @product-manager)

*Authoring targets:*
- Canvas UX per surface (Strategy / outline+timeline / World KB); node types; the Idea-input affordance.
- "策略 / Strategy" terminology adoption (UI labels + this spec; CLI rename deferred — breaking).
- User stories for the steering loop (input Idea → steer canvas → AI executes → review on canvas).
- Accessibility of a graph/canvas surface (non-trivial — keyboard navigation of nodes/edges; this is a known React Flow concern to address in the Draft).

## 5. Non-goals (V1.67)

- No canvas **implement** in V1.67 (Exploration only). V1.67 ships the hygiene foundation (Local API convergence) that the canvas will consume.
- No removal/regression of the shipped V1.65 outline editor (canvas-pivot is V1.68+).
- No CLI/spec rename of `preset` → `strategy` (breaking; deferred). V1.67 adopts the terminology in UI/spec wording only.

## 6. Roadmap (durable tracking)

- **V1.68 lead candidate**: Canvas Strategy Surface implement (Promote this Exploration → Draft → Shipped). May split across iterations (3 surfaces is XL).
- Successor to the retired body-editor roadmap.
