import { type HTMLAttributes, type Ref } from 'react';
import { AlertCircle } from 'lucide-react';

import { cn } from '../lib/cn';

/**
 * Compact text-link CTA styling (V1.136 P2 — ErrorState-aligned; V1.137 L2 quiet link).
 * Both primary and secondary actions use this recipe — not filled `Button`.
 */
const CTA_LINK_CLASS =
  'text-label-12 font-normal text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:text-blue-700 dark:focus-visible:ring-blue-700 dark:hover:text-blue-800 dark:hover:opacity-100';

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
 * Boundary: the primitive does NOT import `react-i18next`. These constants
 * are the **fallback** copy — Studio (English-only) renders them directly.
 * App surfaces pass caller-owned localized copy via the `title?` / `body?` /
 * `primaryCtaLabel?` / `secondaryCtaLabel?` override props so `zh-CN` users
 * see fully localized copy (QC1-F-001). The defaults mirror the
 * `profile.createError.<kind>.*` keys in `apps/web/src/locales/{en,zh-CN}/
 * shell.json` and the `transportCta.*` keys in `common.json`.
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
  // `tls` has no callback-driven primary CTA — the desktop-app instruction
  // is informational and lives in the body copy (QC1-F-002: no no-op button).
  tls:
    'The web app cannot trust a remote self-signed certificate. Use the Nexus desktop app — it can store trust in the OS keychain.',
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
  /**
   * Optional caller-supplied body copy. Falls back to the package's per-kind
   * default (used by the English-only Studio fixture). App surfaces pass a
   * localized `t()` string here (QC1-F-001).
   */
  body?: string;
  /**
   * Optional caller-supplied primary CTA label. Falls back to the package's
   * per-kind default. App surfaces pass a localized `t()` string here.
   */
  primaryCtaLabel?: string;
  /**
   * Optional caller-supplied secondary CTA label. Falls back to the package's
   * per-kind default. App surfaces pass a localized `t()` string here.
   * `undefined` is equivalent to "use the default".
   */
  secondaryCtaLabel?: string;
  /** DOM ref forwarded to the underlying section (React 19 ref-as-prop). */
  ref?: Ref<HTMLDivElement>;
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
 * Connection Settings"): a callback-driven CTA renders iff (a) the matrix
 * lists it for the given `kind` AND (b) the caller supplied the matching
 * callback. The `useDesktopApp` CTA is **informational** — it never renders
 * as a button; the body copy carries the desktop-app instruction (QC1-F-002).
 * So a caller that supplies neither callback gets headline + body only (the
 * toast case); a caller that supplies the matching callbacks gets the full
 * matrix.
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
export function TransportErrorBlock({
  kind,
  onRetry,
  onOpenSettings,
  detail,
  title,
  body,
  primaryCtaLabel,
  secondaryCtaLabel,
  className,
  ref,
  ...rest
}: TransportErrorBlockProps) {
  const headline = title ?? HEADLINE_COPY[kind];
    const bodyText = body ?? BODY_COPY[kind];
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
            <p className="text-copy-13 text-red-800 dark:text-red-200">{bodyText}</p>
            {detail ? (
              <p className="break-words text-copy-13 text-red-800/80 dark:text-red-200/80">
                {detail}
              </p>
            ) : null}
          </div>
        </div>
        {primaryVisible || secondaryVisible ? (
          <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
            {primaryVisible && (
              <button
                type="button"
                className={CTA_LINK_CLASS}
                onClick={ctaOnClick(primary, onRetry, onOpenSettings)}
                data-testid="transport-error-primary"
                data-cta={primary}
              >
                {primaryCtaLabel ?? CTA_LABEL[primary]}
              </button>
            )}
            {secondaryVisible && secondary && (
              <button
                type="button"
                className={CTA_LINK_CLASS}
                onClick={ctaOnClick(secondary, onRetry, onOpenSettings)}
                data-testid="transport-error-secondary"
                data-cta={secondary}
              >
                {secondaryCtaLabel ?? CTA_LABEL[secondary]}
              </button>
            )}
          </div>
        ) : null}
      </section>
  );
}

/**
 * Resolve a callback-driven CTA's `onClick`. `useDesktopApp` has no callback
 * (informational) and is never rendered as a button — see {@link isCtaVisible}.
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
 * A callback-driven CTA is visible iff (a) the matrix lists it AND (b) the
 * caller supplied the matching callback. `useDesktopApp` is informational —
 * it never renders as a button (QC1-F-002: a keyboard-focusable no-op button
 * is misleading and inaccessible). The desktop-app instruction is carried by
 * the body copy instead. The CTA stays in {@link PRIMARY_CTA} so the matrix
 * records the kind's recovery intent.
 */
function isCtaVisible(
  cta: TransportCtaKind,
  onRetry: (() => void) | undefined,
  onOpenSettings: (() => void) | undefined,
): boolean {
  if (cta === 'useDesktopApp') return false;
  if (cta === 'retry') return Boolean(onRetry);
  return Boolean(onOpenSettings);
}
