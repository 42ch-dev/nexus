# Knowledge Base

Engineering reference for the Nexus OSS harness **knowledge** tree.

| Subtree | Role |
| --- | --- |
| **[`../specs/`](../specs/README.md)** | Normative OSS specifications (`{SPECS_DIR}`) |
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
| [architecture-patterns/canvas-surface-implementation-pattern.md](architecture-patterns/canvas-surface-implementation-pattern.md) | Canvas surface implementation pattern — six-layer coupled contract + projection data-completeness + spatial edges + fixture-projection + viewport guard + **layer 11 discoverability** (V1.67–V1.76 distilled; V1.108–V1.111 updates; compound V1.77/V1.109/V1.111) |
| [spoke-adapter-conversion-seam.md](architecture-patterns/spoke-adapter-conversion-seam.md) | 2026-07-26-v1.139-p1-rust-domain-migration | SPOKE adapter conversion-seam: product domain type ↔ spoke wire type; sole extension point for body schema evolution | Active |
| [architecture-patterns/action-registry-command-palette.md](architecture-patterns/action-registry-command-palette.md) | Action registry + command palette — module store + `useSyncExternalStore`, render-time `available?()`, `useHotkey` conflict-avoidance, WAI-ARIA combobox (V1.111 P0 distilled; compound V1.111) |
| [architecture-patterns/contracts-gap-on-shipped-backend.md](architecture-patterns/contracts-gap-on-shipped-backend.md) | Closing the contracts/codegen gap on a shipped handler with hand-written DTOs (V1.78 memory surface distilled; compound V1.78) |
| [architecture-patterns/pagination-cursor-without-total-count-labels.md](architecture-patterns/pagination-cursor-without-total-count-labels.md) | Cursor pagination without `total` — render honest "N+" lower-bound count labels via `has_more` (V1.79 reading-surface distilled; compound V1.79) |
| [architecture-patterns/bounded-drain-completion-contract.md](architecture-patterns/bounded-drain-completion-contract.md) | Bounded drain-completion contract for synchronous local pipelines — `has_more` must reflect queue advancement, not rows attempted (V1.80 REL-01 distilled; compound V1.80) |
| [architecture-patterns/fingerprint-cached-live-aggregate.md](architecture-patterns/fingerprint-cached-live-aggregate.md) | Fingerprint-cached live aggregate — decouple polled-endpoint read-path cost from a sound exact count (V1.81 SOUL narrative distilled; compound V1.81) |
| [architecture-patterns/on-demand-synthesis-read-path-invariant.md](architecture-patterns/on-demand-synthesis-read-path-invariant.md) | On-demand synthesis read-path invariant — gate every LLM call behind explicit intent; verify the poll path never reaches the synthesizer (headless-QA gap; V1.81 greploop-distilled; compound V1.81 post-merge) |
| [architecture-patterns/nexus-brand-token-hierarchy.md](architecture-patterns/nexus-brand-token-hierarchy.md) | Nexus brand & design token hierarchy — root DESIGN pair is the sole token SSOT → `@nexus/design-tokens` shared Tailwind preset + tokens.css → `@42ch/nexus-ui` brand → app implementation; LFS PNG + SVG asset policy; **Chronos dual-role**; **V1.132 P2** theme-split primary + plain vs `*-square`; **V1.135–V1.136** Dock opaque RGB + baked squircle with **contrast margin ≠ plate**; **V1.136 P2** light interactive = `brand-cyan-1000` + white (neon cyan Dark-only); **V1.137 P0** white-on-teal fill audit (semantic `*-active` pairs + check-tokens gate); **V1.137** Button `tiny` + TE quiet `label-12`; **V1.137 P2** Tabs promoted |
| [architecture-patterns/daemon-matchit-colon-capture.md](architecture-patterns/daemon-matchit-colon-capture.md) | Daemon Axum/matchit routes must use `:param` colon capture — `{param}` braces never match → empty-body framework 404 (V1.132 P0 orch-load-404; related hotfix setup-continue) |
| [architecture-patterns/vite-daemon-proxy-boot-window.md](architecture-patterns/vite-daemon-proxy-boot-window.md) | Vite `/v1/daemon` proxy: map `ECONNREFUSED` → 503 (not 500); gate pre-ready queries inside `DaemonLaunchGate` (V1.134 P0) |
| [architecture-patterns/workspace-parent-shell-ia.md](architecture-patterns/workspace-parent-shell-ia.md) | Workspace-parent shell IA — 工作区 footer always on; 创作/编排 modes; **V1.135 P0** hub create in sidebar `panelContent`, content browse-only; **V1.136 P1** inline World\|Work tabs + form + direct API submit (replaces CreateCardButton dialogs) |
| [architecture-patterns/bundler-agnostic-component-library-assets.md](architecture-patterns/bundler-agnostic-component-library-assets.md) | Bundler-agnostic component library assets — a tsup/esbuild-built React component library cannot import `.svg` in source; consumer resolves URL via `src` prop; Chronos shell wrappers use **`logo-primary` only** (no theme-split) (V1.87; **2026-07-22 refresh**) |
| [architecture-patterns/daemon-api-remote-bind-gate.md](architecture-patterns/daemon-api-remote-bind-gate.md) | Daemon API remote-bind gate — opt-in non-loopback bind gated by API key + explicit flag, enforced before `TcpListener::bind` (V1.90 remote-ready rename distilled) |
| [architecture-patterns/daemon-ready-gate-pattern.md](architecture-patterns/daemon-ready-gate-pattern.md) | Daemon-ready gate pattern (**V1.105 Rule 15**: always-start + `DaemonLaunchGate`; Rule 13 superseded) — single source of truth (`SidecarManager`) + multiple observers; subscribe to `onDaemonStatusChanged`, do NOT add `is_daemon_ready()` commands. V1.96 added Rules 5-8: mount-time state probe before subscribe (late-subscription race), explicit state-enum branching (no silent `'starting'` drop), bounded timeout with re-probe, stderr capture for diagnostic surfacing (V1.94 desktop onboarding distilled; V1.96 P0 daemon-hang fix refinements). V1.97 added Rules 9-12: `Stopped` initial state (never `Starting`), `Starting`+`child.is_some()` short-circuit gate, **Tauri v2 `sidecar()` takes the filename not the `externalBin` path** (latent first-launch blocker since V1.66), attach-without-fabricated-ownership (V1.97 desktop first-launch hardening distilled). V1.100 added Rule 13: gate `.setup()` auto-start behind `setup_completed`; `ensure_setup_bootstrap` Tauri IPC creates minimum creator/workspace state before daemon start — closes `R-V197-SMOKE-CLEAN-STATE` (V1.100 P0 desktop clean-state distilled). **V1.105:** Rule 13 superseded by D2 always auto-start + `DaemonLaunchGate` (compound rewrite pending). **V1.101 Rule 14:** Class B process PATH enrichment at daemon boot for agent CLI discovery (`path_enrichment.rs`) — no `schemas/` change. **V1.110:** three-valued port-probe gate (`Free`/`Occupied`/`Unknown`) skips HTTP probe on cold start; two-phase poll (100ms fast / 250ms steady); unit-test boundary (do NOT spawn real daemon in tests) |
| [architecture-patterns/gui-process-path-enrichment.md](architecture-patterns/gui-process-path-enrichment.md) | GUI-process PATH enrichment for agent CLI discovery — macOS GUI apps inherit minimal PATH; nvm/volta/fnm/pnpm/yarn version-manager dirs invisible to `which`; `login_equivalent_bin_dirs()` resolves active versions (bounded alias-hop + highest-semver glob); no shell-out; existence-gated (V1.110 FB-D3 distilled; closes R-V1101P0-003) |
| [architecture-patterns/acp-registry-id-matching.md](architecture-patterns/acp-registry-id-matching.md) | ACP registry matching: id vs display name — match priority/pinning lists by `registry_agent_id` (stable), not by `name` (mutable label); the live CDN emits different names than user mental models; case-insensitive name `includes` fallback for forward-compat (V1.110 FB-D2 C1 distilled) |
| [architecture-patterns/daemon-creator-display-name-dual-store.md](architecture-patterns/daemon-creator-display-name-dual-store.md) | Daemon creator `display_name` dual-store SSOT — SQL `creators` table and `creator_identity_cache.json` are independent stores read by different paths; any display_name write must UPSERT both or surfaces drift silently (V1.117 P0 QC1 F-001 distilled; compound V1.117) |
| [architecture-patterns/profile-aware-reading-chrome.md](architecture-patterns/profile-aware-reading-chrome.md) | Profile-aware reading chrome — map `work_profile` → token-driven ReactMarkdown renderers while preserving the read-only invariant (V1.91 reading chrome distilled) |
| [api-design/additive-batch-patch-helper.md](api-design/additive-batch-patch-helper.md) | Additive batch PATCH helper — cap-bounded, DAO-reused, partial-success loop with per-ID `not_found`/`conflict` arrays (V1.91 findings batch triage distilled) |
| [conventions/surface-rename-hygiene-checklist.md](conventions/surface-rename-hygiene-checklist.md) | Surface-rename hygiene checklist — grep sweeps + anchor-link + stutter verification gates for renaming a cross-language contract surface (V1.90 Local API → Daemon API distilled; V1.93 added anchor gate + pre-commit-self-check lesson) |
| [architecture-patterns/header-key-csrf-defence.md](architecture-patterns/header-key-csrf-defence.md) | Header-key auth is its own CSRF defence — when remote auth uses a custom `X-API-Key` header, the V1.86 Origin allowlist + CORS preflight make a separate CSRF token framework redundant; re-open only if a cookie/session model is adopted (V1.92 remote-access hardening distilled) |
| [architecture-patterns/self-signed-tls-listener-integration.md](architecture-patterns/self-signed-tls-listener-integration.md) | Self-signed TLS listener integration (rcgen + rustls + axum-server) — `axum_server::bind_rustls` preserves the `axum::serve` call site; crypto provider once at boot; **SAN must include the non-loopback bind host or remote-client hostname validation fails before TOFU** (V1.92 W-001 lesson) |
| [architecture-patterns/resolved-residual-verification.md](architecture-patterns/resolved-residual-verification.md) | Residual lifecycle is a claim, not a guarantee — verify against current `main` HEAD: a `resolved` residual may be insufficient/regressed (V1.86), AND a `deferred`-to-V1.(N+1) residual may already be satisfied by V1.N's fix-wave (V1.93 symmetric case) |
| [architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md](architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md) | Tailwind theme-key routing for sizing tokens — a token under `theme.extend.colors` generates only color utilities; `max-w-*`/`p-*`/`h-*`/`w-*` must be registered under `maxWidth`/`padding`/`spacing` or Tailwind silently emits nothing (V1.95 setup-wizard layout-fix distilled) |
| [architecture-patterns/ui-component-promotion-workflow.md](architecture-patterns/ui-component-promotion-workflow.md) | UI component promotion workflow — Studio-first development pattern: compose View fixtures in `apps/design-studio` → validate visually → promote pure presentational primitives to `@42ch/nexus-ui` → integrate into `apps/web` via thin re-export wrappers; promotion rules, boundary constraints, `cn` helper pattern, `@web-ui/*` transitional policy (V1.99 design-system deepening distilled; compound V1.99). **V1.100 hardened:** mechanically-enforced guardrails (`tooling/check-ui-guardrails.sh` + CI) replace manual grep; `cn` consolidated as public `@42ch/nexus-ui` export (design-tokens cycle rejected); form-field promotion proved semantics-first (locked a11y/composition contract before code) (V1.100 P1+P2 distilled). **V1.101:** Select package promotion + AgentPicker stays app-shared (not nexus-ui); Studio README must match guardrails; human smoke ≠ automated Done. **V1.103:** Settings shell module layout + Connect form extract; Stretch Workspace nav only when plan runs. **V1.106–V1.107:** Toast package promotion requires App thin re-export to close duplication (`R-V1106P0-001`); `@web-layout/*` / `@web-settings/*` presentational aliases for shell/Settings Surfaces. **V1.128:** two-tier Studio import model (`@web-*` vs `@42ch/nexus-ui` vs `@web-ui/*`); Surfaces source badges; RF-free `@web-canvas/*` NLE overlay adoption |
| [architecture-patterns/asymmetric-setup-completed-context.md](architecture-patterns/asymmetric-setup-completed-context.md) | Asymmetric setup-completed context — optimistic `true` for wizard Finish vs await-then-clear `false` for Settings Re-run; prevents SetupGate bounce (V1.103 P3 QC F-001 distilled; compound V1.103) |
| [architecture-patterns/native-cli-provider-adapter-pattern.md](architecture-patterns/native-cli-provider-adapter-pattern.md) | Native CLI provider adapter pattern + ACP registry bare-command extraction — how to add native CLI providers (claude-native, codex-native), per-invocation vs persistent lifecycle, NATIVE_PREFERRED_FAMILIES dedup; plus the `bare_command_name()` fix for registry relative-path cmds (V1.116 P0 distilled; compound V1.116) |
| [architecture-patterns/web-i18n-pattern.md](architecture-patterns/web-i18n-pattern.md) | Web i18n architecture pattern - LocaleProvider mirrors ThemeProvider; nine-namespace catalog; Command labelKey + palette render-time resolution for instant locale switch; format.ts active-locale wiring; caller-owned copy convention (V1.112 i18n foundation + full migration distilled; compound V1.112) |

**All OSS feature specs:** [`../specs/README.md`](../specs/README.md) (full index by domain).

**Shipped archive (shared):** [shipped-features-tracker.md](shipped-features-tracker.md) — closed deferred-feature rows / delivery snapshots.  
**Local process archive:** `.mstar/archived/` (gitignored — plan snapshots, legacy knowledge dumps; not clone SSOT).

### V1.119 additions

| [daemon-creator-pool-lazy-attach.md](architecture-patterns/daemon-creator-pool-lazy-attach.md) | Daemon creator pool lazy-attach pattern - `ensure_creator_pool()` before pool access on Tier-1 handlers after `ensureSetupBootstrap`; web-only fixes are dead ends (V1.119 P0 distilled; compound V1.119) |

### V1.102 additions

| [badge-soft-solid-tone.md](architecture-patterns/badge-soft-solid-tone.md) | Badge soft/solid tone axis (V1.102) |

### V1.120 additions

| Document | Description |
| --- | --- |
| [conventions/system-preset-qualified-id-resolution.md](conventions/system-preset-qualified-id-resolution.md) | `_system.*` qualified-id → filesystem resolution convention — strip prefix + canonical `system_preset_bundle_dir`; literal joins are a recurring bug class; sessions `_system.` filter (V1.120 P0/P2 distilled) |
| [architecture-patterns/tailwind-content-scan-for-package-primitives.md](architecture-patterns/tailwind-content-scan-for-package-primitives.md) | Tailwind `content` must scan `packages/nexus-ui/src` or package-only utilities (`appearance-none` etc.) silently never emit — duplicate-chevron root cause (V1.120 P1 distilled) |

### V1.121 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/editorial-typography-voice-split.md](architecture-patterns/editorial-typography-voice-split.md) | Content voice (serif display tier) vs interface voice (sans) discipline — creative-entity titles vs engine chrome; `Card.Title` `voice` prop; greppable both directions, test-pinned, studio-documented (V1.121 v0.4 Literary Engine distilled) |
| [architecture-patterns/self-hosted-ofl-font-wiring.md](architecture-patterns/self-hosted-ofl-font-wiring.md) | Self-hosted OFL font pattern — canonical provenance, bundler-agnostic package boundary, app vendoring, `@font-face` in shared tokens.css, preload, bundle gate ≤ 80 KB gz/weight (V1.121 Source Serif 4 distilled) |

### V1.122 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/canvas-surface-extraction-pattern.md](architecture-patterns/canvas-surface-extraction-pattern.md) | Canvas surface extraction pattern — extracting a new surface from an existing bundled surface via additive enum, single graph source, stable factory, write-boundary reuse, conflict reuse, and wire-free verification gate (V1.122 P1 timeline-canvas-architecture distilled; compound V1.122) |
| [architecture-patterns/world-vs-work-canvas-scope.md](architecture-patterns/world-vs-work-canvas-scope.md) | World vs Work scope discipline for Canvas surfaces — spine surfaces are World-scoped, projection surfaces are Work-scoped; do not cross-compose scoped data sources (V1.122 P1 architect-locked decision distilled; compound V1.122) |
| [conventions/wire-contracts-frozen-verification.md](conventions/wire-contracts-frozen-verification.md) | Wire contracts frozen verification (8-point gate) — systematic checklist for verifying `wire_contracts_changed: false` on additive-frontend iterations (V1.122 P1 timeline-canvas-architecture §9 distilled; compound V1.122) |

### V1.123 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/three-layer-timeline-projection.md](architecture-patterns/three-layer-timeline-projection.md) | Three-layer Timeline projection pattern — extending a Canvas surface with multi-layer projection (Brief/Narrative/Moment) via per-layer adapter factory + URL `?layer=` persistence + cross-surface navigation hooks + additive wire carrier choices (V1.123 P0+P1+P2+P4 compound distilled) |
| [conventions/three-layer-timeline-feel.md](conventions/three-layer-timeline-feel.md) | Three-layer Timeline feel differentiation contract — per-layer layout/density/visual/zoom contract honoring "三层不一样的感受" mandate; semantic zoom (not viewport zoom); CSS keyframe vs Framer Motion; URL persistence; honest empty-state per layer (V1.123 P4 compound distilled) |
| [conventions/profile-b-residual-archival-procedure.md](conventions/profile-b-residual-archival-procedure.md) | Profile B residual archival procedure — eligibility rule + mixed-severity handling + closure-note ND-A2 enum + 8-step procedure for keeping `.mstar/status.json` under the 20 KB hygiene line (V1.126 P3 distilled; closes DF-V1123-STATUS-COMPACT + DF-V1123-RESIDUAL-CLEANUP) |
| [conventions/subagent-empty-response-fallback.md](conventions/subagent-empty-response-fallback.md) | Subagent empty-response fallback pattern — detection + retry/general-fallback sequence + PM inline whitelist + documentation requirement for specialist-subagent empty results on OpenCode (V1.124 first flagged; V1.126 frequency data + fallback sequence distilled) |

### V1.127 additions

| Document | Description |
| --- | --- |
| [workflow-patterns/predictive-scan-endpoint-verification.md](workflow-patterns/predictive-scan-endpoint-verification.md) | Predictive scan endpoint verification — `explore` subagent scans must verify user-visible symptom claims against actual endpoint handlers; architect seat 2 should catch framing errors during AQ resolution; AC reframing during QC fix wave is legitimate when delivered value diverges from locked AC (V1.127 P1 qc1 C-001 distilled) |

### V1.128 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/creator-shell-content-mode-pattern.md](architecture-patterns/creator-shell-content-mode-pattern.md) | Creator shell content mode — `CreatorEntitySelectionContext` SSOT for Create page vs Controller stub; `@web-layout/creator-shell-content` presentational extract; Back clears selection; orthogonal to canvas routes (V1.128 P2 distilled; compound V1.128) |
| [architecture-patterns/ui-component-promotion-workflow.md](architecture-patterns/ui-component-promotion-workflow.md) | **Updated** — V1.128 two-tier Studio import model (`@web-*` extracts vs `@42ch/nexus-ui` promoted primitives vs `@web-ui/*` transitional); Surfaces badge convention; RF-free `@web-canvas/*` NLE overlay adoption pattern (V1.128 P3 + P1 distilled) |

### V1.131 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/settings-modal-primary-host.md](architecture-patterns/settings-modal-primary-host.md) | Settings modal primary host — one `SettingsModalHost`, section-descriptor SSOT, deep links over safe background, BrowserRouter dirty leave (restore URL + host confirm), Studio chrome fixtures (V1.131 P0+P2 distilled) |
| [architecture-patterns/chronos-titlebar-overlay.md](architecture-patterns/chronos-titlebar-overlay.md) | Chronos titlebar Overlay — Tauri v2 `titleBarStyle: Overlay` + native traffic lights, web ink bar, non-interactive drag regions only, maximize IPC, human Overlay smoke gate (V1.131 P0 distilled) |
| [conventions/profile-b-residual-archival-procedure.md](conventions/profile-b-residual-archival-procedure.md) | **Updated** — anti-pattern: do not archive live-smoke residuals (Dock / Overlay) before QA Pass evidence (V1.131 P3 QC2 F-001) |

### V1.137 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/nexus-brand-token-hierarchy.md](architecture-patterns/nexus-brand-token-hierarchy.md) | **Updated** — V1.137 P0 white-on-teal fill audit (semantic active-bg pairs); Button `tiny`; TransportError quiet `label-12`; Tabs promoted to `@42ch/nexus-ui` (compound V1.137) |

### V1.138 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/third-party-codegen-adoption.md](architecture-patterns/third-party-codegen-adoption.md) | Third-party codegen adoption — jstt + typify replace bespoke generators; schemas frozen; hand-maintained `common_types`; typify consumer adaptation (newtypes / DateTime / NonZeroU64 / prefixed enums); drift + clippy gates, not byte-identical output (V1.138 P0+P1 distilled; compound V1.138) |
