/**
 * `SessionRunViewPage` (v1.201 P1-T3) — the acceptance-bearing observation
 * surface (compass D7): the three author-visible states plus the view-identity
 * and refusal behaviour.
 *
 * Every case drives a stubbed `subscribeWorkflowEvents` client injected through
 * `ClientProvider`, so the frames are the test's own (no network, no SSE):
 *  - a **running** view appends frames as they arrive, reached from a session
 *    row through SPA navigation only;
 *  - a forced **gap** renders its indication inline and the reconnect resumes
 *    from the last received id;
 *  - a re-entered session renders replayed history and the live tail as two
 *    distinguishable regions;
 *  - a foreign/child run id renders the typed refusal honestly;
 *  - the surface exposes no run control at all (plan Non-Goal).
 */
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { Route, Routes } from 'react-router';
import { beforeEach, describe, expect, it } from 'vitest';

import { i18n } from '@/lib/i18n/config';
import { BrowserClient, NexusClientError, type NexusClient } from '@/lib/nexus';
import type { WorkflowObservationFrame } from '@/lib/nexus/types';
import { SessionRunViewPage } from '@/pages/session-run-view';
import { SessionsPage } from '@/pages/sessions-page';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

interface SubscribeOptions {
  lastEventId?: string;
  signal: AbortSignal;
}

type Script = (options: SubscribeOptions) => AsyncIterable<WorkflowObservationFrame>;

/** A `BrowserClient` (so msw keeps serving the list) with a scripted stream. */
function stubClientFor(scripts: Script[]): { client: NexusClient; calls: SubscribeOptions[] } {
  const calls: SubscribeOptions[] = [];
  const client = Object.assign(new BrowserClient(), {
    subscribeWorkflowEvents: (_runId: string, options: SubscribeOptions) => {
      const script = scripts[Math.min(calls.length, scripts.length - 1)]!;
      calls.push(options);
      return script(options);
    },
  });
  return { client, calls };
}

const GAP: WorkflowObservationFrame = { kind: 'gap' };
const HISTORY_UNAVAILABLE: WorkflowObservationFrame = { kind: 'history_unavailable' };

/** A `run_state` frame carrying the durable status projection. */
function runState(id: string, status = 'running'): WorkflowObservationFrame {
  const sequence = Number(id.slice(id.indexOf(':') + 1));
  return {
    kind: 'run_state',
    id,
    payload: { run_id: 'run-1', epoch: 'e1', sequence, state_revision: sequence, status },
  };
}

/** A `host_event` frame carrying the run/step/attempt identity. */
function hostEvent(id: string): WorkflowObservationFrame {
  const sequence = Number(id.slice(id.indexOf(':') + 1));
  return {
    kind: 'host_event',
    id,
    payload: {
      run_id: 'run-1',
      epoch: 'e1',
      sequence,
      step_id: 'step-1',
      attempt_id: 'attempt-1',
      host_event: { OpStarted: { session_id: 'run-1', op_id: 'op-1' } },
    },
  };
}

/** A stream that yields `seed`, then ends (a closed batch). */
async function* frames(...seed: WorkflowObservationFrame[]) {
  for (const frame of seed) yield frame;
}

/** A stream that yields `seed`, then stays open until its signal aborts. */
async function* openStream(options: SubscribeOptions, ...seed: WorkflowObservationFrame[]) {
  for (const frame of seed) yield frame;
  if (options.signal.aborted) return;
  // `Promise.withResolvers` is ES2024; this app compiles against the ES2022 lib.
  await new Promise<void>((resolve) => {
    options.signal.addEventListener('abort', () => resolve(), { once: true });
  });
}

interface Pushable {
  stream: Script;
  push: (frame: WorkflowObservationFrame) => void;
}

/** A subscription the test feeds frame by frame — arrival order, no timers. */
function pushable(): Pushable {
  const pending: WorkflowObservationFrame[] = [];
  let wake: (() => void) | null = null;
  const signal = (): void => {
    const resume = wake;
    wake = null;
    resume?.();
  };
  return {
    stream: (options) => ({
      async *[Symbol.asyncIterator]() {
        options.signal.addEventListener('abort', signal, { once: true });
        for (;;) {
          if (pending.length === 0) {
            if (options.signal.aborted) return;
            await new Promise<void>((resolve) => {
              wake = resolve;
            });
            continue;
          }
          const frame = pending.shift();
          if (frame) yield frame;
        }
      },
    }),
    push: (frame) => {
      pending.push(frame);
      signal();
    },
  };
}

async function wait(ms: number): Promise<void> {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
}

/** Minimal route tree mirroring the `App.tsx` `sessions` nesting. */
function SessionsRouteTree() {
  return (
    <Routes>
      <Route path="sessions">
        <Route index element={<SessionsPage />} />
        <Route path=":sessionId" element={<SessionRunViewPage />} />
      </Route>
    </Routes>
  );
}

/** Mount the run view at its own route, as the row affordance reaches it. */
function renderRunView(client: NexusClient, sessionId: string) {
  return renderInApp(
    <Routes>
      <Route path="/sessions/:sessionId" element={<SessionRunViewPage />} />
    </Routes>,
    { client, initialRouterEntries: [`/sessions/${sessionId}`] },
  );
}

function sessionsHandler(sessionId: string): void {
  useHandlers(
    http.get('/v1/daemon/orchestration/sessions', () =>
      HttpResponse.json({
        items: [
          {
            session_id: sessionId,
            creator_id: 'creator-a',
            preset_id: 'preset-a',
            status: 'running',
            current_task_id: 'task-1',
          },
        ],
        pagination: { limit: 20, has_more: false },
      }),
    ),
  );
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('SessionRunViewPage — running', () => {
  it('opens from a session row and appends events as they arrive', async () => {
    sessionsHandler('run-live');
    const feed = pushable();
    const { client, calls } = stubClientFor([feed.stream]);
    renderInApp(<SessionsRouteTree />, { client, initialRouterEntries: ['/sessions'] });

    // Shipped SPA navigation only: the row's own link carries the session id.
    fireEvent.click(
      await screen.findByRole('link', { name: 'Open run view for session run-live' }),
    );

    expect(await screen.findByTestId('session-run-view')).toBeInTheDocument();
    // One events-stream subscription, cursorless first view — no polling hook.
    expect(calls).toHaveLength(1);
    expect(calls[0].lastEventId).toBeUndefined();
    expect(screen.getByTestId('run-identity')).toHaveTextContent('Session run-live');
    expect(screen.getByText('Connecting to the run stream…')).toBeInTheDocument();

    await act(async () => {
      feed.push(runState('e1:1'));
    });

    // Rendered on arrival, with no refresh affordance touched.
    expect(await screen.findByText('e1:1')).toBeInTheDocument();
    expect(screen.getByText('Run state')).toBeInTheDocument();
    expect(screen.getByText('Running')).toBeInTheDocument();
    expect(screen.getByTestId('run-identity')).toHaveTextContent('Run run-1');
    expect(screen.getByTestId('run-phase')).toHaveTextContent('Live');

    await act(async () => {
      feed.push(hostEvent('e1:2'));
    });

    expect(await screen.findByText('OpStarted')).toBeInTheDocument();
    expect(screen.getByText('e1:2')).toBeInTheDocument();
  });

  it('exposes no run control on the surface', async () => {
    const { client } = stubClientFor([(options) => openStream(options, runState('e1:1'))]);
    renderRunView(client, 'run-observe-only');

    expect(await screen.findByText('e1:1')).toBeInTheDocument();
    // Observation only (plan Non-Goal): the live view has no button at all, and
    // nothing names a run control.
    expect(screen.queryAllByRole('button')).toHaveLength(0);
    expect(screen.queryByText(/drive|cancel|restart|resume/i)).toBeNull();
  });
});

describe('SessionRunViewPage — gap', () => {
  it('shows the gap where continuity was lost and resumes the tail after the reconnect', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2'), GAP),
      (options) => openStream(options, hostEvent('e1:3')),
    ]);

    renderRunView(client, 'run-gap');

    const gap = await screen.findByTestId('run-event-gap');
    expect(gap).toHaveTextContent('Events were missed');
    // Contract recovery: the reconnect carries the last received id.
    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1].lastEventId).toBe('e1:2');
    expect(calls[0].signal.aborted).toBe(true);

    // Continuity returns past the hole, and the hole stays visible.
    expect(await screen.findByText('e1:3')).toBeInTheDocument();
    expect(screen.getByTestId('run-event-gap')).toBeInTheDocument();
  });

  it('offers the reconnect affordance once the gap retries are exhausted, and re-arms', async () => {
    const { client, calls } = stubClientFor([() => frames(GAP)]);
    renderRunView(client, 'run-gap-exhausted');

    // Bounded retries: the gap is re-attempted, then the view stays honestly gapped.
    await waitFor(() => expect(calls).toHaveLength(4), { timeout: 5_000 });
    expect(screen.getByTestId('run-phase')).toHaveTextContent('Gap');
    // Every gap this view hit is still on screen — none is smoothed over.
    expect(screen.getAllByTestId('run-event-gap')).toHaveLength(4);

    fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));

    await waitFor(() => expect(calls).toHaveLength(5));
    expect(calls[4].lastEventId).toBeUndefined();
  });
});

describe('SessionRunViewPage — replay', () => {
  it('separates replayed history from the live tail on re-entry', async () => {
    const feed = pushable();
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      feed.stream,
    ]);

    const first = renderRunView(client, 'run-replay');
    expect(await screen.findByText('e1:2')).toBeInTheDocument();
    first.unmount();

    renderRunView(client, 'run-replay');

    // The already-emitted history is on screen at once — never a blank view.
    const replay = await screen.findByTestId('run-replay');
    expect(within(replay).getByText('e1:1')).toBeInTheDocument();
    expect(within(replay).getByText('e1:2')).toBeInTheDocument();
    expect(calls[1].lastEventId).toBe('e1:2');
    const live = screen.getByTestId('run-live');
    expect(within(live).getByText('Waiting for the next event…')).toBeInTheDocument();

    // Past the catch-up handoff, an appended frame is the live tail — not replay.
    await act(async () => {
      await wait(400);
      feed.push(hostEvent('e1:3'));
    });

    expect(await within(live).findByText('e1:3')).toBeInTheDocument();
    expect(within(replay).queryByText('e1:3')).toBeNull();
    expect(screen.getByTestId('run-phase')).toHaveTextContent('Live');
  });

  it('renders the empty state when the stream closes with no replayable history', async () => {
    const { client } = stubClientFor([() => frames(HISTORY_UNAVAILABLE)]);
    renderRunView(client, 'run-history-gone');

    expect(await screen.findByText('No events to show')).toBeInTheDocument();
    expect(
      screen.getByText(/without delivering an event this session can replay/i),
    ).toBeInTheDocument();
    expect(screen.getByTestId('run-phase')).toHaveTextContent('Ended');
  });
});

describe('SessionRunViewPage — failure', () => {
  it('offers a reconnect when the stream keeps failing in transport', async () => {
    const { client, calls } = stubClientFor([
      () => {
        throw new Error('socket closed');
      },
    ]);

    renderRunView(client, 'run-transport-failure');

    expect(await screen.findByRole('alert', {}, { timeout: 5_000 })).toHaveTextContent(
      'socket closed',
    );
    await waitFor(() => expect(calls).toHaveLength(4), { timeout: 5_000 });

    // A transport failure is not a verdict: the view offers the reconnect.
    fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));

    await waitFor(() => expect(calls).toHaveLength(5));
    expect(calls[4].lastEventId).toBeUndefined();
  });

  it('renders the typed refusal of a foreign or child run id, without retrying', async () => {
    const refusal = NexusClientError.fromBody(404, {
      success: false,
      error: { code: 'not_found', message: 'run run-child is not a root run' },
    });
    const { client, calls } = stubClientFor([
      () => {
        throw refusal;
      },
    ]);

    renderRunView(client, 'run-child');

    expect(await screen.findByRole('alert')).toHaveTextContent('run run-child is not a root run');
    // Identity stays honest about what was asked for.
    expect(screen.getByTestId('run-identity')).toHaveTextContent('Session run-child');
    expect(screen.getByTestId('run-phase')).toHaveTextContent('Error');
    // A refusal is deterministic: one attempt, never a retry loop — and the
    // view offers no reconnect to a run it cannot observe.
    expect(calls).toHaveLength(1);
    expect(screen.queryByRole('button')).toBeNull();
  });
});
