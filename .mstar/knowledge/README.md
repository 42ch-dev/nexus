# Knowledge Base

Engineering reference for the Nexus OSS harness **knowledge** tree.

| Subtree | Role |
| --- | --- |
| **[`../specs/`](../specs/README.md)** | Normative OSS specifications |
| **[`architecture-patterns/`](architecture-patterns/)** | Distilled reusable patterns (compound output) — [sub-index](architecture-patterns/README.md) |
| **[`api-design/`](api-design/)** | Distilled reusable API design patterns (compound output) |
| **This directory (root files)** | Cross-cutting policy, boundaries |

**Rules:** [AGENTS.md](AGENTS.md) · **Harness:** [`.mstar/AGENTS.md`](../AGENTS.md)

---

## Index (knowledge root only)

| Document | Role |
| --- | --- |
| [crate-selection-best-practices.md](crate-selection-best-practices.md) | Rust workspace dependency conventions |
| [schemas-external-consumer-boundary.md](../specs/schemas-external-consumer-boundary.md) | **Moved to specs** (2026-08-17) — wire vs local-only contract types |
| [world-kb-runtime-architecture.md](../specs/world-kb-runtime-architecture.md) | **Moved to specs** (2026-08-17) — World KB implementation SSOT |
| [architecture-patterns/canvas-surface-implementation-pattern.md](architecture-patterns/canvas-surface-implementation-pattern.md) | Canvas surface implementation pattern — six-layer coupled contract + projection data-completeness + spatial edges + fixture-projection + viewport guard + **layer 11 discoverability** (V1.67–V1.76 distilled; V1.108–V1.111 updates; compound V1.77/V1.109/V1.111) |
| [architecture-patterns/spoke-adapter-conversion-seam.md](architecture-patterns/spoke-adapter-conversion-seam.md) | SPOKE adapter conversion-seam: product domain type ↔ spoke wire type; sole extension point for body schema evolution |
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
| [architecture-patterns/daemon-ready-gate-pattern.md](architecture-patterns/daemon-ready-gate-pattern.md) | Daemon-ready gate pattern (**V1.105 Rule 15**: always-start + `DaemonLaunchGate`; Rule 13 superseded) — single source of truth (`SidecarManager`) + multiple observers; subscribe to `onDaemonStatusChanged`, do NOT add `is_daemon_ready()` commands. V1.96 added Rules 5-8: mount-time state probe before subscribe (late-subscription race), explicit state-enum branching (no silent `'starting'` drop), bounded timeout with re-probe, stderr capture for diagnostic surfacing (V1.94 desktop onboarding distilled; V1.96 P0 daemon-hang fix refinements). V1.97 added Rules 9-12: `Stopped` initial state (never `Starting`), `Starting`+`child.is_some()` short-circuit gate…
| [architecture-patterns/gui-process-path-enrichment.md](architecture-patterns/gui-process-path-enrichment.md) | GUI-process PATH enrichment for agent CLI discovery — macOS GUI apps inherit minimal PATH; nvm/volta/fnm/pnpm/yarn version-manager dirs invisible to `which`; `login_equivalent_bin_dirs()` resolves active versions (bounded alias-hop + highest-semver glob); no shell-out; existence-gated (V1.110 FB-D3 distilled; closes R-V1101P0-003) |
| [architecture-patterns/acp-registry-id-matching.md](architecture-patterns/acp-registry-id-matching.md) | ACP registry matching: id vs display name — match priority/pinning lists by `registry_agent_id` (stable), not by `name` (mutable label); the live CDN emits different names than user mental models; case-insensitive name `includes` fallback for forward-compat (V1.110 FB-D2 C1 distilled) |
| [architecture-patterns/daemon-creator-display-name-dual-store.md](architecture-patterns/daemon-creator-display-name-dual-store.md) | Daemon creator `display_name` dual-store SSOT — SQL `creators` table and `creator_identity_cache.json` are independent stores read by different paths; any display_name write must UPSERT both or surfaces drift silently (V1.117 P0 QC1 F-001 distilled; compound V1.117) |
| [architecture-patterns/profile-aware-reading-chrome.md](architecture-patterns/profile-aware-reading-chrome.md) | Profile-aware reading chrome — map `work_profile` → token-driven ReactMarkdown renderers while preserving the read-only invariant (V1.91 reading chrome distilled) |
| [api-design/additive-batch-patch-helper.md](api-design/additive-batch-patch-helper.md) | Additive batch PATCH helper — cap-bounded, DAO-reused, partial-success loop with per-ID `not_found`/`conflict` arrays (V1.91 findings batch triage distilled) |
| [api-design/field-level-error-envelope-for-generated-dtos.md](api-design/field-level-error-envelope-for-generated-dtos.md) | Field-level error envelope for generated-DTO write surfaces — lenient schemas + handler-side value validation + closed `details.field` vocabulary mapped 1:1 onto form inputs; member-aware validator seam (CLI byte-identical); 404-before-payload order; typify null-union for absent-vs-empty-array (V1.169 DF-82 distilled) |
| [conventions/surface-rename-hygiene-checklist.md](conventions/surface-rename-hygiene-checklist.md) | Surface-rename hygiene checklist — grep sweeps + anchor-link + stutter verification gates for renaming a cross-language contract surface (V1.90 Local API → Daemon API distilled; V1.93 added anchor gate + pre-commit-self-check lesson) |
| [architecture-patterns/header-key-csrf-defence.md](architecture-patterns/header-key-csrf-defence.md) | Header-key auth is its own CSRF defence — when remote auth uses a custom `X-API-Key` header, the V1.86 Origin allowlist + CORS preflight make a separate CSRF token framework redundant; re-open only if a cookie/session model is adopted (V1.92 remote-access hardening distilled) |
| [architecture-patterns/self-signed-tls-listener-integration.md](architecture-patterns/self-signed-tls-listener-integration.md) | Self-signed TLS listener integration (rcgen + rustls + axum-server) — `axum_server::bind_rustls` preserves the `axum::serve` call site; crypto provider once at boot; **SAN must include the non-loopback bind host or remote-client hostname validation fails before TOFU** (V1.92 W-001 lesson) |
| [architecture-patterns/resolved-residual-verification.md](architecture-patterns/resolved-residual-verification.md) | Residual lifecycle is a claim, not a guarantee — verify against current `main` HEAD: a `resolved` residual may be insufficient/regressed (V1.86), AND a `deferred`-to-V1.(N+1) residual may already be satisfied by V1.N's fix-wave (V1.93 symmetric case) |
| [architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md](architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md) | Tailwind theme-key routing for sizing tokens — a token under `theme.extend.colors` generates only color utilities; `max-w-*`/`p-*`/`h-*`/`w-*` must be registered under `maxWidth`/`padding`/`spacing` or Tailwind silently emits nothing (V1.95 setup-wizard layout-fix distilled) |
| [architecture-patterns/ui-component-promotion-workflow.md](architecture-patterns/ui-component-promotion-workflow.md) | UI component promotion workflow — Studio-first development pattern: compose View fixtures in `apps/design-studio` → validate visually → promote pure presentational primitives to `@42ch/nexus-ui` → integrate into `apps/web` via thin re-export wrappers; promotion rules, boundary constraints, `cn` helper pattern, `@web-ui/*` transitional policy (V1.99 design-system deepening distilled; compound V1.99). **V1.100 hardened:** mechanically-enforced guardrails (`tooling/check-ui-guardrails.sh` + CI) replace manual grep; `cn` consolidated as public `@42ch/nexus-ui` export (design-tokens cycle rejected); form-field promotion proved semantics-first (…
| [architecture-patterns/asymmetric-setup-completed-context.md](architecture-patterns/asymmetric-setup-completed-context.md) | Asymmetric setup-completed context — optimistic `true` for wizard Finish vs await-then-clear `false` for Settings Re-run; prevents SetupGate bounce (V1.103 P3 QC F-001 distilled; compound V1.103) |
| [architecture-patterns/native-cli-provider-adapter-pattern.md](architecture-patterns/native-cli-provider-adapter-pattern.md) | Native CLI provider adapter pattern + ACP registry bare-command extraction — how to add native CLI providers (claude-native, codex-native), per-invocation vs persistent lifecycle, NATIVE_PREFERRED_FAMILIES dedup; plus the `bare_command_name()` fix for registry relative-path cmds (V1.116 P0 distilled; compound V1.116) |
| [architecture-patterns/web-i18n-pattern.md](architecture-patterns/web-i18n-pattern.md) | Web i18n architecture pattern - LocaleProvider mirrors ThemeProvider; nine-namespace catalog; Command labelKey + palette render-time resolution for instant locale switch; format.ts active-locale wiring; caller-owned copy convention (V1.112 i18n foundation + full migration distilled; compound V1.112) |

**All OSS feature specs:** [`../specs/README.md`](../specs/README.md) (full index by domain).

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

### V1.137 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/nexus-brand-token-hierarchy.md](architecture-patterns/nexus-brand-token-hierarchy.md) | **Updated** — V1.137 P0 white-on-teal fill audit (semantic active-bg pairs); Button `tiny`; TransportError quiet `label-12`; Tabs promoted to `@42ch/nexus-ui` (compound V1.137) |

### V1.138 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/third-party-codegen-adoption.md](architecture-patterns/third-party-codegen-adoption.md) | Third-party codegen adoption — jstt + typify replace bespoke generators; schemas frozen; hand-maintained `common_types`; typify consumer adaptation (newtypes / DateTime / NonZeroU64 / prefixed enums); drift + clippy gates, not byte-identical output (V1.138 P0+P1 distilled; compound V1.138) |

### V1.141 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/spoke-adapter-port-orchestration-adoption.md](architecture-patterns/spoke-adapter-port-orchestration-adoption.md) | **Updated (V1.142, V1.143, V1.146)** — SPOKE adapter port + orchestration adoption (Surface B) + production `NexusAdapter` (6 baseline port families against SQLite; V1.146 rename; V1.145 P1b rehome to `nexus-spoke-adapter`); production-vs-stub family matrix (8 families + activation/narrative-read/mca-read adapter modules); 5 production adapter patterns (wire-conversion reuse, CAS mapping §7.4, async→sync bridge, batch tx, per-request construction); orchestrator-cutover-on-write-paths pattern (orchestrate_promote stored=None, transaction-boundary split + retry-safe idempotency, SpokeRejectCode mapping); remaining roadmap…
| [architecture-patterns/spoke-adapter-conversion-seam.md](architecture-patterns/spoke-adapter-conversion-seam.md) | **Updated** — cross-link to companion Surface B doc; tags + applies_when clarified (this is Surface A — pure-helper delegates, V1.139 baseline) |

### V1.143 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/spoke-adapter-port-orchestration-adoption.md](architecture-patterns/spoke-adapter-port-orchestration-adoption.md) | **Updated** — V1.143 section: production cutover count (3 paths — promote_adopt shipped V1.142, upsert shipped V1.143, relate deferred V1.144); `From`/`Into` conversion-seam generalized to `TimelineEvent` ↔ spoke `TimelineEvent`; **structural-mismatch discovery** — spoke `Relation` lacks `revision`, OCC-mirror does NOT transfer; C1 accepted-behavior-diff pattern for merged-terminal semantics (compound V1.143) |

### V1.144 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/spoke-adapter-port-orchestration-adoption.md](architecture-patterns/spoke-adapter-port-orchestration-adoption.md) | **Updated** — V1.144 section: structural-mismatch resolution (spoke 0.5.0 adds `Relation.revision` → `R-V1143P2-DEFER-RELATE` closed at protocol level); production cutover count now 3/3 shipped (promote V1.142, upsert V1.143, relate V1.144); OCC port-extension pattern (insert-only → OCC-aware via existing `kb_*` revision column + CAS guard + revision-seed=1); known gaps (`extensions.nexus` no round-trip on Relation; no spoke 500-class reject code → 400 misclassification across all 4 ports) (compound V1.144) |

### V1.148 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/connect-host-opt-in-feature-gate.md](architecture-patterns/connect-host-opt-in-feature-gate.md) | Connect Host — opt-in feature gate for a heavy transport dep (N-C0 pattern): keep the default build graph free of the dep (gate-checkable `cargo tree`), one shared honest `HostCapabilityManifest` builder, total op-refusal via `invoke_handler = None` (not a per-op refuse handler), atomic identity-key perms + fail-closed allowlist, separate-OS-process topology, deterministic libp2p interop test (mDNS off, fixed seeds). DF-72 N-C0; N-C1 next (compound V1.148) |
| [conventions/nexus-home-layout-path-helpers.md](conventions/nexus-home-layout-path-helpers.md) | `nexus-home-layout` path helpers take **raw home** — never pre-join `.nexus42`. The V1.148 P4 F-1 dogfood bug (device-id double-nesting → host_id churn) root cause + convention. Name the param for the raw input (`home`, not `nexus_home`) (compound V1.148) |

### V1.149 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/spoke-dialect-default-on-engine.md](architecture-patterns/spoke-dialect-default-on-engine.md) | Spoke-dialect consumption as a default-on engine — the lore activation + prompt-control pattern: consumer-only (no spoke-operations matchers), handbook truth-table logic (secondary-empty ⇒ primary-any), ReDoS-safe `regex` crate (NOT `regress` for untrusted input), neutral-only byte-equivalence HARD golden (ship gate), graph hops via storage list (not RelationPort get/put), char-boundary-safe whole_word. **V1.150 (DF-75) extension:** thin post-activation slot routing (never matching logic); off-switch gates ALL activation-product shaping; Moment Directive = product-local prompt control (never on spoke wire; `DirectiveStore` trait + `NoDirec…

### V1.153 additions

| Document | Description |
| --- | --- |
| [workflow-patterns/github-actions-workflow-default-branch-registration.md](workflow-patterns/github-actions-workflow-default-branch-registration.md) | GitHub Actions workflow registration is **default-branch-only** — a brand-new workflow file on a feature/plan branch is not evaluated for `push` / `pull_request` / `workflow_dispatch` / tag triggers (no run, no error, API 404). Mirror the file to the default branch first, then iterate; clean up temporary triggers before merge. (compound V1.153) |
| [workflow-patterns/shared-cargo-target-dir-worktree-stale-manifest-dir.md](workflow-patterns/shared-cargo-target-dir-worktree-stale-manifest-dir.md) | Shared `CARGO_TARGET_DIR` + git worktree add/remove poisons compiled test binaries with a stale `CARGO_MANIFEST_DIR` — `schema_drift_detection` failed with 199 ENOENT errors despite all schema files present; Cargo reuses the "fresh" cached binary whose compile-time workspace root points into a deleted worktree. Remedy: force rebuild (`touch` test file); prevention: workspace-local target dirs or runtime path resolution. (compound V1.153) |
| [architecture-patterns/verify-stored-row-scope-before-cas-write.md](architecture-patterns/verify-stored-row-scope-before-cas-write.md) | Verify the **stored** row's scope before an optimistic-concurrency write — payload-claimed scope ≠ stored-row scope; a scope-agnostic lookup + CAS with revision-leaking OCC rejects = cross-scope write (V1.153 P1 N-C1 L2 Critical). Gate on stored scope via adapter read ports (zero side effects on deny), verify endpoint rows on relation create, and treat OCC as no scope boundary. (compound V1.153) |

### V1.154 additions

| Document | Description |
| --- | --- |
| [workflow-patterns/cargo-lockfile-feature-independent-dependabot.md](workflow-patterns/cargo-lockfile-feature-independent-dependabot.md) | Cargo.lock is feature-independent — removing a cargo feature does NOT remove its lockfile entries; dependabot reports lockfile entries (not the activated graph); alerts close only via upstream release or dep removal from the tree. Verified with `cargo update -p libp2p-mdns` + `cargo generate-lockfile`. (compound V1.154) |
| [workflow-patterns/nexus42-cli-home-resolution-hermetic.md](workflow-patterns/nexus42-cli-home-resolution-hermetic.md) | nexus42 CLI resolves home from `$HOME` ONLY (dirs::home_dir); nexus-runtime reads `--home` > `NEXUS42_HOME` > `$HOME`. Hermetic blocks driving both binaries must export `HOME`; `nexus42 creator register` is platform-only (auth + network). (compound V1.154) |
| [architecture-patterns/gate-vs-execution-module-scope-pin.md](architecture-patterns/gate-vs-execution-module-scope-pin.md) | Module-scope gate bypass: gate-verified id ≠ executed id (orchestrator merged request.computable and re-resolved); fix = key-presence pin (contains_key ⇒ string equal to gated id) + pin the final id through orchestration + shared path-safety helper + non-vacuous denial tests. (compound V1.154) |
| [architecture-patterns/bounded-sync-async-bridge-event-loop.md](architecture-patterns/bounded-sync-async-bridge-event-loop.md) | Bounded sync→async bridge for event-loop handlers: wire path is serialized (handler parks the loop); Semaphore bounds blocking-pool concurrency; ThreadWaker+park_timeout acquire; permit held to completion (not force-cancellable); one shared deadline; response byte cap; mpsc Disconnected → bridge_fault. (compound V1.154) |

### V1.155 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/outbound-observation-peer-bookkeeping.md](architecture-patterns/outbound-observation-peer-bookkeeping.md) | Outbound-observation peer bookkeeping (N-C3) — record ONLY manifest-backed OUTBOUND Connect observations at `connect()` return; inbound invoke boundary carries no manifest (spoke-connect API gap → out-of-scope fallback); `peer_hosts` keyed by claimed `host_id` (libp2p PeerId not stable per installation); `manifest_json` single source of truth (no denormalized capabilities); honesty contract empty→empty; production dial surface = `connect dial` CLI. (compound V1.155) |
| [architecture-patterns/two-gate-token-isolation-composition.md](architecture-patterns/two-gate-token-isolation-composition.md) | Capability-token two-gate isolation composition — enforcement is COMPOSED not duplicated: spoke-side `evaluate_invoke_token_gate` (`auth_failed` before nexus handler) + nexus `PeerScope` allowlist intersection (token can NEVER widen scope); issuer.key distinct create-once 0600; config.json fail-closed (malformed / require-without-issuers → boot error); mint-on-demand closure (no event-loop I/O). (compound V1.155) |
| [best-practices/embedded-pinned-wasm-sha256-alignment.md](best-practices/embedded-pinned-wasm-sha256-alignment.md) | Embedded-pinned `wasm_sha256` gotcha — repo-source `manifest.json` carries a hash pinned to the EMBEDDED artifact (build.rs injection); copying it with a locally-built wasm breaks install (loader verifies bytes before compile); teach align-or-delete (recompute local hash or delete field → stat-fence fallback) in module docs/quickstarts. (compound V1.155) |

### V1.156 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/three-layer-timeline-projection.md](architecture-patterns/three-layer-timeline-projection.md) | **Updated** — V1.156 3×2 matrix completion: P1 (World×Moment, read/projection of bound Works' Outline scene/beat data) + P2 (Work×Brief, projection of bound World's era entities) shipped; all three layers now valid on both surfaces (V1.123 surface-layer URL restriction lifted); read-only inspectors on projected layers (PD-2/PD-3); DR-26 tracks the WorkOutline wire extension for real Moment data. Both frontend-only (`wire_contracts_changed:false`). Compound V1.156 |
| [workflow-patterns/carry-qc-lessons-to-sibling-plan.md](workflow-patterns/carry-qc-lessons-to-sibling-plan.md) | Carry QC lessons to sibling plans proactively — when an iteration ships mirror plans (same pattern, different surface), bake the first plan's QC fix-wave findings into later siblings' briefs proactively; turns a likely N-finding fix-wave into fewer (V1.156: P2 baked in P1's 4 lessons → 1 converged fix vs P1's 4). (compound V1.156) |

### V1.163 additions

| Document | Description |
| [engineering/codegen-optional-field-callsite-coverage.md](engineering/codegen-optional-field-callsite-coverage.md) | Codegen optional field callsite coverage — `pnpm run codegen` regenerates Rust structs but does NOT update struct-literal callsites; `cargo check --workspace` after codegen catches `E0063` before it blocks downstream crates (V1.163 P1 schema change broke P2 compile; compound V1.163) |

### V1.164 additions

| Document | Description |
| --- | --- |
| [conventions/pnpm-toolchain-pin-and-supply-chain-age.md](conventions/pnpm-toolchain-pin-and-supply-chain-age.md) | pnpm 11 pin (2026-08-15): settings live in pnpm-workspace.yaml (pnpm 11 ignores package.json `pnpm` field); `allowBuilds` allowlist or frozen install fails ERR_PNPM_IGNORED_BUILDS; local `minimumReleaseAge` policy rejects same-day publishes; MODULE_NOT_FOUND after failed install = partial wipe (hit 3× in V1.164 under the old 9-pin split) |
| [architecture-patterns/spoke-op-gate-at-adapter-boundary.md](architecture-patterns/spoke-op-gate-at-adapter-boundary.md) | Spoke-op validation gates live at the nexus-spoke-adapter boundary; storage crates stay pure (sole-consumer rule spans 3 docs); V1.146 + V1.164 both caught the same near-miss |
| [engineering/order-insensitive-json-assertions-on-typed-seams.md](engineering/order-insensitive-json-assertions-on-typed-seams.md) | Never assert raw-string equality on JSON crossing a typed seam — BTreeMap-backed spoke types serialize keys alphabetically; parse-both-sides compare (V1.164 P1 QA blocker RCA) |

### V1.165 additions

| Document | Description |
| --- | --- |
| [conventions/crash-resilient-subagent-report-dispatch.md](conventions/crash-resilient-subagent-report-dispatch.md) | Long-running dispatches write report skeletons FIRST + append incrementally; mid-edit crashes require damage-survey re-dispatchs (4 crashes across V1.164–V1.165) |
| [architecture-patterns/scope-discriminated-port-persistence.md](architecture-patterns/scope-discriminated-port-persistence.md) | Route single-method spoke port outputs to multiple nexus homes via extensions.nexus discriminator (world_id vs work_id; V1.165) — and the read-side twin: scope wrappers around spoke orchestrators must be adopted by BOTH callers + manifest compile guard certifies the real entrypoint + validation-gate placement follows wire topology (V1.166 run_checker) |

### V1.167 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/creator-bootstrap-two-store-materialization.md](architecture-patterns/creator-bootstrap-two-store-materialization.md) | Creator bootstrap materializes TWO stores — minting an identity (global state.db) + setting active config is insufficient; the first workspace write FK-prechecks the per-creator+workspace db `creators` row (`ensure_creator_row` helper; register --local complete path; system identity create parity gap tracked) (V1.167 P2 dogfood-sweep fix distilled; compound V1.167) |
| [workflow-patterns/cargo-lockfile-feature-independent-dependabot.md](workflow-patterns/cargo-lockfile-feature-independent-dependabot.md) | **Updated** — lockfile presence ≠ compile reachability: a default-feature-absent entry can be activated under a CI-built feature combo (yamux #41 via connect-host); triage with four probes incl. `cargo tree -i pkg@version --features …`; `cargo metadata` has no `--target` flag (V1.167 P1 disposition distilled) |
| [workflow-patterns/nexus42-cli-home-resolution-hermetic.md](workflow-patterns/nexus42-cli-home-resolution-hermetic.md) | **Updated** — `creator register --local` is now the hermetic bootstrap path (V1.167 P2); plain register remains platform-only |

### V1.168 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/native-cli-provider-adapter-pattern.md](architecture-patterns/native-cli-provider-adapter-pattern.md) | **Updated** — external protocol clients replace self-written wire parsers (claude-codes / codex-codes app-server / deepseek-harness-sdk): per-session client locks (never hold the registry RwLock across I/O), no prompt-timeout as frame-gap, turn-id filtering + interrupt/drain, decode-drift contract (skip unknown vs OpFailed typed-decode, 512-byte message cap), session rotation on timeout, honest `dsh_limited` descriptor, spawnable mock protocol stubs (V1.168 distilled; compound V1.168) |
| [workflow-patterns/process-env-lock-fixture-spawn-serialization.md](workflow-patterns/process-env-lock-fixture-spawn-serialization.md) | Env-mutating tests must serialize with fixture-spawning tests — `#!/usr/bin/env python3` resolves through the live process PATH; one crate-wide `PROCESS_ENV_LOCK` + Drop-restore guards (V1.168 P2 flake root-cause distilled) |

### V1.169 additions

| Document | Description |
| --- | --- |
| [engineering/spoke-lockstep-upgrade-procedure.md](engineering/spoke-lockstep-upgrade-procedure.md) | Spoke lockstep upgrade procedure — five-pin lockstep (3 crates + 2 npm + drift-gate `SPOKE_PIN`), surgical `cargo update -p`, **both** graphs `--all-targets` (feature-gated examples surface extra literal sites), refusal/honesty test pinning for new upstream capability surfaces, feature-graph evidence set, Cargo comment trail + spec pin alignment (V1.139–V1.169 series distilled; compound V1.169) |
| [api-design/field-level-error-envelope-for-generated-dtos.md](api-design/field-level-error-envelope-for-generated-dtos.md) | Field-level error envelope for generated-DTO write surfaces — lenient schemas + handler-side value validation + closed `details.field` vocabulary mapped 1:1 onto form inputs; member-aware validator seam (CLI byte-identical); 404-before-payload order; typify null-union for absent-vs-empty-array (V1.169 DF-82 distilled; compound V1.169) |
| [engineering/codegen-optional-field-callsite-coverage.md](engineering/codegen-optional-field-callsite-coverage.md) | **Updated** — V1.169 addendum: typify optional-**array** collapse (absent ≡ `[]`) — use `["array","null"]` union on update-schema members when absence must differ from empty-array clear (observed on `world-rule-update-request.target_entry_types`) |

### V1.170 additions

| Document | Description |
| --- | --- |
| [engineering/standalone-crate-monorepo-topology.md](engineering/standalone-crate-monorepo-topology.md) | Standalone crate topology in a Cargo workspace — empty `[workspace]` tail table for every in-repo publishable crate; root `exclude` only where a workspace member path-depends (the resolution-conflict mechanics); crates.io-vs-path consumption; literal pins without `workspace = true` (V1.170 P0 AR-1 distilled; compound V1.170) |
| [engineering/compute-module-sdk-authoring-pattern.md](engineering/compute-module-sdk-authoring-pattern.md) | Compute module SDK authoring pattern — `nexus_entry!` macro + trait entry for additive DR-49, typed envelope + `Value` passthrough drift-surface minimization, wire-required vs lock-assumed WorldRef lesson, three-layer drift guard (golden fixtures + mirror-gap script + behavioral parity CI), validator mirroring with shared corpus, host-import sentinel mapping, mini-host honesty boundary (V1.170 P0 AR-2..AR-12 distilled; compound V1.170) |
| [architecture-patterns/user-layer-entrance-split.md](architecture-patterns/user-layer-entrance-split.md) | User-layer entrance split — orthogonal usage axis (`developer`\|`content-creator`) with single route table + typed registry + guard + entrance-aware index redirect + `setup_completed`-class persistence seams; **enforcement-path rule**: every shell surface (settings rail, titlebar gear, mobile nav, wizard re-run) must consume the axis — route-guard alone yields one QC Warning per unwired surface (V1.170 P1 EL-1..8 + AR-15..22 distilled; compound V1.170) |

### V1.171 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/shared-validation-core-migration.md](architecture-patterns/shared-validation-core-migration.md) | Shared validation core migration — moving a model/validator out of a CLI binary into a library crate so a daemon can consume it; CLI re-exports via `pub use` + `From` impl keeping error messages byte-identical; fold the resolver too (V1.171 P2 AR-29 distilled; compound V1.171) |
| [architecture-patterns/cas-preimage-empty-unset-consistency.md](architecture-patterns/cas-preimage-empty-unset-consistency.md) | CAS pre-image consistency — when a stored column treats NULL ≡ empty as "unset", the `is_default` marker and the CAS pre-check must use the same invariant; a client reconstructing a pre-image from parsed fields cannot byte-match arbitrary stored blobs (raw-blob echo or stable 400 rejection); test the empty-string round-trip (V1.171 P2 QC3 W-2 distilled; compound V1.171) |
| [conventions/wire-contracts-frozen-verification.md](conventions/wire-contracts-frozen-verification.md) | **Updated** — additive local-orchestration wire posture (AR-33): iterations shipping hand-coded local-tier DTOs + new daemon routes must NOT claim `wire_contracts_changed: false`; apply the 8-point gate with expected-diff exemptions (schemas/generated/packages stay binding) (V1.171 AR-33 distilled; compound V1.171) |

### V1.172 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/wasm-module-as-capability-executor.md](architecture-patterns/wasm-module-as-capability-executor.md) | WASM module as capability executor (DR-10 minimal) — on-disk descriptor+manifest+wasm trio extends the compile-time Capability registry; admission collision→file→hash→clamp; carry clamped sandbox to run (silent-no-op lesson); re-verify wasm_sha256 on lazy load (TOCTOU); CLI atomic trio + module-id cross-check (V1.172 AR-34..44 distilled; compound V1.172) |
| [conventions/nexus-home-layout-path-helpers.md](conventions/nexus-home-layout-path-helpers.md) | **Updated** — `user_capabilities_dir` joins the raw-home helper family (mirror `user_modules_dir`); daemon callers pass `state.nexus_home().parent()`; parent-less home must not fail boot (compound V1.172) |
| [conventions/wire-contracts-frozen-verification.md](conventions/wire-contracts-frozen-verification.md) | **Updated** — AR-40 sanctioned-diff posture: intentional additive schema fields are audited diffs (not `wire_contracts_changed: false`, not AR-33 empty-schemas exemptions); local DTO camelCase vs generated snake_case (compound V1.172) |

### V1.173 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/connect-host-tools-serving.md](architecture-patterns/connect-host-tools-serving.md) | Connect Host `tools.*` serving (DF-84) — spoke protocol already supports tools; nexus-host serving: single-source manifest composition (LOCAL_TOOL_OPS ++ CORE_OPS), three exact-string authz layers (negotiation intersection + token AND + op_scope), host-level tools skip world gate, process-boundary reachability (no daemon), payload {arguments}→{result}, advertised-schema strictness (compound V1.173) |

### Indexed (compound V1.174)

| Document | Description |
| --- | --- |
| [architecture-patterns/compute-pillar-invoke-and-runs-history.md](architecture-patterns/compute-pillar-invoke-and-runs-history.md) | Compute pillar — daemon invoke route + Runs history semantics: canon-only projection, retain-by-default + manual Clear, sandbox honesty (V1.147 shipped; indexed compound V1.174) |
| [architecture-patterns/local-environment-scan-safety-boundary.md](architecture-patterns/local-environment-scan-safety-boundary.md) | Local-environment scan safety boundary — five normative constraints for scanning PATH for installed tools: registry-known names, bounded concurrency, short timeout, no shell expansion, no user-supplied commands (V1.147; indexed compound V1.174) |
| [AGENTS.md](AGENTS.md) | Knowledge-store rules: supersession discipline (remove entirely, fix tracked links, update README index only) (indexed compound V1.174) |

### V1.174 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/peer-tool-registration-exposure-lanes.md](architecture-patterns/peer-tool-registration-exposure-lanes.md) | Peer-tool two-lane capability architecture (DF-85 consumer) — WS registration lane → `PeerToolTable` + MCP exposure lane over ONE dispatch spine (static rows → table → user-cap arm); fail-closed exact-id admission, four-layer outbound authz (MCP never a second domain), session lifecycle (reserve-at-accept, last-wins replace, eviction same-tick), typed refusal discriminator (compound V1.174) |
| [architecture-patterns/stateless-mcp-bridge-child.md](architecture-patterns/stateless-mcp-bridge-child.md) | Stateless MCP bridge child (Model A) — client-spawned `nexus42 mcp serve` stdio proxy: no registry/policy/cache, live daemon round trips, lockstep by construction; rmcp 1.8.0 realities (runtime-dynamic `ServerHandler`, prompts/resources default = empty lists not errors, two-class error mapping), schema mapping, alternatives → DF-87/88 (compound V1.174) |
| [conventions/graph-pin-honesty-discipline.md](conventions/graph-pin-honesty-discipline.md) | Graph-pin honesty discipline — cargo-tree `-i` semantics (prior art legitimately present, dev-deps in `-p` output), `--edges normal` shipped-graph pins, honest obligations (no-new-default-package, exactly-one lockstep, feature matrix), assert-empty false-green trap (propagate exit status); V1.174 spoke-operations/rmcp corrections (compound V1.174) |
| [conventions/nexus-home-layout-path-helpers.md](conventions/nexus-home-layout-path-helpers.md) | **Updated** — V1.174 `~/.nexus42/connect/` family (`connect_daemon_config_path`, `connect_peer_keys_path` fail-closed, `daemon_identity_key_path`); distinct-file rule: raw Ed25519 seed vs libp2p protobuf `identity.key` formats are disjoint — never share the file (compound V1.174) |
| [conventions/wire-contracts-frozen-verification.md](conventions/wire-contracts-frozen-verification.md) | **Updated** — V1.174 sanctioned-diff instance: `origin` enum `+= "peer"` + one new route `GET /v1/daemon/tools` (declared wire scope, regen'd); `tools/call` reuses existing spine route; graph claims → separate gate (compound V1.174) |
| [architecture-patterns/connect-host-tools-serving.md](architecture-patterns/connect-host-tools-serving.md) | **Updated** — DF-85 consumer side landed (V1.174 daemon lane); serving face frozen; cross-link to peer-tool two-lane doc (compound V1.174) |

### V1.175 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/descriptive-first-builtin-schema-authoring.md](architecture-patterns/descriptive-first-builtin-schema-authoring.md) | Descriptive-first builtin schema authoring (DF-89) — `CatalogDescriptor` on the registry row, named-placeholder + remainder-ledger lockstep, description/schema/handler triple-coherence pin (3/30-row drift lesson), closed-world where the handler is closed, registry ⇄ catalog ⇄ tools/list schema-equality family (compound V1.175) |
| [conventions/cli-surface-honesty-discipline.md](conventions/cli-surface-honesty-discipline.md) | CLI surface honesty discipline — document/test the ACTUAL public `error_code()` (remapped inner codes like `strategy_invalid` → `bad_request`), render the real `error_details()` keys per 409 family (`current_revision` vs `current_version`+`entity_id`), pin `[code]` + arrow units (never OR-match status strings); clap ValueEnum `#[value(name)]` snake_case pins; truncation notes on human defaults with `--json` DTO-verbatim; 204 deletes print empty stdout (compound V1.175) |
| [architecture-patterns/stateless-mcp-bridge-child.md](architecture-patterns/stateless-mcp-bridge-child.md) | **Updated** — V1.175 DF-90/AR-79 child-side `listChanged` watch: 2 s poll + raw-Value digest (object keys order-insensitive, `items` array order-sensitive by route id-sort), baseline-no-notify, notify-retry-on-failure (keep last digest), log once per error-state transition, `WatcherGuard` abort on every exit path, `enable_tools().enable_tool_list_changed()` advertisement (compound V1.175) |

### V1.176 additions

| Document | Description |
| --- | --- |
| [architecture-patterns/frozen-registry-hot-reload-rebuild-swap.md](architecture-patterns/frozen-registry-hot-reload-rebuild-swap.md) | Frozen-registry hot-reload via rebuild-and-swap — boot-frozen `CapabilityRegistry` gains runtime reload without interior mutability: RwLock holder + Arc swap (drop-after-unlock, last-good in-flight), poll+digest watch (no new deps), DigestPoll three-state (Missing≠Unreadable — transient keeps last-good), ScanOutcome.transient carries unmatched entries, boot-digest baseline seeding, live-derived reserved sets, interner for `&'static str` fields, boot-equivalence machine-checked, journey budgets pinned at 1.5× documented bounds (V1.176 P1 RN-2 distilled; compound V1.176) |
| [architecture-patterns/creator-bootstrap-two-store-materialization.md](architecture-patterns/creator-bootstrap-two-store-materialization.md) | **Updated** — V1.176 DF-83 closure: shared `bootstrap_local_creator` helper (both entry points converge three stores), idempotent three-leg convergence (db-write-free no-op with ro-pools + WAL sentinel; repair; honest `creator_name_collision:` byte-exact match), unique partial index + SQLITE_UNIQUE mapping (TOCTOU fails honestly), `creator list` local visibility + degrade-with-warning, 64-byte name bounds (compound V1.176) |

### V1.178 additions

| Document | Description |
| --- | --- |
| [engineering/sqlx-migration-checksum-immutability.md](engineering/sqlx-migration-checksum-immutability.md) | sqlx migration files are immutable once shipped — `migrate!` embeds bytes + `_sqlx_migrations` stores the checksum; any in-place edit (even a comment typo fix) → `VersionMismatch` at boot for every existing install while fresh installs stay green; correct prose errors in the spec corpus with known-immutable notes, keep plans' writable sets excluding `migrations/`, diff-scope assertion `git diff --name-only … \| grep migrations` (V1.178 AR-106 distilled; compound follow-up) |
