# Strategy

## Vision

Nexus is a **local-first, AI-agent-driven creative writing tool** that uses an infinite canvas to organize ideas and orchestrate the writing workflow — putting authors in full control of their craft and data.

## What we build

Three product surfaces, all open-source, targeting creative writers — novelists, worldbuilders, essayists:

| Surface | Tech | Role | What it does |
|---------|------|------|-------------|
| **`nexus42`** | Rust (CLI + daemon) | **Producer** | CLI commands, daemon lifecycle, local HTTP API, orchestration, World KB management |
| **`web`** | TypeScript (React SPA) | **Consumer** | "Control Room + Setup" UI — served by the daemon, provides the infinite canvas and structured writing interface |
| **`desktop`** | TypeScript + Tauri v2 (Rust) | **Consumer** | Native desktop shell — wraps the web SPA, adds OS-level capabilities (file open, reveal in Finder, sidecar lifecycle) |

Shared needs they serve:

- **Local-first privacy** — data stays on their machine by default; cloud sync is optional
- **AI agent orchestration** — leverage the user's own local agents (via ACP or native) to assist in writing, worldbuilding, and narrative structuring, without forcing extra tooling burden
- **Infinite canvas** — visual, non-linear organization of creative material (worlds, outlines, manuscripts, key blocks)
- **Structured narrative tools** — timelines, forks, manuscripts, world knowledge bases — beyond what a plain text editor provides

## What we don't build

- **A cloud platform** — this repo is the open-source CLI/daemon only (the cloud/web platform lives in the private `nexus-platform` repo)
- **A general-purpose note-taking app** — focus is creative writing, not generic notes
- **A competing IDE or editor** — we integrate with the user's existing tools and agents, not replace them

## Guiding Principles

1. **Local-first by default.** Cloud sync is an opt-in feature, never a requirement. The tool works fully offline.
2. **Wire contracts are truth.** JSON Schema is the single source of truth for all cross-language types. No parallel DTO sets.
3. **Simplicity over premature abstraction.** Don't abstract before there are three concrete use cases. Don't add features until the pattern is proven.
4. **Leverage, don't burden.** Directly use the user's local existing Agent infrastructure (ACP or native) — do not introduce extra agents, runtimes, or accounts the user didn't ask for.

## Technology Direction

| Choice | Rationale |
|--------|-----------|
| **Rust** for CLI + daemon | Performance, memory safety, strong ecosystem for local-first tools (sqlx, tokio, wasmtime) |
| **ACP** for agent interoperability | Standard protocol over bespoke — aligns with industry direction; CLI is an ACP client, not a server |
| **JSON Schema → codegen** | Cross-language contracts from a single source — generates TypeScript (npm) and Rust types |
| **SQLite** (via sqlx) | Local-first persistence — simple, portable, zero-ops |
| **Native WASM host** (via wasmtime) | Embeddable compute without browser dependency |
| **Axum** for local HTTP API | Modern, type-safe Rust web framework for the local API surface |
| **React SPA** (`apps/web`) | Local-first Control Room + Setup UI — served by the daemon, Tauri-ready |
| **Tauri v2** (`apps/desktop`) | Cross-platform desktop shell wrapping the web SPA with native OS capabilities |
| **Pre-1.0** | Breaking changes expected and allowed; no deprecation period |

## Decision Log

| Decision | Context | Date |
|----------|---------|------|
| Monorepo with `apps/` + `crates/` + `schemas/` | Product surfaces, reusable Rust libs, and wire contracts each have their own lifecycle | Early 2025 |
| `nexus42` binary hosts both CLI and daemon | Single binary for simplicity (no separate `nexus42d`) | V1.68 (June 2026) |
| Web SPA (`apps/web`) as daemon-served Control Room UI | React SPA, served by the daemon over localhost HTTP; reuses generated contract types | V1.64 (June 2026) |
| Desktop shell (`apps/desktop`) via Tauri v2 | Wraps the web SPA for native OS capabilities (file open, reveal in Finder, sidecar lifecycle); no second DTO set | V1.66 (June 2026) |
| JSON Schema as wire truth source | Enables Rust + TypeScript codegen; avoids hand-written DTO drift | V1.0 planning |
| Integration branches via PR only | All merges to `main` go through GitHub PR for review trail and CI gate | V1.39 (June 2026) |
| `.mstar/` harness conventions | Morning Star harness for plan/QC/QA discipline | V1.39 (June 2026) |
| Canvas program complete (V1.67–V1.76) | 4 β/γ surfaces shipped (Strategy, Outline+Timeline, World KB, Relationships) on React Flow with a structured write-boundary (no raw-file edits from the webview); V1.75 retired the V1.65 whole-document editor in favor of node-granular canvas edits | V1.76 (June 2026) |
| Findings-remediation UI closes the quality loop (V1.77) | Control-Room findings view promoted from read-only (V1.64) to a full remediation authoring surface (6-state status transitions + target_executor assignment + inline edit), consuming the PATCH route the backend already ships; last-writer-wins (single-author triage, no OCC) | V1.77 (June 2026) |
| Creator Memory / SOUL review-loop UI closes the author self-loop (V1.78) | Third and final author-in-command loop (capture → review → internalize), parallel to the canvas writing loop (V1.76) and the findings quality loop (V1.77). Published the memory OSS contracts (14 schemas + codegen + normalized hand-written handler DTOs, fixing the daemon-runtime no-hand-written-DTO invariant; `@42ch/nexus-contracts` 0.12.0 → 0.13.0) + Memory review-loop Control-Room surface. Compound captured the reusable contracts-gap-on-shipped-backend pattern | V1.78 (July 2026) |
| Author Reflection — first post-loop-closure iteration (V1.79) | After all three author-in-command loops closed (V1.76–V1.78), V1.79 deepens the author's ability to *reflect on* what the loops produce rather than opening a new loop: manuscript reading surface (Track A, read-only, no write routes, body-ownership invariant preserved) + SOUL personality visualization (Track B, keyword clusters + temporal drift). Additive wire DTO (`memory-fragment-info` + `keywords`/`created_at`; `@42ch/nexus-contracts` 0.13.0 → 0.14.0). **DF-49 (Standalone MCP server) cancelled** — conflicts with the ACP-client product direction (CLI is an ACP client, not a server) + circular-invocation risk. Compound captured the cursor-pagination-without-total count-label pattern | V1.79 (July 2026) |
| Reliability investment iteration (V1.80) | First "stabilize before extending" iteration after four consecutive feature iterations (V1.76–V1.79). Closed REL-01 (`R-V178P0-QC3-003`): rewrote `POST /v1/local/memory/review` from an unbounded synchronous whole-queue loop into a bounded (`REVIEW_BATCH_LIMIT=50`) / per-creator serialized (in-process mutex map on `WorkspaceState`) / deadline-aware (5s partial-progress) synchronous pipeline — deliberately keeping the synchronous request/response shape (no async job infrastructure) as proportional to the local-only / single-active-creator / small-queue threat model. Additive wire DTO (`ReviewResponse` + `has_more`/`processed`; `@42ch/nexus-contracts` 0.14.0 → 0.15.0). Also closed four low V1.79-QC frontend hygiene residuals. Compound captured the bounded-drain-completion-contract pattern (`has_more` must reflect queue advancement, not rows attempted) | V1.80 (July 2026) |
| Creator SOUL Maturation (V1.81) | Returned to the feature cadence after V1.80 stabilization, deepening the V1.79 reflection axis. Headline = Creator-SOUL Narrative — LLM on-demand synthesis of the creator's accumulated fragments into a reflective "who you are becoming" narrative, world-agnostic, persisted with stale-invalidation (no background jobs; consistent with V1.80's synchronous/proportional discipline). Shipped the user-locked "Creator SOUL (whole) vs World projection (subset)" product model: `memory_fragments.world_id` (additive) threads world provenance through the review-promotion seam, enabling a UI world-selector that drills the keyword/drift/growth viz into a world subset (per-World LLM narratives deferred). Plus independent growth-curve (BL-10) + auto-refresh. Synthesis reuses the `SessionDigestSummarizer` trait-seam pattern (`SoulNarrativeSynthesizer` + `AcpSoulNarrativeSynthesizer` via `CapabilityRegistry` → `acp.prompt`). Additive wire (`memory-fragment-info` + `world_id`; new soul-narrative request/response schemas; `@42ch/nexus-contracts` 0.15.0 → 0.16.0). QC tri-review 3/3 Approve after a 3-round fix-wave that surfaced the read-path-cost vs sound-gate tension; compound captured the fingerprint-cached-live-aggregate pattern | V1.81 (July 2026) |
