import {
  app,
  BrowserWindow,
  ipcMain,
  protocol,
  shell,
  utilityProcess,
  type IpcMainInvokeEvent,
  type UtilityProcess,
} from 'electron';
import { randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  assertNativePayloadPresent,
  buildUtilityConfig,
  nativeRefreshHint,
  repoRootFromMeta,
  resolveProofHome,
  sanitizeInheritedEnv,
} from './env.js';
import {
  CLOSE_JOIN_MS,
  DEFAULT_REQUEST_TIMEOUT_MS,
  INTERRUPT_NOTIFY_MS,
  MAX_ACTIVE_CALLS,
  MAX_PENDING_BYTES,
  MAX_PENDING_CALLS,
  MAX_REQUEST_BYTES,
  assertProofStep,
  errorMessage,
  estimatePayloadBytes,
  ipcErr,
  ipcOk,
  isIpcResponse,
  parseIpcRequest,
  type IpcResponse,
  type LifecycleStatus,
} from './ipc.js';
import {
  attachExistingReadiness,
  mergeUtilityReadyLifecycle,
  remainingCloseBudget,
  resolveCloseLifecycleAfterJoin,
  resolveOpenProofPolicy,
  shouldTreatUtilityExitAsUnexpected,
} from './lifecycle-coord.js';
import {
  allowNavigation,
  assertDistPresent,
  isAllowedExternalUrl,
  isProofOrigin,
  proofIndexUrl,
  registerProofProtocol,
  resolveDistRoot,
} from './protocol.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = repoRootFromMeta(import.meta.url);
const BUNDLE_ID = 'com.nexus42.rft-electron-proof';
const DISPLAY_NAME = 'Nexus RFT Feasibility';

interface PendingEntry {
  generation: number;
  payloadBytes: number;
  resolve: (response: IpcResponse) => void;
  timer: NodeJS.Timeout;
}

function resolveWebDistRoot(): string {
  const packaged = join(process.resourcesPath, 'web-dist');
  if (existsSync(join(packaged, 'index.html'))) {
    return packaged;
  }
  return resolveDistRoot(repoRoot);
}

const distRoot = resolveWebDistRoot();

let mainWindow: BrowserWindow | null = null;
let utility: UtilityProcess | null = null;
let utilityGeneration = 0;
let lifecycle: LifecycleStatus = {
  phase: 'idle',
  owner_alive: false,
  cleanup_confirmed: null,
  pending_operations: [],
  reason: null,
  last_close_report: null,
};
let closeInitiatedForGeneration: number | null = null;
let reopenRequired = false;
let rendererDetached = false;
let closeInFlight: Promise<void> | null = null;
const pendingRequests = new Map<string, PendingEntry>();

function syncPendingLifecycle(): void {
  setLifecycle({ pending_operations: [...pendingRequests.keys()] });
}

function setLifecycle(next: Partial<LifecycleStatus>): void {
  lifecycle = { ...lifecycle, ...next };
  for (const win of BrowserWindow.getAllWindows()) {
    win.webContents.send('nexus-proof:lifecycle-changed', lifecycle);
  }
}

function utilityEntryPath(): string {
  const unpacked = join(process.resourcesPath, 'app.asar.unpacked', 'dist', 'utility-host.js');
  if (existsSync(unpacked)) return unpacked;
  return join(__dirname, 'utility-host.js');
}

function preloadPath(): string {
  return join(__dirname, 'preload.js');
}

function settlePending(requestId: string, response: IpcResponse): boolean {
  const entry = pendingRequests.get(requestId);
  if (!entry) return false;
  clearTimeout(entry.timer);
  pendingRequests.delete(requestId);
  syncPendingLifecycle();
  entry.resolve(response);
  return true;
}

function settleAllPending(generation: number, code: string, message: string): void {
  for (const requestId of [...pendingRequests.keys()]) {
    const entry = pendingRequests.get(requestId);
    if (!entry || entry.generation !== generation) continue;
    settlePending(requestId, ipcErr(requestId, code, message));
  }
}

function applyResponseLifecycle(requestOperation: string | null, response: IpcResponse): void {
  if (response.ok && requestOperation === 'open') {
    setLifecycle({ phase: 'open', owner_alive: true, reason: null });
  }
  if (response.ok && requestOperation === 'close') {
    lifecycle.last_close_report = response.result;
  }
  if (!response.ok && requestOperation === 'close') {
    setLifecycle({
      phase: 'interrupted',
      owner_alive: false,
      cleanup_confirmed: false,
      reason: response.error.message,
      last_close_report: null,
    });
    reopenRequired = true;
  }
}

function onUtilityMessage(message: unknown, generation: number): void {
  if (generation !== utilityGeneration) return;
  const body = message as Record<string, unknown>;
  if (body && typeof body === 'object' && 'type' in body) {
    if (body.type === 'utility-ready') {
      const snapshot = body.lifecycle as LifecycleStatus | undefined;
      const merged = mergeUtilityReadyLifecycle(lifecycle, snapshot);
      setLifecycle({ ...merged, reason: null });
      return;
    }
    if (body.type === 'utility-crashed') {
      void handleUtilityCrash(generation, (body.lifecycle as LifecycleStatus | undefined)?.reason ?? 'utility crashed');
      return;
    }
    return;
  }
  if (!isIpcResponse(message)) return;
  const pendingEntry = pendingRequests.get(message.request_id);
  if (!pendingEntry || pendingEntry.generation !== generation) return;
  settlePending(message.request_id, message);
}

function spawnUtilityOwner(): UtilityProcess {
  utilityGeneration += 1;
  const generation = utilityGeneration;
  closeInitiatedForGeneration = null;
  const proofHome = resolveProofHome(process.env.NEXUS_PROOF_HOME, repoRoot);
  const config = buildUtilityConfig(proofHome);
  const env = sanitizeInheritedEnv(process.env);
  env.NEXUS_PROOF_UTILITY_CONFIG = JSON.stringify(config);

  const child = utilityProcess.fork(utilityEntryPath(), [], {
    serviceName: 'nexus-proof-native-owner',
    env,
    stdio: 'pipe',
  });

  child.on('spawn', () => {
    if (generation !== utilityGeneration) return;
    setLifecycle({ phase: 'starting', owner_alive: true, reason: null });
  });

  child.stdout?.on('data', (chunk) => {
    process.stderr.write(`[utility stdout] ${chunk.toString()}`);
  });
  child.stderr?.on('data', (chunk) => {
    process.stderr.write(`[utility stderr] ${chunk.toString()}`);
  });

  child.on('message', (msg) => onUtilityMessage(msg, generation));

  child.on('exit', (code) => {
    if (generation !== utilityGeneration) return;
    utility = null;
    settleAllPending(generation, 'interrupted', `utility exited unexpectedly (${code ?? 'unknown'})`);
    if (shouldTreatUtilityExitAsUnexpected(closeInitiatedForGeneration, generation)) {
      reopenRequired = true;
      setLifecycle({
        phase: 'interrupted',
        owner_alive: false,
        cleanup_confirmed: false,
        reason: `utility exited unexpectedly (${code ?? 'unknown'})`,
        pending_operations: [...pendingRequests.keys()],
      });
      notifyInterrupted();
    }
  });

  utility = child;
  return child;
}

async function killAndJoinUtility(generation: number, deadlineAt: number): Promise<boolean> {
  const proc = utility;
  if (!proc || generation !== utilityGeneration) {
    return true;
  }
  const budgetMs = remainingCloseBudget(deadlineAt);
  const { promise, resolve } = Promise.withResolvers<boolean>();
  let exitConfirmed = false;
  const onExit = () => {
    exitConfirmed = true;
    resolve(true);
  };
  proc.once('exit', onExit);
  proc.kill();
  setTimeout(() => resolve(exitConfirmed), budgetMs);
  const confirmed = await promise;
  if (confirmed && generation === utilityGeneration) {
    utility = null;
    return true;
  }
  reopenRequired = true;
  setLifecycle({
    phase: 'interrupted',
    owner_alive: Boolean(utility),
    cleanup_confirmed: false,
    reason: 'utility join unconfirmed — owner remains fenced',
    pending_operations: [...pendingRequests.keys()],
  });
  notifyInterrupted();
  return false;
}

async function handleUtilityCrash(generation: number, reason: string): Promise<void> {
  if (generation !== utilityGeneration) return;
  reopenRequired = true;
  setLifecycle({
    phase: 'interrupted',
    owner_alive: false,
    cleanup_confirmed: false,
    reason,
    pending_operations: [...pendingRequests.keys()],
  });
  settleAllPending(generation, 'interrupted', reason);
  notifyInterrupted();
  await killAndJoinUtility(generation, Date.now() + CLOSE_JOIN_MS);
}

function notifyInterrupted(): void {
  setTimeout(() => {
    if (lifecycle.phase === 'interrupted' && mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send('nexus-proof:lifecycle-changed', lifecycle);
    }
  }, Math.min(INTERRUPT_NOTIFY_MS, CLOSE_JOIN_MS));
}

function ensureUtilityOwner(): UtilityProcess {
  if (!utility) {
    return spawnUtilityOwner();
  }
  return utility;
}

async function utilityRequest(
  raw: unknown,
  opts?: { timeoutMs?: number; operationHint?: string },
): Promise<IpcResponse> {
  const request = parseIpcRequest(raw);
  if (reopenRequired && request.operation !== 'close') {
    return ipcErr(request.request_id, 'interrupted', 'prior owner interrupted — explicit reopen required');
  }
  if (lifecycle.phase === 'closing' && request.operation !== 'close') {
    return ipcErr(request.request_id, 'closing', 'owner is closing');
  }

  const payloadBytes = estimatePayloadBytes(request.payload);
  if (payloadBytes > MAX_REQUEST_BYTES) {
    return ipcErr(request.request_id, 'input_too_large', 'request payload exceeds 1 MiB');
  }
  if (pendingRequests.size >= MAX_ACTIVE_CALLS + MAX_PENDING_CALLS) {
    return ipcErr(request.request_id, 'busy', 'main admission cap exceeded');
  }
  let pendingBytes = 0;
  for (const entry of pendingRequests.values()) {
    pendingBytes += entry.payloadBytes;
  }
  if (pendingBytes + payloadBytes > MAX_PENDING_BYTES) {
    return ipcErr(request.request_id, 'busy', 'main pending byte cap exceeded');
  }

  ensureUtilityOwner();
  const activeGeneration = utilityGeneration;

  const { promise, resolve } = Promise.withResolvers<IpcResponse>();
  const timeoutMs = opts?.timeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
  const timer = setTimeout(() => {
    settlePending(
      request.request_id,
      ipcErr(request.request_id, 'timeout', `utility request timed out after ${timeoutMs}ms`),
    );
  }, timeoutMs);

  pendingRequests.set(request.request_id, {
    generation: activeGeneration,
    payloadBytes,
    resolve: (response) => {
      applyResponseLifecycle(opts?.operationHint ?? request.operation, response);
      resolve(response);
    },
    timer,
  });
  syncPendingLifecycle();

  utility?.postMessage({
    request_id: request.request_id,
    operation: request.operation,
    payload: request.payload,
  });

  return promise;
}

async function initiateOwnerClose(): Promise<void> {
  if (closeInFlight) return closeInFlight;
  const { promise, resolve } = Promise.withResolvers<void>();
  closeInFlight = promise;
  closeInitiatedForGeneration = utilityGeneration;
  setLifecycle({ phase: 'closing', owner_alive: Boolean(utility), reason: null });

  if (!utility) {
    setLifecycle({ phase: 'closed', owner_alive: false, cleanup_confirmed: true, reason: null });
    closeInFlight = null;
    resolve();
    return;
  }

  const generation = utilityGeneration;
  const deadlineAt = Date.now() + CLOSE_JOIN_MS;
  const request_id = randomUUID();
  let closeResponse: IpcResponse | null = null;
  try {
    closeResponse = await utilityRequest(
      { request_id, operation: 'close' },
      {
        timeoutMs: remainingCloseBudget(deadlineAt),
        operationHint: 'close',
      },
    );
  } finally {
    const confirmed = await killAndJoinUtility(generation, deadlineAt);
    const outcome = resolveCloseLifecycleAfterJoin({
      confirmed,
      closeOk: Boolean(closeResponse?.ok),
      ownerStillReferenced: Boolean(utility),
      cleanupConfirmed: closeResponse?.ok
        ? ((closeResponse.result as { cleanup_confirmed?: boolean })?.cleanup_confirmed ?? null)
        : null,
      closeErrorMessage: closeResponse && !closeResponse.ok ? closeResponse.error.message : undefined,
    });
    setLifecycle({
      phase: outcome.phase,
      owner_alive: outcome.owner_alive,
      cleanup_confirmed: outcome.cleanup_confirmed,
      reason: outcome.reason,
      last_close_report: closeResponse?.ok ? closeResponse.result : lifecycle.last_close_report,
    });
    reopenRequired = outcome.reopenRequired;
    closeInFlight = null;
    resolve();
  }
}

function assertProofSender(event: IpcMainInvokeEvent): void {
  if (!mainWindow || mainWindow.isDestroyed()) {
    throw Object.assign(new Error('proof window unavailable'), { code: 'invalid_sender' });
  }
  if (event.sender !== mainWindow.webContents) {
    throw Object.assign(new Error('ipc sender is not the selected proof window'), { code: 'invalid_sender' });
  }
  const frameUrl = event.senderFrame?.url ?? event.sender.getURL();
  if (!isProofOrigin(frameUrl)) {
    throw Object.assign(new Error('ipc sender frame is not nexus-proof origin'), { code: 'invalid_origin' });
  }
}

function hardenWindow(win: BrowserWindow): void {
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  win.webContents.on('will-navigate', (event, url) => {
    if (!allowNavigation(url)) event.preventDefault();
  });
  win.webContents.on('will-redirect', (event, url) => {
    if (!allowNavigation(url)) event.preventDefault();
  });
  win.webContents.on('render-process-gone', () => {
    rendererDetached = true;
    if (mainWindow === win) {
      mainWindow = null;
    }
    if (utility && (lifecycle.phase === 'open' || lifecycle.phase === 'starting')) {
      setLifecycle({
        phase: lifecycle.phase,
        owner_alive: true,
        reason:
          lifecycle.phase === 'starting'
            ? 'renderer crashed while owner starting — replacement may attach and await utility-ready'
            : 'renderer crashed — native owner remains authoritative',
        pending_operations: [...pendingRequests.keys()],
      });
      notifyInterrupted();
      return;
    }
    setLifecycle({
      phase: lifecycle.phase === 'closed' ? 'closed' : 'interrupted',
      owner_alive: Boolean(utility),
      cleanup_confirmed: utility ? false : lifecycle.cleanup_confirmed,
      reason: 'renderer crashed — awaiting explicit reopen or replacement window',
      pending_operations: [...pendingRequests.keys()],
    });
    notifyInterrupted();
  });
}

function createMainWindow(): BrowserWindow {
  const win = new BrowserWindow({
    title: DISPLAY_NAME,
    width: 1280,
    height: 800,
    show: false,
    webPreferences: {
      preload: preloadPath(),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
      disableBlinkFeatures: 'Auxclick',
    },
  });
  hardenWindow(win);
  rendererDetached = false;
  win.on('close', () => {
    initiateOwnerClose().catch(() => undefined);
  });
  win.once('ready-to-show', () => win.show());
  win.loadURL(proofIndexUrl()).catch((err) => {
    console.error(errorMessage(err));
    app.exit(1);
  });
  return win;
}

function registerIpcHandlers(): void {
  ipcMain.handle('nexus-proof:lifecycle', async (event) => {
    assertProofSender(event);
    return lifecycle;
  });

  ipcMain.handle('nexus-proof:renderer-ready', async (event) => {
    assertProofSender(event);
    return { lifecycle, bundle_id: BUNDLE_ID };
  });

  ipcMain.handle('nexus-proof:proof-step', async (event, raw: { step?: string; payload?: Record<string, unknown> }) => {
    assertProofSender(event);
    if (lifecycle.phase === 'closing') {
      return ipcErr(randomUUID(), 'closing', 'owner is closing');
    }
    const step = assertProofStep(raw?.step);
    const request_id = randomUUID();
    switch (step) {
      case 'lifecycle_status':
        return ipcOk(request_id, lifecycle);
      case 'compatibility':
        return utilityRequest({ request_id, operation: 'compatibility' });
      case 'open': {
        const policy = resolveOpenProofPolicy({
          reopenRequired,
          rendererDetached,
          phase: lifecycle.phase,
          ownerAlive: lifecycle.owner_alive,
        });
        if (policy === 'reopen_after_fence') {
          const confirmed = await killAndJoinUtility(utilityGeneration, Date.now() + CLOSE_JOIN_MS);
          if (!confirmed) {
            return ipcErr(
              request_id,
              'interrupted',
              utility
                ? 'prior owner join unconfirmed — reopen blocked'
                : 'prior owner join unconfirmed — owner reference lost',
            );
          }
          utility = null;
          reopenRequired = false;
          closeInitiatedForGeneration = null;
          rendererDetached = false;
          return utilityRequest({ request_id, operation: 'open' });
        }
        if (policy === 'attach_existing_owner') {
          rendererDetached = false;
          return ipcOk(request_id, {
            readiness: attachExistingReadiness(lifecycle.phase),
            attached: true,
            lifecycle,
          });
        }
        return utilityRequest({ request_id, operation: 'open' });
      }
      case 'graph':
        return utilityRequest({ request_id, operation: 'graph', payload: raw.payload });
      case 'patch':
        return utilityRequest({ request_id, operation: 'patch', payload: raw.payload });
      case 'provider_probe':
        return utilityRequest({ request_id, operation: 'provider', payload: raw.payload });
      case 'provider_pull':
        return utilityRequest({ request_id, operation: 'pull', payload: raw.payload });
      case 'close':
        await initiateOwnerClose();
        return ipcOk(request_id, lifecycle.last_close_report ?? lifecycle);
      default:
        return ipcErr(request_id, 'invalid_input', `unsupported proof step ${step}`);
    }
  });
}

async function bootstrap(): Promise<void> {
  if (!app.requestSingleInstanceLock()) {
    app.quit();
    return;
  }

  app.setName(DISPLAY_NAME);
  if (process.platform === 'darwin') {
    app.dock?.show();
  }

  protocol.registerSchemesAsPrivileged([
    {
      scheme: 'nexus-proof',
      privileges: {
        standard: true,
        secure: true,
        supportFetchAPI: true,
        corsEnabled: false,
        stream: true,
      },
    },
  ]);

  await app.whenReady();

  assertDistPresent(distRoot);
  try {
    assertNativePayloadPresent();
  } catch (err) {
    throw new Error(`${errorMessage(err)}. ${nativeRefreshHint()}`);
  }
  registerProofProtocol(distRoot);
  registerIpcHandlers();

  mainWindow = createMainWindow();

  app.on('web-contents-created', (_event, contents) => {
    contents.setWindowOpenHandler(() => ({ action: 'deny' }));
    contents.on('will-navigate', (event, navigationUrl) => {
      if (!allowNavigation(navigationUrl)) event.preventDefault();
    });
  });

  app.on('open-url', (event, url) => {
    event.preventDefault();
    if (isAllowedExternalUrl(url)) {
      shell.openExternal(url).catch(() => undefined);
    }
  });

  app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit();
  });

  app.on('before-quit', (event) => {
    if (lifecycle.phase !== 'closed' && !closeInFlight) {
      event.preventDefault();
      initiateOwnerClose().finally(() => app.quit());
    }
  });

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      mainWindow = createMainWindow();
    }
  });
}

bootstrap().catch((err) => {
  console.error(errorMessage(err));
  app.exit(1);
});

export { BUNDLE_ID, DISPLAY_NAME, repoRoot, distRoot };
