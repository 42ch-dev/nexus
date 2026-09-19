# Unsigned Packaging Evidence Guide

This guide is the evidence checklist for the Nexus Electron unsigned distribution
lane. It records package provenance without claiming signing, notarization,
Gatekeeper trust, GUI parity, or installed-app behavior.

## Required artifact inventory

For each native architecture, retain the complete directory:

```text
artifacts/desktop/<version>/darwin-arm64/
  Nexus.app/
  Nexus-<version>-darwin-arm64-unsigned.dmg
  Nexus-<version>-darwin-arm64-unsigned.app.zip
  SHA256SUMS
  receipt.json
artifacts/desktop/<version>/darwin-x64/
  Nexus.app/
  Nexus-<version>-darwin-x64-unsigned.dmg
  Nexus-<version>-darwin-x64-unsigned.app.zip
  SHA256SUMS
  receipt.json
```

The raw `.app` is required for inspection; the ZIP is only transport. A missing
artifact, receipt, or checksum is a failed CI row, not an incomplete success.

## Evidence capture

1. Record the exact source revision and dirty state from each `receipt.json`.
2. Retain each receipt's lockfile, web-dist, service, native payload, native
   compatibility, and app-file-manifest digests.
3. Run the read-only verifier:

   ```sh
   node apps/desktop-electron/scripts/verify-package.mjs \
     --dir artifacts/desktop/<version>/darwin-<arch>
   ```

4. Retain verifier output with the downloaded artifact set. It checks identity,
   minimum macOS (`13.0`), native Mach-O headers, app-manifest digests, ZIP/DMG
   SHA-256 records, unpacked native closure, and compiled host security policy.
5. CI runs PATH sentinels around packaging and records zero calls to forbidden
   signing dispatch. Ordinary packaging uses no Apple credentials.

## Architecture and confidence matrix

| Evidence | arm64 | x64 |
|---|---|---|
| Native runner and `process.arch` match | CI receipt/log | **[UNVERIFIED] — CI-only (macos-15-intel)**; CI receipt/log |
| `Nexus.app` + DMG + ZIP + receipt + checksums | CI artifact set | **[UNVERIFIED] — CI-only (macos-15-intel)**; CI artifact set |
| Read-only verifier | CI verifier output | **[UNVERIFIED] — CI-only (macos-15-intel)**; CI verifier output |
| No signing dispatch sentinel calls | CI sentinel log | **[UNVERIFIED] — CI-only (macos-15-intel)**; CI sentinel log |
| macOS 13 runtime execution | **[UNVERIFIED]** unless separately tested | **[UNVERIFIED]** unless separately tested |
| GUI parity / installed-app behavior | **[UNVERIFIED]** | **[UNVERIFIED]** |
| Gatekeeper trust / notarization | Not claimed | Not claimed |

When only one architecture is available locally, list the other row as
**[UNVERIFIED]**. Never substitute Rosetta, a cross-architecture build, a
universal/fat package, or a filename check for native evidence.

## Honest interpretation

`signing_performed:false` means this product pipeline did not apply a Developer ID
or ad-hoc signature and did not notarize or staple the artifact. An upstream
Electron/vendor Mach-O signature may remain; the receipt records that metadata
without stripping or modifying it. Verifier success proves package structure and
recorded inputs only, not GUI qualification or a successful launch on every
supported macOS release.
