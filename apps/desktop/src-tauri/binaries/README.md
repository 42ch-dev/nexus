# `apps/desktop/src-tauri/binaries/`

**P1 `nexus42` sidecar binaries** (compass §5 #2 — `bundle.externalBin` +
`@tauri-apps/plugin-shell` Sidecar).

This directory contains the target-triple-suffixed `nexus42` binaries that the
Tauri desktop shell bundles and lifecycle-manages:

```
binaries/
  nexus42-aarch64-apple-darwin
  nexus42-x86_64-apple-darwin
```

The binaries themselves are **not committed** (`/binaries/*` is gitignored);
they are produced by:

```bash
pnpm -w run sidecar
# or directly:
bash scripts/fetch-sidecar.sh
```

which builds `cargo build --release -p nexus42` for the configured target(s) and
copies/renames the artifacts here. By default V1.66 produces `aarch64-apple-darwin`;
use `SIDECAR_TARGETS=...` to build `x86_64-apple-darwin` or a universal pair.

`tauri.conf.json` declares:

```jsonc
"bundle": {
  "externalBin": ["binaries/nexus42"]
}
```

Tauri resolves the correct arch suffix at bundle time. See
`.mstar/knowledge/specs/desktop-shell.md` §7 (sidecar lifecycle).
