# Desktop Shell — Native-Host Retirement and Bootstrap History Historical Notes

**Provenance**: frozen from [`specs/active/surfaces/desktop-shell.md`](../../active/surfaces/desktop-shell.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 completed the host transition begun by the v1.192 cutover (RFT-09/10/11) and v1.194 P1 closed Row18; the retired native-host/bootstrap-era narration frozen here spans those events. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (15 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Classification:** Master. **Status:** Electron is the shipped desktop host: the v1.192 cutover (RFT-09) is accepted, unsigned packaging is delivered (RFT-10), and the replaced native composition is retired (RFT-11), leaving exactly one desktop host. This is **not a claim that dual-architecture GUI qualification, Gatekeeper trust or installed-deployment behavior has been verified** — those rows remain **[UNVERIFIED]** (§12). This in-place revision replaces native-host-specific normative hosting, preserves the shipped setup/product behavior, and leaves historical evidence unchanged.
```

### at pre-freeze line ~11

```markdown
Bundle id `io.nexus42.desktop`; product/app name `Nexus`. Product version comes from root `package.json.version`, not an iteration label or independent staging literal. `apps/desktop-electron/resources/product.json` owns id/name. Existing brand icon composition is retained in Electron resources without the retired native CLI.
```

### at pre-freeze line ~19

```markdown
- The native product tree/toolchain/CI retirement landed in v1.192, after accepted host parity and unsigned packaging. **Delivered (v1.193 P2):** the integrated legacy daemon + embedded SPA composition and the dormant CLI rows were retired in that iteration — the whole `nexus42 daemon` group, hidden `daemon-run`, the `web-embed` SPA bytes and the thin `DaemonClient` leaves are deleted, with no replacement service launcher. **No public operator name survives:** the desktop host and the standalone TS service own the service lifecycle, and the CLI is a one-shot direct-core/cloud/Connect product ([rust-core-service-boundary.md](../../active/architecture/rust-core-service-boundary.md) §7.2, [cli-spec.md](../../active/cli/cli-spec.md) §6.3).
```

### at pre-freeze line ~25

```markdown
`pnpm build:desktop -- --arch arm64|x64` delegates to the package-scoped unsigned driver. `nexus42 desktop bundle` delegates to that same entry; the command name is preserved. No second Rust implementation of packaging.
```

### at pre-freeze line ~49

```markdown
`DesktopClient` replaces the legacy-host-named thin subclass; inherited BrowserClient methods stay the one HTTP implementation. Runtime detection requires a valid versioned preload; no build-env-only desktop selection or retained legacy host-global branch. Web/browser mode and same-origin `/v1/daemon/*` remain supported independently of desktop.
```

### at pre-freeze line ~51

```markdown
Main stores connection configuration with Electron safeStorage encryption, atomic replacement and owner-only file permissions at product userData. No new plaintext-write fallback. A one-time import can read the old `nexus42`/`connection_config` macOS keychain entry or old app-data JSON on main only; **the accepted v1.194 P1 contract makes that migration ordered**: validate → encrypt → successful encrypted persist (atomic, decrypt-checked) → delete both legacy sources → publish the migration. Nothing is deleted after a parse/URL/encrypt/persist failure, and the plaintext originals stay readable until the encrypted bytes are readable. A cleanup refusal is non-fatal: it is reported as the sanitized structured `legacy_credential_cleanup_failed`, observable as `ConnectionStore.legacyCleanupFailure`, and leaves both the encrypted store and any remaining legacy source intact — the next successful open retries the same idempotent cleanup. Once selected, encrypted storage is sole authority; a durable non-secret cleared marker prevents re-import after deletion, and a cleared install neither imports nor deletes legacy material. An absent store (ENOENT) is distinguished from an invalid/unreadable existing one: the latter neither activates nor triggers legacy import/overwrite or cleanup, and keeps its bytes on disk for recovery. Unavailable encryption or corrupt state errors rather than silently wiping or retaining a fake success.
```

### at pre-freeze line ~67

```markdown
**Accepted v1.194 P1 endpoint contract (delivered).** One shared primitive — `connectionEndpointOrigin(endpointUrl)` in browser-safe `desktop-contract.ts` — is the single endpoint grammar: the existing URL byte cap and control-byte discipline, then an explicit `http`/`https` authority with a concrete non-wildcard host (DNS, IPv4, bracketed IPv6 and an explicit port stay supported), no userinfo, no raw `?`/`#`/`@`/padding/backslash-bearing input, no query or fragment, and no non-root path (a single trailing slash is accepted). It returns the parsed exact origin and the caller keeps the **original string** — stored endpoint identity is never normalized. Four boundaries consume it with no bypass: the IPC payload validator, the CSP origin assertion in `protocol.ts` (which delegates to the primitive), `ConnectionStore` (direct `set`, persisted read, legacy import, and `getAuth`, which fails closed) and Electron main (`activeServiceOrigin()` derives only through it, falling back to the validated app-managed local origin). An opaque or host-less URL is rejected as `invalid_input` instead of yielding `"null"`. This is the **remote Nexus service endpoint**, not the provider/model API URL field: arbitrary provider API paths, websocket URLs and prefix-mounted reverse proxies are not part of the supported contract. Proof is headless at the real contract/store/host/network seams — no live provider, browser or Electron GUI qualification is implied.
```

### at pre-freeze line ~73

```markdown
App launch always starts or attaches irrespective of `setup_completed`. Desktop local port precedence: explicit launch value → valid `NEXUS_DAEMON_PORT` →8420. Standalone service's own default8421 is unchanged. Invalid explicit port errors. Selected remote HTTP connection does not grant local controls authority over remote processes.
```

### at pre-freeze line ~164

```markdown
At the reconciled revision, three observed mismatches were recorded as **fix-required, not accepted deviations**. Row18 is now closed by the accepted v1.194 P1 delivery (§5/§6); the remaining two still stand as written:
```

### at pre-freeze line ~166

```markdown
1. **Row18 — durable clear: closed by the accepted v1.194 P1 delivery.** The reconciled-revision observation (deletion followed by a legacy re-import on the next `ConnectionStore.open()`) no longer holds: `clear` writes the §5 durable non-secret tombstone, and the store tests now cover the tombstone, the no-re-import reopen, the invalid/unreadable versus ENOENT distinction and the cleanup refusal/retry paths. Redaction/no-plaintext tests still cannot substitute for real macOS encryption runtime.
```

### at pre-freeze line ~183

```markdown
### 13.3–13.9 Superseded bootstrap assumptions

Earlier Welcome/Daemon-numbered-step and bootstrap-before-daemon designs are no longer normative. Keep their product outcomes through the current flow, not their native startup mechanism. No implicit data deletion when settings re-runs setup.

No automatic agent installation/download/update, broader profile CRUD, execution-mode matrix or new provider capability is introduced by desktop replacement. The Settings IA is unchanged by the host replacement: the shipped V1.103 settings shell implemented by the existing `apps/web` settings routes and presentational components stays the authority, with [web-ui.md](../../active/surfaces/web-ui.md) as the durable product authority for the authoring UI (the per-section implement notes were iteration-plan material and are deliberately not linked from a tracked doc).
```
