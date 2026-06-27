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

## Unsigned build

Omit both the flag and the environment variable:

```bash
nexus42 desktop bundle
```

Tauri will produce an unsigned `.app` bundle and `.dmg`. This is the default for
local development and PR builds.

## CI gating

GitHub Actions workflows use the `APPLE_SIGNING_IDENTITY` repository secret:

- `desktop-build.yml` sets the secret as an environment variable only when it is
  present, producing a signed bundle on protected branches.
- `desktop-release.yml` requires the secret; release jobs fail closed if it is
  missing so an unsigned artifact cannot ship from a release tag.

Do not hard-code identity strings in workflow files, Tauri config, or source.
The single source of truth for the signing identity is the runtime environment.

## Notarization

Notarization is out of scope for V1.71. When it is added, it will be gated by
additional secrets (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) in the
release workflow only.
