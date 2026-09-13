import {
  app,
  BrowserWindow,
  ipcMain,
  protocol,
  shell,
  utilityProcess,
  type UtilityProcess,
} from 'electron';
import { randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { nativeCompatibility } from '@42ch/nexus-native';
import {
  buildUtilityConfig,
  nativeRefreshHint,
  repoRootFromMeta,
  resolveProofHome,
  sanitizeInheritedEnv,
} from './env.js';
import {
  CLOSE_JOIN_MS,
  INTERRUPT_NOTIFY_MS,
  assertProofStep,
  errorMessage,
  ipcErr,
  ipcOk,
  parseIpcRequest,
  type IpcResponse,
  type LifecycleStatus,
} from './ipc.js';
import {
  allowNavigation,
  assertDistPresent,
  isAllowedExternalUrl,
  proofIndexUrl,
  registerProofProtocol,
  resolveDistRoot,
} from './protocol.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = repoRootFromMeta(import.meta.url);
const BUNDLE_ID = 'com.nexus42.rft-electron-proof';
const DISPLAY_NAME = 'Nexus RFT Feasibility';

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
let lifecycle: LifecycleStatus = {
  phase: 'idle',
  owner_alive: false,
  cleanup_confirmed: null,
  pending_operations: [],
  reason: null,
  last_close_report: null,
};
let closeInitiated = false;
let reopenRequired = false;

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

function assertNativeArtifacts(): void {
  try {
    nativeCompatibility();
  } catch (err) {
    const message = errorMessage(err);
    throw new Error(`${message}. ${nativeRefreshHint()}`);
  }
}

function spawnUtilityOwner(): UtilityProcess {
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
    setLifecycle({ phase: 'starting', owner_alive: true, reason: null });
  });

  child.stdout?.on('data', (chunk) => {
    process.stderr.write(`[utility stdout] ${chunk.toString()}`);
  });
  child.stderr?.on('data', (chunk) => {
    process.stderr.write(`[utility stderr] ${chunk.toString()}`);
  });

  child.on('exit', (code) => {
    utility = null;
    if (!closeInitiated) {
      reopenRequired = true;
      setLifecycle({
        phase: 'interrupted',
        owner_alive: false,
        cleanup_confirmed: false,
        reason: `utility exited unexpectedly (${code ?? 'unknown'})`,
      });
      notifyInterrupted();
    }
  });

  return child;
}

function notifyInterrupted(): void {
  setTimeout(() => {
    if (lifecycle.phase === 'interrupted' && mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send('nexus-proof:lifecycle-changed', lifecycle);
    }
  }, Math.min(INTERRUPT_NOTIFY_MS, CLOSE_JOIN_MS));
}

async function utilityRequest(raw: unknown): Promise<IpcResponse> {
  const request = parseIpcRequest(raw);
  if (reopenRequired && request.operation !== 'close') {
    return ipcErr(request.request_id, 'interrupted', 'prior owner interrupted — explicit reopen required');
  }
  if (!utility) {
    utility = spawnUtilityOwner();
  }
  const active = utility;
  return new Promise((resolve) => {
    const onMessage = (message: unknown) => {
      const body = message as IpcResponse & { type?: string; lifecycle?: LifecycleStatus };
      if (body && typeof body === 'object' && 'type' in body) {
        if (body.type === 'utility-ready') {
          setLifecycle({ ...(body.lifecycle ?? {}), owner_alive: true });
        }
        if (body.type === 'utility-crashed') {
          reopenRequired = true;
          setLifecycle({
            phase: 'interrupted',
            owner_alive: false,
            cleanup_confirmed: false,
            reason: body.lifecycle?.reason ?? 'utility crashed',
          });
          notifyInterrupted();
        }
        return;
      }
      active?.off('message', onMessage);
      const response = body as IpcResponse;
      if (response.ok && request.operation === 'open') {
        setLifecycle({ phase: 'open', owner_alive: true, reason: null });
      }
      if (response.ok && request.operation === 'close') {
        setLifecycle({
          phase: 'closed',
          owner_alive: false,
          cleanup_confirmed: (response.result as { cleanup_confirmed?: boolean })?.cleanup_confirmed ?? null,
          last_close_report: response.result,
          reason: null,
        });
        reopenRequired = false;
      }
      if (!response.ok && request.operation === 'close') {
        setLifecycle({
          phase: 'interrupted',
          owner_alive: false,
          cleanup_confirmed: false,
          reason: response.error.message,
          last_close_report: null,
        });
        reopenRequired = true;
      }
      resolve(response);
    };
    active?.on('message', onMessage);
    active?.postMessage({
      request_id: request.request_id,
      operation: request.operation,
      payload: request.payload,
    });
  });
}

async function initiateOwnerClose(): Promise<void> {
  if (closeInitiated) return;
  closeInitiated = true;
  setLifecycle({ phase: 'closing', owner_alive: Boolean(utility), reason: null });
  if (!utility) {
    setLifecycle({ phase: 'closed', owner_alive: false, cleanup_confirmed: true, reason: null });
    return;
  }
  const request_id = randomUUID();
  const timer = setTimeout(() => {
    if (utility) {
      utility.kill();
      utility = null;
      reopenRequired = true;
      setLifecycle({
        phase: 'interrupted',
        owner_alive: false,
        cleanup_confirmed: false,
        reason: 'close join exceeded 5s',
      });
      notifyInterrupted();
    }
  }, CLOSE_JOIN_MS);
  try {
    await utilityRequest({ request_id, operation: 'close' });
  } finally {
    clearTimeout(timer);
    if (utility) {
      utility.kill();
      utility = null;
    }
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
    setLifecycle({
      phase: lifecycle.phase === 'closed' ? 'closed' : 'interrupted',
      owner_alive: Boolean(utility),
      reason: 'renderer crashed — native owner remains authoritative',
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
  ipcMain.handle('nexus-proof:lifecycle', async () => lifecycle);

  ipcMain.handle('nexus-proof:renderer-ready', async () => ({ lifecycle, bundle_id: BUNDLE_ID }));

  ipcMain.handle('nexus-proof:proof-step', async (_event, raw: { step?: string; payload?: Record<string, unknown> }) => {
    const step = assertProofStep(raw?.step);
    const request_id = randomUUID();
    switch (step) {
      case 'lifecycle_status':
        return ipcOk(request_id, lifecycle);
      case 'compatibility':
        return utilityRequest({ request_id, operation: 'compatibility' });
      case 'open':
        if (reopenRequired) {
          utility = null;
          reopenRequired = false;
          closeInitiated = false;
        }
        return utilityRequest({ request_id, operation: 'open' });
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
  assertNativeArtifacts();
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
    if (!closeInitiated) {
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
