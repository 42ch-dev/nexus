# apps/web — AGENTS.md

The local-first **Control Room + Setup** Web UI. Daemon-served React SPA,
Tauri-ready. Parent rules: [`../../AGENTS.md`](../../AGENTS.md) (repo),
[`../../.mstar/AGENTS.md`](../../.mstar/AGENTS.md) (harness).

## Identity & placement

- This is the **OSS local Web UI** (`apps/web`), a pnpm workspace member under
  `apps/*`. It is **not** the private `nexus-platform` cloud SaaS — do not
  introduce cloud/platform-gated features, platform auth, or platform-only types
  here (spec invariant: `web-ui.md` §2.2).
- Consumes `@42ch/nexus-contracts` via `workspace:*`. **Never** hand-write a
  second source of wire DTO types in this package — all wire types come from the
  generated contracts (web-ui.md §12.6).

## SSOT & authority

- **Design tokens**: [`DESIGN.md`](./DESIGN.md) is the SSOT (authored by
  `@architect`). `src/index.css` + `tailwind.config.ts` *consume* it; they do
  not invent tokens. If a token you need is missing from DESIGN.md, **report**
  it to the architect — do not fabricate a value.
- **Product contract**: [`web-ui.md`](../../.mstar/knowledge/specs/web-ui.md).
- **Transport boundary**: the `NexusClient` interface
  (`src/lib/nexus/types.ts`). Screens must depend only on the interface, never
  on `fetch`/`invoke` directly — that is what keeps the V1.65 Tauri shell a
  one-impl swap (web-ui.md §5, §9).

## Pending contracts alignment

This scaffold builds against the **V1.63 contract base**. The following land
with Track-B / plan P0 on the integration branch and unlock corresponding client
methods (deliberately omitted now to avoid handwritten wire shapes):

| Method | Blocked on | Note |
| --- | --- | --- |
| `listFindings` | `ListFindingsResponse` (F-P2 endpoint) | Findings screen stays a placeholder until then. |
| Works cursor list | F-P1 migration of `ListWorksResponse` | `listWorks` currently uses offset/limit `{ works, total }`. |
| Preset get/update/delete | No daemon routes / request types yet | Only list/scaffold/validate/reload are wired. |
| `getWork`/`patchWork` return | drift `R-V163-P1-T6` (handler emits `WorkApiDto`) | Typed against generated `WorkDetailResponse`; P0 makes reality match. |
| Error body parsing | shared `ErrorResponse` schema (F-E1) | `NexusClientError` is an app-side abstraction that already matches the planned `{ code, message, details? }` shape; tighten parsing when the generated type lands. |

PM re-verifies `pnpm --filter web typecheck` after the Wave-1 merge reconciles
these.

## Build / typecheck contract

- `build` and `typecheck` resolve `@42ch/nexus-contracts` types from its `dist/`.
  Build the contracts package first: `pnpm --filter @42ch/nexus-contracts run build`.
  CI's `web-build` job does this automatically.
- Workspace + lockfile surfaces touched here (`pnpm-workspace.yaml`, root
  `package.json`, lockfile) are shared with P0's codegen — coordinate at
  integration merge (compass §3 parallelism note).

## Conventions

- **TypeScript strict.** No `any` for wire shapes; prefer generated types.
- **Styling**: Tailwind utilities referencing DESIGN.md theme keys; compose with
  `cn()` (`src/lib/utils.ts`). Component primitives live in `src/components/ui/`
  and read from the DESIGN.md component tables.
- **Accessibility (WCAG 2.1 AA floor)**: keep keyboard paths, the global
  focus-visible ring (`src/index.css`), visible labels (no icon-only nav), and
  reduced-motion handling. DESIGN.md dark/light tokens must both pass contrast.
- **Voice & Content**: follow DESIGN.md §Voice & Content — Title Case for titles/
  nav/buttons/headers; sentence case for helpers/errors/toasts; Verb + Noun
  actions; name the changed object. Avoid protocol jargon (`ACP`, `cursor token`)
  in the UI surface.
- **Daemon port**: default HTTP transport `127.0.0.1:8420`
  (`crates/nexus-daemon-runtime/src/boot.rs`); override via `NEXUS_DAEMON_PORT`
  or `VITE_DAEMON_URL` (dev proxy).
