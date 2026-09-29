/**
 * Session run view — the acceptance-bearing observation surface of v1.201 P1
 * (compass D7; audit DIR-03): one orchestration root run's retained/live event
 * stream, rendered read-only from {@link useRunObservation}.
 *
 * The three author-visible states (Product Scope state table) are rendered
 * honestly from the hook's frame stream and its replay/live boundary:
 *
 * - **Running** — frames appear as they arrive from the events stream. The
 *   view holds no polling and no manual refresh: `events` is the subscription's
 *   own output, so a live append renders on the next publish.
 * - **Gap** — a `gap` control frame stays inline at the exact position where
 *   continuity was lost (never smoothed over), and the reconnect affordance
 *   re-arms the subscription from the last received cursor — the contract §3
 *   recovery. Reconnecting is a stream affordance, not a run control.
 * - **Replay** — a re-entered session renders `events.slice(0, liveFrom)` as
 *   replayed history and `events.slice(liveFrom)` as the live tail, in two
 *   separately labeled sections, so the replay/live handoff stays visible
 *   instead of blending into one undifferentiated log.
 *
 * Observation only (plan Non-Goal): the view exposes no run control — there is
 * no drive, cancel or restart affordance anywhere on this surface. A typed
 * refusal (absent/foreign/child run id, malformed cursor) is rendered as the
 * typed message it is, never retried.
 */
import type { ProviderHostEvent } from '@42ch/nexus-contracts';
import { ArrowLeft, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Link, useParams } from 'react-router';

import {
  useRunObservation,
  type RunObservationPhase,
  type RunObservationResult,
} from '@/api/run-observation';
import { statusVariant } from '@/components/status-badge';
import { Badge, type BadgeProps } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { humanizeStatus, shortId } from '@/lib/format';
import { NexusClientError } from '@/lib/nexus';
import type { WorkflowObservationFrame } from '@/lib/nexus/types';
import { NotFoundPage } from '@/pages/not-found-page';

/** Badge variant of each phase — state meaning per DESIGN.md §Badge. */
const PHASE_VARIANT: Record<RunObservationPhase, BadgeProps['variant']> = {
  connecting: 'queued',
  replaying: 'queued',
  live: 'running',
  gapped: 'warning',
  terminal: 'neutral',
  error: 'error',
};

/** i18n key of each phase label (the badge text states the stream's phase). */
const PHASE_LABEL_KEY: Record<RunObservationPhase, string> = {
  connecting: 'runView.phase.connecting',
  replaying: 'runView.phase.replaying',
  live: 'runView.phase.live',
  gapped: 'runView.phase.gapped',
  terminal: 'runView.phase.terminal',
  error: 'runView.phase.error',
};

/** Event-list chrome — the shared list container recipe (DESIGN.md §Data Table). */
const EVENT_LIST_CLASS =
  'flex flex-col divide-y divide-gray-alpha-400 rounded-control border border-gray-alpha-400';

/**
 * One-line detail of a host event, when its variant carries one worth showing.
 * The wire vocabulary is the generated contract's; nothing is summarized away.
 */
function hostEventDetail(event: ProviderHostEvent): string | null {
  if ('OpFailed' in event) return event.OpFailed.error_message;
  if ('Status' in event) return event.Status.message;
  if ('ToolCall' in event) return event.ToolCall.tool_name;
  if ('ToolCallUpdate' in event) return event.ToolCallUpdate.content;
  if ('PlanUpdate' in event) return event.PlanUpdate.content;
  if ('OpFinished' in event) return event.OpFinished.reason;
  if ('SessionStopped' in event) return event.SessionStopped.reason;
  if ('ThoughtDelta' in event) return event.ThoughtDelta.text;
  if ('MessageDelta' in event) return event.MessageDelta.text;
  return null;
}

/** The run id the frames identify, or `null` before the first data frame. */
function observedRunId(frames: WorkflowObservationFrame[]): string | null {
  for (let index = frames.length - 1; index >= 0; index -= 1) {
    const frame = frames[index]!;
    if (frame.kind === 'host_event' || frame.kind === 'run_state') return frame.payload.run_id;
  }
  return null;
}

/** Frames the event list renders: data frames plus the inline gap marker. */
type ListedFrame = Exclude<WorkflowObservationFrame, { kind: 'history_unavailable' }>;

/**
 * Drop the `history_unavailable` control frame from a slice — it is the sole
 * frame of a closed subscription (core `run_events.rs` §437), so it is answered
 * by the terminal empty state rather than by a row. The predicate keeps the
 * narrowing at the call site instead of casting the frame shape.
 */
function listed(frames: WorkflowObservationFrame[]): ListedFrame[] {
  return frames.filter((frame): frame is ListedFrame => frame.kind !== 'history_unavailable');
}

/** One observed frame: a data frame, or the inline gap marker where it arrived. */
function EventRow({ frame }: { frame: ListedFrame }) {
  const { t } = useTranslation('sessions');

  if (frame.kind === 'gap') {
    return (
      <li
        className="flex flex-col gap-1 bg-warning-surface px-3 py-2.5"
        data-testid="run-event-gap"
      >
        <p className="text-copy-13 text-gray-1000">{t('runView.gapTitle')}</p>
        <p className="text-copy-13 text-gray-900">{t('runView.gapDescription')}</p>
      </li>
    );
  }

  const status = frame.kind === 'run_state' ? frame.payload.status : null;
  const detail =
    frame.kind === 'run_state'
      ? (frame.payload.reason ?? null)
      : hostEventDetail(frame.payload.host_event);

  return (
    <li className="flex flex-col gap-1 px-3 py-2.5" data-testid="run-event">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="text-copy-13 text-gray-1000">
          {/* Externally tagged provider host event: its single key IS the variant. */}
          {frame.kind === 'run_state'
            ? t('runView.runState')
            : (Object.keys(frame.payload.host_event)[0] ?? 'HostEvent')}
        </span>
        {status && <Badge variant={statusVariant(status)}>{humanizeStatus(status)}</Badge>}
        <span className="text-label-12-mono text-gray-700">{frame.id}</span>
        {frame.kind === 'host_event' && (
          <span className="text-label-12-mono text-gray-700">
            {t('runView.stepCursor', {
              step: shortId(frame.payload.step_id),
              attempt: shortId(frame.payload.attempt_id),
            })}
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

/**
 * The observed stream: the failure/empty states, then the replayed span and the
 * live tail as two separately labeled sections.
 */
function RunStream({ observation }: { observation: RunObservationResult }) {
  const { t } = useTranslation('sessions');
  const { phase, events, liveFrom, error, retry } = observation;
  const replayed = listed(events.slice(0, liveFrom));
  const live = listed(events.slice(liveFrom));
  const streamOpen = phase !== 'terminal' && phase !== 'error';
  const hasDataFrame = events.some(
    (frame) => frame.kind === 'host_event' || frame.kind === 'run_state',
  );

  if (phase === 'error') {
    // A typed refusal (>= 400: absent/foreign/child run, malformed cursor) is
    // deterministic — the hook never auto-retries it, so the view must not
    // offer to either. Transport failures do get the reconnect affordance.
    const retryable = !(error instanceof NexusClientError && error.status >= 400);
    return (
      <div className="flex flex-col gap-4">
        <ErrorState
          title={t('runView.errorTitle')}
          description={error?.message ?? t('runView.errorDescription')}
          onRetry={retryable ? retry : undefined}
          retryLabel={t('runView.reconnect')}
        />
        {/* Frames already received stay on screen — recovering the stream never
            costs the observation that was honest at the time. */}
        {replayed.length + live.length > 0 && (
          <StreamSections replayed={replayed} live={live} streamOpen={false} />
        )}
      </div>
    );
  }

  if (events.length === 0) return <LoadingState label={t('runView.loading')} />;

  // A terminal stream that never delivered a data frame: the run's retained
  // history is gone, so there is nothing to observe (not a crash).
  if (phase === 'terminal' && !hasDataFrame) {
    return <EmptyState title={t('runView.emptyTitle')} description={t('runView.emptyDescription')} />;
  }

  return <StreamSections replayed={replayed} live={live} streamOpen={streamOpen} />;
}

/** The replayed span above the live tail, each under its own label. */
function StreamSections({
  replayed,
  live,
  streamOpen,
}: {
  replayed: ListedFrame[];
  live: ListedFrame[];
  streamOpen: boolean;
}) {
  const { t } = useTranslation('sessions');

  return (
    <div className="flex flex-col gap-4">
      {replayed.length > 0 && (
        <section className="flex flex-col gap-2" data-testid="run-replay">
          <h2 className="text-label-14 text-gray-900">{t('runView.replayedHeading')}</h2>
          <ol aria-label={t('runView.eventsAria')} className={EVENT_LIST_CLASS}>
            {replayed.map((frame, index) => (
              <EventRow key={`replay-${index}`} frame={frame} />
            ))}
          </ol>
        </section>
      )}
      {live.length > 0 || (replayed.length > 0 && streamOpen) ? (
        <section className="flex flex-col gap-2" data-testid="run-live">
          <h2 className="text-label-14 text-gray-900">{t('runView.liveHeading')}</h2>
          {live.length > 0 ? (
            <ol aria-label={t('runView.eventsAria')} className={EVENT_LIST_CLASS}>
              {live.map((frame, index) => (
                <EventRow key={`live-${index}`} frame={frame} />
              ))}
            </ol>
          ) : (
            <p className="text-copy-13 text-gray-700">{t('runView.waitingForLive')}</p>
          )}
        </section>
      ) : null}
    </div>
  );
}

/**
 * Route entry for `/sessions/:sessionId` — the run view opened from a session
 * row. Reads the row's `session_id` from the URL (orchestration session rows are
 * root runs, so it IS the events-route run id) and observes it read-only.
 */
export function SessionRunViewPage() {
  const { t } = useTranslation('sessions');
  const { sessionId } = useParams<{ sessionId: string }>();
  const observation = useRunObservation(sessionId ?? null);

  if (!sessionId) return <NotFoundPage />;

  const runId = observedRunId(observation.events);

  return (
    <div className="flex flex-col gap-4" data-testid="session-run-view">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h1 className="font-display text-display-24 text-gray-1000">{t('runView.title')}</h1>
          <p className="text-copy-14 text-gray-900">{t('runView.description')}</p>
        </div>
        <Button asChild variant="tertiary" size="small">
          <Link to="/sessions">
            <ArrowLeft className="h-4 w-4" aria-hidden />
            {t('runView.back')}
          </Link>
        </Button>
      </div>
      <Card className="shadow-card">
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <CardTitle>{t('runView.observingTitle')}</CardTitle>
              <CardDescription data-testid="run-identity">
                <span className="text-copy-13-mono text-gray-900">
                  {t('runView.sessionFact', { id: shortId(sessionId) })}
                </span>
                {runId && (
                  <span className="text-copy-13-mono text-gray-900">
                    {' · '}
                    {t('runView.runFact', { id: shortId(runId) })}
                  </span>
                )}
                {observation.lastEventId && (
                  <span className="text-copy-13-mono text-gray-900">
                    {' · '}
                    {t('runView.cursorFact', { cursor: observation.lastEventId })}
                  </span>
                )}
              </CardDescription>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant={PHASE_VARIANT[observation.phase]} data-testid="run-phase">
                {t(PHASE_LABEL_KEY[observation.phase])}
              </Badge>
              {observation.phase === 'gapped' && (
                <Button
                  type="button"
                  variant="secondary"
                  size="small"
                  onClick={observation.retry}
                >
                  <RefreshCw className="h-4 w-4" aria-hidden />
                  {t('runView.reconnect')}
                </Button>
              )}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <RunStream observation={observation} />
        </CardContent>
      </Card>
    </div>
  );
}
