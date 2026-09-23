import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * Host-side regression for the shared SSE writer's TERMINAL marker (P1-T3 L2
 * Critical): a retained gap slot recorded AFTER the hub's terminal sorts after
 * it by sequence, so a stale/cursorless replay must END at the terminal frame —
 * never write the gap behind it onto the wire.
 *
 * The consumer observable is the wire itself: the real `OperationEventHub`
 * replay plan is written through the real `SseWriter` against a recording
 * response, and the assertion is on the frames that reached it.
 */
describe('sse-writer-terminal (shared writer terminal marker)', () => {
  let sse;

  before(async () => {
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
      0,
    );
    sse = await import(join(serviceRoot, 'dist/sse.js'));
  });

  test('a cursorless replay of a terminal hub never delivers a gap frame after the terminal', async () => {
    const hub = new sse.OperationEventHub('op-terminal', 'sess-terminal');
    assert.ok(hub.recordEvent({ OpStarted: { session_id: 'mock-session' } }), 'the data frame is retained');
    assert.ok(
      hub.recordEvent({ OpFinished: { reason: 'end_turn' } }),
      'the terminal frame is retained',
    );
    assert.ok(
      hub.recordGap({
        reason: 'interrupted',
        operation_id: 'op-terminal',
        resync_required: true,
        inspect_url: '/v1/daemon/agent-host/operations/op-terminal',
      }),
      'the late gap slot is retained for the stale replay',
    );

    const plan = hub.planReplay(undefined);
    assert.equal(plan.kind, 'all', 'a cursorless replay replays every retained frame');

    // The real response the writer writes to: every byte it emits is observed.
    const chunks = [];
    const res = new EventEmitter();
    res.write = (chunk) => {
      chunks.push(Buffer.from(chunk));
      return true;
    };
    res.writableEnded = false;
    res.destroyed = false;
    res.end = () => {};
    const writer = new sse.SseWriter(res, hub);

    // The Host replay loop's own stop condition: the terminal frame ends the
    // replay, so nothing retained behind it can reach the wire.
    let endedAtTerminal = false;
    for (const frame of plan.frames) {
      const result = await writer.writeFrame(frame);
      assert.equal(result, 'ok', `every replay frame is writable: ${frame.event}`);
      if (frame.isTerminal) {
        endedAtTerminal = true;
        break;
      }
    }

    assert.equal(endedAtTerminal, true, 'the replay must end at the Host terminal frame');
    const wire = Buffer.concat(chunks).toString('utf8');
    assert.match(wire, /event: provider_event\n/, 'the terminal frame is on the wire');
    assert.doesNotMatch(
      wire,
      /event: gap\n/,
      `a stale replay must not deliver a gap frame after the Host terminal:\n${wire}`,
    );
  });
});
