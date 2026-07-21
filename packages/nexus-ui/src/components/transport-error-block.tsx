import { forwardRef, type HTMLAttributes } from 'react';
import { AlertCircle } from 'lucide-react';

import { Button } from './button';
import { cn } from '../lib/cn';

/**
 * Transport-failure sub-classification mirror of the apps/web
 * {@code TransportErrorKind} (V1.129 P0 — `apps/web/src/lib/nexus/errors.ts`).
 *
 * The package cannot import from `apps/web`, so the literal-union is mirrored
 * here. TypeScript checks structural compatibility at the call site — both
 * unions carry the same six literals, so an apps/web {@code TransportErrorKind}
 * value is assignable to this prop without a cast.
 *
 * Future hardening (out of V1.129 P1 scope): promote the type into a shared
 * non-wire location so the mirror is unnecessary.
 */
export type TransportErrorKind =
  /** TCP refuse, DNS failure, offline, or undifferentiated fetch throw. */
  | 'network'
  /** Certificate failure (best-effort). Browsers hide the precise reason. */
  | 'tls'
  /** Explicit abort / timeout (AbortError, AbortSignal.timeout). */
  | 'timeout'
  /** Daemon returned `text/html` with HTTP 200 — release-mode SPA fallback. */
  | 'http_fallback'
  /** Local-mode daemon is not running (browser-tab client with empty baseUrl). */
  | 'daemon_down'
  /** No classifier matched; recovery copy stays generic. */
  | 'unknown';

/**
 * Locked CTA set (spec § Interfaces · Dialog UX contract).
 *
 * - `retry` → caller-supplied `onRetry`
 * - `openConnectionSettings` → caller-supplied `onOpenSettings`
 * - `useDesktopApp` → informational only (no callback — body copy carries the
 *   instruction). Locked by the P0 dialog table; the primitive honors the
 *   matrix without inventing a new CTA kind.
 */
type TransportCtaKind = 'retry' | 'openConnectionSettings' | 'useDesktopApp';

/**
 * CTA visibility matrix per spec § Dialog UX contract.
 *
 * Every kind surfaces a primary CTA. The secondary is omitted (`undefined`)
 * when the spec table lists only one CTA for that kind. Final visibility also
 * depends on whether the caller supplied the matching callback — see
 * {@link isCtaVisible}.
 */
const PRIMARY_CTA: Record<TransportErrorKind, TransportCtaKind> = {
  daemon_down: 'retry',
  http_fallback: 'retry',
  network: 'openConnectionSettings',
  tls: 'useDesktopApp',
  timeout: 'retry',
  unknown: 'retry',
};

const SECONDARY_CTA: Partial<Record<TransportErrorKind, TransportCtaKind>> = {
  network: 'retry',
  tls: 'openConnectionSettings',
  timeout: 'openConnectionSettings',
  unknown: 'openConnectionSettings',
};

/**
 * Built-in English copy table (V1.129 ship baseline).
 *
 * Boundary (locked): the primitive does NOT import `react-i18next`. App
 * surfaces override `title` for locale-specific headlines when needed; the
 * body and CTA labels stay package-owned for V1.129 ship. Localization of
 * body/CTA copy is a follow-up — extend with `body?` / `ctaLabel?` props or
 * extract to an i18n-free constants module when needed. The V1.129 P0 spec
 * locked these strings as the shared single source (mirrors the
 * `profile.createError.<kind>.*` keys in `apps/web/src/locales/{en,zh-CN}/
 * shell.json`).
 */
const HEADLINE_COPY: Record<TransportErrorKind, string> = {
  daemon_down: 'Local daemon is not running',
  network: 'Could not connect to the daemon at this address',
  tls: 'This browser rejected the daemon certificate',
  http_fallback: 'The app could not complete this request',
  timeout: 'The daemon took too long to respond',
  unknown: 'Could not reach the daemon',
};

const BODY_COPY: Record<TransportErrorKind, string> = {
  daemon_down: 'Start it with `nexus42 daemon start`, then try again.',
  network:
    'Check the URL and port in Connection settings, or confirm the network can reach that host.',
  tls:
    'The web app cannot trust a remote self-signed certificate. The Nexus desktop app can store trust in the OS keychain.',
  http_fallback:
    'The daemon answered with a page instead of an API response. Retry once; if it keeps happening, check the daemon status.',
  timeout: 'The connection stalled or the daemon is busy. Retry in a moment.',
  unknown:
    'Something went wrong before a response arrived. Retry, or check Connection settings if it continues.',
};

const CTA_LABEL: Record<TransportCtaKind, string> = {
  retry: 'Retry',
  openConnectionSettings: 'Open Connection Settings',
  useDesktopApp: 'Use Desktop App',
};

export interface TransportErrorBlockProps
  extends Omit<HTMLAttributes<HTMLDivElement>, 'title'> {
  /** Transport-failure sub-classification; drives copy + CTA matrix. */
  kind: TransportErrorKind;
  /** Retry callback. Omit to hide the Retry CTA. */
  onRetry?: () => void;
  /** "Open Connection Settings" callback. Omit to hide that CTA. */
  onOpenSettings?: () => void;
  /**
   * Optional caller-supplied detail line (e.g., the daemon's last detail
   * string). Rendered below the body copy when present.
   */
  detail?: string;
  /** Optional headline override (rare; defaults to per-kind headline). */
  title?: string;
}

/**
 * `<TransportErrorBlock>` — locked presentational primitive for transport-
 * failure UX (V1.129 P1).
 *
 * Renders the per-kind headline + body + CTA matrix. The CTA visibility and
 * ordering are owned by the primitive (driven by `kind`); the callbacks are
 * supplied by the caller. One variant; caller composes with surrounding
 * layout (full-page ErrorState vs inline region vs toast adaptation).
 *
 * CTA visibility rule (spec lock — "omit to hide Retry" / "omit to hide Open
 * Connection Settings"): a CTA renders iff (a) the matrix lists it for the
 * given `kind` AND (b) the caller supplied the matching callback. The
 * exception is `useDesktopApp`, which is informational and always renders
 * when the matrix lists it. So a caller that supplies neither callback gets
 * headline + body only (the toast case); a caller that supplies both gets
 * the full matrix.
 *
 * Boundary: presentational only — no daemon calls, no routing, no product
 * state, no `react-i18next` import. See `transport-error-ux.md` § Interfaces.
 *
 * @example
 * <TransportErrorBlock
 *   kind="daemon_down"
 *   onRetry={() => verify()}
 *   onOpenSettings={() => navigate('/settings/advanced#connection')}
 * />
 */
export const TransportErrorBlock = forwardRef<HTMLDivElement, TransportErrorBlockProps>(
  function TransportErrorBlock(
    { kind, onRetry, onOpenSettings, detail, title, className, ...rest },
    ref,
  ) {
    const headline = title ?? HEADLINE_COPY[kind];
    const body = BODY_COPY[kind];
    const primary = PRIMARY_CTA[kind];
    const secondary = SECONDARY_CTA[kind];

    const primaryVisible = isCtaVisible(primary, onRetry, onOpenSettings);
    const secondaryVisible = secondary
      ? isCtaVisible(secondary, onRetry, onOpenSettings)
      : false;

    return (
      <section
        ref={ref}
        role="alert"
        data-testid="transport-error-block"
        data-kind={kind}
        className={cn(
          'rounded-control border border-red-300 bg-red-50 p-3 dark:border-red-800 dark:bg-red-950',
          className,
        )}
        {...rest}
      >
        <div className="flex items-start gap-3">
          <AlertCircle
            className="mt-0.5 h-5 w-5 flex-shrink-0 text-red-700 dark:text-red-400"
            aria-hidden
          />
          <div className="min-w-0 flex-1 space-y-1">
            <p className="text-copy-14 font-medium text-red-900 dark:text-red-100">{headline}</p>
            <p className="text-copy-13 text-red-800 dark:text-red-200">{body}</p>
            {detail ? (
              <p className="break-words text-copy-13 text-red-800/80 dark:text-red-200/80">
                {detail}
              </p>
            ) : null}
          </div>
        </div>
        {primaryVisible || secondaryVisible ? (
          <div className="mt-3 flex flex-wrap gap-2">
            {primaryVisible && (
              <Button
                type="button"
                variant="primary"
                // `useDesktopApp` is informational only (spec lock) — the
                // button renders so the matrix is visually complete, but no
                // callback is wired. The body copy carries the instruction.
                onClick={ctaOnClick(primary, onRetry, onOpenSettings)}
                data-testid="transport-error-primary"
                data-cta={primary}
              >
                {CTA_LABEL[primary]}
              </Button>
            )}
            {secondaryVisible && secondary && (
              <Button
                type="button"
                variant="tertiary"
                onClick={ctaOnClick(secondary, onRetry, onOpenSettings)}
                data-testid="transport-error-secondary"
                data-cta={secondary}
              >
                {CTA_LABEL[secondary]}
              </Button>
            )}
          </div>
        ) : null}
      </section>
    );
  },
);

/**
 * Resolve a CTA's `onClick`. Returns `undefined` for `useDesktopApp`
 * (informational only — the button renders but does nothing).
 */
function ctaOnClick(
  cta: TransportCtaKind,
  onRetry: (() => void) | undefined,
  onOpenSettings: (() => void) | undefined,
): (() => void) | undefined {
  if (cta === 'retry') return onRetry;
  if (cta === 'openConnectionSettings') return onOpenSettings;
  return undefined;
}

/**
 * A CTA is visible iff (a) the matrix lists it AND (b) the caller supplied
 * the matching callback — except `useDesktopApp`, which is informational and
 * always visible when listed.
 */
function isCtaVisible(
  cta: TransportCtaKind,
  onRetry: (() => void) | undefined,
  onOpenSettings: (() => void) | undefined,
): boolean {
  if (cta === 'useDesktopApp') return true;
  if (cta === 'retry') return Boolean(onRetry);
  return Boolean(onOpenSettings);
}
