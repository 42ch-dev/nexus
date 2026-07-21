# P1 Spec — Transport-error UX sweep across the app

> **Iteration:** `.mstar/iterations/v1.129/delivery-compass.md`
> **Status:** product-reviewed, architect-locked, writing-hygiene done (2026-07-21)
> **Plan:** `.mstar/plans/2026-07-21-v1.129-p1-transport-error-ux.md`
> **Depends on:** P0 (`profile-create-reliability.md`) — reuses `TransportErrorKind` and the per-kind copy table.
> **SSOT:** `.mstar/status.json`

## Problem statement (user value)

**Symptom a manual tester recognizes:** After (or before) Profile create is fixed, opening the app with a stopped daemon or a stale remote URL still shows the **same multi-cause paragraph** on the resume gate, the desktop launch gate, Connection settings, and random mutation toasts. The author learns nothing new at each surface and has no consistent next step.

P0 fixes create and defines the classified language. P1 makes that language the **only** transport-failure voice in the app so the author does not re-learn a different error UI at every gate.

| Surface (user-facing) | What the tester sees today |
|----------------------|----------------------------|
| Resume / identity check gate | Full-page error; description is often the generic blob |
| Desktop daemon launch gate | Error state with partial i18n; weak or missing Retry |
| Settings → Connection | Inline verify/pin failures reuse the blob |
| Any failed save/create toast | Entire blob dumped as the toast body |
| Desktop shell (Tauri) | Same blob when the throw path is shared |

## Scope (in)

- **One language, every surface:** FingerprintGate, DaemonLaunchGate, Connection settings error region, and mutation toasts all consume `NexusClientError.kind` (from P0) and show the same kind-matched headline family + recovery CTAs as the P0 dialog table (shared copy source — not six independent paraphrases).
- **Shared presentational block:** `<TransportErrorBlock>` (Studio-first → promote to `@42ch/nexus-ui` → thin app wrapper). Author-visible: consistent layout for kind + CTAs in light and dark.
- **CTA behavior the author can trust:** Retry = caller callback; Open Connection Settings = `/settings/advanced#connection`; Use Desktop App = honest informational copy only (no fake launch).
- **Toast:** short classified headline + body when `kind` is present — never the full multi-cause blob as the default description.
- **Desktop parity:** Tauri path shows the same kinds/copy when transport fails.

## Scope (out)

- **Create-dialog implementation** — owned by P0; P1 reuses kinds/copy, does not rework footer create.
- **Re-pinning remote fingerprints** — Connection settings already owns that flow; deep-link only.
- **Replacing the transport layer** (`fetch` / `TauriClient`) — classify only.
- **`R-V192SEC-001` OS-keychain TLS pinning** — deferred desktop hardening.
- **Per-endpoint retry / backoff / circuit breaker** — one author-driven Retry, not automated resilience.
- **HTTP 4xx/5xx API error redesign** — already via API error envelope; P1 touches transport-unreachable only.
- **Visual redesign of gates beyond error block swap** — no FingerprintGate / LaunchGate IA restyle.

## Interfaces

### `<TransportErrorBlock>` — locked API (Seat 2)

```tsx
import type { TransportErrorKind } from '@/lib/nexus/browser-client';

export interface TransportErrorBlockProps {
  kind: TransportErrorKind;
  onRetry?: () => void;        // omit to hide Retry
  onOpenSettings?: () => void; // omit to hide "Open Connection Settings"
  /** Optional caller-supplied detail (e.g., the daemon's last detail string). */
  detail?: string;
  /** Optional title override (rare; defaults to per-kind headline). */
  title?: string;
}
```

**Boundaries (locked):**
- **Presentational only** — no daemon calls, no routing, no product state, no `react-i18next` import. Caller composes surrounding behavior and passes copy via props.
- **No `size`/`variant` prop** — one variant; caller composes with surrounding layout (full-page vs inline vs toast). This keeps the package primitive simple and avoids a style-matrix explosion.
- Lives in `packages/nexus-ui` after Studio acceptance (T1 → T2 → app wiring).
- Copy strings come from the P0 dialog table — shared source, not independent paraphrases. P0 and P1 use the same locale keys.
- **CTA behavior:** `onRetry` = caller callback; `onOpenSettings` = caller calls `navigate('/settings/advanced#connection')`; Use Desktop App = informational copy only (no button CTA, just the body text instructing author to switch to desktop app).
- **Toast adaptation:** compact rendering (headline in title slot, body in description slot); CTAs omitted (toast action slot not yet supported by the current `useToast` API — document as a limitation in T5 report if adding action-slot support is not feasible).

### Promote-in-iteration decision (locked Seat 2)

**Product preference: promote inside P1; do NOT ship `@web-*` first.** Rationale: P1 is the first consumer of the primitive and V1.122-style drift (tokens in `design-tokens`, implementation in `apps/web`, no `nexus-ui` representation, no Studio fixture) must not be repeated.

Workflow (locked): T1 Studio fixture → T2 promote to `@42ch/nexus-ui` + promotion-list entry per `packages/nexus-ui/AGENTS.md` → T3-T6 app wiring from the promoted package.

The promotion-list entry in `packages/nexus-ui/AGENTS.md` must record: component name (`TransportErrorBlock`), plan/spec reference, and date. See [Promotion rules](https://github.com/42ch/nexus/blob/main/packages/nexus-ui/AGENTS.md).

### `NexusClientError` extension (locked by P0)

See P0 spec § Interfaces. P1 is a pure consumer.

### `useErrorToast` upgrade

```ts
function useErrorToast() {
  // …
  return (error: unknown, key: string) => {
    const kind = error instanceof NexusClientError ? error.kind : undefined;
    const headline = t(key, { defaultValue: key });
    const body = kind ? t(`transport.${kind}.toastBody`) : errorMessage(error);
    toast({ variant: 'error', title: headline, description: body, kind });
  };
}
```

(Exact shape locks in T2; principle: headline + short body + optional CTA via the toast's action slot if it exists.)

## Acceptance criteria

- **AC-V1129-P1-1 (FingerprintGate):** Stale remote URL or stopped daemon on resume. **Pass:** gate shows kind-matched headline + Retry + Open Connection Settings; no generic blob. **Fail:** blob description or missing CTA.
- **AC-V1129-P1-2 (DaemonLaunchGate):** Desktop daemon-stuck / daemon-error. **Pass:** same transport error block pattern; **Reset Local Database** still available as secondary; Retry present. **Fail:** blob-only or Reset DB removed.
- **AC-V1129-P1-3 (Toasts):** Any mutation with daemon stopped (e.g. create Work). **Pass:** toast title/body are short classified strings; generic multi-cause paragraph is not the default description. **Fail:** full blob in toast.
- **AC-V1129-P1-4 (Studio-first):** Studio fixture shows all six kinds, light + dark, CTA matrix visible. **Pass:** gallery review before App wiring claims done. **Fail:** App-only implementation with no Studio fixture.
- **AC-V1129-P1-5 (Promotion):** Primitive in `@42ch/nexus-ui` with promotion-list entry. *Needs architect lock* if Seat 2 chooses `@web-*` ship-first inside the same iteration — product preference remains promote-in-P1 to avoid V1.122-style drift.
- **AC-V1129-P1-6 (No regression):** `client-context`, `daemon-launch-gate`, `use-toast`, `settings-connection-section` tests pass with updated copy assertions; non-transport errors still show their full useful message.

## Test strategy

- **Studio fixture:** visual review checklist (light + dark, all six kinds, CTA visibility matrix).
- **App unit/integration:** each surface gets a test feeding each relevant kind; assert copy + CTA visible + click leads to right action.
- **Toast:** `use-toast.test.tsx` extended to cover classified body; ensure legacy non-transport errors still render their full message.

## Risks / open questions (architect Seat 2 — locked)

1. ~~Should `<TransportErrorBlock>` accept a `size`/`variant` (toast vs full-page ErrorState vs inline)?~~ **Locked: no.** One variant; caller composes with surrounding layout. See § Interfaces for rationale.
2. ~~Does promotion to `@42ch/nexus-ui` block P1 ship, or can we ship with a `@web-*` app-local wrapper first and promote in the same iteration?~~ **Locked: promote inside P1.** See § Promote-in-iteration decision above. Product preference + V1.122 anti-pattern avoidance.
3. ~~Toast CTA (action slot) support — does the current `useToast` shape support a primary action?~~ **Locked: T5 implementer investigates.** If the toast API supports an action slot, add CTAs. If not, document the limitation in the T5 task report and keep CTAs on full-page/inline error blocks only — toast gets headline + body, CTAs omitted. Do NOT expand scope to add action-slot support to the toast component itself.

## References

- Root cause: P0 spec (`profile-create-reliability.md`) § Root-cause hypothesis
- Root `AGENTS.md` UI Component Policy (Studio-first; promotion rules)
- Surfaces:
  - `apps/web/src/lib/client-context.tsx:99-163` (`FingerprintGate`)
  - `apps/web/src/components/setup/daemon-launch-gate.tsx`
  - `apps/web/src/pages/settings/settings-connection-section.tsx`
  - `apps/web/src/api/queries.ts:249-261` (`useErrorToast`)
  - `apps/web/src/lib/use-toast.ts` (toast shape)
- Studio rules: `apps/design-studio/AGENTS.md`
- Promotion rules: `packages/nexus-ui/AGENTS.md`
