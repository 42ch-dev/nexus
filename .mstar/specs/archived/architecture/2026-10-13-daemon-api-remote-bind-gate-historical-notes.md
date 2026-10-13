# daemon api remote bind gate — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/daemon-api-remote-bind-gate.md`](../../../knowledge/architecture-patterns/daemon-api-remote-bind-gate.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 49

```markdown
## Historical (retired Rust daemon boot gate, deleted v1.193 P2)
```

### at pre-freeze line 51

```markdown
The original V1.90 gate lived in the Rust daemon: a pure function `ensure_remote_bind_allowed(host)` checked `NEXUS42_DAEMON_API_KEY` **and** `NEXUS_DAEMON_REMOTE_BIND=1` immediately before `TcpListener::bind` and refused a non-loopback bind otherwise. That HTTP-service path is gone. The **same** env-var pairing survives only in the core **Connect peer-tools lane**, `crates/nexus-core/src/connect/accept.rs:204-228` (`ensure_remote_bind_allowed`): a non-loopback peer-tools bind is permitted only when both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1` are set. It does **not** gate the Daemon API HTTP surface.
```

### at pre-freeze line 83

```markdown
- Spec (historical, V1.90; superseded): `.mstar/specs/archived/runtime/2026-09-21-daemon-runtime.md`
```
