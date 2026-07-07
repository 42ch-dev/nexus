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
| [architecture-patterns/daemon-ready-gate-pattern.md](architecture-patterns/daemon-ready-gate-pattern.md) | Daemon-ready gate pattern — single source of truth (`SidecarManager`) + multiple observers; subscribe to `onDaemonStatusChanged`, do NOT add `is_daemon_ready()` commands. V1.96 added Rules 5-8: mount-time state probe before subscribe (late-subscription race), explicit state-enum branching (no silent `'starting'` drop), bounded timeout with re-probe, stderr capture for diagnostic surfacing (V1.94 desktop onboarding distilled; V1.96 P0 daemon-hang fix refinements) |
| [architecture-patterns/profile-aware-reading-chrome.md](architecture-patterns/profile-aware-reading-chrome.md) | Profile-aware reading chrome — map `work_profile` → token-driven ReactMarkdown renderers while preserving the read-only invariant (V1.91 reading chrome distilled) |
| [api-design/additive-batch-patch-helper.md](api-design/additive-batch-patch-helper.md) | Additive batch PATCH helper — cap-bounded, DAO-reused, partial-success loop with per-ID `not_found`/`conflict` arrays (V1.91 findings batch triage distilled) |
| [conventions/surface-rename-hygiene-checklist.md](conventions/surface-rename-hygiene-checklist.md) | Surface-rename hygiene checklist — grep sweeps + anchor-link + stutter verification gates for renaming a cross-language contract surface (V1.90 Local API → Daemon API distilled; V1.93 added anchor gate + pre-commit-self-check lesson) |
| [architecture-patterns/header-key-csrf-defence.md](architecture-patterns/header-key-csrf-defence.md) | Header-key auth is its own CSRF defence — when remote auth uses a custom `X-API-Key` header, the V1.86 Origin allowlist + CORS preflight make a separate CSRF token framework redundant; re-open only if a cookie/session model is adopted (V1.92 remote-access hardening distilled) |
| [architecture-patterns/self-signed-tls-listener-integration.md](architecture-patterns/self-signed-tls-listener-integration.md) | Self-signed TLS listener integration (rcgen + rustls + axum-server) — `axum_server::bind_rustls` preserves the `axum::serve` call site; crypto provider once at boot; **SAN must include the non-loopback bind host or remote-client hostname validation fails before TOFU** (V1.92 W-001 lesson) |
| [architecture-patterns/resolved-residual-verification.md](architecture-patterns/resolved-residual-verification.md) | Residual lifecycle is a claim, not a guarantee — verify against current `main` HEAD: a `resolved` residual may be insufficient/regressed (V1.86), AND a `deferred`-to-V1.(N+1) residual may already be satisfied by V1.N's fix-wave (V1.93 symmetric case) |
| [architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md](architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md) | Tailwind theme-key routing for sizing tokens — a token under `theme.extend.colors` generates only color utilities; `max-w-*`/`p-*`/`h-*`/`w-*` must be registered under `maxWidth`/`padding`/`spacing` or Tailwind silently emits nothing (V1.95 setup-wizard layout-fix distilled) |

**All OSS feature specs:** [`specs/README.md`](specs/README.md) (full index by domain).

**Archived:** [`.mstar/archived/knowledge/`](../archived/knowledge/README.md) · [shipped-features-tracker](../archived/shipped-features-tracker.md)
