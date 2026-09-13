import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import {
  OperationDelivery,
  MAX_EVENT_BYTES,
  MAX_PENDING_MESSAGES,
} from '../dist/index.js';

describe('OperationDelivery bounds', () => {
  test('overflow marks delivery_overflow terminal', () => {
    const delivery = new OperationDelivery('op-1');
    const big = { MessageDelta: { session_id: 's', op_id: 'op-1', text: 'x'.repeat(MAX_EVENT_BYTES + 1) } };
    const ok = delivery.enqueue(big);
    assert.equal(ok, false);
    assert.equal(delivery.hasOverflow, true);
    const terminal = delivery.drainTerminalEvents('s', 'op-1');
    assert.equal(terminal[0]?.OpFailed?.error_message, 'delivery_overflow');
  });

  test('pull respects batch caps', () => {
    const delivery = new OperationDelivery('op-2');
    for (let i = 0; i < MAX_PENDING_MESSAGES; i += 1) {
      delivery.enqueue({ Status: { session_id: 's', level: 'info', message: `m${i}` } });
    }
    assert.equal(delivery.enqueue({ Status: { session_id: 's', level: 'info', message: 'overflow' } }), false);
    delivery.setTerminal({ kind: 'finished', reason: 'end_turn' });
    delivery.tryBeginPull();
    const batch = delivery.pull(16, 256 * 1024);
    delivery.endPull();
    assert.ok(batch.events.length <= 16);
    assert.equal(batch.has_more, false);
  });
});
