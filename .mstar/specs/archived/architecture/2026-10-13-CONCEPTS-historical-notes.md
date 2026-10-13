# CONCEPTS — Historical Notes

**Provenance**: frozen from [`CONCEPTS.md`](../../../../CONCEPTS.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed V1.90/V1.170/v1.192/v1.193 supersession wording; freeze date used).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 69

```markdown
Subordinate **execution context** paired with an `ActorRef` — logically `{world_id, optional binding_id/branch_id/event_id}` (it does not repeat an actor id; Character execution requires the binding id, Creator omits it) — describing *from where* an [Actor](#actor) acts or reads within a session. Viewpoint is not an identity, not an Actor kind, and not the name of any Character↔World association. The earlier same-day Viewpoint-as-identity direction (2026-09-04) is superseded by the Actor product lock ([actor-product-model.md](../../active/architecture/actor-product-model.md) §0).
```

### at pre-freeze line 197

```markdown
### Daemon Runtime (historical)
**Historical term — no live process.** The integrated Rust host (`nexus-daemon-runtime`, started with `nexus42 daemon start`) managed the World KB SQLite stores, scheduled quality/knowledge loops, served the `/v1/daemon/*` HTTP surface, and coordinated with the agent host. **v1.193 P2 deleted it**: the crate, the whole `nexus42 daemon` group, hidden `daemon-run`, and the `web-embed` embedded-SPA bytes are gone, with no replacement service launcher. Current owners of the retained contracts are the standalone TypeScript service (`apps/nexus-service`) for `/v1/daemon/*`, Electron for the desktop and its service lifecycle, and `nexus-runtime` for the independent Connect host. The retained historical record is [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md).

```

### at pre-freeze line 201

```markdown
The **retained wire family** — the HTTP surface reachable under `/v1/daemon/*` (previously `/v1/local/*`, renamed in V1.90), exposing world/knowledge, creator, orchestration, and manuscript endpoints to the CLI, web SPA, and desktop shell. It is **not** a live daemon process: browser/Electron clients reach it through the standalone TypeScript service (`apps/nexus-service`) over `nexus-core`, while retained CLI leaves call core directly — the URL prefix does not imply a Rust daemon host. By default it binds to loopback; a non-loopback bind (fail-closed) requires `--allow-remote` plus `--tls-cert`/`--tls-key` (off-loopback TLS is mandatory) with an API key (`NEXUS42_DAEMON_API_KEY`). `NEXUS_DAEMON_REMOTE_BIND` is **not** this service's gate — it governs the core Connect peer-tools lane only (`crates/nexus-core/src/connect/accept.rs`). Conventions: [daemon-api-surface-conventions.md](../../active/runtime/daemon-api-surface-conventions.md); host boundary: [rust-core-service-boundary.md](../../active/architecture/rust-core-service-boundary.md).
```

### at pre-freeze line 207

```markdown
The first-launch flow (Entrance → Agent → Workspace → Done since V1.170 P1; historically welcome + workspace → daemon ready → ACP agent detection → done) gated by a `setup_completed` marker in `~/.nexus42/config.toml`. The Entrance step persists the [Entrance](#entrance) through the desktop Electron IPC (`set_entrance` — desktop-only: the browser build skips the step) before completion. Triggers again if the marker is cleared. Every app launch (not only first) verifies the daemon is running before entering the main UI.
```

### at pre-freeze line 230

```markdown
The Electron desktop client (`apps/desktop-electron`) — the **shipped desktop host** since the v1.192 cutover (the former native desktop composition is retired and is not a supported concurrent runtime). It wraps the web SPA (`apps/web/dist`) in a native window, forks the standalone TypeScript service (`apps/nexus-service`) as the runtime serving `/v1/daemon/*` and owns that service's lifecycle, and adds OS-level capabilities (Open with…, Reveal in Finder, Copy Path, active-Creator switch, entrance/setup persistence) behind a typed preload bridge (`window.nexusDesktop.invoke`). The web bundle detects desktop mode from that bridge alone and selects `DesktopClient` + `ElectronDesktopCapabilities` over `BrowserClient`; browser mode degrades honestly — it never performs a fake desktop switch. Distribution is an unsigned macOS app/DMG for both architectures; GUI, Gatekeeper, and installed-deployment qualification remain unverified. Spec: [desktop-shell.md](../../active/surfaces/desktop-shell.md).
```

### at pre-freeze line 297

```markdown
| Daemon Runtime (historical) | Local Database, Agent Host, Daemon API | [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) |
```
