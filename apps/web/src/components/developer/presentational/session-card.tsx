/**
 * Presentational session card for the `/sessions` rail (plan
 * v221-caps-sessions-rail T2).
 *
 * The product page (`components/developer/sessions-page.tsx`) owns the query,
 * the i18n lookups and the router `Link`; this module owns the card markup so
 * the Design Studio fixture can mount it from static view models with literal
 * copy. The whole card is the link: `renderLink` wraps the frame, so a card has
 * exactly one focus stop and `HubCard`'s `go` slot stays unused (no nested link).
 *
 * Boundary: no transport, no router, no i18n, no formatting. The caller
 * resolves the short ids, the humanized status label and the pill variant from
 * the `SessionSummary` contract.
 */
import type { ReactNode } from 'react';

// Relative (not `@/`) so the Design Studio gallery resolves it through the
// `@web-developer/session-card` alias; Studio's `@` points at its own src.
import { HubCard } from '../hub-card';
import type { BadgeProps } from '@/components/ui/badge';

/** One session reduced to the fields the card renders. */
export interface SessionCardView {
  /** Full contract id — carried in the run-view href. */
  sessionId: string;
  /** `shortId(session_id)` — the visible title. */
  shortId: string;
  /** `humanizeStatus(status)` + `statusVariant(status)` — unchanged mapping. */
  status: { label: string; variant: BadgeProps['variant'] };
  /** `shortId(preset_id)`. */
  preset: string;
  /** `shortId(creator_id)`. */
  creator: string;
}

export interface SessionCardCopy {
  /** `Open run view for session {{id}}` with the short id. */
  openRunAria: (shortId: string) => string;
  presetLabel: string;
  creatorLabel: string;
}

export interface SessionCardLinkArgs {
  /** `/sessions/<encodeURIComponent(session_id)>` — the run view for this card. */
  href: string;
  ariaLabel: string;
  children: ReactNode;
}

/** Caller-owned link element (`<Link>` in the app, `<a>` in the gallery). */
export type SessionCardLinkRenderer = (args: SessionCardLinkArgs) => ReactNode;

export interface SessionCardChromeProps {
  card: SessionCardView;
  copy: SessionCardCopy;
  renderLink: SessionCardLinkRenderer;
}

export function SessionCardChrome({ card, copy, renderLink }: SessionCardChromeProps) {
  return renderLink({
    href: `/sessions/${encodeURIComponent(card.sessionId)}`,
    ariaLabel: copy.openRunAria(card.shortId),
    children: (
      <HubCard
        id={`session-${card.sessionId}`}
        title={<span className="text-copy-13-mono">{card.shortId}</span>}
        status={card.status}
      >
        <Fact label={copy.presetLabel} value={card.preset} />
        <Fact label={copy.creatorLabel} value={card.creator} />
      </HubCard>
    ),
  });
}

/** The card grid — shared by the page and the fixture so the list layout has one home. */
export function SessionCardGrid({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">{children}</div>;
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-copy-13 text-gray-700">{label}</span>
      <span className="text-copy-13-mono text-gray-900">{value}</span>
    </div>
  );
}
