# desktop-electron (unsigned Nexus desktop product)

This package is the Electron desktop host for Nexus. It produces native-architecture
unsigned macOS artifacts only: no credentials, signing, notarization, stapling,
auto-update, or public-release lane is part of this package.

## Entrypoints

| Script | Purpose |
|---|---|
| `pnpm run build` | Compile the Electron host and preload into `dist/`; does not package or sign |
| `pnpm run package -- --arch arm64\|x64` | Build one native-architecture `.app`, `.dmg`, app ZIP, receipt, and checksums under `artifacts/desktop/<version>/darwin-<arch>/` |
| `node scripts/verify-package.mjs --dir artifacts/desktop/<version>/darwin-<arch>` | Read-only receipt, archive, app-manifest, Mach-O, native-closure, and compiled-host-policy verification |
| `pnpm run icons` | Compose the product ICNS from the approved Nexus logo source |

The root aliases `pnpm build:desktop -- --arch <arch>` to the same package driver.
Package prerequisites are built explicitly by CI or the maintainer before packaging;
the package driver fails closed when compiled inputs or the frozen dependency
closure are absent. The driver accepts only `--arch`, `--out`, and `--help`.

## Product and runtime invariants

- Product identity is `Nexus` / `io.nexus42.desktop`; version comes from the root
  `package.json` and must match the Electron manifest and receipt.
- Package jobs run natively on macOS: arm64 uses `macos-15`, x64 uses
  `macos-15-intel`. Rosetta and universal/fat builds are not substitutes.
- Renderer policy remains `sandbox=true`, `contextIsolation=true`,
  `nodeIntegration=false`, and `webSecurity=true`; native `.node` dependencies
  stay in `app.asar.unpacked` and load only from the utility-process owner.
- The receipt records `signing_performed:false` and
  `notarization_performed:false`. Vendor signature metadata is reported honestly
  and is not stripped or modified.
- The verifier does not claim GUI qualification, Gatekeeper trust, installed-app
  behavior, or macOS 13 execution. Those require separately scoped evidence.

## Local verification

Run the verifier against a published architecture directory after packaging:

```sh
node apps/desktop-electron/scripts/verify-package.mjs \
  --dir artifacts/desktop/<version>/darwin-<native-arch>
```

The CI workflow adds PATH sentinels around the real packaging command and fails if
any signing-related dispatch is attempted. No Apple credentials are configured.
