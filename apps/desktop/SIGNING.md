# Desktop Code Signing

The Nexus desktop shell is a Tauri v2 application. By default it builds
**unsigned**. Code signing is opt-in and gated by environment so that local
development and CI do not require a certificate.

## Local signed build

Set the Apple Developer ID application identity and run the CLI wrapper:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: 42ch Inc (TEAMID)"
nexus42 desktop bundle
```

Or pass the identity explicitly:

```bash
nexus42 desktop bundle --sign-identity "Developer ID Application: 42ch Inc (TEAMID)"
```

`nexus42 desktop bundle` reads `--sign-identity` first, then falls back to the
`APPLE_SIGNING_IDENTITY` environment variable. If neither is set, Tauri produces
an unsigned `.app` bundle and `.dmg`.

## Unsigned build

Omit both the flag and the environment variable:

```bash
nexus42 desktop bundle
```

Tauri will produce an unsigned `.app` bundle and `.dmg`. This is the default for
local development and PR builds.

## CI / release signing secrets

The `.github/workflows/desktop-release.yml` release workflow can sign, notarize,
and staple the macOS `.dmg` when all five secrets are present:

| Secret | Purpose | How to obtain |
|--------|---------|---------------|
| `APPLE_SIGN_IDENTITY` | SHA-1 hash or exact name of the **Developer ID Application** certificate installed in the runner keychain. | Apple Developer Portal → Certificates, Identifiers & Profiles → create/export a Developer ID Application certificate. |
| `APPLE_SIGN_CERT_P12_BASE64` | Base64-encoded `.p12` export of the same Developer ID Application certificate. | Export the private key + certificate from Keychain Access as `.p12`, then `base64 -i cert.p12`. |
| `APPLE_ID` | Apple ID email used for notarization. | Use the Apple ID account associated with the team. |
| `APPLE_PWD` | App-specific password for the Apple ID. | Apple ID account security → App-Specific Passwords. |
| `APPLE_TEAM_ID` | Apple Developer Team ID. | Apple Developer Portal → Membership details. |

### Secret-presence behavior

| Secrets present | Result |
|-----------------|--------|
| None | Unsigned `.dmg` + `.app.zip` are built and uploaded. A notice is emitted. |
| Partial (1–4 of 5) | The workflow **fails after uploading unsigned artifacts** with a clear message listing the missing secrets. |
| All five | A temporary keychain is created, the `.p12` is imported, the `.app` is codesigned with the hardened runtime, the `.dmg` is rebuilt from the signed `.app`, submitted to Apple Notary Service, stapled, and uploaded. |

### CI signing flow

When all five secrets are present, `desktop-release.yml` performs:

1. **Keychain import**: create a uniquely named temporary keychain, base64-decode
   `APPLE_SIGN_CERT_P12_BASE64` into a `.p12`, import it, unlock the keychain,
   and add it to the user search list.
2. **Codesign**: sign the `.app` bundle with the hardened runtime:
   ```bash
   codesign --force --sign "$APPLE_SIGN_IDENTITY" --options runtime --timestamp --deep <app-bundle>
   ```
3. **Rebuild DMG**: the unsigned DMG produced by Tauri is replaced with a new
   DMG built from the signed `.app`.
4. **Notarize**: submit the DMG to Apple and wait for approval:
   ```bash
   xcrun notarytool submit <dmg> --apple-id "$APPLE_ID" --password "$APPLE_PWD" --team-id "$APPLE_TEAM_ID" --wait
   ```
5. **Staple**: staple the notarization ticket to the DMG:
   ```bash
   xcrun stapler staple <dmg>
   ```
6. **Cleanup**: restore the original keychain search list and delete the
   temporary keychain and `.p12`.

Do not hard-code identity strings in workflow files, Tauri config, or source.
The single source of truth for the signing identity is the runtime environment.
