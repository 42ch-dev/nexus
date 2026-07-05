# Knowledge Base

Engineering reference for the Nexus OSS harness **knowledge** tree.

| Subtree | Role |
| --- | --- |
| **[`specs/`](specs/README.md)** | Normative OSS specifications (flat layout) |
| **[`architecture-patterns/`](architecture-patterns/)** | Distilled reusable patterns (compound output) |
| **[`api-design/`](api-design/)** | Distilled reusable API design patterns (compound output) |
| **This directory (root files)** | Cross-cutting policy, boundaries, trackers |

**Rules:** [AGENTS.md](AGENTS.md) · **Harness:** [`.mstar/AGENTS.md`](../AGENTS.md) · **Iterations:** [`.mstar/iterations/`](../iterations/README.md)

---

## Index (knowledge root only)

| Document | Role |
| --- | --- |
| [crate-selection-best-practices.md](crate-selection-best-practices.md) | Rust workspace dependency conventions |
| [schemas-external-consumer-boundary.md](schemas-external-consumer-boundary.md) | Wire vs local-only contract types |
| [deferred-features-cross-version-tracker.md](deferred-features-cross-version-tracker.md) | Open/backlog deferred features (active) |
| [architecture-patterns/canvas-surface-implementation-pattern.md](architecture-patterns/canvas-surface-implementation-pattern.md) | Canvas surface implementation pattern (V1.67–V1.76 distilled; compound V1.77) |
| [architecture-patterns/contracts-gap-on-shipped-backend.md](architecture-patterns/contracts-gap-on-shipped-backend.md) | Closing the contracts/codegen gap on a shipped handler with hand-written DTOs (V1.78 memory surface distilled; compound V1.78) |
| [architecture-patterns/pagination-cursor-without-total-count-labels.md](architecture-patterns/pagination-cursor-without-total-count-labels.md) | Cursor pagination without `total` — render honest "N+" lower-bound count labels via `has_more` (V1.79 reading-surface distilled; compound V1.79) |
| [architecture-patterns/bounded-drain-completion-contract.md](architecture-patterns/bounded-drain-completion-contract.md) | Bounded drain-completion contract for synchronous local pipelines — `has_more` must reflect queue advancement, not rows attempted (V1.80 REL-01 distilled; compound V1.80) |
| [architecture-patterns/fingerprint-cached-live-aggregate.md](architecture-patterns/fingerprint-cached-live-aggregate.md) | Fingerprint-cached live aggregate — decouple polled-endpoint read-path cost from a sound exact count (V1.81 SOUL narrative distilled; compound V1.81) |
| [architecture-patterns/on-demand-synthesis-read-path-invariant.md](architecture-patterns/on-demand-synthesis-read-path-invariant.md) | On-demand synthesis read-path invariant — gate every LLM call behind explicit intent; verify the poll path never reaches the synthesizer (headless-QA gap; V1.81 greploop-distilled; compound V1.81 post-merge) |
| [architecture-patterns/nexus-brand-token-hierarchy.md](architecture-patterns/nexus-brand-token-hierarchy.md) | Nexus brand token hierarchy — root DESIGN → `@42ch/nexus-ui` → app DESIGN mapping → implementation; LFS PNG + SVG asset policy (V1.83 brand foundation distilled; compound V1.83) |
| [architecture-patterns/bundler-agnostic-component-library-assets.md](architecture-patterns/bundler-agnostic-component-library-assets.md) | Bundler-agnostic component library assets — a tsup/esbuild-built React component library cannot import `.svg` in source; the consumer resolves the asset URL via its bundler and passes it as a `src` prop (V1.87 nexus-ui promotion distilled; compound V1.87) |
| [architecture-patterns/daemon-api-remote-bind-gate.md](architecture-patterns/daemon-api-remote-bind-gate.md) | Daemon API remote-bind gate — opt-in non-loopback bind gated by API key + explicit flag, enforced before `TcpListener::bind` (V1.90 remote-ready rename distilled) |
| [architecture-patterns/profile-aware-reading-chrome.md](architecture-patterns/profile-aware-reading-chrome.md) | Profile-aware reading chrome — map `work_profile` → token-driven ReactMarkdown renderers while preserving the read-only invariant (V1.91 reading chrome distilled) |
| [api-design/additive-batch-patch-helper.md](api-design/additive-batch-patch-helper.md) | Additive batch PATCH helper — cap-bounded, DAO-reused, partial-success loop with per-ID `not_found`/`conflict` arrays (V1.91 findings batch triage distilled) |
| [conventions/surface-rename-hygiene-checklist.md](conventions/surface-rename-hygiene-checklist.md) | Surface-rename hygiene checklist — grep sweeps and verification gates for renaming a cross-language contract surface (V1.90 Local API → Daemon API distilled) |

**All OSS feature specs:** [`specs/README.md`](specs/README.md) (full index by domain).

**Archived:** [`.mstar/archived/knowledge/`](../archived/knowledge/README.md) · [shipped-features-tracker](../archived/shipped-features-tracker.md)
