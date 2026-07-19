/**
 * Studio fixtures for Global Timeline list chrome (V1.124 P2 Task 2).
 *
 * Composes `@web-global-timeline/global-timeline-list-chrome` — the same
 * presentational extract App `GlobalTimelineView` uses after mapping hooks →
 * row props.
 *
 * Boundary: no `@xyflow/react`, no contracts, no daemon, no `useTranslation`.
 * Static English product vocabulary (World, Timeline, Brief, Narrative).
 */
import { type ReactNode } from 'react';

import {
  GlobalTimelineListChrome,
  type GlobalTimelineListRow,
} from '@web-global-timeline/global-timeline-list-chrome'; // @web-global-timeline/global-timeline-list-chrome - transitional until package promotion criteria met

/* ------------------------------------------------------------------ */
/*  Shared fixture frame                                                */
/* ------------------------------------------------------------------ */

function FixtureFrame({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

const POPULATED_ROWS: GlobalTimelineListRow[] = [
  {
    id: 'world-ashen',
    label: 'Ashen Gate Chronicles',
    activityText: 'Brief · 3 eras · 12 events',
    lastEditedText: 'Last edited 2 hours ago',
    layer: 'brief',
    href: '#',
  },
  {
    id: 'world-hearth',
    label: 'Hearthstone Cycle',
    activityText: 'Narrative · 0 eras · 8 events',
    lastEditedText: 'Last edited yesterday',
    layer: 'narrative',
    href: '#',
  },
  {
    id: 'world-crossing',
    label: 'The Crossing',
    activityText: 'Brief · 1 era · 4 events',
    lastEditedText: 'Last edited 3 days ago',
    layer: 'brief',
    href: '#',
  },
  {
    id: 'world-quiet',
    label: 'Quiet Accord',
    activityText: 'Narrative · 0 eras · 2 events',
    layer: 'narrative',
    href: '#',
  },
];

const SHARED_COPY = {
  title: 'Global Timeline',
  description:
    'Recent Timeline activity across your Worlds. Open a row to enter that World’s Timeline.',
  listAriaLabel: 'World Timeline activity',
  emptyTitle: 'No Worlds yet',
  emptyDescription:
    'Create a World to start tracking Timeline activity across your workspace.',
  loadingLabel: 'Loading Timeline activity…',
  errorDescription: 'Could not load Timeline activity. Try again.',
  retryLabel: 'Retry',
} as const;

function PopulatedFrame() {
  return (
    <FixtureFrame
      title="Global Timeline — populated"
      description="Cross-World Timeline activity list (≥3 rows). World name + layer/activity summary + last-edited. Click affordance is a plain link in Studio (App uses react-router Link)."
      testId="global-timeline-fixture-populated"
    >
      <GlobalTimelineListChrome
        {...SHARED_COPY}
        state="ready"
        rows={POPULATED_ROWS}
        data-testid="global-timeline-fixture-list"
      />
    </FixtureFrame>
  );
}

function EmptyFrame() {
  return (
    <FixtureFrame
      title="Global Timeline — empty"
      description="Empty-state Card when the workspace has no Worlds yet."
      testId="global-timeline-fixture-empty"
    >
      <GlobalTimelineListChrome
        {...SHARED_COPY}
        state="empty"
        rows={[]}
        data-testid="global-timeline-fixture-empty-list"
      />
    </FixtureFrame>
  );
}

function LoadingFrame() {
  return (
    <FixtureFrame
      title="Global Timeline — loading"
      description="Loading frame while Worlds list is pending (no daemon — static label)."
      testId="global-timeline-fixture-loading"
    >
      <GlobalTimelineListChrome
        {...SHARED_COPY}
        state="loading"
        rows={[]}
      />
    </FixtureFrame>
  );
}

function ErrorFrame() {
  return (
    <FixtureFrame
      title="Global Timeline — error"
      description="Error frame with Retry affordance (no-op in Studio)."
      testId="global-timeline-fixture-error"
    >
      <GlobalTimelineListChrome
        {...SHARED_COPY}
        state="error"
        rows={[]}
        onRetry={() => {}}
      />
    </FixtureFrame>
  );
}

/**
 * Global Timeline fixtures — populated (≥3 rows), empty, loading, error.
 * Presentational-only; light + dark via Studio theme toggle.
 */
export function GlobalTimelineFixtures() {
  return (
    <div data-testid="global-timeline-fixtures">
      <PopulatedFrame />
      <EmptyFrame />
      <LoadingFrame />
      <ErrorFrame />
    </div>
  );
}
