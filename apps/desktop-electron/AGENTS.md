# desktop-electron (P3-T2 feasibility shell)

Private, non-published Electron proof shell (`com.nexus42.rft-electron-proof` / **Nexus RFT Feasibility**). It wraps the unchanged `apps/web/dist` static artifact via the secure custom `nexus-proof:` scheme and hosts the sole native owner in an Electron utility process.

## Entrypoints

| Script | Purpose |
|---|---|
| `pnpm run build` | Compile `src/*.ts` → `dist/` |
| `pnpm run proof:dev` | Development launch (requires `NEXUS_PROOF_HOME`, built `apps/web/dist`, native payload) |
| `pnpm run package -- --arch arm64\|x64` | Pin Electron 44.3.0 / packager 20.3.0 packaging |
| `pnpm run proof:package -- --arch … --signed-required --out …` | P3-T3 packaged verification driver |
| `pnpm run proof:native-smoke` | P2 compatibility smoke only (not the GUI shell) |

## Security / ownership invariants

- Renderer: `sandbox=true`, `contextIsolation=true`, `nodeIntegration=false`, narrow `window.nexusProof` preload API.
- Native `.node` loads only in the utility process; never in renderer or asar without unpack.
- IPC whitelist: `compatibility`, `open`, `graph`, `patch`, `provider`, `pull`, `close` with 1 MiB / 256 KiB bounds.
- `NEXUS_PROOF_HOME` is main-process input; renderer cannot supply home/principal claims.
- Not a product release channel; no P4 BrowserClient routing or full-app parity claim.
