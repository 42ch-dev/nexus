/**
 * Electron utility-process owner of the app-managed TypeScript service
 * (v1.192 P0-T4). Loaded only via `utilityProcess.fork` — never in the
 * sandboxed renderer.
 *
 * The utility is the app's single native owner: it imports the existing
 * `startService(options): Promise<RunningService>` and closes the returned
 * handle, and it is the only process that loads the native binding, for the
 * user-confirmed local-state reset. It opens no core/provider owner of its own
 * — the proof shell's separate `openCore` owner and its
 * `compatibility/open/graph/patch/provider/pull` operation bodies are gone.
 *
 * Wire protocol (frozen by the plan's "TS-service lifecycle and quit"):
 * `{generation,request_id,operation,payload}` in, `{generation,request_id,ok,…}`
 * plus the `service-ready` discovery push out. Main is the only sender: a
 * renderer never reaches this process and never supplies a filesystem path.
 */

import { isAbsolute } from 'node:path';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
import { resetLocalState } from '@42ch/nexus-native';
import { startService, type RunningService, type ServiceOptions } from '@42ch/nexus-service';
import { isConfirmedCloseReport } from './lifecycle-coord.js';
import {
  SERVICE_CLOSE_BUDGET_MS,
  parseUtilityRequest,
  utilityErr,
  utilityError,
  utilityMessageCode,
  utilityMessageText,
  utilityOk,
  type ServiceReadyMessage,
  type UtilityCrashedMessage,
  type UtilityRequest,
  type UtilityReadyMessage,
} from './service-controller.js';
import { admitUtilityOperation, type ServiceOwnerState } from './utility-admission.js';

interface OwnerState {
  serviceState: ServiceOwnerState;
  running: RunningService | null;
}

const state: OwnerState = { serviceState: 'none', running: null };
let inFlight = false;

const port = process.parentPort;
if (!port) {
  throw new Error('utility-host must run inside an Electron utility process');
}

const post = (message: unknown): void => port.postMessage(message);

/** Main-owned trusted launch payload; `startService` re-validates every field. */
function requireServiceOptions(payload: unknown): ServiceOptions {
  if (!payload || typeof payload !== 'object') {
    throw utilityError('invalid_input', 'start payload must be the trusted service options');
  }
  if (!('home' in payload) || typeof payload.home !== 'string' || !isAbsolute(payload.home)) {
    throw utilityError('invalid_input', 'start requires an absolute trusted home');
  }
  if (!('host' in payload) || typeof payload.host !== 'string') {
    throw utilityError('invalid_input', 'start requires a host');
  }
  if (!('port' in payload) || typeof payload.port !== 'number') {
    throw utilityError('invalid_input', 'start requires a port');
  }
  const options = payload as ServiceOptions; // home/host/port admitted above; service validates the rest
  return options;
}

/** Reset carries the trusted home only, after main confirmed the service close. */
function requireResetHome(payload: unknown): string {
  if (!payload || typeof payload !== 'object' || !('home' in payload)) {
    throw utilityError('invalid_input', 'reset-local-state requires the trusted home');
  }
  const { home } = payload;
  if (typeof home !== 'string' || !isAbsolute(home)) {
    throw utilityError('invalid_input', 'reset-local-state requires an absolute trusted home');
  }
  return home;
}

function closeBudgetExceeded(): Promise<never> {
  const { promise, reject } = Promise.withResolvers<never>();
  setTimeout(
    () => reject(utilityError('interrupted', `service close exceeded the ${SERVICE_CLOSE_BUDGET_MS}ms budget`)),
    SERVICE_CLOSE_BUDGET_MS,
  ).unref?.();
  return promise;
}

/**
 * Cooperative close inside the frozen 5s budget. A confirmed report releases
 * the handle; an unconfirmed close retains it (and the published discovery) so
 * a retry can still confirm cleanup — never a successful forced cleanup.
 */
async function closeService(): Promise<CoreCloseReport> {
  const running = state.running;
  if (!running) {
    state.serviceState = 'closed';
    return { state: 'closed', cleanup_confirmed: true, pending_operations: [] };
  }
  state.serviceState = 'closing';
  let report: CoreCloseReport;
  try {
    report = await Promise.race([running.close(), closeBudgetExceeded()]);
  } catch (error) {
    state.serviceState = 'unconfirmed';
    throw error;
  }
  if (isConfirmedCloseReport(report)) {
    state.running = null;
    state.serviceState = 'closed';
  } else {
    state.serviceState = 'unconfirmed';
  }
  return report;
}

async function dispatch(request: UtilityRequest): Promise<unknown> {
  switch (request.operation) {
    case 'start': {
      const options = requireServiceOptions(request.payload);
      let running: RunningService;
      try {
        running = await startService(options);
      } catch (error) {
        state.running = null;
        state.serviceState = 'none';
        throw error;
      }
      state.running = running;
      state.serviceState = 'running';
      // The published, schema-derived discovery of the service that just
      // reached readiness — the same record `startService` returns.
      post({ type: 'service-ready', generation: request.generation, discovery: running.discovery } satisfies ServiceReadyMessage);
      return { discovery: running.discovery };
    }
    case 'close':
      return closeService();
    case 'reset-local-state': {
      const home = requireResetHome(request.payload);
      const removed = await resetLocalState(home);
      return { removed };
    }
  }
}

async function handleMessage(raw: unknown): Promise<void> {
  let generation = 0;
  let request_id = 'unknown';
  try {
    const request = parseUtilityRequest(raw);
    generation = request.generation;
    request_id = request.request_id;
    if (inFlight) {
      post(utilityErr(generation, request_id, 'busy', 'a lifecycle request is already in flight'));
      return;
    }
    const admission = admitUtilityOperation(request.operation, state.serviceState);
    if (!admission.ok) {
      post(utilityErr(generation, request_id, admission.code, admission.message));
      return;
    }
    inFlight = true;
    try {
      const result = await dispatch(request);
      post(utilityOk(generation, request_id, result));
    } catch (error) {
      post(utilityErr(generation, request_id, utilityMessageCode(error), utilityMessageText(error)));
    } finally {
      inFlight = false;
    }
  } catch (error) {
    // A frame that failed admission cannot be correlated beyond its own ids.
    post(utilityErr(generation, request_id, utilityMessageCode(error), utilityMessageText(error)));
  }
}

port.on('message', (event) => {
  void handleMessage(event.data);
});

post({ type: 'utility-ready' } satisfies UtilityReadyMessage);

function reportCrash(reason: unknown): void {
  post({ type: 'utility-crashed', message: utilityMessageText(reason) } satisfies UtilityCrashedMessage);
  setImmediate(() => {
    process.exit(1);
  });
}

process.on('uncaughtException', reportCrash);

process.on('unhandledRejection', reportCrash);
