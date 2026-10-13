# Strategy

## Vision

**Nexus is a local-first narrative-orchestration platform.** Developers get an orchestratable creator-agent service — Harness (preset engine + agent host + capability registry + skills) plus Computable modules they can author and ship. The first-party authoring app is the **reference creator surface** for non-dev authors: Canvas + Timeline remain how humans steer worlds, not a separate product family.

### Three pillars

| Pillar | Means (developer + author) | Maps to in the codebase |
|--------|-------------------------|-------------------------|
| **Harness** | How developers *and* authors harness AI agents: presets, orchestration, capability routing, skills. Developers configure and author this service; authors consume named modes without operator chrome. | `crates/nexus-orchestration/` + `crates/nexus-agent-host/` + `crates/nexus-acp-host/` + capability registry + presets. Surfaced in UI as **Harness**; **Preset stays** as the mechanism name under Harness (product lock PD-4; durable authority: [web-ui.md](.mstar/specs/active/surfaces/web-ui.md) §29.4). Internal identifiers (route `/strategies`, `CanvasSurfaceKind = 'strategy'`, `preset` wire fields, CSS classes, hook names) stay unchanged. Specs: [orchestration-engine.md](.mstar/specs/active/orchestration/orchestration-engine.md), [agent-host.md](.mstar/specs/active/agents/agent-host.md), [capability-registry.md](.mstar/specs/active/agents/capability-registry.md). |
| **Canvas** | Spatial steering for the reference creator surface — **Timeline-centric World building** as the hero author entry. Timeline is **three layers** — **Brief** / **Narrative** / **Moment** — **World Timeline** = Brief+Narrative (Brief-led); **Work Timeline** = Narrative+Moment (Moment-led peer; Work entry stays Outline). Strategy canvas is the developer explainability surface for preset orchestration. | `apps/web` React Flow canvas surfaces (`CanvasSurfaceKind = "strategy" \| "outline" \| "world-kb-entities" \| "world-kb-relationships" \| "timeline" \| "work-timeline"`). Timeline is the default **World-entry** surface (default layer Brief when data exists); Work entry stays Outline; Work Timeline is a peer surface. Spec: [canvas-strategy-surface.md](.mstar/specs/active/surfaces/canvas-strategy-surface.md). |
| **Computable** | The WASM layer that makes worlds *react* — combat, dice, graph computation, integrator modules. Developers author modules via the official SDK; authors run installed modules (review-then-apply). | Native WASM host (`wasmtime`) + compute-module ABI + combat-engine preset (shipped). Specs: [compute-module-abi.md](.mstar/specs/active/compute/compute-module-abi.md), [wasm-host.md](.mstar/specs/active/compute/wasm-host.md). |

**Spine vs projection** (locked product model — both audiences): **World + Timeline + KnowledgeEntry + Fork** are the *spine* (truth of the narrative universe the service stores and Connect serves); **Work + Outline + Manuscript** are *projections* (the authoring plan and prose bound to a World). Timeline is the World's *when* axis at three scales (Brief · Narrative · Moment); Outline is the Work's structural projection; Work Timeline is the Work's when-axis peer (Narrative · Moment). Under the Actor model, KnowledgeEntry stays **one spine primitive with exactly one canonical owner scope** — World, Character, or ActorWorldBinding. Character and ActorWorldBinding owner scopes ship alongside World-owned KE; World remains the pre-Actor default ([CONCEPTS.md](CONCEPTS.md) § *KnowledgeEntry*). Dual authoring defaults: **World first for World building (Timeline, Brief-led); Work first for chapter writing (Outline), with Work Timeline for scene precision.** See [CONCEPTS.md](CONCEPTS.md) § *Three Pillars* and § *Brief* / *Narrative* / *Moment*.

### Actors — who can think and act

**Actor** is the cross-cutting narrative identity primitive (accepted product direction): *who can think and act* in a story. It cuts across the three pillars rather than adding a fourth — Harness executes an Actor, Canvas surfaces one, Computable modules react to one. `ActorRef` is a closed v1 sum with exactly two kinds:

- **Creator** — *conducts* the story: the god/orchestrator/narrative driver and operational owner. Carries SOUL + Memory and holds omniscient read over all KnowledgeEntries in every World it owns (including creator-only facts). This is the shipped `creator_id` identity aggregate; its storage and execution remain byte-stable.
- **Character** — *lives* the story: a durable, Creator-owned identity with its own SOUL, Memory, ToM (built additively on the l5 MindState/belief/observation foundation), and image/persona assets. A Character enters **one or more** Worlds only through explicit `ActorWorldBinding`s — never implicitly, and no World KB is ever copied per Character. Character creation establishes an initial binding atomically; an active Character never has zero active bindings, so removal of its last active binding rejects with zero mutation; transitioning a Character out of active state is a separate later lifecycle contract (explicit archive/restore), never an implicit effect of binding removal.

**Viewpoint** is subordinate execution context paired with an `ActorRef` (`{world_id, optional binding_id/branch_id/event_id}` — it does not repeat an actor id; Character execution requires the binding id, Creator omits it), not identity. One Agent Host / runtime / provider plane serves both kinds; Character sessions execute under the owning Creator's admission boundary with isolated conversation histories. No consumption end changes: still no first-party player (PD-09), no second runtime, no companion app. Outward line: **Nexus Actors are who can think and act — Creators conduct the story; Characters live it.** Durable authority: [actor-product-model.md](.mstar/specs/active/architecture/actor-product-model.md) (accepted product direction). Vocabulary: [CONCEPTS.md](CONCEPTS.md) § *Actors & Narrative Identity*.

## What we build

Three **consumption ends** (no new first-party apps) plus developer-facing assets:

| Consumption end | Surface | Role |
|-----------------|---------|------|
| **Developers** | `nexus42` CLI (direct-core `cli` cohort) + Connect host (`nexus-runtime`) | Primary audience. Full operator/config surface, module/strategy authoring DX, capability registry, Run Studio, Develop hub. |
| **Content creators** | `apps/web` + `apps/desktop-electron` (same SPA, **Create** layout; local HTTP served by the standalone TypeScript service) | Reference creator surface: World/Work, Timeline/Outline/KB, findings, memory, guided Run of *installed* modules. Operator chrome hidden. |
| **Third-party users** | `nexus-runtime` + Connect (PD-09 / FL-R) | End users of *integrator* products. Partner UI lives outside this repo. No first-party player. |

| Asset | Role |
|-------|------|
| **`nexus42`** (Rust CLI, direct-core `cli` cohort) | Producer: authoring/operator/cloud commands calling core, cloud and Connect directly — orchestration engine, World KB; no daemon lifecycle, no CLI-launched HTTP service, no embedded SPA |
| **`nexus-runtime`** (headless Connect host) | Independent `connect-host` binary of the `nexus42` crate; the integrator runtime — Connect-only, never implies the CLI cohort |
| **`nexus-service`** (standalone TypeScript service) | The local HTTP host for browser/Electron: `/v1/daemon/*` over the Rust authority via `@42ch/nexus-native`; not an operator CLI destination |
| **`web`** (React SPA) | Single SPA, two layout trees (`Create` / `Develop`) selected by User-layer entrance — not a second build |
| **`desktop-electron`** (Electron) | Desktop host of the same SPA (bundled `web-dist` resources, app-managed service, file/OS integration); entrance is a setup-wizard step |
| **Official compute-module SDK** | Standalone crates.io-publishable crate + shared manifest helper; scaffold/build/validate/install/run without hand-copied ABI |
| **Strategy / module authoring DX** | CLI groups, validators, embedded skills/demos; strategy canvas as explainability SSOT on the developer layout |
| **Entrance split** | User-layer identity `developer` \| `content-creator`, orthogonal to agent-layer Creator profiles |

Shared: local-first privacy; harness the user's own ACP/native agents; structured narrative spine (timelines, forks, manuscripts, World KB); WASM compute with review-then-apply.

## What we don't build

- **A cloud platform** — this repo is the open-source CLI, runtime and first-party hosts (cloud/web platform lives in private `nexus-platform`)
- **A general-purpose note-taking app** — narrative orchestration + reference authoring, not generic notes
- **A competing IDE or editor** — we integrate with the user's tools and agents, not replace them
- **A new first-party player app for third-party users** (PD-09) — partners ship their UI; we ship runtime + Connect
- **A second runtime, companion Character app, NPC swarm, or per-Character copied World KB** (Actor product lock, 2026-09-04 — [actor-product-model.md](.mstar/specs/active/architecture/actor-product-model.md)) — Characters are an Actor kind executed by the one Agent Host under the owning Creator's admission boundary; not a new process plane or player surface (the PD-09 no-first-party-player stance above is unchanged)
- **A general-purpose MCP server** — nexus serves only a scoped
  **tools-only MCP exposure surface**: nexus serves `tools/list` +
  `tools/call` over the single capability-registry spine (builtin `nexus.*`
  + user capabilities + peer tools) via stdio-first MCP so external ACP /
  native-CLI agents can consume nexus tools capability. Not a
  prompts/resources/sampling surface; the CLI remains an ACP client.
- **Full user-authored capability plugin ABI** (DR-10 remainder) — shell/process executors, arbitrary host imports, DF-03 signing/publish, ABI V2: none of these ship. A **minimal** capability extension (DR-10 minimal: WASM compute module + descriptor as the capability executor over the existing compute sandbox) is unlocked by decision-lock D6; browsing the registry still ≠ a full plugin ABI

## Guiding Principles

1. **Local-first by default.** Cloud sync is opt-in. The tool works fully offline.
2. **Wire contracts are truth.** JSON Schema is the single source of truth for cross-language types. No parallel DTO sets.
3. **Simplicity over premature abstraction.** Don't abstract before three concrete use cases. Don't add features until the pattern is proven.
4. **Leverage, don't burden.** Use the user's existing local Agent infrastructure (ACP or native) — no extra agents, runtimes, or accounts they didn't ask for.
5. **SDK and interfaces first.** Developers consume official SDKs, CLI, and registries; first-party UI is a projection of those contracts, not a second implementation.

## Technology Direction

| Choice | Rationale |
|--------|-----------|
| **Rust** for the `nexus42` CLI cohort + `nexus-runtime` | Performance, memory safety, local-first ecosystem (sqlx, tokio, wasmtime) |
| **ACP** for agent interoperability | Standard protocol; CLI is an ACP client, not a server |
| **JSON Schema → codegen** | Cross-language contracts from one source (TypeScript + Rust) |
| **SQLite** (via sqlx) | Local-first persistence — simple, portable, zero-ops |
| **Native WASM host** (via wasmtime) | Embeddable compute without browser dependency |
| **Standalone module SDK** (crates.io-publishable) | Guest modules cannot depend on the workspace; official crate owns ABI glue, envelopes, sentinels |
| **TypeScript** for the local HTTP host (`apps/nexus-service`) | Serves the retained `/v1/daemon/*` surface for browser/Electron over the Rust authority; Rust keeps domain and execution truth |
| **React SPA** (`apps/web`) | One SPA for the browser and the Electron desktop host; Control Room + Setup + dual layout trees |
| **Entrance split = single SPA, two layouts** | No second web build, no second embed; typed entrance registry + route guards |
| **Electron** (`apps/desktop-electron`) | Desktop host of the same SPA; native OS + persisted entrance |
| **Pre-1.0** | Breaking changes expected; no deprecation period |

## Standing Decisions

These decisions are current and normative. The per-iteration decision history they replaced is frozen verbatim in [.mstar/specs/archived/architecture/2026-10-13-strategy-decision-log-history-historical-notes.md](.mstar/specs/archived/architecture/2026-10-13-strategy-decision-log-history-historical-notes.md).

**Releases are a standalone process, fully decoupled from development.** No release action — version-bump PR, tag, producer runs, GitHub Release publish — executes inside an iteration, plan, task, or any development phase / post-phase step; iteration Done never depends on a release, and release execution is triggered independently after the shipped work merges. A **mandatory pre-trigger operator gate** must pass before merge/trigger (canary producer runs on the release branch, packaged launch PASS for every architecture, receipts `dirty:false`, family checksum checks, and a local aggregate canary built by the frozen generator); the publish job's count/name guards run automatically before the draft-clearing PATCH, post-publication checks are additional, and a manual PATCH-hold remains an explicit ops-owned prerequisite. Rule: `AGENTS.md` → Release Policy.

**Release version suffix policy.** `alpha` (explicitly unstable daily builds) and `rc` (release-candidate builds for testing) suffixes are supported: the pipeline accepts, bumps, and validates `X.Y.Z-alpha.N` / `X.Y.Z-rc.N`; a suffixed version is **always** published as a GitHub prerelease regardless of the dispatch toggle; tags carry the full suffixed version (`v0.2.0-rc.1`); the changelog base is re-derived with the semver comparator rather than git's version sort. Contract: `docs/release.md`.

**Release baseline.** The first governed release is `0.1.0`. The workspace version surfaces hold the `0.0.0` never-released baseline until a release moves them. The version-surface set includes the five native npm manifests (loader package plus four platform packages) that the loader's compatibility fence and the native packaging assertions require to move with the release version. The first release is a forced prerelease (signing unimplemented).

**Release governance.** New-release-driven semver is decoupled from iteration numbers, with a forced-prerelease unsigned channel: `new-release` (dispatch → validated version-bump PR, GitHub-signed commit, `release` label, conventional-commits CHANGELOG) → merge → annotated `v<version>` tag at the merge commit → `workflow_call` producers build three-platform `nexus-runtime` zips + unsigned Electron (darwin arm64/x64) packages → a draft-first GitHub Release verified complete before publication. The release version is independent of the internal iteration line; prerelease is forced while `tooling/release/release-config.mjs` reports `signingImplemented: false` — the signed-package future arrival flips that one export and the flow is unchanged.

**Development concurrency is resource-budgeted and reclaimed, not capped.** The repo-local worktree count is derived from a budget (`K = min(ready independent tasks, floor(disk budget / per-track target estimate), max(1, cores/2))`, re-measured each iteration) with a disk watermark gate (root free ≥ 90 GiB, feature targets ≤ 120 GiB) and a same-slice reclamation SLA enforced by a checked-in sweeper. The durable guard is provable reclamation, not a worktree quota. Applies to every iteration's scheduling and cleanup decisions.

**Repository layout and merge discipline.** The monorepo keeps `apps/` (product surfaces), `crates/` (reusable Rust libs), and `schemas/` (wire contracts) separate because each has its own lifecycle. JSON Schema is the wire truth source — Rust + TypeScript codegen from one schema, no hand-written DTO drift. All merges to `main` go through a GitHub PR for the review trail and CI gate, and the `.mstar/` harness convention governs plan/QC/QA discipline.

### Current owners of the retained contracts

The current owners of the retained contracts are the Electron desktop host, the direct-core `nexus42` CLI cohort, the standalone TypeScript service as the HTTP host for browser/Electron, and the independent `nexus-runtime` Connect host ([rust-core-service-boundary.md](.mstar/specs/active/architecture/rust-core-service-boundary.md)). `/v1/daemon/*` is retained wire vocabulary, not a live daemon process; exactly one desktop host ships.
