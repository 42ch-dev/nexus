import { AlertTriangle, ArrowLeft, RefreshCw } from 'lucide-react';
import type { ReactNode } from 'react';

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@42ch/nexus-ui';
import { ErrorState, LoadingState } from '@web-ui/states'; // transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)

/**
 * Studio fixture for the session **run view** (`apps/web/src/pages/session-run-view.tsx`,
 * v1.201 P1-T3 — the acceptance-bearing observation surface of the plan).
 *
 * The run view is coupled to product state (`useRunObservation`) and to the
 * daemon events route, so it is out of the Studio import tiers: this fixture
 * composes the same chrome, copy, and token-backed recipes as a Studio-local
 * replica, variant by variant, so the surface can be tuned and reviewed here
 * before any App wiring claim (root `AGENTS.md` §UI Component Policy —
 * studio-first). Every state the view can render is on screen:
 *
 *  - `running` — the live tail appending under the `Live` heading;
 *  - `gap` — the inline "Events were missed" marker between the frames that
 *    surround it, with the `Gap` badge and the reconnect affordance;
 *  - `replay` — the replayed span above the live tail, each under its own
 *    heading, plus the disclosure note that the split is inferred from attach
 *    order (the wire carries no replay/live marker);
 *  - `history-unavailable` — the lost-history notice above the frames the view
 *    had already retained, with the badge stating the loss;
 *  - `refusal` — the typed refusal (absent/foreign/child run id) as an error
 *    block with no retry, because a refusal is deterministic;
 *  - `transport-error` — the same block for an exhausted transport episode,
 *    which does offer the reconnect;
 *  - `loading` — the pre-frame state (no events yet: the stream is still
 *    connecting), which is also the empty state of this surface.
 *
 * Theme: single theme-following specimens. Every class is token-backed, so the
 * Surfaces pair view renders this fixture in light and dark without a second
 * specimen tree, and no ancestor `.dark` wrapper is hard-coded here.
 *
 * Boundary: imports promoted primitives from `@42ch/nexus-ui` plus the
 * transitional `@web-ui/states` ErrorState/LoadingState (annotated above) so
 * the failure and pre-frame variants match production presentation exactly.
 * All copy is literal English caller-owned strings mirroring
 * `apps/web/src/locales/en/sessions.json#runView` (studio is
 * developer-auxiliary and excluded from i18n catalogs); all data is fake. No
 * daemon, no routing, no contracts, no `react-i18next`, no product hooks.
 */

/** Event-list chrome — the run view's shared list recipe (`DESIGN.md` §Data Table). */
const EVENT_LIST_CLASS =
  'flex flex-col divide-y divide-gray-alpha-400 rounded-control border border-gray-alpha-400';

/** One observed frame: a data frame, or the inline gap marker where it arrived. */
function FixtureEventRow({
  name,
  cursor,
  detail,
  status,
  step,
}: {
  name: string;
  cursor: string;
  detail?: string;
  status?: string;
  step?: { step: string; attempt: string };
}) {
  return (
    <li className="flex flex-col gap-1 px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="text-copy-13 text-gray-1000">{name}</span>
        {status && <Badge variant="running">{status}</Badge>}
        <span className="text-label-12-mono text-gray-700">{cursor}</span>
        {step && (
          <span className="text-label-12-mono text-gray-700">
            step {step.step} · attempt {step.attempt}
          </span>
        )}
      </div>
      {detail && (
        <p className="whitespace-pre-wrap break-words text-copy-13 leading-[1.5] text-gray-900">
          {detail}
        </p>
      )}
    </li>
  );
}

/** The inline gap marker — the exact position where continuity was lost. */
function FixtureGapRow({ testId }: { testId: string }) {
  return (
    <li className="flex flex-col gap-1 bg-warning-surface px-3 py-2.5" data-testid={testId}>
      <p className="text-copy-13 text-gray-1000">Events were missed</p>
      <p className="text-copy-13 text-gray-900">
        The stream skipped events this view never received. Reconnect resumes from the last event
        received.
      </p>
    </li>
  );
}

/** The lost-history notice — a `history_unavailable` close is not an ordinary end. */
function FixtureHistoryUnavailableNotice() {
  return (
    <div
      role="status"
      data-testid="run-view-history-unavailable-notice"
      className="flex items-start gap-2 rounded-card border border-warning-surface-border bg-warning-surface p-3"
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-700" aria-hidden />
      <div className="flex flex-col gap-1">
        <p className="text-label-14 font-medium text-amber-1000">Server history unavailable</p>
        <p className="text-copy-13 text-amber-900">
          The server no longer holds this run&apos;s history (a restart, or an evicted event ring).
          The events below are only what this view received earlier.
        </p>
      </div>
    </div>
  );
}

/** One labeled stream section — the replayed span and the live tail are separate. */
function FixtureStreamSection({
  heading,
  testId,
  children,
}: {
  heading: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2" data-testid={testId}>
      <h4 className="text-label-14 text-gray-900">{heading}</h4>
      {children}
    </section>
  );
}

/** The replay/live disclosure — the boundary is inferred, never reported. */
function FixtureBoundaryNote() {
  return (
    <p className="text-copy-13 text-gray-700" data-testid="run-view-boundary-note">
      Replay and live are split by attach order — the server sends the replay when the stream opens,
      before the live tail — so the boundary is inferred, not reported by the server.
    </p>
  );
}

/**
 * One run-view variant: the page chrome (title, back affordance), the observed
 * run identity, the phase badge, the optional reconnect affordance, and the
 * stream body this variant exists to show.
 */
function RunViewVariant({
  testId,
  badge,
  badgeVariant,
  cursor,
  reconnect = false,
  children,
}: {
  testId: string;
  badge: string;
  badgeVariant: 'queued' | 'running' | 'warning' | 'neutral' | 'error';
  cursor: string | null;
  reconnect?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2" data-testid={testId}>
      <div className="flex flex-col gap-4 rounded-card border border-gray-alpha-400 bg-background-200 p-4">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div>
            <h4 className="font-display text-display-24 text-gray-1000">Run View</h4>
            <p className="text-copy-14 text-gray-900">What this run is doing, as it happens.</p>
          </div>
          <Button variant="tertiary" size="small">
            <ArrowLeft className="h-4 w-4" aria-hidden />
            Back to Sessions
          </Button>
        </div>
        <Card className="shadow-card">
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <CardTitle>Observed run</CardTitle>
                <CardDescription data-testid={`${testId}-identity`}>
                  <span className="text-copy-13-mono text-gray-900">Session run-1a2b3c4d</span>
                  <span className="text-copy-13-mono text-gray-900"> · Run run-1</span>
                  {cursor && (
                    <span className="text-copy-13-mono text-gray-900"> · Last event {cursor}</span>
                  )}
                </CardDescription>
              </div>
              <div className="flex items-center gap-2">
                <Badge variant={badgeVariant} data-testid={`${testId}-phase`}>
                  {badge}
                </Badge>
                {reconnect && (
                  <Button type="button" variant="secondary" size="small">
                    <RefreshCw className="h-4 w-4" aria-hidden />
                    Reconnect
                  </Button>
                )}
              </div>
            </div>
          </CardHeader>
          <CardContent>{children}</CardContent>
        </Card>
      </div>
    </section>
  );
}

/** Studio-only retry sink — the fixture never touches a stream. */
function handleRetry() {
  /* Studio fixture only — reconnection is product behavior. */
}

export function RunObservationViewFixtures() {
  return (
    <div data-testid="run-observation-view-fixtures" className="grid gap-8">
      <p className="text-copy-14 text-gray-700">
        The session run view, state by state: live tail, inline gap, replay/live split with the
        inferred boundary disclosure, lost server history, typed refusal, exhausted transport, and
        the pre-frame stream. Single theme-following specimens — every class is token-backed, so the
        pair view shows all of these in light and dark.
      </p>

      <RunViewVariant
        testId="run-view-variant-running"
        badge="Live"
        badgeVariant="running"
        cursor="e1:42"
      >
        <FixtureStreamSection heading="Live" testId="run-view-running-live">
          <ol aria-label="Run events" className={EVENT_LIST_CLASS}>
            <FixtureEventRow
              name="OpStarted"
              cursor="e1:41"
              step={{ step: 'step-1', attempt: 'attempt-1' }}
            />
            <FixtureEventRow
              name="ThoughtDelta"
              cursor="e1:42"
              detail="Reading the outline before the first beat…"
              step={{ step: 'step-1', attempt: 'attempt-1' }}
            />
          </ol>
        </FixtureStreamSection>
      </RunViewVariant>

      <RunViewVariant
        testId="run-view-variant-gap"
        badge="Gap"
        badgeVariant="warning"
        cursor="e1:44"
        reconnect
      >
        <FixtureStreamSection heading="Live" testId="run-view-gap-live">
          <ol aria-label="Run events" className={EVENT_LIST_CLASS}>
            <FixtureEventRow
              name="OpStarted"
              cursor="e1:41"
              step={{ step: 'step-1', attempt: 'attempt-1' }}
            />
            <FixtureGapRow testId="run-view-gap-marker" />
            <FixtureEventRow
              name="MessageDelta"
              cursor="e1:44"
              detail="Continuity resumes after the missed range."
              step={{ step: 'step-1', attempt: 'attempt-1' }}
            />
          </ol>
        </FixtureStreamSection>
      </RunViewVariant>

      <RunViewVariant
        testId="run-view-variant-replay"
        badge="Live"
        badgeVariant="running"
        cursor="e1:44"
      >
        <div className="flex flex-col gap-4">
          <FixtureStreamSection heading="Replayed history" testId="run-view-replay-replayed">
            <FixtureBoundaryNote />
            <ol aria-label="Run events" className={EVENT_LIST_CLASS}>
              <FixtureEventRow
                name="Run state"
                cursor="e1:41"
                status="Running"
                detail="queued → running"
              />
              <FixtureEventRow
                name="OpStarted"
                cursor="e1:42"
                step={{ step: 'step-1', attempt: 'attempt-1' }}
              />
            </ol>
          </FixtureStreamSection>
          <FixtureStreamSection heading="Live" testId="run-view-replay-live">
            <ol aria-label="Run events" className={EVENT_LIST_CLASS}>
              <FixtureEventRow
                name="MessageDelta"
                cursor="e1:44"
                detail="Continuity resumes after the missed range."
                step={{ step: 'step-1', attempt: 'attempt-1' }}
              />
            </ol>
          </FixtureStreamSection>
        </div>
      </RunViewVariant>

      <RunViewVariant
        testId="run-view-variant-history-unavailable"
        badge="History unavailable"
        badgeVariant="warning"
        cursor="e1:42"
      >
        <div className="flex flex-col gap-4">
          <FixtureHistoryUnavailableNotice />
          <FixtureStreamSection heading="Replayed history" testId="run-view-history-retained">
            <FixtureBoundaryNote />
            <ol aria-label="Run events" className={EVENT_LIST_CLASS}>
              <FixtureEventRow
                name="Run state"
                cursor="e1:41"
                status="Running"
                detail="queued → running"
              />
              <FixtureEventRow
                name="OpStarted"
                cursor="e1:42"
                step={{ step: 'step-1', attempt: 'attempt-1' }}
              />
            </ol>
          </FixtureStreamSection>
        </div>
      </RunViewVariant>

      <RunViewVariant
        testId="run-view-variant-refusal"
        badge="Error"
        badgeVariant="error"
        cursor={null}
      >
        <ErrorState
          title="Could not observe this run"
          description="run run-child is not a root run"
        />
      </RunViewVariant>

      <RunViewVariant
        testId="run-view-variant-transport-error"
        badge="Error"
        badgeVariant="error"
        cursor={null}
        reconnect
      >
        <ErrorState title="Could not observe this run" description="socket closed" onRetry={handleRetry} />
      </RunViewVariant>

      <RunViewVariant
        testId="run-view-variant-loading"
        badge="Connecting"
        badgeVariant="queued"
        cursor={null}
      >
        <LoadingState label="Connecting to the run stream…" />
      </RunViewVariant>
    </div>
  );
}
