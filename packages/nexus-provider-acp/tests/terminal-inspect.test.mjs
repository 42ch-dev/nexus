import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { createTestEngine, ProviderNextError } from '../dist/index.js';

describe('terminal inspectability', () => {
  test('65th terminal does not make the first not_found', async () => {
    const engine = createTestEngine();
    const firstOp = 'op-first';
    engine.seedTerminalTombstone(firstOp);
    for (let i = 1; i <= 65; i += 1) {
      engine.seedTerminalTombstone(`op-${i}`);
    }
    assert.equal(engine.operationCount(), 66);

    const batch = await engine.next(firstOp, 16, 256 * 1024);
    assert.deepEqual(batch.events, []);
    assert.equal(batch.has_more, false);
    assert.equal(batch.operation_id, firstOp);
  });

  test('unknown operation still throws ProviderNextError', async () => {
    const engine = createTestEngine();
    await assert.rejects(() => engine.next('missing-op', 1, 1024), ProviderNextError);
  });
});
