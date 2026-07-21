/**
 * Caller-owned copy resolver for `<TransportErrorBlock>` (V1.129 P1, QC1-F-001).
 *
 * The promoted primitive in `@42ch/nexus-ui` owns the per-kind CTA matrix but
 * does NOT import `react-i18next` — it renders English fallback copy by
 * default. App surfaces must pass localized `t()` strings so `zh-CN` authors
 * see fully localized transport-failure UX. This helper resolves the four
 * override props (`title`, `body`, `primaryCtaLabel`, `secondaryCtaLabel`)
 * for a given kind from the existing `shell:profile.createError.<kind>.*`
 * and `common:transportCta.*` locale keys added in V1.129 P0.
 *
 * The CTA matrix mirrors the presentation matrix owned by the primitive
 * (spec § Dialog UX contract — locked). A parallel implementation exists in
 * `components/layout/footer-profiles.tsx` for the Create-Creator dialog;
 * consolidating the two is a follow-up (out of this fix's scope).
 */
import type { TransportErrorKind } from './errors';

/** Locked CTA set (mirrors `@42ch/nexus-ui` `TransportCtaKind`). */
type TransportCtaKind = 'retry' | 'openConnectionSettings' | 'useDesktopApp';

function primaryCtaFor(kind: TransportErrorKind): TransportCtaKind {
  switch (kind) {
    case 'network':
      return 'openConnectionSettings';
    case 'tls':
      return 'useDesktopApp';
    // daemon_down, http_fallback, timeout, unknown
    default:
      return 'retry';
  }
}

function secondaryCtaFor(kind: TransportErrorKind): TransportCtaKind | null {
  switch (kind) {
    case 'daemon_down':
    case 'http_fallback':
      return null;
    case 'network':
      return 'retry';
    case 'tls':
    case 'timeout':
    case 'unknown':
      return 'openConnectionSettings';
    default:
      return null;
  }
}

export interface TransportErrorCopy {
  /** Caller-owned headline (primitive `title` prop). */
  title: string;
  /** Caller-owned body (primitive `body` prop). */
  body: string;
  /** Caller-owned primary CTA label (primitive `primaryCtaLabel` prop). */
  primaryCtaLabel: string;
  /** Caller-owned secondary CTA label, or `undefined` when no secondary. */
  secondaryCtaLabel: string | undefined;
}

/**
 * Resolve the four caller-owned copy override props for a transport-error kind.
 *
 * @param shellT  Bound `t` for the `shell` namespace (headline + body).
 * @param commonT Bound `t` for the `common` namespace (CTA labels).
 * @param kind    Transport-failure sub-classification.
 */
export function transportErrorCopyFor(
  kind: TransportErrorKind,
  shellT: (key: string) => string,
  commonT: (key: string) => string,
): TransportErrorCopy {
  const secondary = secondaryCtaFor(kind);
  return {
    title: shellT(`profile.createError.${kind}.headline`),
    body: shellT(`profile.createError.${kind}.body`),
    primaryCtaLabel: commonT(`transportCta.${primaryCtaFor(kind)}`),
    secondaryCtaLabel: secondary ? commonT(`transportCta.${secondary}`) : undefined,
  };
}
