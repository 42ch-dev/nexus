# STRATEGY — Historical Notes

**Provenance**: frozen from [`STRATEGY.md`](../../../../STRATEGY.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed V1.64–V1.168 iteration-history table and v1.192–v1.193 supersession section; freeze date used).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 156

```markdown
### Superseded by the delivered architecture (v1.192–v1.193)
```

### at pre-freeze line 158

```markdown
The rows above are that history, not the current architecture. v1.192 delivered the Electron desktop cutover and v1.193 retired the integrated local host; the current owners are the Electron desktop host, the direct-core `nexus42` CLI cohort, the standalone TypeScript service as the HTTP host for browser/Electron, and the independent `nexus-runtime` Connect host ([rust-core-service-boundary.md](../../active/architecture/rust-core-service-boundary.md)).

| Historical entry | Superseded by |
|------------------|---------------|
| `nexus42` binary hosts both CLI and daemon (V1.68) | v1.193 P2 retired the integrated daemon group and its public names — no `nexus42d`, no `nexus42 service` alias, no CLI service launcher. The CLI is the direct-core `cli` cohort; `nexus-runtime` is the independent Connect host. |
| Web SPA (`apps/web`) as daemon-served Control Room UI (V1.64) | The SPA is served by the first-party hosts — the standalone TypeScript service for browser/Electron (v1.193). `/v1/daemon/*` stays retained wire vocabulary, not a live daemon process. |
| Desktop shell (`apps/desktop`) via a native host (V1.66) | v1.192 delivered the Electron host `apps/desktop-electron` and retired the native composition; exactly one desktop host ships. |
| Local API Trust-Boundary Hardening (V1.86) | Historical host framing — the retired daemon host. The origin-admission discipline is re-owned by the current hosts: the TS service's `checkOrigin` allowlist and Electron's exact-origin CSP and IPC sender checks. |
| Rows naming daemon HTTP routes, `nexus-daemon-runtime`, the Daemon API or the daemon-served SPA — e.g. V1.92, V1.148, V1.151, V1.152, V1.162, V1.163, V1.165, V1.166, and PD-11's "CLI+Daemon API" | Records of the retired host. Retained families now call the Rust authority directly, or through the standalone TS service for browser/Electron; Connect serves third-party integrators. PD-11's three consumption ends stand — only its developer-surface naming changed. |
| "First-class Actor product model" row — "**v1.185** developer maintenance (edit/archive, WorldSheet, KE content, `--remember`) is **not shipped**" | A record-time statement: v1.185 shipped that developer maintenance loop (PR #241). Durable authority: [actor-product-model.md](../../active/architecture/actor-product-model.md) §11. |

Retained uses of *daemon* in the rows above are wire and configuration vocabulary — `/v1/daemon/*` paths, `NEXUS_DAEMON_ALLOWED_ORIGINS`, the desktop host's service-status vocabulary — and never a launch route for a retired Rust daemon.
```
