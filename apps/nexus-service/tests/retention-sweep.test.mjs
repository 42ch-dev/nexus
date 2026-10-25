import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * The admission-time retention sweep (`ageSettledActorOperations`) reads the
 * authority once per retained Actor record. Those reads are native crossings, so
 * the sweep must spend ONE principal for the whole arm and issue the reads as one
 * bounded wave — while every record keeps the verdict the sequential chain gave
 * it (Character running ⇒ keep; settled ⇒ retire; Character miss ⇒ keep only
 * while the authority's generic row exists). The native authority is stubbed at
 * the `ServiceCore` seam and every crossing is recorded, so both the shape and
 * the per-record verdicts are observable from the same fixture.
 */
let provider;
let registryModule;
let errors;
let cfg;

/** Absence at the native seam (a Character/generic read that serves no row). */
const ABSENT = Symbol('absent');

before(async () => {
  assert.equal(
    spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
    0,
  );
  provider = await import(join(serviceRoot, 'dist/provider.js'));
  registryModule = await import(join(serviceRoot, 'dist/provider-registry.js'));
  errors = await import(join(serviceRoot, 'dist/errors.js'));
  cfg = await import(join(serviceRoot, 'dist/config.js'));
});

/**
 * One execute admission against the stubbed authority.
 *
 * `retained` are the Actor records the mirror already holds; `characterOf` and
 * `rowOf` answer each record's Character read and (on a Character miss) its
 * generic operation row. Records the fixtures do not name are absent on both.
 */
function admission({ retained = [], characterOf = () => ABSENT, rowOf = () => ABSENT } = {}) {
  const registry = new registryModule.ProviderRegistry();
  const sessionId = randomUUID();
  for (const operationId of retained) {
    registry.registerOperation({
      operationId,
      sessionId,
      providerId: 'mock-acp',
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
  }
  const mints = [];
  const characterReads = [];
  const genericReads = [];
  const admitted = [];
  let inFlight = 0;
  let peak = 0;
  const core = {
    activePrincipal: async () => {
      const principal = { mint: mints.length };
      mints.push(principal);
      return principal;
    },
    hostQuery: async (request) => {
      if (request.query === 'get_session') {
        return {
          session: {
            session_id: request.session_id,
            provider_id: 'mock-acp',
            state: 'Running',
            active_op_id: null,
            actor_ref: { actor_kind: 'character', character_id: `chr_${'a'.repeat(32)}` },
          },
        };
      }
      genericReads.push(request.operation_id);
      const row = rowOf(request.operation_id);
      if (row === ABSENT) throw new errors.HttpError(404, 'not_found', 'no operation row');
      return { operation: row };
    },
    hostCharacterOperation: async (principal, operationId) => {
      characterReads.push({ operationId, principal });
      inFlight += 1;
      peak = Math.max(peak, inFlight);
      try {
        // A turn of the event loop: a sequential chain shows one read in flight
        // at a time, a wave shows every record's read at once.
        await new Promise((resolve) => setImmediate(resolve));
        const character = characterOf(operationId);
        if (character === ABSENT) throw new errors.HttpError(404, 'not_found', 'no Character operation');
        return character;
      } finally {
        inFlight -= 1;
      }
    },
    hostExecuteOperation: async () => {
      const operationId = randomUUID();
      admitted.push(operationId);
      return { operation_id: operationId, session_id: sessionId, status: 'started' };
    },
  };
  return {
    registry,
    sessionId,
    mints,
    characterReads,
    genericReads,
    admitted,
    peak: () => peak,
    execute: () =>
      provider.executeProviderOperation(
        { providerRegistry: registry, domainOnly: false, core },
        sessionId,
        { kind: 'prompt', content: 'retention-sweep' },
      ),
  };
}

/** The Character a live run answers with. */
const running = (operationId) => ({
  operation_id: operationId,
  session_id: randomUUID(),
  run_status: 'running',
});
/** The Character an authority-settled run answers with. */
const settled = (operationId) => ({
  operation_id: operationId,
  session_id: randomUUID(),
  run_status: 'succeeded',
});

describe('retention sweep (one principal, one wave)', () => {
  test('every retained record is read once, under one minted principal, in one wave', async () => {
    const retained = Array.from({ length: 4 }, () => randomUUID());
    const fixture = admission({ retained, characterOf: running });
    await fixture.execute();

    assert.equal(fixture.characterReads.length, retained.length, 'one Character read per retained record');
    assert.deepEqual(
      new Set(fixture.characterReads.map((read) => read.operationId)),
      new Set(retained),
      'each retained record is read exactly once',
    );
    assert.equal(
      new Set(fixture.characterReads.map((read) => read.principal)).size,
      1,
      'the whole sweep must share one minted principal',
    );
    // One mint for the execute itself, one for the sweep — never one per record.
    assert.equal(
      fixture.mints.length,
      2,
      `an admission of ${retained.length} retained records mints exactly twice (minted ${fixture.mints.length})`,
    );
    assert.equal(fixture.peak(), retained.length, 'the sweep issues its reads as one wave');
  });

  test('mixed records keep the sequential verdicts: running kept, settled retired, generic owner kept, absent retired', async () => {
    const liveCharacter = randomUUID();
    const settledCharacter = randomUUID();
    const creatorActor = randomUUID();
    const agedOut = randomUUID();
    const fixture = admission({
      retained: [liveCharacter, settledCharacter, creatorActor, agedOut],
      characterOf: (operationId) => {
        if (operationId === liveCharacter) return running(operationId);
        if (operationId === settledCharacter) return settled(operationId);
        return ABSENT;
      },
      rowOf: (operationId) =>
        operationId === creatorActor
          ? { operation_id: operationId, session_id: randomUUID(), status: 'running' }
          : ABSENT,
    });
    await fixture.execute();

    const held = (operationId) => fixture.registry.operationRecord(operationId) !== undefined;
    assert.equal(held(liveCharacter), true, 'a running Character operation is live');
    assert.equal(held(settledCharacter), false, 'a settled Character operation is retired');
    assert.equal(held(creatorActor), true, 'a Creator Actor operation is kept by its generic row');
    assert.equal(held(agedOut), false, 'an operation the authority serves at all is retired');
    // The generic row is consulted only where the Character arm is empty.
    assert.equal(fixture.genericReads.length, 2, 'only the two Character misses spend a generic read');
    assert.deepEqual(
      new Set(fixture.genericReads),
      new Set([creatorActor, agedOut]),
      'a Character hit never spends a generic read',
    );
    assert.equal(fixture.admitted.length, 1);
    assert.notEqual(fixture.registry.operationRecord(fixture.admitted[0]), undefined, 'the new run is retained');
  });

  test('an empty arm mints nothing and reads nothing', async () => {
    const fixture = admission();
    await fixture.execute();

    assert.equal(fixture.characterReads.length, 0);
    assert.equal(fixture.genericReads.length, 0);
    assert.equal(fixture.mints.length, 1, 'with no retained record the sweep spends no principal at all');
  });

  test('a rejection inside the wave fails the admission with its mapped error and retires nothing', async () => {
    const live = randomUUID();
    const refusing = randomUUID();
    const fixture = admission({
      retained: [live, refusing],
      characterOf: (operationId) => {
        if (operationId === refusing) throw new errors.HttpError(503, 'busy', 'authority is busy');
        return running(operationId);
      },
    });

    await assert.rejects(fixture.execute(), (error) => {
      assert.equal(error.code, 'busy', 'a non-absence rejection keeps its mapped code');
      assert.equal(error.status, 503, 'a non-absence rejection keeps its mapped status');
      return true;
    });
    assert.equal(fixture.admitted.length, 1, 'the authority had already admitted the run');
    assert.equal(
      fixture.registry.operationRecord(fixture.admitted[0]),
      undefined,
      'a failed sweep must not register the new run',
    );
    assert.notEqual(fixture.registry.operationRecord(live), undefined, 'a rejection never retires a record');
    assert.notEqual(fixture.registry.operationRecord(refusing), undefined, 'a rejection never retires its own record');
  });

  test('a full arm is swept in one wave without widening past the retention bound', async () => {
    const cap = cfg.REGISTRY_MAX_ACTOR_OPERATIONS;
    const retained = Array.from({ length: cap }, () => randomUUID());
    const fixture = admission({ retained, characterOf: running });
    await fixture.execute();

    assert.equal(fixture.characterReads.length, cap, 'every retained record is read');
    assert.equal(fixture.peak(), cap, 'the retention cap bounds the wave, and the wave issues every read at once');
    assert.equal(
      new Set(fixture.characterReads.map((read) => read.principal)).size,
      1,
      `${cap} records still cost one principal`,
    );
    assert.equal(fixture.registry.actorBackedOperations().length, cap, 'the arm stays inside its bound');
  });
});
