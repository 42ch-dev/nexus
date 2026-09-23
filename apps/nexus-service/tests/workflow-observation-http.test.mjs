import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const acpFixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

const CREATOR = 'ctr_testcreator';
const SLUG = 'default';
const FOREIGN_CREATOR = 'other_creator';
const FOREIGN_SESSION_ID = 'sess_foreign_observation';

/** The selected deterministic ACP fixture peer every scheduled prompt binds to. */
const AGENT_BINDINGS = { default: { provider_id: 'mock-acp' } };

/**
 * P1-T3 bounded real-native target: the SAME-RUN observation transport
 * (`GET /v1/daemon/orchestration/sessions/{run_id}/events`).
 *
 * Every observation below comes from the real compiled native addon behind the
 * real in-process service over a real HTTP listener and a real seeded store:
 * the subscription authority, the run's own event ring, its caps, the
 * `<epoch>:<sequence>` cursor and the durable run rows are the production
 * owners. Nothing here stages a ring, seeds a frame or mocks a pull. The
 * deterministic no-model ACP fixture peer is the only peer (its `prompt` mode
 * returns a transformed reply and no model request is involved anywhere).
 *
 * A restart is a real reopen of the same home: the new owner has no ring for
 * the durable run, which is exactly the state the transport must answer
 * truthfully.
 */

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return spawnSync('python3', ['-c', `import os,sys;print(os.path.realpath(sys.argv[1]))`, which], {
    encoding: 'utf8',
  }).stdout.trim();
}

/**
 * The selected ACP fixture peer with `OVERSIZED_UPDATE` on: its prompt reply is
 * one ~384 KiB chunk, so a run that walks several prompt boundaries pushes its
 * own ring past the frozen 1 MiB per-run byte cap and the ring trims its oldest
 * records — the condition a LATE subscriber must see as an explicit gap rather
 * than a silently short history. Runs with a single prompt stay well under the
 * cap and are unaffected by the flag.
 */
function acpProviderConfig() {
  return `
[[providers]]
id = "mock-acp"
protocol = "acp"
command = ${JSON.stringify(resolvePython())}
args = [${JSON.stringify(acpFixture)}]
enabled = true
[providers.env]
OVERSIZED_UPDATE = "1"
`;
}

/**
 * Seed one disposable home: the shared native wire fixture plus the two things
 * an initialized profile needs — the agent-host provider selection and the
 * selected workspace's registered creative root, written the way
 * `nexus42 creator workspace create --creative-root <abs>` writes it.
 */
function seededHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-p1t3-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root, stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const agentHostDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  writeFileSync(join(agentHostDir, 'config.toml'), acpProviderConfig());
  const creativeRoot = join(home, 'creative-root');
  mkdirSync(creativeRoot, { recursive: true });
  writeFileSync(
    join(home, '.nexus42', 'creators', CREATOR, 'workspaces', SLUG, 'meta.json'),
    JSON.stringify({ local_root: creativeRoot }),
  );
  return home;
}

/**
 * One durable ROOT run row owned by a DIFFERENT creator, in the same workspace
 * store the service will open. It is written with the engine writer identity
 * the fixture's own `init_engine_pool` committed (the store validates every
 * write through connection-local scalar functions), so a subscription that
 * ignored the stored owner would find a readable run there.
 */
function seedForeignRun(home) {
  const script = `
import sqlite3, glob, sys
path = glob.glob(sys.argv[1] + "/.nexus42/creators/*/workspaces/*/state.db")[0]
conn = sqlite3.connect(path)
gate = conn.execute("SELECT migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk=1").fetchone()
wid = conn.execute("SELECT writer_id FROM core_writer_registration WHERE mode='engine' AND engine_epoch=?", (gate[1],)).fetchone()[0]
conn.create_function("nexus_writer_protocol", 0, lambda: 1)
conn.create_function("nexus_writer_mode", 0, lambda: "engine")
conn.create_function("nexus_writer_id", 0, lambda: wid)
conn.create_function("nexus_migration_epoch", 0, lambda: gate[0])
conn.create_function("nexus_engine_epoch", 0, lambda: gate[1])
conn.execute("INSERT INTO orchestration_sessions (session_id, creator_id, preset_id, preset_version, parent_session_id, current_task_id, status, context_json, created_at, updated_at) VALUES (?, ?, 'foreign-preset', 1, NULL, NULL, 'completed', ?, 1, 1)", (${JSON.stringify(
    FOREIGN_SESSION_ID,
  )}, ${JSON.stringify(FOREIGN_CREATOR)}, b"{}"))
conn.commit()
`;
  const seeded = spawnSync('python3', ['-c', script, home], { encoding: 'utf8' });
  assert.equal(seeded.status, 0, seeded.stderr || seeded.stdout);
}

/**
 * One prompt boundary followed by an unreachable-branch converge gate: the
 * admitted run dispatches a real `acp.prompt` (which publishes host-event
 * frames on its own ring) and then parks durably at the gate, so a subscriber
 * sees retained frames on a LIVE run — the state the same-run stream, its
 * cursor and its release semantics all have to hold in.
 */
function observedParkPresetYaml(id) {
  return `preset:
  id: ${id}
  version: 1
  kind: creator
  description: "P1-T3 observation fixture: one prompt boundary, then an unreachable-branch converge gate parks the run"
  requires_capabilities: [acp.prompt]
  initial: start
  terminal: done
states:
  - id: start
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "P1T3-OBSERVE"
          tool_policy: deny_all
    next: branch_a
  - id: branch_a
    next:
      branches: []
      default: join
  - id: branch_b
    description: "Hanging upstream edge — never walked, never arrives"
    next: join
  - id: join
    converge: { strategy: wait_for_all }
    next: done
  - id: done
    terminal: true
`;
}

/**
 * Four oversized prompt boundaries in a row and a terminal state: this run's
 * ring exceeds its 1 MiB per-run byte cap while it is running, so its oldest
 * records are gone by the time a LATE subscriber arrives.
 */
function ringTrimPresetYaml(id) {
  const boundary = (stateId, next) => `  - id: ${stateId}
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "P1T3-TRIM:${stateId}"
          tool_policy: deny_all
    next: ${next}`;
  const states = ['first', 'second', 'third', 'fourth'];
  return `preset:
  id: ${id}
  version: 1
  kind: creator
  description: "P1-T3 bounded-history fixture: four oversized prompt boundaries exceed the run ring byte cap"
  requires_capabilities: [acp.prompt]
  initial: first
  terminal: done
states:
${states.map((stateId, index) => boundary(stateId, states[index + 1] ?? 'done')).join('\n')}
  - id: done
    terminal: true
`;
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

async function waitFor(read, done, { label, timeout = 30_000 } = {}) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const value = await read();
    if (done(value)) return value;
    if (Date.now() > deadline) {
      assert.fail(`${label ?? 'waitFor'} did not settle within ${timeout}ms`);
    }
    await delay(100);
  }
}

async function jsonFetch(url, { method = 'GET', body, headers = {} } = {}) {
  const response = await fetch(url, {
    method,
    headers,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return {
    status: response.status,
    contentType: response.headers.get('content-type') ?? '',
    payload,
    text,
  };
}

async function startServiceOn(home) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port: 0, allowRemote: false });
}

/**
 * One SSE block's fields exactly as the wire carried them. `has('id')` is
 * load-bearing: a control frame that must NOT carry a cursor omits the `id`
 * LINE entirely, which is a different wire fact from `id: ` — an empty id
 * field would reset a client's last-event-id instead of leaving it alone.
 */
function parseSseFields(block) {
  const fields = new Map();
  for (const line of block.split('\n')) {
    const sep = line.indexOf(':');
    if (sep <= 0) continue;
    const name = line.slice(0, sep);
    const value = line.slice(sep + 1).replace(/^ /, '');
    const values = fields.get(name);
    if (values === undefined) fields.set(name, [value]);
    else values.push(value);
  }
  return fields;
}

function frameOf(fields) {
  return {
    hasId: fields.has('id'),
    id: fields.get('id')?.[0] ?? '',
    event: fields.get('event')?.[0] ?? '',
    data: fields.get('data')?.join('\n') ?? '',
  };
}

function sequenceOf(frame) {
  return Number.parseInt(frame.id.slice(frame.id.lastIndexOf(':') + 1), 10);
}

function eventsUrl(base, runId) {
  return `${base}/v1/daemon/orchestration/sessions/${runId}/events`;
}

/**
 * The read side of ONE SSE response, kept across steps: the same connection is
 * read again after the run is signalled, so the live tail is observed on the
 * subscription that already delivered the retained replay — never a second
 * subscription pretending to be the same stream.
 */
class EventReader {
  constructor(response) {
    this.response = response;
    this.reader = response.body.getReader();
    this.decoder = new TextDecoder();
    this.buffer = '';
    this.frames = [];
    this.ended = false;
    this.pending = null;
  }

  /** Parse every whole frame the buffer already holds. */
  drainBuffer() {
    for (;;) {
      const end = this.buffer.indexOf('\n\n');
      if (end < 0) return;
      const block = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end + 2);
      if (block.trim().length > 0) this.frames.push(frameOf(parseSseFields(block)));
    }
  }

  /**
   * The single outstanding read is cached and reused: a window that closes
   * early must not leave two readers racing for the same connection.
   */
  next() {
    this.pending ??= this.reader.read().then(
      (read) => read,
      () => null,
    );
    return this.pending;
  }

  /** One step of stream progress: `false` when the window closed or it ended. */
  async step(timeoutMs) {
    const read = await Promise.race([this.next(), delay(timeoutMs).then(() => null)]);
    if (read === null) return false;
    if (read.done) {
      this.ended = true;
      return false;
    }
    this.pending = null;
    this.buffer += this.decoder.decode(read.value, { stream: true });
    this.drainBuffer();
    return true;
  }

  /** Read until `count` frames have ARRIVED on this stream, or it ended. */
  async read(count, timeoutMs = 8_000) {
    const deadline = Date.now() + timeoutMs;
    while (this.frames.length < count && !this.ended) {
      const remaining = deadline - Date.now();
      if (remaining <= 0) break;
      if (!(await this.step(remaining))) break;
    }
    return this.frames.length >= count;
  }

  /** Read until the server closes the stream (or the window closes). */
  async readToEnd(timeoutMs = 20_000) {
    const deadline = Date.now() + timeoutMs;
    while (!this.ended) {
      const remaining = deadline - Date.now();
      if (remaining <= 0) break;
      if (!(await this.step(remaining))) break;
    }
    return this.ended;
  }

  /** Disconnect exactly as a closed tab does: the request is aborted. */
  async disconnect() {
    await this.reader.cancel().catch(() => undefined);
  }
}

/**
 * Subscribe and fail loudly on anything but an admitted SSE stream. The resume
 * cursor is the retained `Last-Event-ID` header — the ONLY place this transport
 * reads a cursor from.
 */
async function openEvents(base, runId, lastEventId) {
  const response = await fetch(eventsUrl(base, runId), {
    ...(lastEventId === undefined ? {} : { headers: { 'Last-Event-ID': lastEventId } }),
  });
  assert.equal(response.status, 200, `event stream must open: ${response.status}`);
  assert.match(
    response.headers.get('content-type') ?? '',
    /text\/event-stream/,
    'an admitted observation is an SSE stream',
  );
  return new EventReader(response);
}

describe('workflow-observation-http (v1.195 P1-T3 same-run SSE transport)', () => {
  let home;
  let service;
  let base;
  let parkScheduleId;
  let parkRunId;
  let capRunId;

  /**
   * Author one preset through the public surface, create one schedule of it and
   * wait for its owned run. Admission is asynchronous by contract, so the run
   * identity is observed by polling the public read, never assumed.
   *
   * `concurrency` is passed through when a case needs an INDEPENDENT run: the
   * scaffolded default is `serial`, which admits only while no other schedule of
   * the same creator is `running`. `parked` waits out the run's own drive to the
   * converge-gate park, so the ring under test is settled rather than mid-step.
   */
  async function admitRun(presetName, { yaml, concurrency, parked = false }) {
    const scaffolded = await jsonFetch(`${base}/v1/daemon/presets`, {
      method: 'POST',
      body: { name: presetName },
    });
    assert.equal(scaffolded.status, 201, scaffolded.text);
    const patched = await jsonFetch(`${base}/v1/daemon/presets/${presetName}`, {
      method: 'PATCH',
      body: { yaml: yaml(presetName) },
    });
    assert.equal(patched.status, 200, patched.text);
    assert.equal(patched.payload.updated, true, patched.text);

    const created = await jsonFetch(`${base}/v1/daemon/orchestration/schedules`, {
      method: 'POST',
      body: {
        creator_id: CREATOR,
        preset_id: presetName,
        agent_bindings: AGENT_BINDINGS,
        ...(concurrency === undefined ? {} : { concurrency }),
      },
    });
    assert.equal(created.status, 201, created.text);
    const scheduleId = created.payload.schedule_id;
    const inspect = async () => {
      const response = await jsonFetch(`${base}/v1/daemon/orchestration/schedules/${scheduleId}`);
      assert.equal(response.status, 200, response.text);
      return response.payload;
    };
    const admitted = await waitFor(inspect, (payload) => Boolean(payload.schedule.current_session_id), {
      label: `schedule ${scheduleId} owned run identity`,
    });
    const runId = admitted.schedule.current_session_id;

    if (parked) {
      // A still-driving run shows itself as `running` inside a quiet window;
      // only the committed gate park stays put.
      await waitFor(
        async () => {
          const first = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${runId}`);
          assert.equal(first.status, 200, first.text);
          if (first.payload.session.status !== 'paused') return false;
          await delay(500);
          const second = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${runId}`);
          return second.payload.session.status === 'paused';
        },
        (steady) => steady === true,
        { label: `run ${runId} steady converge-gate park`, timeout: 30_000 },
      );
    }
    return { scheduleId, runId };
  }

  before(async () => {
    // The Rust addon carries the subscription family, the TS facade declares it
    // and the service compiles against that declaration: all three are built
    // here (each is incremental after the first run).
    assert.equal(
      spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], { cwd: root, stdio: 'inherit' })
        .status,
      0,
    );
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'packages/nexus-native/tsconfig.json'], {
        cwd: root,
        stdio: 'inherit',
      }).status,
      0,
    );
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
      0,
    );

    // The first-pull gate defaults to 450 ms; this file holds many streams, so
    // it takes the retained test-only override to its smallest legal value.
    process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = '1';
    home = seededHome();
    seedForeignRun(home);
    service = await startServiceOn(home);
    base = service.url;
    ({ scheduleId: parkScheduleId, runId: parkRunId } = await admitRun('p1t3-observe-park', {
      yaml: observedParkPresetYaml,
      parked: true,
    }));
    // A SECOND independent live run: the subscriber-permit case must not share
    // a ring with an earlier case whose readers may still be draining.
    ({ runId: capRunId } = await admitRun('p1t3-observe-cap', {
      yaml: observedParkPresetYaml,
      concurrency: 'parallel_any',
      parked: true,
    }));
  });

  after(async () => {
    delete process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
    if (service) await service.close();
  });

  test('same run replay: retained frames carry the inspected root run id and the cursor is exact', async () => {
    // W4 is the authority for "this run": the stream must name THAT session id,
    // never a Host session and never a freshly created prompt.
    const inspected = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${parkRunId}`);
    assert.equal(inspected.status, 200, inspected.text);
    assert.equal(inspected.payload.session.session_id, parkRunId, inspected.text);

    const stream = await openEvents(base, parkRunId);
    try {
      assert.equal(
        await stream.read(2, 8_000),
        true,
        'the live run must replay at least two retained frames',
      );
      for (const frame of stream.frames) {
        assert.equal(frame.hasId, true, `every data frame carries its core cursor: ${JSON.stringify(frame)}`);
        assert.match(frame.id, /^[0-9a-f-]{36}:\d+$/, 'the cursor is `<UUID epoch>:<decimal sequence>`');
        assert.ok(Number.isSafeInteger(sequenceOf(frame)), `cursor sequence parses: ${frame.id}`);
        assert.ok(
          ['host_event', 'run_state'].includes(frame.event),
          `a same-run data frame keeps the ring vocabulary: ${frame.event}`,
        );
        assert.equal(
          JSON.parse(frame.data).run_id,
          parkRunId,
          'the frame belongs to the inspected root run',
        );
      }
      assert.ok(
        stream.frames.some((frame) => frame.event === 'run_state'),
        `the durable run state is observable on its own ring: ${JSON.stringify(stream.frames.map((f) => f.event))}`,
      );

      // A numeric-only cursor is NOT a substitute for the ring's
      // `<epoch>:<sequence>`: it is refused before any frame is written.
      const numeric = await jsonFetch(eventsUrl(base, parkRunId), {
        headers: { 'Last-Event-ID': '7' },
      });
      assert.equal(numeric.status, 400, numeric.text);
      assert.equal(numeric.payload.error.code, 'invalid_input', numeric.text);
      assert.equal(numeric.payload.error.details?.field, 'last_event_id', numeric.text);
      assert.doesNotMatch(numeric.contentType, /event-stream/, numeric.contentType);

      // A cursor AHEAD of the retained events is the same typed refusal.
      const epoch = stream.frames[0].id.slice(0, stream.frames[0].id.indexOf(':'));
      const future = await jsonFetch(eventsUrl(base, parkRunId), {
        headers: { 'Last-Event-ID': `${epoch}:99999999` },
      });
      assert.equal(future.status, 400, future.text);
      assert.equal(future.payload.error.code, 'invalid_input', future.text);

      // A cursor naming an epoch this process never minted is NOT an error: the
      // run is owned and readable, so the stream opens, carries ONE explicit
      // `history_unavailable` control frame and closes — never an invented
      // history and never a silent empty 200.
      const priorEpoch = await openEvents(
        base,
        parkRunId,
        '00000000-0000-0000-0000-000000000000:1',
      );
      assert.equal(
        await priorEpoch.readToEnd(8_000),
        true,
        'the unresumable stream closes itself after its control frame',
      );
      assert.equal(priorEpoch.frames.length, 1, JSON.stringify(priorEpoch.frames));
      const [control] = priorEpoch.frames;
      assert.equal(control.event, 'history_unavailable', JSON.stringify(control));
      assert.equal(
        control.hasId,
        false,
        'a control frame carrying no cursor must write no id line',
      );
      assert.equal(JSON.parse(control.data).run_id, parkRunId, JSON.stringify(control));
      assert.equal(
        JSON.parse(control.data).inspect_url,
        `/v1/daemon/orchestration/sessions/${parkRunId}`,
        'the control frame names the public inspect URL of the SAME run',
      );

      // Resume from an already-delivered frame: every frame the resumed
      // subscription delivers is strictly AFTER it, and the replay starts at the
      // very next retained frame — no duplicate, no skipped handoff.
      const resumed = await openEvents(base, parkRunId, stream.frames[0].id);
      try {
        await resumed.read(1, 8_000);
        assert.ok(resumed.frames.length >= 1, 'the retained tail after the cursor is replayed');
        for (const frame of resumed.frames) {
          assert.ok(
            sequenceOf(frame) > sequenceOf(stream.frames[0]),
            `exclusive replay must not re-send a delivered frame: ${frame.id}`,
          );
        }
        assert.equal(
          resumed.frames[0].id,
          stream.frames[1].id,
          'the replay resumes at the very next retained frame',
        );
      } finally {
        await resumed.disconnect();
      }
    } finally {
      await stream.disconnect();
    }
  });

  test('same run replay: a disconnected reader releases the run subscriber permit', async () => {
    // The per-run subscriber cap is the core ring's own limit (16). Holding
    // exactly that many open streams is how "released on disconnect" becomes
    // observable from outside: while they are held, the next subscription is the
    // pre-header refusal; after the readers leave, the permit is reusable.
    const capRunUrl = eventsUrl(base, capRunId);
    const held = [];
    for (let index = 0; index < 16; index += 1) {
      const response = await fetch(capRunUrl);
      assert.equal(response.status, 200, `subscription ${index + 1} must be admitted`);
      held.push(response);
    }
    const overflow = await jsonFetch(capRunUrl);
    assert.equal(overflow.status, 503, overflow.text);
    assert.equal(overflow.payload.error.code, 'busy', overflow.text);
    assert.doesNotMatch(overflow.contentType, /event-stream/, overflow.contentType);

    for (const response of held) {
      await response.body.cancel().catch(() => undefined);
    }

    // The release belongs to the core owner and lands with the socket close, so
    // the permit must become reusable inside a bounded window.
    const reopened = await waitFor(
      async () => {
        const response = await fetch(capRunUrl);
        if (response.status === 200) return response;
        await response.body.cancel().catch(() => undefined);
        return response;
      },
      (response) => response.status === 200,
      { label: `run ${capRunId} released subscriber permit`, timeout: 15_000 },
    );
    await reopened.body.cancel().catch(() => undefined);

    // The runs are left exactly as admitted: releasing a reader never settles
    // the run it was observing.
    const capInspect = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${capRunId}`);
    assert.equal(capInspect.status, 200, capInspect.text);
    assert.equal(capInspect.payload.session.status, 'paused', capInspect.text);
  });

  test('same run replay: a live subscriber sees the durable cancel terminal and the stream closes', async () => {
    const stream = await openEvents(base, parkRunId);
    try {
      assert.equal(await stream.read(1, 8_000), true, 'the retained replay must arrive before the signal');
      const replayed = stream.frames.length;

      const cancelled = await jsonFetch(
        `${base}/v1/daemon/orchestration/schedules/${parkScheduleId}/signal`,
        { method: 'POST', body: { signal: 'cancel' } },
      );
      assert.equal(cancelled.status, 200, cancelled.text);
      assert.equal(cancelled.payload.status, 'cancelled', cancelled.text);

      // The SAME subscription must carry the live terminal frame and then end —
      // a closed stream, not a hang and not a fabricated event.
      assert.equal(await stream.readToEnd(20_000), true, 'the stream closes after its durable final frame');
      const live = stream.frames.slice(replayed);
      assert.ok(live.length >= 1, 'the live tail must be delivered on the open subscription');
      const last = live[live.length - 1];
      assert.equal(JSON.parse(last.data).run_id, parkRunId, JSON.stringify(last));
      assert.equal(
        JSON.parse(last.data).status,
        'cancelled',
        `the live tail ends on the durable cancel frame: ${JSON.stringify(live.map((f) => f.data))}`,
      );
      for (const frame of stream.frames) {
        assert.ok(
          frame.id === '' || sequenceOf(frame) > 0,
          `every delivered cursor stays the ring's own: ${frame.id}`,
        );
      }

      const settled = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${parkRunId}`);
      assert.equal(settled.status, 200, settled.text);
      assert.equal(settled.payload.session.status, 'cancelled', settled.text);
    } finally {
      await stream.disconnect();
    }
  });

  test('same run replay: a trimmed ring answers an explicit gap, never fabricated history', async () => {
    const { runId } = await admitRun('p1t3-ring-trim', {
      yaml: ringTrimPresetYaml,
      concurrency: 'parallel_any',
    });

    // The run reaches its declared terminal state through its own engine; its
    // ring then closes and stays readable as a terminal ring.
    await waitFor(
      async () => {
        const response = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${runId}`);
        assert.equal(response.status, 200, response.text);
        return response.payload;
      },
      (payload) => payload.session.status === 'completed',
      { label: `run ${runId} terminal settlement`, timeout: 120_000 },
    );

    const stream = await openEvents(base, runId);
    try {
      assert.equal(await stream.readToEnd(30_000), true, 'a terminal run closes its stream');
      assert.ok(
        stream.frames.length >= 2,
        `the trimmed ring still delivers its retained tail: ${stream.frames.length}`,
      );

      const [head, ...tail] = stream.frames;
      assert.equal(
        head.event,
        'gap',
        `a trimmed replay opens with the ring's explicit gap: ${JSON.stringify(head.event)}`,
      );
      const gap = JSON.parse(head.data);
      assert.equal(gap.run_id, runId, JSON.stringify(gap));
      assert.equal(gap.from_sequence, 1, 'the gap names the range that is gone, from the first dropped record');
      assert.ok(gap.to_sequence >= gap.from_sequence, JSON.stringify(gap));
      assert.equal(
        sequenceOf(tail[0]),
        gap.to_sequence + 1,
        'the retained frames resume exactly where the gap ends — nothing fabricated in between',
      );

      let last = 0;
      for (const frame of stream.frames) {
        if (frame.event === 'gap') continue;
        assert.equal(JSON.parse(frame.data).run_id, runId, JSON.stringify(frame.event));
        assert.ok(sequenceOf(frame) > last, `frames stay strictly ordered: ${frame.id}`);
        last = sequenceOf(frame);
      }
      const finalFrame = stream.frames[stream.frames.length - 1];
      assert.equal(
        JSON.parse(finalFrame.data).status,
        'completed',
        `the durable final frame is the last word: ${JSON.stringify(finalFrame.data)}`,
      );

      // Reconnect from a delivered cursor: the tail after it is replayed
      // EXCLUSIVELY and in full, with no re-sent and no skipped frame.
      const cursorIndex = Math.max(0, stream.frames.length - 2);
      const cursor = stream.frames[cursorIndex].id;
      const resumed = await openEvents(base, runId, cursor);
      try {
        assert.equal(await resumed.readToEnd(15_000), true, 'the resumed terminal stream closes itself');
        assert.deepEqual(
          resumed.frames.map((frame) => frame.id),
          stream.frames.slice(cursorIndex + 1).map((frame) => frame.id),
          'Last-Event-ID resumes exclusively and completely',
        );
      } finally {
        await resumed.disconnect();
      }
    } finally {
      await stream.disconnect();
    }
  });

  test('restart history: a restarted service answers history-unavailable and keeps durable truth', async () => {
    const beforeRestart = await jsonFetch(
      `${base}/v1/daemon/orchestration/sessions/${parkRunId}`,
    );
    assert.equal(beforeRestart.payload.session.status, 'cancelled', beforeRestart.text);

    await service.close();
    service = await startServiceOn(home);
    base = service.url;

    // A restart keeps the durable record: the same run is still inspectable and
    // still reports the cancel truth it settled with.
    const inspected = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${parkRunId}`);
    assert.equal(inspected.status, 200, inspected.text);
    assert.equal(inspected.payload.session.session_id, parkRunId, inspected.text);
    assert.equal(inspected.payload.session.status, 'cancelled', inspected.text);

    // The ring did not survive the process, and the transport says exactly
    // that: one control frame naming the same run and its inspect URL, then a
    // closed stream — never a history it no longer holds.
    const stream = await openEvents(base, parkRunId);
    try {
      assert.equal(
        await stream.readToEnd(8_000),
        true,
        'the post-restart stream closes after the control frame',
      );
      assert.equal(stream.frames.length, 1, JSON.stringify(stream.frames));
      const [control] = stream.frames;
      assert.equal(control.event, 'history_unavailable', JSON.stringify(control));
      assert.equal(control.hasId, false, 'the control frame carries no cursor');
      assert.deepEqual(
        JSON.parse(control.data),
        {
          run_id: parkRunId,
          inspect_url: `/v1/daemon/orchestration/sessions/${parkRunId}`,
        },
        JSON.stringify(control),
      );
    } finally {
      await stream.disconnect();
    }
  });

  test('foreign run: unknown, foreign and child session ids close before any SSE header', async () => {
    // Absent, foreign and child ids are ONE refusal: same status, same code, no
    // payload — so an unauthorized observer cannot tell them apart, and no ring,
    // epoch or cursor is consulted for any of them (S1-4 / O4).
    const refused = [];
    for (const id of [FOREIGN_SESSION_ID, 'sess_absent_observation', `${parkRunId}:child:step`]) {
      const response = await jsonFetch(eventsUrl(base, id));
      assert.equal(response.status, 404, `${id}: ${response.text}`);
      assert.equal(response.payload.error.code, 'not_found', `${id}: ${response.text}`);
      assert.doesNotMatch(response.contentType, /event-stream/, `${id}: ${response.contentType}`);
      refused.push(`${response.status} ${response.payload.error.code}`);
    }
    assert.deepEqual(
      [...new Set(refused)],
      ['404 not_found'],
      `foreign, absent and child ids stay indistinguishable: ${refused.join(' | ')}`,
    );

    // The identity really is observable: a run owned by the admitted creator
    // opens a stream on the very same route, so the refusal above is
    // authorization and not a missing route.
    const owned = await openEvents(base, capRunId);
    await owned.read(1, 15_000);
    assert.ok(owned.frames.length >= 1, 'the owned run opens its stream on the same identity');
    await owned.disconnect();

    // A query parameter this identity does not serve is refused, never silently
    // ignored into a full replay.
    const strayQuery = await jsonFetch(`${eventsUrl(base, capRunId)}?after_sequence=1`);
    assert.equal(strayQuery.status, 400, strayQuery.text);
    assert.equal(strayQuery.payload.error.code, 'invalid_input', strayQuery.text);
  });
});
