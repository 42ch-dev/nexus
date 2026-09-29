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
        // The re-attached stream's first frame anchors the catch-up burst (a
        // replay-side frame), then the run appends for real past the handoff.
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
    // Everything the full window holds is catch-up until the handoff.
    expect(reentry.result.current.liveFrom).toBe(256);

    await waitFor(() => expect(reentry.result.current.phase).toBe('live'));
    await waitFor(() =>
      expect(
        reentry.result.current.events.slice(reentry.result.current.liveFrom).map(cursorOf),
      ).toEqual(['e1:302']),
    );

    // The window is still full (256). The boundary must slide with it, or the
    // frame appended after the handoff would sit outside the live tail.
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
        // Connect and replay delivery are both slower than the settle window: a
        // timer armed before the stream connects would already have declared the
        // view live here, mislabeling this whole replay burst as the live tail.
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
    // Nothing has arrived yet: the view is waiting on the stream, not live.
    await wait(200);
    expect(reentry.result.current.phase).toBe('replaying');

    await waitFor(() => expect(reentry.result.current.events.map(cursorOf)).toContain('e1:5'));

    expect(calls[1].lastEventId).toBe('e1:2');
    // The delayed replay burst stays replayed; only the post-settle frame is live.
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
    // A re-entry replay that outruns the observer: frames keep arriving faster
    // than the settle window, so neither the quiet gap nor the 1 s total can end
    // the catch-up. Only the hard frame total can, and it must still hand off —
    // the 64th catch-up frame is the first live-tail frame (QC3-002).
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
    await waitFor(() => expect(reentry.result.current.events).toHaveLength(72));

    // 70 catch-up frames (e1:3…e1:72): the 64th, `e1:66`, is the first live one,
    // and the six frames after it keep arriving live rather than replaying.
    expect(calls[1].lastEventId).toBe('e1:2');
    expect(reentry.result.current.liveFrom).toBe(65);
    expect(cursorOf(reentry.result.current.events[reentry.result.current.liveFrom])).toBe('e1:66');
    expect(reentry.result.current.events.slice(reentry.result.current.liveFrom)).toHaveLength(7);
    expect(reentry.result.current.phase).toBe('live');
  });
});

describe('useRunObservation — gap recovery', () => {
  it('keeps consuming the same stream after a gap instead of reconnecting into the hole it reported', async () => {
    // The server writes the gap and then the frames its ring still holds, on the
    // same stream (workflow-observation.ts forwards the ring in order and then
    // blocks for the next batch). Aborting at the gap would discard that
    // retained span and reconnect into the range whose gap was just reported.
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
    await waitFor(() => expect(probe.result.current.events.map(cursorOf)).toContain('e1:3'));

    // One subscription, never aborted: the frames after the gap arrived on the
    // stream that reported it.
    expect(calls).toHaveLength(1);
    expect(calls[0].signal.aborted).toBe(false);
    expect(probe.phases).toContain('gapped');
    // The hole stays inline at its exact position; the tail continues past it.
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2', 'gap', 'e1:3']);
    expect(probe.result.current.lastEventId).toBe('e1:3');
    // Frames arrived again, so the subscription is live once more.
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));
  });

  it('reconnects from the cursor a gapped stream advanced to, never from the gap itself', async () => {
    const { client, calls } = stubClientFor([
      // The stream gaps, then carries the retained frames past the hole, then
      // ends: the cursor it moved to is what a reconnect must ask from.
      () => frames(GAP, hostEvent('e1:1'), hostEvent('e1:2')),
      (options) => openStream(options, hostEvent('e1:3')),
    ]);

    const probe = observeRun(client, 'run-gap-advanced');
    await waitFor(() => expect(probe.result.current.phase).toBe('live'));

    expect(calls).toHaveLength(2);
    expect(calls[1].lastEventId).toBe('e1:2');
    // The hole is still on screen; only the recovery cursor moved past it.
    expect(probe.result.current.events.map(cursorOf)).toEqual(['gap', 'e1:1', 'e1:2', 'e1:3']);
  });

  it('stays honestly gapped when the stream ends at the gap, and retry() re-arms', async () => {
    // The stream never moved past its gap, so `Last-Event-ID` still names the
    // gapped range: reconnecting would re-report the same gap. The hook reports
    // it once and rests — no attempt is spent on a reconnect that cannot help.
    const { client, calls } = stubClientFor([() => frames(runState('e1:1'), GAP)]);

    const probe = observeRun(client, 'run-gap-exhausted');
    await waitFor(() => expect(probe.result.current.phase).toBe('gapped'));

    await wait(900);
    expect(calls).toHaveLength(1);
    expect(probe.result.current.error).toBeNull();
    expect(probe.result.current.events.map(cursorOf)).toEqual(['e1:1', 'gap']);

    // The affordance is the only way out, and it re-arms from the retained cursor.
    act(() => probe.result.current.retry());
    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1].lastEventId).toBe('e1:1');
  });
});

describe('useRunObservation — bounded reconnect episodes', () => {
  it('exhausts the retry budget on a repeated gap-then-advance episode instead of reconnecting forever', async () => {
    // Every attempt gaps, then carries a frame past the gap and dies: each
    // reconnect is individually productive, so only the monotonic budget can end
    // the episode — and it must, rather than resetting on each productive-looking
    // attempt (QC3-001).
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
    // Every attempt streams real data for far longer than a "stable" attempt
    // would need, and only then dies. No outcome buys extra attempts: the budget
    // is strictly monotonic, so the episode still exhausts at the fixed total and
    // the hook rests in `error` — only the user's explicit retry() re-arms it
    // (QC3-001).
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

  it('advances the publish revision when an inline gap lands on a full window', async () => {
    // A control frame has no cursor and, on a full window, does not change the
    // retained length either — so length/lastEventId are unsafe memoization
    // keys. The revision must still move (QC3-003).
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

    // The gap is consumed in place: the stream stays open, so nothing reconnects.
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

describe('useRunObservation — connection-scoped retention', () => {
  it('starts cold when the same run is re-observed over a different connection', async () => {
    // A rebuilt client — a daemon reconnect, a changed endpoint or key — is a
    // different connection. The frames a previous connection retained must not
    // be shown under it before its own subscription has been authorized, so a
    // new connection re-subscribes cursorless with no restored history.
    const first = stubClientFor([
      (options) => openStream(options, runState('e1:1'), hostEvent('e1:2')),
    ]);

    const observed = observeRun(first.client, 'run-connection');
    await waitFor(() => expect(observed.result.current.phase).toBe('live'));
    expect(observed.result.current.events.map(cursorOf)).toEqual(['e1:1', 'e1:2']);
    observed.unmount();

    const second = stubClientFor([(options) => openStream(options, runState('e1:9'))]);
    const reconnected = observeRun(second.client, 'run-connection');

    // Nothing of the old connection's history leaks through: cold start, no cursor.
    expect(reconnected.result.current.phase).toBe('connecting');
    expect(reconnected.result.current.events).toEqual([]);
    expect(reconnected.result.current.lastEventId).toBeNull();

    await waitFor(() => expect(second.calls).toHaveLength(1));
    expect(second.calls[0].lastEventId).toBeUndefined();
    // The first connection observed nothing new — no shared subscription state.
    expect(first.calls).toHaveLength(1);
    expect(reconnected.result.current.events.map(cursorOf)).toEqual(['e1:9']);
  });
});
