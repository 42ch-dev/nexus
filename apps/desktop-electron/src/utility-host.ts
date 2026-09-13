/**
 * Electron utility-process owner for @42ch/nexus-native + @42ch/nexus-provider-acp.
 * Loaded only via utilityProcess.fork — never in the sandboxed renderer.
 */
import { createAcpProvider } from '@42ch/nexus-provider-acp';
import type {
  NativeOpenOptions,
  ProviderCall,
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';
import {
  nativeCompatibility,
  openCore,
  type NativeCore,
  type PrincipalHandle,
} from '@42ch/nexus-native';
import {
  CLOSE_JOIN_MS,
  MAX_ACTIVE_CALLS,
  MAX_BATCH_BYTES,
  MAX_PENDING_BYTES,
  MAX_PENDING_CALLS,
  MAX_REQUEST_BYTES,
  clampPullBounds,
  errorCode,
  errorMessage,
  estimatePayloadBytes,
  ipcErr,
  ipcOk,
  parseIpcRequest,
  type IpcRequest,
  type IpcResponse,
  type LifecyclePhase,
  type UtilityConfig,
} from './ipc.js';
import { PullReservationLedger } from './utility-admission.js';

interface OwnerState {
  phase: LifecyclePhase;
  core: NativeCore | null;
  principal: PrincipalHandle | null;
  acceptingWork: boolean;
  closeReport: unknown | null;
  lastError: string | null;
}

interface QueuedWork {
  request: IpcRequest;
  payloadBytes: number;
}

const state: OwnerState = {
  phase: 'idle',
  core: null,
  principal: null,
  acceptingWork: true,
  closeReport: null,
  lastError: null,
};

let frozenConfig: UtilityConfig | null = null;
let closePromise: Promise<IpcResponse> | null = null;
let activeCount = 0;
let queuedCount = 0;
let queuedBytes = 0;
const pendingOperationIds: string[] = [];
const workQueue: QueuedWork[] = [];
const pullReservations = new PullReservationLedger();
let draining = false;

function readConfig(): UtilityConfig {
  if (!frozenConfig) {
    const raw = process.env.NEXUS_PROOF_UTILITY_CONFIG;
    if (!raw) {
      throw new Error('NEXUS_PROOF_UTILITY_CONFIG missing — main must inject frozen open options');
    }
    frozenConfig = JSON.parse(raw) as UtilityConfig;
  }
  return frozenConfig;
}

function lifecycleSnapshot() {
  return {
    phase: state.phase,
    owner_alive: state.phase !== 'closed' && state.phase !== 'interrupted',
    cleanup_confirmed:
      state.closeReport && typeof state.closeReport === 'object' && state.closeReport !== null
        ? (state.closeReport as { cleanup_confirmed?: boolean }).cleanup_confirmed ?? null
        : null,
    pending_operations: [...pendingOperationIds],
    reason: state.lastError,
    last_close_report: state.closeReport,
  };
}

function rejectIfClosing(): void {
  if (!state.acceptingWork || state.phase === 'closing' || state.phase === 'closed' || state.phase === 'interrupted') {
    throw Object.assign(new Error('owner is closing or interrupted'), { code: 'closing' });
  }
}

function trackOperationStart(requestId: string): void {
  pendingOperationIds.push(requestId);
}

function trackOperationEnd(requestId: string, request: IpcRequest): void {
  const index = pendingOperationIds.indexOf(requestId);
  if (index >= 0) pendingOperationIds.splice(index, 1);
  pullReservations.release(request);
}

function admitOrReject(request: IpcRequest): IpcResponse | null {
  const payloadBytes = estimatePayloadBytes(request.payload);
  if (payloadBytes > MAX_REQUEST_BYTES) {
    return ipcErr(request.request_id, 'input_too_large', 'request payload exceeds 1 MiB');
  }
  if (request.operation === 'pull') {
    const body = request.payload as { operation_id?: string } | undefined;
    if (typeof body?.operation_id !== 'string' || body.operation_id.length === 0) {
      return ipcErr(request.request_id, 'invalid_input', 'operation_id is required');
    }
    if (pullReservations.isReserved(body.operation_id)) {
      return ipcErr(request.request_id, 'busy', 'one outstanding pull per operation');
    }
  }
  if (activeCount < MAX_ACTIVE_CALLS) {
    if (request.operation === 'pull' && !pullReservations.tryReserve(request)) {
      return ipcErr(request.request_id, 'busy', 'one outstanding pull per operation');
    }
    return null;
  }
  if (queuedCount >= MAX_PENDING_CALLS || queuedBytes + payloadBytes > MAX_PENDING_BYTES) {
    return ipcErr(request.request_id, 'busy', 'utility admission cap exceeded');
  }
  if (request.operation === 'pull' && !pullReservations.tryReserve(request)) {
    return ipcErr(request.request_id, 'busy', 'one outstanding pull per operation');
  }
  return null;
}

async function runRequest(request: IpcRequest): Promise<void> {
  activeCount += 1;
  trackOperationStart(request.request_id);
  try {
    const response = await dispatch(request);
    port.postMessage(response);
  } catch (err) {
    port.postMessage(ipcErr(request.request_id, errorCode(err), errorMessage(err)));
  } finally {
    trackOperationEnd(request.request_id, request);
    activeCount -= 1;
    void drainQueue();
  }
}

async function drainQueue(): Promise<void> {
  if (draining) return;
  draining = true;
  try {
    while (activeCount < MAX_ACTIVE_CALLS && workQueue.length > 0) {
      const next = workQueue.shift();
      if (!next) break;
      queuedCount -= 1;
      queuedBytes -= next.payloadBytes;
      await runRequest(next.request);
    }
  } finally {
    draining = false;
  }
}

function enqueue(request: IpcRequest, payloadBytes: number): void {
  workQueue.push({ request, payloadBytes });
  queuedCount += 1;
  queuedBytes += payloadBytes;
}

async function handleCompatibility(request_id: string): Promise<IpcResponse> {
  return ipcOk(request_id, nativeCompatibility());
}

async function handleOpen(request_id: string): Promise<IpcResponse> {
  rejectIfClosing();
  if (state.core) {
    return ipcErr(request_id, 'owner_busy', 'native core already open in this utility owner');
  }
  const config = readConfig();
  state.phase = 'starting';
  const options: NativeOpenOptions = {
    user_home: config.user_home,
    access: config.access,
    allow_uninitialized: config.allow_uninitialized,
  };
  const providers = createAcpProvider();
  state.core = await openCore(options, providers);
  state.principal = await state.core.activePrincipal();
  state.phase = 'open';
  return ipcOk(request_id, { readiness: 'open', principal: 'opaque' });
}

async function handleGraph(request_id: string, payload: unknown): Promise<IpcResponse> {
  rejectIfClosing();
  if (!state.core || !state.principal) {
    return ipcErr(request_id, 'uninitialized', 'open the native core before graph');
  }
  const body = payload as { world_id?: string; include_suggested?: boolean };
  if (typeof body?.world_id !== 'string' || body.world_id.length === 0) {
    return ipcErr(request_id, 'invalid_input', 'world_id is required');
  }
  const graph = await state.core.worldKbGraph(
    state.principal,
    body.world_id,
    Boolean(body.include_suggested),
  );
  return ipcOk(request_id, graph);
}

async function handlePatch(request_id: string, payload: unknown): Promise<IpcResponse> {
  rejectIfClosing();
  if (!state.core || !state.principal) {
    return ipcErr(request_id, 'uninitialized', 'open the native core before patch');
  }
  const body = payload as { world_id?: string; request?: WorldKbPatchEntityRequest };
  if (typeof body?.world_id !== 'string' || !body.request) {
    return ipcErr(request_id, 'invalid_input', 'world_id and request are required');
  }
  const encoded = Buffer.byteLength(JSON.stringify(body.request), 'utf8');
  if (encoded > MAX_REQUEST_BYTES) {
    return ipcErr(request_id, 'input_too_large', 'patch request too large');
  }
  const response = await state.core.patchWorldKbEntity(state.principal, body.world_id, body.request);
  return ipcOk(request_id, response);
}

async function handleProvider(request_id: string, payload: unknown): Promise<IpcResponse> {
  rejectIfClosing();
  if (!state.core) {
    return ipcErr(request_id, 'uninitialized', 'open the native core before provider');
  }
  const call = payload as ProviderCall;
  if (!call || typeof call !== 'object') {
    return ipcErr(request_id, 'invalid_input', 'provider payload required');
  }
  const encoded = Buffer.byteLength(JSON.stringify(call), 'utf8');
  if (encoded > MAX_REQUEST_BYTES) {
    return ipcErr(request_id, 'input_too_large', 'provider call too large');
  }
  const reply = await state.core.providerCall(call);
  return ipcOk(request_id, reply);
}

async function handlePull(request_id: string, payload: unknown): Promise<IpcResponse> {
  rejectIfClosing();
  if (!state.core) {
    return ipcErr(request_id, 'uninitialized', 'open the native core before pull');
  }
  const body = payload as { operation_id?: string; max_events?: unknown; max_bytes?: unknown };
  if (typeof body?.operation_id !== 'string') {
    return ipcErr(request_id, 'invalid_input', 'operation_id is required');
  }
  let bounds: { maxEvents: number; maxBytes: number };
  try {
    bounds = clampPullBounds(body.max_events, body.max_bytes);
  } catch (err) {
    return ipcErr(request_id, errorCode(err), errorMessage(err));
  }
  const batch = await state.core.nextProviderEvents(body.operation_id, bounds.maxEvents, bounds.maxBytes);
  const encoded = Buffer.byteLength(JSON.stringify(batch), 'utf8');
  if (encoded > MAX_BATCH_BYTES) {
    return ipcErr(request_id, 'delivery_overflow', 'provider batch exceeds bounded encode limit');
  }
  return ipcOk(request_id, batch);
}

async function handleClose(request_id: string): Promise<IpcResponse> {
  if (closePromise) {
    const prior = await closePromise;
    return prior.request_id === request_id ? prior : ipcOk(request_id, state.closeReport ?? lifecycleSnapshot());
  }
  state.acceptingWork = false;
  state.phase = 'closing';
  workQueue.length = 0;
  queuedCount = 0;
  queuedBytes = 0;
  closePromise = (async () => {
    const { promise: onTimeout, resolve: resolveTimeout } = Promise.withResolvers<IpcResponse>();
    const timer = setTimeout(() => {
      state.phase = 'interrupted';
      state.lastError = 'close join exceeded 5s';
      state.closeReport = {
        state: 'interrupted',
        cleanup_confirmed: false,
        pending_operations: [...pendingOperationIds],
        reason: state.lastError,
      };
      resolveTimeout(ipcErr(request_id, 'interrupted', state.lastError));
    }, CLOSE_JOIN_MS);

    const closeTask = (async (): Promise<IpcResponse> => {
      try {
        if (state.core) {
          const report = await state.core.close();
          state.closeReport = report;
          state.phase = report.state === 'closed' ? 'closed' : 'interrupted';
          if (report.state !== 'closed') {
            state.lastError = report.reason ?? 'interrupted';
          }
          return ipcOk(request_id, state.closeReport);
        }
        state.phase = 'closed';
        state.closeReport = { state: 'closed', cleanup_confirmed: true, pending_operations: [] };
        return ipcOk(request_id, state.closeReport);
      } catch (err) {
        state.phase = 'interrupted';
        state.lastError = errorMessage(err);
        state.closeReport = {
          state: 'interrupted',
          cleanup_confirmed: false,
          pending_operations: [...pendingOperationIds],
          reason: 'writer_fenced',
        };
        return ipcErr(request_id, errorCode(err), errorMessage(err));
      } finally {
        clearTimeout(timer);
        state.core = null;
        state.principal = null;
      }
    })();

    return Promise.race([closeTask, onTimeout]);
  })();
  return closePromise;
}

async function dispatch(message: IpcRequest): Promise<IpcResponse> {
  switch (message.operation) {
    case 'compatibility':
      return handleCompatibility(message.request_id);
    case 'open':
      return handleOpen(message.request_id);
    case 'graph':
      return handleGraph(message.request_id, message.payload);
    case 'patch':
      return handlePatch(message.request_id, message.payload);
    case 'provider':
      return handleProvider(message.request_id, message.payload);
    case 'pull':
      return handlePull(message.request_id, message.payload);
    case 'close':
      return handleClose(message.request_id);
    default:
      return ipcErr(message.request_id, 'invalid_input', `unsupported operation ${message.operation}`);
  }
}

const port = process.parentPort;
if (!port) {
  throw new Error('utility-host must run inside an Electron utility process');
}

port.on('message', (event) => {
  let request_id = 'unknown';
  let parsedForRelease: IpcRequest | null = null;
  try {
    const parsed = parseIpcRequest(event.data);
    parsedForRelease = parsed;
    request_id = parsed.request_id;
    if (parsed.operation === 'close') {
      state.acceptingWork = false;
    } else {
      rejectIfClosing();
    }
    const rejection = admitOrReject(parsed);
    if (rejection) {
      port.postMessage(rejection);
      return;
    }
    const payloadBytes = estimatePayloadBytes(parsed.payload);
    if (activeCount < MAX_ACTIVE_CALLS) {
      void runRequest(parsed).catch((err) => {
        pullReservations.release(parsed);
        port.postMessage(ipcErr(parsed.request_id, errorCode(err), errorMessage(err)));
      });
      return;
    }
    enqueue(parsed, payloadBytes);
  } catch (err) {
    if (parsedForRelease) {
      pullReservations.release(parsedForRelease);
    }
    port.postMessage(ipcErr(request_id, errorCode(err), errorMessage(err)));
  }
});

port.postMessage({ type: 'utility-ready', lifecycle: lifecycleSnapshot() });

function reportCrash(reason: unknown): void {
  state.phase = 'interrupted';
  state.acceptingWork = false;
  state.lastError = errorMessage(reason);
  port.postMessage({ type: 'utility-crashed', message: state.lastError, lifecycle: lifecycleSnapshot() });
  setImmediate(() => {
    process.exit(1);
  });
}

process.on('uncaughtException', (err) => {
  reportCrash(err);
});

process.on('unhandledRejection', (reason) => {
  reportCrash(reason);
});
