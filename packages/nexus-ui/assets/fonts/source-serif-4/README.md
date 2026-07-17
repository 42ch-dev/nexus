# Source Serif 4 — canonical font provenance

Canonical OFL binaries for the DESIGN.md v0.4 display typography tier
(`typography.font-display`, plan `2026-07-17-v1.121-design-language-foundation` T3).

Product surfaces consume these fonts **only** via vendored copies in their own
`public/fonts/source-serif-4/` directories — apps must not import font binaries
from package source (`packages/nexus-ui/AGENTS.md` asset policy; spec T1 concern 1).
This directory is provenance + regeneration source of truth; nothing here is
served at runtime.

## Source

- **Family:** Source Serif 4 — static weights **400 Regular** + **600 Semibold**, normal style
- **Acquisition:** npm `@fontsource/source-serif-4@5.2.9` (pre-subset woff2; upstream: Adobe Source Serif 4 via <https://github.com/google/fonts>, fontsource mirror `font-files/fonts/google/source-serif-4`)
- **License:** SIL Open Font License 1.1 (OFL-1.1, attribution: Google Inc.) — see `OFL.txt` alongside
- **Format:** woff2 only (no woff fallback shipped; all supported browsers read woff2)

## Subset parameters

- **Subset:** `latin` (fontsource default subset) — Latin + common punctuation + figures
- **unicode-range:** `U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD` (declared in `tooling/design-tokens/src/tokens.css` `@font-face`)
- **Axes:** none (static instances; no `opsz`/`wght` variable axes, no italic)

## Integrity + bundle-delta gate (≤ 80 KB gzipped/weight — PASS)

| File | SHA-256 | Raw bytes | `gzip -9` bytes |
|------|---------|-----------|-----------------|
| `source-serif-4-latin-400-normal.woff2` | `02194deb92d3975dd30e11a3824a1f1db32b48c93654e60560cb81ce8e7b5f95` | 20,088 | 20,154 (~19.7 KiB) |
| `source-serif-4-latin-600-normal.woff2` | `f2b7e1cf1d277b7608231868135648f8ad8e2b58d8e97ca088bee15dc357bee7` | 21,532 | 21,598 (~21.1 KiB) |

(woff2 is pre-compressed; gzip size ≈ raw size. Both weights land far under the
80 KB gate, so the self-hosted path ships — no system-serif fallback needed.)

## Regeneration / copy step

1. `npm pack @fontsource/source-serif-4@5.2.9` (or fetch the tarball from the npm registry).
2. Extract; copy `package/files/source-serif-4-latin-{400,600}-normal.woff2` into this directory.
3. Copy the same two files into `apps/web/public/fonts/source-serif-4/` and `apps/design-studio/public/fonts/source-serif-4/`.
4. Verify `shasum -a 256` matches the table above; re-measure the gzip gate.
5. Bump the pinned version in this README if the acquisition version changes.

## Storage notes

- The canonical binaries here are **Git LFS**-tracked (matches the
  `assets/logos/*.png` provenance precedent).
- App-vendored copies under `apps/*/public/fonts/` are plain git on purpose:
  Vite serves `public/` verbatim, and an LFS-pointer checkout must never break
  a dev server. At ~20 KB/weight the object-store cost is negligible.
