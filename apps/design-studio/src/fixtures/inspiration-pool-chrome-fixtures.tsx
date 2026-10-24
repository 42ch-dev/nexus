/**
 * Studio fixtures for the Inspiration Pool chrome (v1.222 P1 T2).
 *
 * Composes `@web-works/inspiration-pool-chrome` — the same presentational
 * extract App `WorksInspirationPage` renders after mapping hooks → props.
 *
 * Boundary: no contracts, no daemon, no query client, no router, no
 * `useTranslation`. Static English copy mirroring `locales/en/works.json`, and
 * a plain anchor Work link where the App injects a router `Link`.
 *
 * Five frames: empty, populated list, loading, read error, and the add form
 * with a blank (invalid) title.
 */
import { useState, type ReactNode } from 'react';

import {
  InspirationPoolChrome,
  type InspirationPoolChromeState,
  type InspirationPoolCopy,
  type InspirationPoolRow,
} from '@web-works/inspiration-pool-chrome';

/* ------------------------------------------------------------------ */
/*  Shared fixture frame                                               */
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
      <h4 className="mb-1 text-heading-16 font-heading text-gray-1000">{title}</h4>
      <p className="mb-4 text-copy-13 text-gray-700">{description}</p>
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Caller-owned copy (mirrors locales/en/works.json)                  */
/* ------------------------------------------------------------------ */

const COPY: InspirationPoolCopy = {
  title: 'Inspiration Pool',
  description:
    'Ideas waiting to become Works. Add one, then promote it to a Work or archive it.',
  listAriaLabel: 'Inspiration Pool',
  loadingLabel: 'Loading inspiration…',
  emptyTitle: 'No inspiration yet',
  emptyDescription: 'Add a first idea to start the pool.',
  errorTitle: 'Could not load this view',
  errorDescription:
    'The daemon did not return the inspiration pool. Check that the local daemon is running.',
  retryLabel: 'Try again',
  addTitle: 'Add inspiration',
  titleLabel: 'Title',
  titlePlaceholder: 'One line that captures the idea.',
  submitLabel: 'Add',
  submittingLabel: 'Adding…',
  statusLabel: 'Status',
  addedLabel: 'Added',
  openWork: 'Open Work',
  promoteLabel: 'Promote to Work',
  promotingLabel: 'Promoting…',
  archiveLabel: 'Archive',
  archivingLabel: 'Archiving…',
};

/* ------------------------------------------------------------------ */
/*  Fake rows — one per known status                                   */
/* ------------------------------------------------------------------ */

const ROWS: readonly InspirationPoolRow[] = [
  {
    itemId: 'insp-7f3a91c2',
    title: 'The lighthouse keeper catalogues the ships it failed to save',
    status: 'idea',
    statusLabel: 'Idea',
    addedText: 'Oct 22, 2026',
  },
  {
    itemId: 'insp-2b8c4a',
    title: 'A cartographer who maps only the roads that disappeared',
    status: 'promoted',
    statusLabel: 'Promoted',
    addedText: 'Oct 20, 2026',
    workId: 'work-9d1e0f42',
  },
  {
    itemId: 'insp-9d1e0f',
    title: 'Two rival archivists trade a single unfinished sentence every winter',
    status: 'archived',
    statusLabel: 'Archived',
    addedText: 'Oct 18, 2026',
  },
];

/** Controlled form harness — the page owns this state, so does the fixture. */
function useAddFrame(initial = '') {
  const [addTitleValue, setAddTitleValue] = useState(initial);
  const [submitted, setSubmitted] = useState<string[]>([]);
  return {
    addTitleValue,
    setAddTitleValue,
    submitted,
    submit: (value: string) => setSubmitted((prev) => [...prev, value]),
  };
}

function ChromeFrame({
  state,
  rows,
  onRetry,
  addTitleValue,
  onAddTitleChange,
  onAdd,
}: {
  state: InspirationPoolChromeState;
  rows: readonly InspirationPoolRow[];
  onRetry?: () => void;
  addTitleValue: string;
  onAddTitleChange: (value: string) => void;
  onAdd: (title: string) => void;
}) {
  return (
    <InspirationPoolChrome
      copy={COPY}
      state={state}
      rows={[...rows]}
      total={rows.length}
      onRetry={onRetry}
      addTitleValue={addTitleValue}
      onAddTitleChange={onAddTitleChange}
      onAdd={onAdd}
      data-testid="inspiration-pool-chrome"
    />
  );
}

function EmptyFrame() {
  const form = useAddFrame();
  return (
    <FixtureFrame
      title="Inspiration Pool — empty"
      description="Zero pool items: the empty state wins over the list, and the add form stays available."
      testId="works-inspiration-fixture-empty"
    >
      <ChromeFrame
        state="empty"
        rows={[]}
        addTitleValue={form.addTitleValue}
        onAddTitleChange={form.setAddTitleValue}
        onAdd={form.submit}
      />
    </FixtureFrame>
  );
}

function ListFrame() {
  const form = useAddFrame();
  return (
    <FixtureFrame
      title="Inspiration Pool — populated"
      description="One row per stored status: an idea (Promote + Archive), a promoted item (Work link, no Promote) and an archived item (read-only)."
      testId="works-inspiration-fixture-list"
    >
      <ChromeFrame
        state="ready"
        rows={ROWS}
        addTitleValue={form.addTitleValue}
        onAddTitleChange={form.setAddTitleValue}
        onAdd={form.submit}
      />
    </FixtureFrame>
  );
}

function LoadingFrame() {
  const form = useAddFrame();
  return (
    <FixtureFrame
      title="Inspiration Pool — loading"
      description="First page of the creator-scoped pool still in flight (no daemon — static label)."
      testId="works-inspiration-fixture-loading"
    >
      <ChromeFrame
        state="loading"
        rows={[]}
        addTitleValue={form.addTitleValue}
        onAddTitleChange={form.setAddTitleValue}
        onAdd={form.submit}
      />
    </FixtureFrame>
  );
}

function ErrorFrame() {
  const form = useAddFrame();
  return (
    <FixtureFrame
      title="Inspiration Pool — read error"
      description="Pool read failure with the shared ErrorState retry affordance (no-op in Studio)."
      testId="works-inspiration-fixture-error"
    >
      <ChromeFrame
        state="error"
        rows={[]}
        onRetry={() => {}}
        addTitleValue={form.addTitleValue}
        onAddTitleChange={form.setAddTitleValue}
        onAdd={form.submit}
      />
    </FixtureFrame>
  );
}

function AddInvalidFrame() {
  const form = useAddFrame();
  return (
    <FixtureFrame
      title="Inspiration Pool — add form, blank title"
      description="Blank title keeps submit disabled; a typed title enables it and submits the trimmed value."
      testId="works-inspiration-fixture-add-invalid"
    >
      <ChromeFrame
        state="empty"
        rows={[]}
        addTitleValue={form.addTitleValue}
        onAddTitleChange={form.setAddTitleValue}
        onAdd={form.submit}
      />
      <p className="mt-3 text-copy-13 text-gray-700" data-testid="works-inspiration-fixture-submitted">
        {form.submitted.join(' | ')}
      </p>
    </FixtureFrame>
  );
}

/**
 * Inspiration Pool fixtures — empty, populated, loading, read error and the
 * blank-title add form.
 */
export function InspirationPoolChromeFixtures() {
  return (
    <div className="flex flex-col" data-testid="works-inspiration-fixtures">
      <EmptyFrame />
      <ListFrame />
      <LoadingFrame />
      <ErrorFrame />
      <AddInvalidFrame />
    </div>
  );
}
