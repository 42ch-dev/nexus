/**
 * Studio fixtures for shared conflict-modal chrome (V1.124 P2 Task 3b).
 *
 * One shared shell via `@web-canvas/conflict-modal-chrome` — not three parallel
 * Strategy/Outline/WorldKB modal redraws. Domain wrappers stay App adapters.
 *
 * Boundary: no RF, no daemon, no contracts, no `useTranslation`.
 */
import { useState, type ReactNode } from 'react';

import {
  ConflictModalChrome,
  type ConflictField,
  type ConflictReviewRow,
} from '@web-canvas/conflict-modal-chrome'; // @web-canvas/conflict-modal-chrome - transitional until package promotion criteria met

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

const NON_OVERLAP_SERVER: ConflictField[] = [
  { id: 'label', label: 'Label', serverValue: 'Crossing' },
  { id: 'description', label: 'Description', serverValue: 'Server summary' },
];

const NON_OVERLAP_LOCAL: ConflictField[] = [
  { id: 'nextTarget', label: 'Target', localValue: 'epilogue' },
];

const OVERLAP_SERVER: ConflictField[] = [
  { id: 'label', label: 'Label', serverValue: 'Crossing (server)' },
  { id: 'description', label: 'Description', serverValue: 'Server body' },
];

const OVERLAP_LOCAL: ConflictField[] = [
  { id: 'label', label: 'Label', localValue: 'Crossing (local)' },
  { id: 'nextTarget', label: 'Target', localValue: 'climax' },
];

const REVIEW_ROWS: ConflictReviewRow[] = [
  {
    label: 'Label',
    server: 'Crossing (server)',
    draft: 'Crossing (local)',
    changed: true,
  },
  {
    label: 'Description',
    server: 'Server body',
    draft: 'Local body',
    changed: true,
  },
  {
    label: 'Target',
    server: 'midpoint',
    draft: 'climax',
    changed: true,
  },
];

function InlineModalHost({
  children,
}: {
  children: (api: {
    open: boolean;
    setOpen: (v: boolean) => void;
  }) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative min-h-[120px]">
      <button
        type="button"
        className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
        onClick={() => setOpen(true)}
      >
        Open conflict modal
      </button>
      {children({ open, setOpen })}
    </div>
  );
}

function ResolvePathFrame() {
  return (
    <FixtureFrame
      title="Conflict modal — resolve path (no overlap)"
      description="Server and local touch different fields — Reapply stays enabled. Use current / Keep editing / Review actions visible."
      testId="conflict-modal-fixture-resolve"
    >
      <InlineModalHost>
        {({ open, setOpen }) => (
          <ConflictModalChrome
            open={open}
            title="This state changed while you were editing"
            currentRevision={14}
            revisionLabel="Server is at revision"
            defaultDescription="Choose how to resolve the conflict."
            serverSectionTitle="Server version"
            localSectionTitle="Your edit"
            serverChanges={NON_OVERLAP_SERVER}
            localChanges={NON_OVERLAP_LOCAL}
            reviewRows={REVIEW_ROWS}
            onUseCurrent={() => setOpen(false)}
            onReapply={() => setOpen(false)}
            onDismiss={() => setOpen(false)}
            useCurrentLabel="Use current"
            reapplyLabel="Reapply"
            keepEditingLabel="Keep editing"
            reviewLabel="Review side-by-side"
          />
        )}
      </InlineModalHost>
    </FixtureFrame>
  );
}

function OverlapPathFrame() {
  return (
    <FixtureFrame
      title="Conflict modal — overlap (reapply disabled)"
      description="Server and local both touch Label — Reapply is disabled. Primary action remains Use current."
      testId="conflict-modal-fixture-overlap"
    >
      <InlineModalHost>
        {({ open, setOpen }) => (
          <ConflictModalChrome
            open={open}
            title="This state changed while you were editing"
            currentRevision={22}
            revisionLabel="Server is at revision"
            defaultDescription="Choose how to resolve the conflict."
            serverSectionTitle="Server version"
            localSectionTitle="Your edit"
            serverChanges={OVERLAP_SERVER}
            localChanges={OVERLAP_LOCAL}
            reviewRows={REVIEW_ROWS}
            onUseCurrent={() => setOpen(false)}
            onReapply={() => setOpen(false)}
            onDismiss={() => setOpen(false)}
            useCurrentLabel="Use current"
            reapplyLabel="Reapply"
            keepEditingLabel="Keep editing"
            reviewLabel="Review side-by-side"
          />
        )}
      </InlineModalHost>
    </FixtureFrame>
  );
}

function AlwaysOpenPreview() {
  return (
    <FixtureFrame
      title="Conflict modal — open preview (gallery)"
      description="Always-open shell for light/dark visual acceptance without clicking. Positioned relatively inside the frame (not full-viewport) via a scaled host."
      testId="conflict-modal-fixture-open-preview"
    >
      <div
        className="relative h-[520px] overflow-hidden rounded-card border border-gray-alpha-200 bg-background-200"
        data-testid="conflict-modal-open-preview-host"
      >
        <div className="absolute inset-0 [&_[data-testid=conflict-modal-chrome]]:absolute [&_[data-testid=conflict-modal-chrome]]:inset-0">
          <ConflictModalChrome
            open
            title="This state changed while you were editing"
            currentRevision={9}
            revisionLabel="Server is at revision"
            defaultDescription="Choose how to resolve the conflict."
            serverSectionTitle="Server version"
            localSectionTitle="Your edit"
            serverChanges={OVERLAP_SERVER}
            localChanges={OVERLAP_LOCAL}
            reviewRows={REVIEW_ROWS}
            onUseCurrent={() => {}}
            onReapply={() => {}}
            onDismiss={() => {}}
            useCurrentLabel="Use current"
            reapplyLabel="Reapply"
            keepEditingLabel="Keep editing"
            reviewLabel="Review side-by-side"
          />
        </div>
      </div>
    </FixtureFrame>
  );
}

/**
 * Conflict-modal family fixtures — one shared chrome, resolve + overlap paths.
 */
export function ConflictModalFixtures() {
  return (
    <div data-testid="conflict-modal-fixtures">
      <AlwaysOpenPreview />
      <ResolvePathFrame />
      <OverlapPathFrame />
    </div>
  );
}
