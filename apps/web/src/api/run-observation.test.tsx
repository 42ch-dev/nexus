/**
 * Every case drives a stubbed `AsyncIterable` client injected through
 * `ClientProvider`, so no network (and no msw handler) is involved: the hook
 * consumes only `NexusClient.subscribeWorkflowEvents`.
 */
import { act, renderHook, waitFor } from '@testing-library/react';
import { useLayoutEffect, type ReactNode } from 'react';
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

async function waitForCursor(
  probe: { result: { current: RunObservationResult } },
  cursor: string,
  buffer: () => void,
): Promise<void> {
  await waitFor(() => {
    expect(probe.result.current.lastEventId).toBe(cursor);
    buffer();
  });
}

const committedFrames: Array<{ phase: RunObservationPhase; cursors: string[] }> = [];

describe('useRunObservation — replay/live handoff', () => {
  it('replays the retained history on re-entry and resumes the live tail without duplicating an id', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      (options) => openStream(options, hostEvent('e1:2'), runState('e1:3')),
    ]);

    const first = observeRun(client, 'run-replay');
    await waitFor(() => expect(first.result.current.phase).toBe('live'));
    expect(first.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);
    expect(first.result.current.lastEventId).toBe('e1:2');
    expect(calls[0].lastEventId).toBeUndefined();
    first.unmount();

    const reentry = observeRun(client, 'run-replay');
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
    expect(probe.result.current.liveFrom).toBe(0);
  });

  it('draws the boundary after a multi-frame re-entry catch-up and appends the live tail past it', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      async function* reentry(options: SubscribeOptions) {
        yield runState('e1:3');
        yield hostEvent('e1:4');
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
    await waitForCursor(reentry, 'e1:5', () =>
      expect(reentry.result.current.events.map(cursorOf)).toContain('e1:5'),
    );

    expect(calls[1].lastEventId).toBe('e1:2');
    expect(reentry.result.current.events.map(cursorOf)).toEqual([
      'e1:1',
      'e1:2',
      'e1:3',
      'e1:4',
      'e1:5',
    ]);
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
        yield hostEvent('e1:301');
        await wait(300);
        yield hostEvent('e1:302');
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
    expect(reentry.result.current.liveFrom).toBe(256);

    await waitFor(() => expect(reentry.result.current.phase).toBe('live'));
    await waitForCursor(reentry, 'e1:302', () =>
      expect(
        reentry.result.current.events.slice(reentry.result.current.liveFrom).map(cursorOf),
      ).toEqual(['e1:302']),
    );

    const { events, liveFrom } = reentry.result.current;
    expect(calls[1].lastEventId).toBe('e1:300');
    expect(events).toHaveLength(256);
    expect(liveFrom).toBe(255);
    expect(cursorOf(events[0])).toBe('e1:47');
    expect(cursorOf(events[liveFrom])).toBe('e1:302');
    expect(events.slice(liveFrom).map(cursorOf)).toEqual(['e1:302']);
  });

  it('anchors the catch-up at the first received frame, so a delayed connect keeps its replay on the replay side', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      async function* delayedReplay(options: SubscribeOptions) {
        await wait(300);
        yield runState('e1:3');
        yield hostEvent('e1:4');
        await wait(300);
        yield hostEvent('e1:5');
        if (options.signal.aborted) return;
        await new Promise<void>((resolve) => {
          options.signal.addEventListener('abort', () => resolve(), { once: true });
        });
      },
    ]);

    const first = observeRun(client, 'run-delayed-replay');
    await waitFor(() => expect(first.result.current.phase).toBe('live'));
    first.unmount();

    const reentry = observeRun(client, 'run-delayed-replay');
    await wait(200);
    expect(reentry.result.current.phase).toBe('replaying');

    await waitForCursor(reentry, 'e1:5', () =>
      expect(reentry.result.current.events.map(cursorOf)).toContain('e1:5'),
    );

    expect(calls[1].lastEventId).toBe('e1:2');
    expect(reentry.result.current.events.map(cursorOf)).toEqual([
      'e1:1',
      'e1:2',
      'e1:3',
      'e1:4',
      'e1:5',
    ]);
    expect(reentry.result.current.liveFrom).toBe(4);
    expect(reentry.result.current.events.slice(reentry.result.current.liveFrom).map(cursorOf)).toEqual([
      'e1:5',
    ]);
    expect(reentry.result.current.phase).toBe('live');
  });

  it('hands a busy re-entry burst off at the catch-up frame total while frames keep arriving', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
      async function* busyReplay(options: SubscribeOptions) {
        for (let sequence = 3; sequence <= 72; sequence += 1) {
          yield hostEvent(`e1:${sequence}`);
          await wait(5);
        }
        if (options.signal.aborted) return;
        await new Promise<void>((resolve) => {
          options.signal.addEventListener('abort', () => resolve(), { once: true });
        });
      },
    ]);

    const first = observeRun(client, 'run-busy-replay');
    await waitFor(() => expect(first.result.current.phase).toBe('live'));
    first.unmount();

    const reentry = observeRun(client, 'run-busy-replay');
    await waitForCursor(reentry, 'e1:72', () =>
      expect(reentry.result.current.events).toHaveLength(72),
    );

    expect(calls[1].lastEventId).toBe('e1:2');
    expect(reentry.result.current.liveFrom).toBe(65);
    expect(cursorOf(reentry.result.current.events[reentry.result.current.liveFrom])).toBe('e1:66');
    expect(reentry.result.current.events.slice(reentry.result.current.liveFrom)).toHaveLength(7);
    expect(reentry.result.current.phase).toBe('live');
  });
});

describe('useRunObservation — gap recovery', () => {
  it('keeps consuming the same stream after a gap instead of reconnecting into the hole it reported', async () => {
    async function* gappedThenTail(options: SubscribeOptions) {
      yield runState('e1:1');
      yield hostEvent('e1:2');
      yield GAP;
      await wait(150);
      yield hostEvent('e1:3');
      if (options.signal.aborted) return;
      await new Promise<void>((resolve) => {
        options.signal.addEventListener('abort', () => resolve(), { once: true });
      });
    }

    const { client, calls } = stubClientFor([(options) => gappedThenTail(options)]);

    const probe = observeRun(client, 'run-gap-inline');
    await waitForCursor(probe, 'e1:3', () =>
      expect(probe.result.current.events.map(cursorOf)).toContain('e1:3'),
    );

    expect(calls).toHaveLength(1);
    expect(calls[0].signal.aborted).toBe(false);
    expect(probe.phases).toContain('gapped');
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2', 'gap', 'e1:3']);
    expect(probe.result.current.lastEventId).toBe('e1:3');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));
  });

  it('reconnects from the cursor a gapped stream advanced to, never from the gap itself', async () => {
    const { client, calls } = stubClientFor([
      () => frames(GAP, hostEvent('e1:1'), hostEvent('e1:2')),
      (options) => openStream(options, hostEvent('e1:3')),
    ]);

    const probe = observeRun(client, 'run-gap-advanced');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));

    expect(calls).toHaveLength(2);
    expect(calls[1].lastEventId).toBe('e1:2');
    expect(probe.result.current.events.map(cursorOf)).toEqual(['gap', 'e1:1', 'e1:2', 'e1:3']);
  });

  it('rests honestly gapped when a gapped stream THROWS at the gap, charging no attempt', async () => {
    const { client, calls } = stubClientFor([
      async function* script() {
        yield runState('e1:1');
        yield GAP;
        throw new Error('socket reset');
      },
    ]);

    const probe = observeRun(client, 'run-gap-throw');
    await waitFor(() => expect(probe.result.current.phase).toBe('gapped'));
    await wait(900);

    expect(calls).toHaveLength(1);
    expect(probe.result.current.error).toBeNull();
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'gap']);
    expect(probe.result.current.lastEventId).toBe('e1:1');
    expect(probe.result.current.phase).toBe('gapped');

    act(() => probe.result.current.retry());
    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1].lastEventId).toBe('e1:1');
  });

  it('stays honestly gapped when the stream ends at the gap, and retry() re-arms', async () => {
    const { client, calls } = stubClientFor([() => frames(runState('e1:1'), GAP)]);

    const probe = observeRun(client, 'run-gap-exhausted');
    await waitFor(() => expect(probe.result.current.phase).toBe('gapped'));

    await wait(900);
    expect(calls).toHaveLength(1);
    expect(probe.result.current.error).toBeNull();
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'gap']);

    act(() => probe.result.current.retry());
    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1].lastEventId).toBe('e1:1');
  });
});

describe('useRunObservation — bounded reconnect episodes', () => {
  it('exhausts the retry budget on a repeated gap-then-advance episode instead of reconnecting forever', async () => {
    const { client, calls } = stubClientFor(
      [0, 1, 2, 3].map((attempt) => () => frames(GAP, hostEvent(`e1:${attempt + 1}`))),
    );

    const probe = observeRun(client, 'run-gap-churn');
    await waitFor(() => expect(calls.length).toBeGreaterThanOrEqual(4), { timeout: 5_000 });
    await wait(900);

    expect(calls).toHaveLength(4);
    expect(calls[1].lastEventId).toBe('e1:1');
    expect(calls[3].lastEventId).toBe('e1:3');
    expect(probe.result.current.phase).toBe('gapped');
    expect(probe.result.current.error).toBeNull();
  });

  it('exhausts the retry budget on a repeated data-plus-throw episode instead of reconnecting forever', async () => {
    const { client, calls } = stubClientFor([
      async function* script() {
        yield runState('e1:1');
        throw new Error('socket reset');
      },
    ]);

    const probe = observeRun(client, 'run-throw-churn');
    await waitFor(() => expect(calls.length).toBeGreaterThanOrEqual(4), { timeout: 5_000 });
    await wait(900);

    expect(calls.length).toBe(4);
    expect(probe.result.current.phase).toBe('error');
    expect(probe.result.current.error?.message).toBe('socket reset');
  });

  it('never resets the reconnect budget for a long productive attempt — only retry() re-arms it', async () => {
    const { client, calls } = stubClientFor([
      async function* productive() {
        yield runState('e1:1');
        await wait(1_050);
        throw new Error('socket reset after a productive burst');
      },
    ]);

    const probe = observeRun(client, 'run-monotonic-budget');
    await waitFor(() => expect(probe.result.current.phase).toBe('error'), { timeout: 15_000 });

    expect(calls.length).toBe(4);
    expect(probe.result.current.error?.message).toBe('socket reset after a productive burst');

    await wait(900);
    expect(calls.length).toBe(4);

    act(() => probe.result.current.retry());
    await waitFor(() => expect(calls.length).toBe(5), { timeout: 5_000 });
  }, 20_000);
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
    await waitForCursor(probe, 'e1:300', () =>
      expect(probe.result.current.events.length).toBeGreaterThan(128),
    );

    const retained = probe.result.current.events;
    expect(retained.length).toBeLessThan(300);
    expect(cursorOf(retained[retained.length - 1])).toBe('e1:300');
    expect(retained.some((frame) => cursorOf(frame) === 'e1:1')).toBe(false);
    expect(probe.result.current.lastEventId).toBe('e1:300');
  });

  it('advances the publish revision when an inline gap lands on a full window', async () => {
    async function* full(options: SubscribeOptions) {
      for (let sequence = 1; sequence <= 256; sequence += 1) yield hostEvent(`e1:${sequence}`);
      yield GAP;
      if (options.signal.aborted) return;
      await new Promise<void>((resolve) => {
        options.signal.addEventListener('abort', () => resolve(), { once: true });
      });
    }

    const { client, calls } = stubClientFor([(options) => full(options)]);
    const probe = observeRun(client, 'run-revision');
    await waitFor(() => expect(probe.result.current.phase).toBe('gapped'));

    expect(calls).toHaveLength(1);
    expect(probe.result.current.events).toHaveLength(256);
    expect(probe.result.current.lastEventId).toBe('e1:256');
    expect(cursorOf(probe.result.current.events[255])).toBe('gap');
    expect(probe.result.current.revision).toBe(257);
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

    const clock = vi.spyOn(Date, 'now').mockReturnValue(Date.now() + 60 * 60_000);
    const reentry = observeRun(client, 'run-idle');
    clock.mockRestore();

    expect(reentry.result.current.phase).toBe('connecting');
    expect(reentry.result.current.events).toEqual([]);
    expect(reentry.result.current.lastEventId).toBeNull();

    await waitFor(() => expect(calls.length).toBe(2));
    expect(calls[1].lastEventId).toBeUndefined();
    await waitFor(() => expect(reentry.result.current.phase).toBe('live'));
    expect(reentry.result.current.events.map(cursorOf)).toEqual(['e1:2']);
  });
});

describe('useRunObservation — connection-scoped retention', () => {
  it('starts cold when the same run is re-observed over a different connection', async () => {
    const first = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
    ]);

    const observed = observeRun(first.client, 'run-connection');
    await waitFor(() => expect(observed.result.current.phase).toBe('live'));
    expect(observed.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);
    observed.unmount();

    const second = stubClientFor([(options) => openStream(options, runState('e1:9'))]);
    const reconnected = observeRun(second.client, 'run-connection');

    expect(reconnected.result.current.phase).toBe('connecting');
    expect(reconnected.result.current.events).toEqual([]);
    expect(reconnected.result.current.lastEventId).toBeNull();

    await waitFor(() => expect(second.calls).toHaveLength(1));
    expect(second.calls[0].lastEventId).toBeUndefined();
    expect(first.calls).toHaveLength(1);
    expect(reconnected.result.current.events.map(cursorOf)).toEqual(['e1:9']);
  });

  it('renders cold on an in-place client replacement — never a frame of the old connection', async () => {
    const oldConnection = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
    ]);
    const sent: { client: NexusClient } = { client: oldConnection.client };

    let releaseNew: (() => void) | undefined;
    const newStreamReady = new Promise<void>((resolve) => {
      releaseNew = resolve;
    });
    const newConnection = stubClientFor([
      async function* script(options: SubscribeOptions) {
        await newStreamReady;
        yield runState('e1:9');
        if (options.signal.aborted) return;
        await new Promise<void>((resolve) => {
          options.signal.addEventListener('abort', () => resolve(), { once: true });
        });
      },
    ]);

    const hook = renderHook(
      () => {
        const observation = useRunObservation('run-in-place-swap');
        useLayoutEffect(() => {
          committedFrames.push({
            phase: observation.phase,
            cursors: observation.events.map(cursorOf),
          });
        });
        return observation;
      },
      {
        wrapper: ({ children }: { children: ReactNode }) => (
          <MemoryRouter>
            <ClientProvider client={sent.client} desktop={null} connectionConfig={null}>
              {children}
            </ClientProvider>
          </MemoryRouter>
        ),
      },
    );
    await waitFor(() =>
      expect(committedFrames[committedFrames.length - 1]?.cursors).toEqual(['e1:1', 'e1:2']),
    );

    sent.client = newConnection.client;
    committedFrames.length = 0;
    act(() => hook.rerender());

    expect(committedFrames[0]).toEqual({ phase: 'connecting', cursors: [] });
    expect(committedFrames.every((frame) => frame.cursors.length === 0)).toBe(true);
    expect(hook.result.current.lastEventId).toBeNull();

    releaseNew?.();
    await waitFor(() => expect(newConnection.calls).toHaveLength(1));
    expect(newConnection.calls[0].lastEventId).toBeUndefined();
    expect(oldConnection.calls).toHaveLength(1);
    await waitFor(() =>
      expect(committedFrames[committedFrames.length - 1]?.cursors).toEqual(['e1:9']),
    );
  });

  it('renders cold when the observed run changes — never a frame of the previous run', async () => {
    const { client, calls } = stubClientFor([
      (options) => openStream(options, runState('e1:1')),
      (options) => openStream(options, runState('e1:9')),
    ]);

    const hook = renderHook(
      ({ runId }: { runId: string }) => {
        const observation = useRunObservation(runId);
        useLayoutEffect(() => {
          committedFrames.push({
            phase: observation.phase,
            cursors: observation.events.map(cursorOf),
          });
        });
        return observation;
      },
      { wrapper: wrapperFor(client), initialProps: { runId: 'run-switch-a' } },
    );
    await waitForCursor(hook, 'e1:1', () =>
      expect(hook.result.current.events.map(cursorOf)).toEqual(['e1:1']),
    );

    committedFrames.length = 0;
    act(() => hook.rerender({ runId: 'run-switch-b' }));

    expect(committedFrames[0]).toEqual({ phase: 'connecting', cursors: [] });
    expect(committedFrames.every((frame) => frame.cursors.length === 0)).toBe(true);

    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1].lastEventId).toBeUndefined();
    await waitFor(() =>
      expect(committedFrames[committedFrames.length - 1]?.cursors).toEqual(['e1:9']),
    );
  });
});
