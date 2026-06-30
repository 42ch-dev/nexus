# Strategy

## Vision

Nexus is a **local-first, AI-agent-driven creative writing tool** that uses an infinite canvas to organize ideas and orchestrate the writing workflow — putting authors in full control of their craft and data.

## What we build

An open-source CLI (`nexus42`) and daemon for creative writers — novelists, worldbuilders, essayists — who need:

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
| **Pre-1.0** | Breaking changes expected and allowed; no deprecation period |

## Decision Log

| Decision | Context | Date |
|----------|---------|------|
| Monorepo with `apps/` + `crates/` + `schemas/` | Product surfaces, reusable Rust libs, and wire contracts each have their own lifecycle | Early 2025 |
| `nexus42` binary hosts both CLI and daemon | Single binary for simplicity (no separate `nexus42d`) | V1.68 (June 2026) |
| JSON Schema as wire truth source | Enables Rust + TypeScript codegen; avoids hand-written DTO drift | V1.0 planning |
| Integration branches via PR only | All merges to `main` go through GitHub PR for review trail and CI gate | V1.39 (June 2026) |
| `.mstar/` harness conventions | Morning Star harness for plan/QC/QA discipline | V1.39 (June 2026) |
