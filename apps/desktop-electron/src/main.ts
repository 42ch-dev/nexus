/**
 * Product desktop host (v1.192 P0-T7) — the ONE Electron main process.
 *
 * Composes the reviewed modules into the shipped app:
 *  - identity from `resources/product.json` (P1-T1): app name, bundle-id
 *    userData, Dock icon, single instance (second launch focuses, never
 *    spawns a duplicate service);
 *  - window chrome (parity row 22): 1280×800, min 960×640, overlay titlebar,
 *    traffic lights at 12,14, main-owned maximize toggle;
 *  - `nexus://app` protocol + exact-origin CSP (P0-T1), refreshed per response
 *    with the active connection origin;
 *  - typed desktop IPC (P0-T1) with the mandatory live `getCurrentGeneration`
 *    source, config handlers (P0-T2), guarded OS actions (P0-T3), connection
 *    store + exact-origin network hooks (P0-T5), the serialized service
 *    controller (P0-T4) with status events on `nexus:desktop:status-changed`,
 *    and the three-choice quit gate (P0-T6) wired into `before-quit`;
 *  - standard edit/quit menu roles only (plan menu decision — no native
 *    product menu), no update route (parity row 29).
 *
 * The proof shell (proof scheme, proof step surface, proof DOM, proof
 * home/log env) is deleted from the runtime path; the modules' retained proof
 * exports died with this file's callsites (see task-7 report).
 *
 * The `electron` value import is dynamic (documented exception, same
 * rationale as desktop-ipc.ts / protocol.ts): `composeDesktopHost` receives
 * the Electron surfaces as an injected bag so
 * `tests/desktop-host.test.mjs` can drive the whole composition headlessly
 * under plain node. A real GUI launch is not exercisable headlessly — the
 * report says so explicitly for what remains `[UNVERIFIED]`.
 */
import { execFile } from 'node:child_process';
import { accessSync, constants, existsSync, readFileSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import type {
  App,
  BrowserWindow,
  Dialog,
  IpcMain,
  Menu,
  MenuItemConstructorOptions,
  Session,
  Shell,
  UtilityProcess,
} from 'electron';
import type { DaemonStatus, PublicConnectionConfig } from './desktop-contract.js';
import {
  DESKTOP_HOST,
  DESKTOP_RUNTIME_CHANNEL,
  DESKTOP_SCHEME,
  errorMessage,
  isAllowedDesktopExternalUrl,
} from './desktop-contract.js';
import type { DesktopHandlers, RegisterDesktopIpcOptions, RegisterDesktopIpcResult } from './desktop-ipc.js';
import { registerDesktopIpc, sendDesktopStatus } from './desktop-ipc.js';
import { createDesktopConfig } from './desktop-config.js';
import { createDesktopActions } from './desktop-actions.js';
import type { ActiveConnectionAuth, SessionLike } from './desktop-network.js';
import { attachDesktopNetworkHooks } from './desktop-network.js';
import type { SecureStorageAdapter } from './connection-store.js';
import { ConnectionStore } from './connection-store.js';
import type { UtilityChildProcess } from './service-controller.js';
import { DesktopServiceController } from './service-controller.js';
import type { QuitChoice, QuitOutcome } from './quit-controller.js';
import { DesktopQuitController, createDetachedServiceHandoff, findNodeOnPath } from './quit-controller.js';
import {
  DESKTOP_SERVICE_HOST,
  assertNativePayloadPresent,
  nativeRefreshHint,
  repoRootFromMeta,
  resolveDesktopServicePort,
  sanitizeInheritedEnv,
} from './env.js';
import type { DesktopCspPolicy } from './protocol.js';
import {
  allowDesktopNavigation,
  buildDesktopCsp,
  desktopIndexUrl,
  registerDesktopProtocol,
  registerDesktopSchemes,
  resolveDistRoot,
} from './protocol.js';

const requireModule = createRequire(import.meta.url);
const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));

// ---------------------------------------------------------------------------
// Product identity (resources/product.json, P1-T1 producer / P0-T7 consumer)
// ---------------------------------------------------------------------------

export interface ProductIdentity {
  /** Bundle id, e.g. io.nexus42.desktop — also the userData directory name. */
  id: string;
  /** Display name, e.g. Nexus. */
  name: string;
  /** Marketing version carried by the packaging layout. */
  version: string;
  /** Minimum macOS version, e.g. 13.0. */
  minimumMacos: string;
}

export function loadProductIdentity(resourcesDir: string): ProductIdentity {
  const raw = JSON.parse(readFileSync(join(resourcesDir, 'product.json'), 'utf8')) as Record<string, unknown>;
  const id = raw.id;
  const name = raw.name;
  const version = raw.version;
  const minimumMacos = raw.minimum_macos;
  if (typeof id !== 'string' || id.length === 0 || !id.includes('.')) {
    throw new Error(`resources/product.json: invalid product id: ${String(id)}`);
  }
  if (typeof name !== 'string' || name.length === 0) {
    throw new Error(`resources/product.json: invalid product name: ${String(name)}`);
  }
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error(`resources/product.json: invalid product version: ${String(version)}`);
  }
  if (typeof minimumMacos !== 'string' || minimumMacos.length === 0) {
    throw new Error(`resources/product.json: invalid minimum_macos: ${String(minimumMacos)}`);
  }
  return { id, name, version, minimumMacos };
}

/** Minimal app surface identity setup needs; the real `App` satisfies this. */
export interface IdentityApp {
  setName(name: string): void;
  setPath(name: string, value: string): void;
  getPath(name: string): string;
  dock?: { setIcon(path: string): void };
}

/**
 * Product identity on the app: display name, bundle-id userData (proof
 * identity must never become production state) and the Dock icon from
 * `resources/icons/app.icns`. MUST run before `app.whenReady()` resolves
 * (userData is ready-fixed).
 */
export function applyProductIdentity(
  app: IdentityApp,
  product: ProductIdentity,
  paths: { userDataDir: string; resourcesDir: string },
): void {
  app.setName(product.name);
  app.setPath('userData', paths.userDataDir);
  if (process.platform === 'darwin' && app.dock) {
    const iconPath = join(paths.resourcesDir, 'icons', 'app.icns');
    if (existsSync(iconPath)) {
      app.dock.setIcon(iconPath);
    }
  }
}

// ---------------------------------------------------------------------------
// Launch-time resolution (pure — covered headlessly by the host test)
// ---------------------------------------------------------------------------

/**
 * Explicitly launched dev HMR URL. Only the two frozen Vite origins are
 * accepted; packaged builds ignore the override entirely.
 */
export function resolveDevUrl(env: NodeJS.ProcessEnv, isPackaged: boolean): string | null {
  if (isPackaged) return null;
  const raw = env.NEXUS_DESKTOP_DEV_URL;
  if (typeof raw !== 'string' || raw.trim() === '') return null;
  const url = raw.trim();
  const allowed =
    url === 'http://localhost:5173' ||
    url === 'http://127.0.0.1:5173' ||
    url.startsWith('http://localhost:5173/') ||
    url.startsWith('http://127.0.0.1:5173/');
  return allowed ? url : null;
}

/**
 * Packaged builds serve only the bundled `Resources/web-dist` (a missing
 * artifact is an error, never a silent development-assets fallback); dev
 * serves the built `apps/web/dist`.
 */
export function resolveDesktopDistRoot(input: {
  isPackaged: boolean;
  resourcesPath?: string;
  repoRoot: string;
}): string {
  if (input.isPackaged) {
    const packaged = join(input.resourcesPath ?? '', 'web-dist');
    if (existsSync(join(packaged, 'index.html'))) {
      return packaged;
    }
    throw new Error(
      `packaged build is missing its bundled web artifact at ${packaged}. ` +
        'Repackage with the current web dist: pnpm --filter web build, then ' +
        'node apps/desktop-electron/scripts/package.mjs --arch <arm64|x64>.',
    );
  }
  return resolveDistRoot(input.repoRoot);
}

function isExecutableFile(path: string): boolean {
  try {
    if (!statSync(path).isFile()) return false;
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Trusted standalone Node for the detached Keep-on-quit handoff (P0-T6
 * carry-over): a GUI launch inherits launchd's minimal PATH, so a packaged
 * bundle Node is preferred and PATH is only a fallback.
 */
export function resolveTrustedNodeExecutable(input: {
  isPackaged: boolean;
  resourcesPath?: string;
  env: NodeJS.ProcessEnv;
}): string | null {
  if (input.isPackaged && input.resourcesPath) {
    const bundled = join(input.resourcesPath, 'node', 'bin', 'node');
    if (isExecutableFile(bundled)) return bundled;
  }
  return findNodeOnPath(sanitizeInheritedEnv(input.env));
}

// ---------------------------------------------------------------------------
// Injected Electron surfaces
// ---------------------------------------------------------------------------

/** Narrowest structural view of Electron's `utilityProcess` module main needs. */
export interface UtilityProcessFork {
  fork(modulePath: string, args?: string[], options?: Record<string, unknown>): UtilityProcess;
}

export interface DesktopElectronSurfaces {
  app: App;
  BrowserWindow: typeof BrowserWindow;
  dialog: Dialog;
  shell: Shell;
  /** The main session the selected window loads in (defaultSession). */
  session: Session;
  ipcMain: IpcMain;
  utilityProcess: UtilityProcessFork;
  Menu: typeof Menu;
  safeStorage: SecureStorageAdapter;
}

export interface DesktopHostPaths {
  /** Directory holding product.json and icons/ (dev: apps/desktop-electron/resources). */
  resourcesDir: string;
  distRoot: string;
  preloadPath: string;
  /** Compiled utility owner entry (dist/utility-host.js). */
  utilityEntry: string;
  /** Packaged standalone TS service entry (nexus-service dist/main.js). */
  serviceEntry: string;
  userDataDir: string;
  /** Trusted raw user home (launch environment). */
  home: string;
  documentsPath: string;
}

export interface ComposeDesktopHostOptions {
  electron: DesktopElectronSurfaces;
  product: ProductIdentity;
  paths: DesktopHostPaths;
  env: NodeJS.ProcessEnv;
  isPackaged: boolean;
  devUrl: string | null;
  /** Trusted standalone Node (see {@link resolveTrustedNodeExecutable}). */
  nodeExecutable: string | null;
  /** Opened store override (test seam); defaults to the encrypted userData store. */
  connectionStore?: ConnectionStore;
  adapters?: {
    registerSchemes?: () => Promise<void>;
    registerProtocol?: (distRoot: string, policy: DesktopCspPolicy) => Promise<void>;
    registerIpc?: (
      window: BrowserWindow,
      handlers: DesktopHandlers,
      options: RegisterDesktopIpcOptions,
    ) => Promise<RegisterDesktopIpcResult>;
    attachNetworkHooks?: (
      session: SessionLike,
      getActiveAuth: () => ActiveConnectionAuth | null,
    ) => void;
  };
}

export interface DesktopHostIpcBinding {
  window: BrowserWindow;
  options: RegisterDesktopIpcOptions;
}

export interface DesktopHost {
  window: BrowserWindow;
  handlers: DesktopHandlers;
  controller: DesktopServiceController;
  quitController: DesktopQuitController;
  connectionStore: ConnectionStore;
  /** Live generation source: bumps on every window replacement. */
  generation(): number;
  /** IPC bindings in registration order (one per window generation). */
  ipcBindings: DesktopHostIpcBinding[];
  /** Focuses the sole window; a second-instance launch never spawns anything. */
  focusExistingWindow(): void;
  /** Settles when the in-flight (re)registration has completed. */
  settled(): Promise<void>;
  dispose(): void;
}

const QUIT_BUTTONS = ['Stop Daemon & Quit', 'Keep Daemon & Quit', 'Cancel'] as const;

function quitDetail(status: DaemonStatus): string {
  const base = `The local service is ${status.state} on port ${status.port}.`;
  return status.detail ? `${base} ${status.detail}` : base;
}

/**
 * One-time legacy connection import (frozen secure-store contract): the old
 * macOS keychain entry (`nexus42` / `connection_config`) via
 * `/usr/bin/security` — no shell, the secret is never logged or returned
 * anywhere else — otherwise the old app-data `connection_config.json`.
 */
function createLegacyConnectionReader(appDataDir: string): () => Promise<string | null> {
  async function keychainSecret(): Promise<string | null> {
    try {
      const { stdout } = await execFileAsync(
        '/usr/bin/security',
        ['find-generic-password', '-s', 'nexus42', '-a', 'connection_config', '-w'],
        { timeout: 5_000 },
      );
      const trimmed = String(stdout).trim();
      return trimmed.length > 0 ? trimmed : null;
    } catch {
      return null;
    }
  }
  return async () => {
    if (process.platform === 'darwin') {
      const secret = await keychainSecret();
      if (secret !== null) return secret;
    }
    const fallback = join(appDataDir, 'io.nexus42.desktop', 'connection_config.json');
    try {
      return readFileSync(fallback, 'utf8');
    } catch {
      return null;
    }
  };
}

/**
 * Compose the product host: one selected window, the full typed handler map,
 * the service controller + quit gate, the connection store and network hooks.
 * All Electron effects go through the injected surfaces so the composition is
 * exercisable headlessly; defaults bind the reviewed modules.
 */
export async function composeDesktopHost(input: ComposeDesktopHostOptions): Promise<DesktopHost> {
  const e = input.electron;
  const product = input.product;
  const dev = input.devUrl !== null;
  const adapters = input.adapters ?? {};
  const registerSchemes = adapters.registerSchemes ?? registerDesktopSchemes;
  const registerProtocol = adapters.registerProtocol ?? registerDesktopProtocol;
  const registerIpc = adapters.registerIpc ?? registerDesktopIpc;
  const attachNetworkHooks = adapters.attachNetworkHooks ?? attachDesktopNetworkHooks;

  const resolvedPort = resolveDesktopServicePort(undefined, input.env);
  const localEndpoint = `http://${DESKTOP_SERVICE_HOST}:${resolvedPort}`;
  const navigationOptions = { dev };

  // ── identity ──────────────────────────────────────────────────────────
  applyProductIdentity(e.app, product, {
    userDataDir: input.paths.userDataDir,
    resourcesDir: input.paths.resourcesDir,
  });

  // ── trusted runtime metadata (preload reads it synchronously) ─────────
  const runtimeMetadata = { localEndpoint };
  const onRuntimeChannel = (event: { returnValue: unknown }): void => {
    event.returnValue = runtimeMetadata;
  };
  e.ipcMain.on(DESKTOP_RUNTIME_CHANNEL, onRuntimeChannel);

  // ── config, connection store, network policy ──────────────────────────
  const config = createDesktopConfig(input.paths.home, input.paths.documentsPath);
  const { resolveWorkspaceRoot, ...configHandlers } = config;

  const connectionStore =
    input.connectionStore ??
    (await ConnectionStore.open({
      filePath: join(input.paths.userDataDir, 'connection-config.enc'),
      storage: e.safeStorage,
      readLegacy: createLegacyConnectionReader(e.app.getPath('appData')),
    }));
  let activeConfig: PublicConnectionConfig | null = await connectionStore.get();
  const refreshActiveConfig = async (): Promise<void> => {
    activeConfig = await connectionStore.get();
  };

  attachNetworkHooks(e.session, () => connectionStore.getAuth());

  // Exact-origin CSP, refreshed per response with the ACTIVE connection
  // origin (frozen contract: main validates and inserts exact origins only).
  const activeServiceOrigin = (): string => {
    if (activeConfig?.active === true) {
      try {
        return new URL(activeConfig.endpointUrl).origin;
      } catch {
        // fall through to the app-managed local endpoint
      }
    }
    return localEndpoint;
  };
  if (typeof e.session.webRequest.onHeadersReceived === 'function') {
    e.session.webRequest.onHeadersReceived(
      { urls: [`${DESKTOP_SCHEME}://${DESKTOP_HOST}/*`] },
      (details, callback) => {
        const headers: Record<string, string[]> = { ...(details.responseHeaders ?? {}) };
        for (const key of Object.keys(headers)) {
          if (key.toLowerCase() === 'content-security-policy') delete headers[key];
        }
        const origin = activeServiceOrigin();
        headers['Content-Security-Policy'] = [
          buildDesktopCsp({ serviceOrigin: origin, fingerprintProbeOrigin: origin, dev }),
        ];
        callback({ responseHeaders: headers });
      },
    );
  }

  // ── service controller (P0-T4) with renderer status events ────────────
  const controller = new DesktopServiceController({
    home: input.paths.home,
    resolvedPort,
    spawnUtility: () =>
      e.utilityProcess.fork(input.paths.utilityEntry, [], {
        serviceName: 'nexus-desktop-service',
        env: sanitizeInheritedEnv(input.env),
        stdio: 'ignore',
      }) as unknown as UtilityChildProcess,
    emitStatus: (_channel, status) => {
      const win = currentWindow;
      if (win && !win.isDestroyed()) {
        void sendDesktopStatus(win, status).catch(() => undefined);
      }
    },
    handoff: createDetachedServiceHandoff({
      serviceEntry: input.paths.serviceEntry,
      ...(input.nodeExecutable === null ? {} : { nodeExecutable: input.nodeExecutable }),
      env: sanitizeInheritedEnv(input.env),
    }),
  });

  // ── window + IPC generation ───────────────────────────────────────────
  let generation = 0;
  let currentWindow: BrowserWindow | null = null;
  let ipcRegistration: RegisterDesktopIpcResult | null = null;
  let lastSelect: Promise<void> = Promise.resolve();
  const ipcBindings: DesktopHostIpcBinding[] = [];

  // ── quit gate (P0-T6), wired into before-quit below ───────────────────
  const chooseQuitChoice = async (status: DaemonStatus): Promise<QuitChoice | null> => {
    const options = {
      type: 'question' as const,
      title: product.name,
      message: `Quit ${product.name}?`,
      detail: quitDetail(status),
      buttons: [...QUIT_BUTTONS],
      defaultId: 2,
      cancelId: 2,
      noLink: true,
    };
    const result =
      currentWindow && !currentWindow.isDestroyed()
        ? await e.dialog.showMessageBox(currentWindow, options)
        : await e.dialog.showMessageBox(options);
    if (result.response === 0) return 'stop';
    if (result.response === 1) return 'keep';
    return 'cancel';
  };
  const reportQuitOutcome = (outcome: QuitOutcome): void => {
    if (!outcome.allowed) {
      void e.dialog
        .showMessageBox({
          type: 'warning',
          title: product.name,
          message: `Quit cancelled — ${product.name} stays open`,
          detail: outcome.detail ?? 'the local service could not be stopped safely',
          buttons: ['OK'],
          noLink: true,
        })
        .catch(() => undefined);
      return;
    }
    if (outcome.choice === 'keep') {
      // D-21: the user is told in-flight work was interrupted.
      void e.dialog
        .showMessageBox({
          type: 'info',
          title: product.name,
          message: 'The service continues as an independent process',
          detail: outcome.detail,
          buttons: ['OK'],
          noLink: true,
        })
        .catch(() => undefined);
    }
  };
  const quitController = new DesktopQuitController({ controller, chooseQuitChoice, reportQuitOutcome });

  // ── the full typed handler map ────────────────────────────────────────
  const handlers: DesktopHandlers = {
    ...configHandlers,
    ...createDesktopActions({
      resolveWorkspaceRoot,
      shell: {
        openPath: (path) => e.shell.openPath(path),
        showItemInFolder: (path) => e.shell.showItemInFolder(path),
        openExternal: (url) => e.shell.openExternal(url),
      },
      dialog: {
        pickDirectory: async (options) => {
          const result = await e.dialog.showOpenDialog(currentWindow ?? e.BrowserWindow.getAllWindows()[0]!, {
            properties: ['openDirectory', 'createDirectory'],
            ...(options.defaultPath === undefined ? {} : { defaultPath: options.defaultPath }),
          });
          return result.canceled || result.filePaths.length === 0 ? null : result.filePaths[0];
        },
        confirmReset: async () => {
          const win = currentWindow ?? e.BrowserWindow.getAllWindows()[0] ?? null;
          const options = {
            type: 'warning' as const,
            title: product.name,
            message: 'Reset the local Nexus state?',
            detail:
              'This closes the local service and deletes the local state databases under the current workspace. In-flight work is interrupted. This cannot be undone.',
            buttons: ['Reset Local State', 'Cancel'],
            defaultId: 1,
            cancelId: 1,
            noLink: true,
          };
          const result = win
            ? await e.dialog.showMessageBox(win, options)
            : await e.dialog.showMessageBox(options);
          return result.response === 0;
        },
      },
      controller,
    }),
    async get_connection_config() {
      return connectionStore.get();
    },
    async set_connection_config({ config: next, credential }) {
      const saved = await connectionStore.set(next, credential);
      await refreshActiveConfig();
      return saved;
    },
    async delete_connection_config() {
      await connectionStore.delete();
      await refreshActiveConfig();
      return null;
    },
    async get_daemon_status() {
      return controller.getStatus();
    },
    async start_daemon() {
      await controller.start();
      return null;
    },
    async stop_daemon() {
      await controller.stop();
      return null;
    },
    async restart_daemon() {
      await controller.restart();
      return null;
    },
    /** Parity row 22: main-owned maximize toggle. */
    async toggle_maximize_window() {
      const win = currentWindow;
      if (!win || win.isDestroyed()) {
        return null;
      }
      if (win.isMaximized()) {
        win.unmaximize();
      } else {
        win.maximize();
      }
      return null;
    },
  };

  // ── window chrome + hardening (parity rows 22/26/25/29) ───────────────
  const preloadPath = input.paths.preloadPath;

  function hardenWindow(win: BrowserWindow): void {
    // Deny all window.open and webview creation, everywhere.
    win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    // Deny navigation/redirect outside the active app origin. An outbound
    // link must call `open_external_url` — an untrusted navigation is never
    // auto-opened as an external URL.
    win.webContents.on('will-navigate', (event, url) => {
      if (!allowDesktopNavigation(url, navigationOptions)) event.preventDefault();
    });
    win.webContents.on('will-redirect', (event, url) => {
      if (!allowDesktopNavigation(url, navigationOptions)) event.preventDefault();
    });
    win.webContents.on('render-process-gone', () => {
      if (currentWindow === win) {
        currentWindow = null;
      }
      // The renderer alone is replaced; the service owner is never killed by
      // a renderer crash and no second owner is created.
      recreateWindow();
    });
  }

  function createMainWindow(): BrowserWindow {
    const win = new e.BrowserWindow({
      title: product.name,
      width: 1280,
      height: 800,
      minWidth: 960,
      minHeight: 640,
      show: false,
      titleBarStyle: 'hiddenInset',
      trafficLightPosition: { x: 12, y: 14 },
      webPreferences: {
        preload: preloadPath,
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        webSecurity: true,
        disableBlinkFeatures: 'Auxclick',
      },
    });
    hardenWindow(win);
    win.once('ready-to-show', () => win.show());
    win.loadURL(input.devUrl ?? desktopIndexUrl()).catch((err) => {
      process.stderr.write(`[desktop] web dist failed to load: ${errorMessage(err)}\n`);
      e.app.exit(1);
    });
    return win;
  }

  async function selectWindow(win: BrowserWindow): Promise<void> {
    ipcRegistration?.dispose();
    generation += 1;
    const boundGeneration = generation;
    currentWindow = win;
    const registration = await registerIpc(win, handlers, {
      generation: boundGeneration,
      getCurrentGeneration: () => generation,
      dev,
    });
    // A slow registration resolving after a newer generation won must not
    // clobber the live binding.
    if (currentWindow === win && generation === boundGeneration) {
      ipcRegistration = registration;
      ipcBindings.push({
        window: win,
        options: { generation: boundGeneration, getCurrentGeneration: () => generation, dev },
      });
    }
  }

  let recreateInFlight: Promise<void> | null = null;
  function recreateWindow(): void {
    if (recreateInFlight) return;
    recreateInFlight = (async () => {
      const win = createMainWindow();
      await selectWindow(win);
    })()
      .catch(() => undefined)
      .finally(() => {
        recreateInFlight = null;
      });
    lastSelect = recreateInFlight;
  }

  const firstWindow = createMainWindow();
  lastSelect = selectWindow(firstWindow);

  function focusExistingWindow(): void {
    const win = currentWindow;
    if (!win || win.isDestroyed()) {
      recreateWindow();
      return;
    }
    if (win.isMinimized()) win.restore();
    win.show();
    win.focus();
  }

  // ── standard menu roles only (no native product menu) ─────────────────
  const menuTemplate: MenuItemConstructorOptions[] =
    process.platform === 'darwin'
      ? [{ role: 'appMenu' }, { role: 'editMenu' }]
      : [{ role: 'fileMenu' }, { role: 'editMenu' }];
  e.Menu.setApplicationMenu(e.Menu.buildFromTemplate(menuTemplate));

  // ── app-level wiring ──────────────────────────────────────────────────
  // Second launch: focus the sole window — never spawn a duplicate service.
  e.app.on('second-instance', () => {
    focusExistingWindow();
  });
  e.app.on('activate', () => {
    if (!currentWindow || currentWindow.isDestroyed()) {
      recreateWindow();
    }
  });
  e.app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
      e.app.quit();
    }
  });

  // The one quit gate: Cmd+Q / menu quit / window-all-closed all funnel
  // through here. DesktopQuitController is single-flight, so a re-raised
  // before-quit during the decision joins the same promise.
  let quitAllowed = false;
  e.app.on('before-quit', (event) => {
    if (quitAllowed) return;
    event.preventDefault();
    void quitController
      .requestQuit()
      .then((allowed) => {
        if (!allowed) return;
        quitAllowed = true;
        e.app.quit();
      })
      .catch(() => undefined);
  });

  // Outbound URLs only ever through the shared predicate + shell.openExternal.
  e.app.on('open-url', (event, url) => {
    event.preventDefault();
    if (isAllowedDesktopExternalUrl(url)) {
      e.shell.openExternal(url).catch(() => undefined);
    }
  });

  e.app.on('web-contents-created', (_event, contents) => {
    contents.setWindowOpenHandler(() => ({ action: 'deny' }));
    contents.on('will-navigate', (event, url) => {
      if (!allowDesktopNavigation(url, navigationOptions)) event.preventDefault();
    });
  });

  // ── protocol (packaged resources only, exact-origin CSP) ──────────────
  await registerSchemes();
  await registerProtocol(input.paths.distRoot, {
    serviceOrigin: localEndpoint,
    fingerprintProbeOrigin: localEndpoint,
    dev,
  });

  return {
    window: firstWindow,
    handlers,
    controller,
    quitController,
    connectionStore,
    generation: () => generation,
    ipcBindings,
    focusExistingWindow,
    settled: () => lastSelect,
    dispose: () => {
      ipcRegistration?.dispose();
      e.ipcMain.removeListener(DESKTOP_RUNTIME_CHANNEL, onRuntimeChannel as never);
    },
  };
}

// ---------------------------------------------------------------------------
// Bootstrap (the real Electron entry)
// ---------------------------------------------------------------------------

async function bootstrap(): Promise<void> {
  const electron = (await import('electron')) as typeof import('electron');
  const app = electron.app;

  const resourcesDir = app.isPackaged
    ? join(process.resourcesPath, 'resources')
    : join(__dirname, '..', 'resources');
  const product = loadProductIdentity(resourcesDir);

  // Single instance: a second launch focuses the sole window and exits —
  // it never spawns a duplicate service.
  if (!app.requestSingleInstanceLock()) {
    app.quit();
    return;
  }

  const userDataDir = join(app.getPath('appData'), product.id);
  applyProductIdentity(app, product, { userDataDir, resourcesDir });

  const env = process.env;
  const devUrl = resolveDevUrl(env, app.isPackaged);
  const distRoot = resolveDesktopDistRoot({
    isPackaged: app.isPackaged,
    resourcesPath: process.resourcesPath,
    repoRoot: repoRootFromMeta(import.meta.url),
  });
  const nodeExecutable = resolveTrustedNodeExecutable({
    isPackaged: app.isPackaged,
    resourcesPath: process.resourcesPath,
    env,
  });
  const serviceEntry = join(
    dirname(requireModule.resolve('@42ch/nexus-service/package.json')),
    'dist',
    'main.js',
  );

  // The `nexus` scheme must be privileged before the app is ready.
  await registerDesktopSchemes();
  await app.whenReady();

  try {
    assertNativePayloadPresent();
  } catch (err) {
    throw new Error(`${errorMessage(err)}. ${nativeRefreshHint()}`);
  }

  await composeDesktopHost({
    electron: {
      app,
      BrowserWindow: electron.BrowserWindow,
      dialog: electron.dialog,
      shell: electron.shell,
      session: electron.session.defaultSession,
      ipcMain: electron.ipcMain,
      utilityProcess: electron.utilityProcess,
      Menu: electron.Menu,
      safeStorage: {
        isEncryptionAvailable: () => electron.safeStorage.isEncryptionAvailable(),
        encryptString: (plain) => new Uint8Array(electron.safeStorage.encryptString(plain)),
        decryptString: (bytes) => electron.safeStorage.decryptString(Buffer.from(bytes)),
      },
    },
    product,
    paths: {
      resourcesDir,
      distRoot,
      preloadPath: join(__dirname, 'preload.js'),
      utilityEntry: join(__dirname, 'utility-host.js'),
      serviceEntry,
      userDataDir,
      home: app.getPath('home'),
      documentsPath: app.getPath('documents'),
    },
    env,
    isPackaged: app.isPackaged,
    devUrl,
    nodeExecutable,
  });
}

// Auto-run only inside the real Electron main process; plain-node test
// imports get the composition factory without side effects.
if (process.type === 'browser') {
  bootstrap().catch((err) => {
    process.stderr.write(`[desktop] bootstrap failed: ${errorMessage(err)}\n`);
    process.exit(1);
  });
}
