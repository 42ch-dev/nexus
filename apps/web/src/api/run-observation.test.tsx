/**
 * `useRunObservation` (v1.201 P1-T2) — subscription lifecycle, the
 * replay/live/gap/terminal state machine and the bounded reconnect policy.
 *
 * Every case drives a stubbed `AsyncIterable` client injected through
 * `ClientProvider`, so no network (and no msw handler) is involved: the hook
 * consumes only `NexusClient.subscribeWorkflowEvents`.
 */
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it, vi } from 'vitest';

import {
  useRunObservation,
  type RunObservationPhase,
  type RunObservationResult,
} from '@/api/run-observation';
import { ClientProvider } from '@/lib/client-context';
import { NexusClientError, type NexusClient } from '@/lib/nexus';
import type { WorkflowObservationFrame } from '@/lib/nexus/types';

interface SubscribeOptions {
  lastEventId?: string;
  signal: AbortSignal;
}

type Script = (options: SubscribeOptions) => AsyncIterable<WorkflowObservationFrame>;

interface StubClient {
  client: NexusClient;
  calls: SubscribeOptions[];
}

/** A client whose Nth subscription runs the Nth script (the last one repeats). */
function stubClientFor(scripts: Script[]): StubClient {
  const calls: SubscribeOptions[] = [];
  const client = {
    subscribeWorkflowEvents: (_runId: string, options: SubscribeOptions) => {
      const script = scripts[Math.min(calls.length, scripts.length - 1)];
      calls.push(options);
      return script(options);
    },
  } as unknown as NexusClient;
  return { client, calls };
}

function wrapperFor(client: NexusClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <MemoryRouter>
        <ClientProvider client={client} desktop={null} connectionConfig={null}>
          {children}
        </ClientProvider>
      </MemoryRouter>
    );
  };
}

interface Probe {
  result: { current: RunObservationResult };
  /** Distinct phases in render order — timing-proof view of the transitions. */
  phases: RunObservationPhase[];
  unmount: () => void;
}

function observeRun(client: NexusClient, runId: string | null): Probe {
  const phases: RunObservationPhase[] = [];
  const hook = renderHook(
    () => {
      const observation = useRunObservation(runId);
      if (phases[phases.length - 1] !== observation.phase) phases.push(observation.phase);
      return observation;
    },
    { wrapper: wrapperFor(client) },
  );
  return { result: hook.result, phases, unmount: hook.unmount };
}

const GAP: WorkflowObservationFrame = { kind: 'gap' };
const HISTORY_UNAVAILABLE: WorkflowObservationFrame = { kind: 'history_unavailable' };

/** The durable run statuses the core projects into a `run_state` frame. */
function runState(id: string, status = 'running'): WorkflowObservationFrame {
  const sequence = Number(id.slice(id.indexOf(':') + 1));
  return {
    kind: 'run_state',
    id,
    payload: { run_id: 'run-1', epoch: 'e1', sequence, state_revision: sequence, status },
  };
}

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

/** A stream that yields `seed`, then ends. */
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

/** Cursor of a frame: its `<epoch>:<sequence>` id, or the control kind. */
function cursorOf(frame: WorkflowObservationFrame): string {
  return frame.kind === 'run_state' || frame.kind === 'host_event' ? frame.id : frame.kind;
}

async function wait(ms: number): Promise<void> {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
}

describe('useRunObservation — replay/live handoff', () => {
  it('replays the retained history on re-entry and resumes the live tail without duplicating an id', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      // The re-entry replay re-delivers the last id the client already holds.
      (options) => openStream(options, hostEvent('e1:2'), runState('e1:3')),
    ]);

    const first = observeRun(client, 'run-replay');
    await waitFor(() => expect(first.result.current.phase).toBe('live'));
    expect(first.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);
    expect(first.result.current.lastEventId).toBe('e1:2');
    expect(calls[0].lastEventId).toBeUndefined();
    first.unmount();

    const reentry = observeRun(client, 'run-replay');
    // The already-emitted history is on screen immediately — never a blank view.
    expect(reentry.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);
    expect(reentry.result.current.phase).toBe('replaying');

    await waitFor(() => expect(reentry.result.current.phase).toBe('live'));
    expect(calls[1].lastEventId).toBe('e1:2');
    expect(reentry.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2', 'e1:3']);
    expect(reentry.phases.indexOf('replaying')).toBeLessThan(reentry.phases.indexOf('live'));
  });

  it('subscribes without a cursor for the first view of a run', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1')),
    ]);

    const probe = observeRun(client, 'run-fresh');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));

    expect(calls[0].lastEventId).toBeUndefined();
    expect(probe.phases).toContain('connecting');
    expect(probe.phases).not.toContain('replaying');
    // No retained history to catch up: every frame of this view is the live tail.
    expect(probe.result.current.liveFrom).toBe(0);
  });

  it('draws the boundary after a multi-frame re-entry catch-up and appends the live tail past it', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      async function* reentry(options: SubscribeOptions) {
        // Missed while the view was closed: the server's strictly-after replay.
        yield runState('e1:3');
        yield hostEvent('e1:4');
        // The catch-up settles, then the run appends for real.
        await wait(300);
        yield hostEvent('e1:5');
        if (options.signal.aborted) return;
        await new Promise<void>((resolve) => {
          options.signal.addEventListener('abort', () => resolve(), { once: true });
        });
      },
    ]);

    const first = observeRun(client, 'run-boundary');
    await waitFor(() => expect(first.result.current.phase).toBe('live'));
    first.unmount();

    const reentry = observeRun(client, 'run-boundary');
    await waitFor(() => expect(reentry.result.current.events.map(cursorOf)).toContain('e1:5'));

    expect(calls[1].lastEventId).toBe('e1:2');
    expect(reentry.result.current.events.map(cursorOf)).toEqual([
      'e1:1',
      'e1:2',
      'e1:3',
      'e1:4',
      'e1:5',
    ]);
    // The two replayed frames are history; only the append after the settle is live.
    expect(reentry.result.current.liveFrom).toBe(4);
    expect(reentry.result.current.phase).toBe('live');
    expect(reentry.phases.indexOf('replaying')).toBeLessThan(reentry.phases.indexOf('live'));
  });

  it('keeps the live boundary anchored when eviction slides the retained window', async () => {
    async function* fullWindow(options: SubscribeOptions) {
      for (let sequence = 1; sequence <= 300; sequence += 1) yield hostEvent(`e1:${sequence}`);
      if (options.signal.aborted) return;
      await new Promise<void>((resolve) => {
        options.signal.addEventListener('abort', () => resolve(), { once: true });
      });
    }

    const { client, calls } = stubClientFor([
      (options) => fullWindow(options),
      async function* reentry(options: SubscribeOptions) {
        // Away long enough for the catch-up to settle before the run appends
        // again: nothing to replay, then exactly one genuinely live frame.
        await wait(300);
        yield hostEvent('e1:301');
        if (options.signal.aborted) return;
        await new Promise<void>((resolve) => {
          options.signal.addEventListener('abort', () => resolve(), { once: true });
        });
      },
    ]);

    const first = observeRun(client, 'run-window-boundary');
    await waitFor(() => expect(first.result.current.lastEventId).toBe('e1:300'));
    expect(first.result.current.events).toHaveLength(256);
    first.unmount();

    const reentry = observeRun(client, 'run-window-boundary');
    expect(reentry.result.current.phase).toBe('replaying');
    // Everything the full window holds is catch-up until the handoff.
    expect(reentry.result.current.liveFrom).toBe(256);

    await waitFor(() =>
      expect(cursorOf(reentry.result.current.events[reentry.result.current.liveFrom])).toBe('e1:301'),
    );
    await waitFor(() => expect(reentry.result.current.phase).toBe('live'));

    // The window is still full (256). The boundary must slide with it, or the
    // frame appended after the handoff would sit outside the live tail.
    const { events, liveFrom } = reentry.result.current;
    expect(calls[1].lastEventId).toBe('e1:300');
    expect(events).toHaveLength(256);
    expect(liveFrom).toBe(255);
    expect(cursorOf(events[0])).toBe('e1:46');
    expect(cursorOf(events[liveFrom])).toBe('e1:301');
    expect(events.slice(liveFrom).map(cursorOf)).toEqual(['e1:301']);
  });
});

describe('useRunObservation — gap recovery', () => {
  it('aborts an inline gap on an open stream and resubscribes from the last received id', async () => {
    const { client, calls } = stubClientFor([
      // The server delivers the gap and then keeps the stream open.
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2'), GAP),
      (options) => openStream(options),
    ]);

    const probe = observeRun(client, 'run-gap-inline');
    await waitFor(() => expect(calls.length).toBe(2));

    expect(calls[1].lastEventId).toBe('e1:2');
    expect(calls[0].signal.aborted).toBe(true);
    expect(probe.phases).toContain('gapped');
    expect(probe.result.current.phase).toBe('gapped');
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2', 'gap']);
  });

  it('resumes the live tail on the reconnect after a gap', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), GAP),
      (options) => openStream(options, hostEvent('e1:2')),
    ]);

    const probe = observeRun(client, 'run-gap-resume');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));

    expect(calls[1].lastEventId).toBe('e1:1');
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'gap', 'e1:2']);
    expect(probe.phases).toContain('gapped');
  });

  it('stays honestly gapped once the bounded retries are exhausted, and retry() re-arms', async () => {
    const { client, calls } = stubClientFor([() => frames(GAP)]);

    const probe = observeRun(client, 'run-gap-exhausted');
    await waitFor(() => expect(calls.length).toBe(4), { timeout: 5_000 });
    expect(probe.result.current.phase).toBe('gapped');

    await wait(900);
    expect(calls.length).toBe(4);
    expect(probe.result.current.phase).toBe('gapped');

    act(() => probe.result.current.retry());
    await waitFor(() => expect(calls.length).toBe(5));
  });
});

describe('useRunObservation — terminal', () => {
  it('reaches terminal on a clean stream end at a terminal run state and never resubscribes', async () => {
    const { client, calls } = stubClientFor([
      () => frames(runState('e1:1'), runState('e1:2', 'completed')),
    ]);

    const probe = observeRun(client, 'run-terminal');
    await waitFor(() => expect(probe.result.current.phase).toBe('terminal'));
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);

    await wait(300);
    expect(calls.length).toBe(1);
    expect(probe.result.current.phase).toBe('terminal');
  });

  it('reconnects from the cursor when a clean stream end leaves the run still running', async () => {
    const { client, calls } = stubClientFor([
      () => frames(runState('e1:1'), hostEvent('e1:2')),
      (options) => openStream(options, runState('e1:3')),
    ]);

    const probe = observeRun(client, 'run-eof-running');
    await waitFor(() => expect(calls.length).toBe(2), { timeout: 5_000 });

    expect(calls[1].lastEventId).toBe('e1:2');
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2', 'e1:3']);
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));
  });

  it('bounds the reconnect policy when a clean end keeps reporting a running run', async () => {
    const { client, calls } = stubClientFor([() => frames(runState('e1:1'))]);

    const probe = observeRun(client, 'run-eof-loop');
    await waitFor(() => expect(probe.result.current.phase).toBe('error'), { timeout: 5_000 });

    expect(calls.length).toBe(4);
    expect(probe.result.current.error).toBeInstanceOf(Error);

    await wait(900);
    expect(calls.length).toBe(4);
  });

  it('treats a history_unavailable close as terminal and records the marker', async () => {
    const { client, calls } = stubClientFor([
      () => frames(runState('e1:1'), HISTORY_UNAVAILABLE),
    ]);

    const probe = observeRun(client, 'run-history-unavailable');
    await waitFor(() => expect(probe.result.current.phase).toBe('terminal'));
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'history_unavailable']);

    await wait(300);
    expect(calls.length).toBe(1);
  });
});

describe('useRunObservation — failure and unmount', () => {
  it('reconnects from the cursor after a mid-stream transport failure', async () => {
    const { client, calls } = stubClientFor([
      async function* script() {
        yield runState('e1:1');
        throw new Error('socket reset');
      },
      (options) => openStream(options, hostEvent('e1:2')),
    ]);

    const probe = observeRun(client, 'run-transport-failure');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));

    expect(calls[1].lastEventId).toBe('e1:1');
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);
    expect(probe.phases).toContain('connecting');
  });

  it('surfaces a typed refusal as error without retrying', async () => {
    const { client, calls } = stubClientFor([
      // A transport refusal is thrown before the first frame is yielded.
      () => ({
        [Symbol.asyncIterator]: () => ({
          next: () => Promise.reject(new NexusClientError(404, 'not_found', 'no such run')),
        }),
      }),
    ]);

    const probe = observeRun(client, 'run-refused');
    await waitFor(() => expect(probe.result.current.phase).toBe('error'));

    expect(probe.result.current.error?.message).toBe('no such run');
    await wait(300);
    expect(calls.length).toBe(1);
  });

  it('aborts the subscription on unmount', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1')),
    ]);

    const probe = observeRun(client, 'run-unmount');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));

    probe.unmount();
    expect(calls[0].signal.aborted).toBe(true);

    await wait(300);
    expect(calls.length).toBe(1);
  });
});

describe('useRunObservation — retention bounds', () => {
  it('drops a run’s retained history once the run reaches a terminal state', async () => {
    const { client, calls } = stubClientFor([
      () => frames(runState('e1:1'), runState('e1:2', 'completed')),
    ]);

    const first = observeRun(client, 'run-evicted');
    await waitFor(() => expect(first.result.current.phase).toBe('terminal'));
    first.unmount();

    // A closed run is re-observed from the server's own ring, not from the SPA.
    const again = observeRun(client, 'run-evicted');
    expect(again.result.current.phase).toBe('connecting');
    expect(again.result.current.events).toEqual([]);
    await waitFor(() => expect(calls.length).toBe(2));
    expect(calls[1].lastEventId).toBeUndefined();
  });

  it('caps the retained history per run, keeping the newest frames', async () => {
    async function* many(options: SubscribeOptions) {
      for (let sequence = 1; sequence <= 300; sequence += 1) yield hostEvent(`e1:${sequence}`);
      if (options.signal.aborted) return;
      await new Promise<void>((resolve) => {
        options.signal.addEventListener('abort', () => resolve(), { once: true });
      });
    }

    const { client } = stubClientFor([(options) => many(options)]);
    const probe = observeRun(client, 'run-capped');
    await waitFor(() => expect(probe.result.current.events.length).toBeGreaterThan(128));

    const retained = probe.result.current.events;
    expect(retained.length).toBeLessThan(300);
    expect(cursorOf(retained[retained.length - 1])).toBe('e1:300');
    expect(retained.some((frame) => cursorOf(frame) === 'e1:1')).toBe(false);
    expect(probe.result.current.lastEventId).toBe('e1:300');
  });

  it('evicts an idle non-terminal run and re-subscribes from scratch on re-entry', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1')),
      (options) => openStream(options, runState('e1:2')),
    ]);

    const first = observeRun(client, 'run-idle');
    await waitFor(() => expect(first.result.current.phase).toBe('live'));
    expect(first.result.current.events.map(cursorOf)).toEqual(['e1:1']);
    first.unmount();

    // The run never reaches a terminal state; the user just never comes back.
    const clock = vi.spyOn(Date, 'now').mockReturnValue(Date.now() + 60 * 60_000);
    const reentry = observeRun(client, 'run-idle');
    clock.mockRestore();

    // The idle entry was evicted, so re-entry is a cold subscription — no
    // restored history and no cursor (an honest re-fetch, not a stale window).
    expect(reentry.result.current.phase).toBe('connecting');
    expect(reentry.result.current.events).toEqual([]);
    expect(reentry.result.current.lastEventId).toBeNull();

    await waitFor(() => expect(calls.length).toBe(2));
    expect(calls[1].lastEventId).toBeUndefined();
    await waitFor(() => expect(reentry.result.current.phase).toBe('live'));
    expect(reentry.result.current.events.map(cursorOf)).toEqual(['e1:2']);
  });
});
