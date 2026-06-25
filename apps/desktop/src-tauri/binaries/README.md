# `apps/desktop/src-tauri/binaries/`

**Placeholder for the P1 `nexus42` sidecar** (compass §5 #2 — `bundle.externalBin` +
`@tauri-apps/plugin-shell` Sidecar).

V1.66 **P0** runs the desktop shell against an **externally-started** daemon
(`nexus42 daemon start --foreground`); no binary is bundled here and
`tauri.conf.json` intentionally omits `bundle.externalBin` so `tauri dev`/`build`
succeed without a sidecar present.

**P1** populates this directory with target-triple-suffixed binaries:

```
binaries/
  nexus42-aarch64-apple-darwin
  nexus42-x86_64-apple-darwin
```

and adds to `tauri.conf.json`:

```jsonc
"bundle": {
  "externalBin": ["binaries/nexus42"]
}
```

Tauri resolves the correct arch suffix at bundle time (`-universal-apple-darwin`
attempted first per compass §5 #10). See
`.mstar/knowledge/specs/desktop-shell.md` §7 (sidecar lifecycle).
