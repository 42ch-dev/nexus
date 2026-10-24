import { EmptyState } from '@42ch/nexus-ui';
import {
  SessionCardChrome,
  SessionCardGrid,
  type SessionCardCopy,
  type SessionCardLinkArgs,
  type SessionCardView,
} from '@web-developer/session-card';

/**
 * Studio fixture for the `/sessions` rail (plan v221-caps-sessions-rail T2).
 * Cards rendered from static view models with literal copy and a plain `<a>`
 * link renderer — no router, no QueryClient, no daemon. Two frames: the mixed
 * running/completed/failed grid and the zero-session empty state.
 */

/* ------------------------------------------------------------------ */
/*  Caller-owned copy (mirrors locales/en/sessions.json)               */
/* ------------------------------------------------------------------ */

const COPY: SessionCardCopy = {
  openRunAria: (id) => `Open run view for session ${id}`,
  presetLabel: 'Preset',
  creatorLabel: 'Creator',
};

/* ------------------------------------------------------------------ */
/*  Fake data                                                          */
/* ------------------------------------------------------------------ */

const MIXED: readonly SessionCardView[] = [
  {
    sessionId: 'sess-7f3a91c2d4e8',
    shortId: 'sess-7f3…d4e8',
    status: { label: 'Running', variant: 'running' },
    preset: 'novel-writing',
    creator: 'author-01',
  },
  {
    sessionId: 'sess-2b8c4a',
    shortId: 'sess-2b8c4a',
    status: { label: 'Completed', variant: 'running' },
    preset: 'world-build',
    creator: 'author-02',
  },
  {
    sessionId: 'sess-9d1e0f',
    shortId: 'sess-9d1e0f',
    status: { label: 'Failed', variant: 'error' },
    preset: 'outline-pass',
    creator: 'author-01',
  },
];

/** The gallery's link is a plain anchor; the app injects a router `<Link>`. */
function renderLink({ href, ariaLabel, children }: SessionCardLinkArgs) {
  return (
    <a
      href={href}
      aria-label={ariaLabel}
      className="block rounded-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
    >
      {children}
    </a>
  );
}

export function SessionsRailFixtures() {
  return (
    <div className="flex flex-col gap-10" data-testid="sessions-rail-fixtures">
      <section data-testid="sessions-rail-fixture-mixed">
        <p className="mb-3 text-label-12 uppercase tracking-wide text-gray-700">
          Mixed statuses — whole card opens the run view
        </p>
        <SessionCardGrid>
          {MIXED.map((card) => (
            <SessionCardChrome
              key={card.sessionId}
              card={card}
              copy={COPY}
              renderLink={renderLink}
            />
          ))}
        </SessionCardGrid>
      </section>

      <section data-testid="sessions-rail-fixture-empty">
        <p className="mb-3 text-label-12 uppercase tracking-wide text-gray-700">Zero sessions</p>
        <EmptyState
          title="No sessions yet"
          description="Sessions appear here when a preset run starts."
        />
      </section>
    </div>
  );
}
