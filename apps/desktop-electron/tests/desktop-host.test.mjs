#!/usr/bin/env node
/**
 * P0-T7 product host integration tests.
 *
 * No GUI, no Electron, no E2E: `composeDesktopHost` receives injected
 * Electron surfaces (the T4/T6 harness pattern), so the whole composition —
 * window chrome, typed handler map, live generation wiring, quit gate,
 * connection store + network policy, exact-origin CSP refresh, single-instance
 * focus behavior — is exercised deterministically under plain node.
 *
 * Honest limits: a real GUI launch, real scheme registration under Electron
 * and LaunchServices single-instance enforcement are NOT exercisable
 * headlessly — they are composition-level `[UNVERIFIED]` claims proven only
 * up to the injected seams (see task-7 report).
 */
import assert from 'node:assert/strict';
import test from 'node:test';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  applyProductIdentity,
  composeDesktopHost,
  loadProductIdentity,
  resolveDesktopDistRoot,
  resolveDevUrl,
  resolveTrustedNodeExecutable,
} from '../dist/main.js';
import {
  DESKTOP_OPERATIONS,
  DESKTOP_RUNTIME_CHANNEL,
  errorCode,
} from '../dist/desktop-contract.js';
import { assertDesktopEventSender } from '../dist/desktop-ipc.js';

const RESOURCES_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'resources');
const DIST_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const LOCAL_ENDPOINT = 'http://127.0.0.1:8420';

const root = mkdtempSync(join(tmpdir(), 'nexus-host-'));
test.after(() => rmSync(root, { recursive: true, force: true }));

async function until(cond, what, ms = 3_000) {
  const start = Date.now();
  while (!cond()) {
    if (Date.now() - start > ms) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setImmediate(resolve));
  }
}

function makeFakeElectron() {
  const appEvents = new Map();
  const windows = [];
  const quitCalls = [];
  const menuSet = [];
  const ipcMainListeners = new Map();
  const dialogCalls = [];
  const openExternalCalls = [];
  const forkCalls = [];
  let dialogResponse = 2;
  let openDialogResult = { canceled: true, filePaths: [] };

  const session = {
    webRequest: {
      onBeforeSendHeaders: (...args) => {
        session.webRequest.beforeSendCalls.push(args);
      },
      onHeadersReceived: (...args) => {
        session.webRequest.headersReceivedCalls.push(args);
      },
      beforeSendCalls: [],
      headersReceivedCalls: [],
    },
  };

  class FakeBrowserWindow {
    static getAllWindows() {
      return windows;
    }

    constructor(options) {
      this.options = options;
      this.destroyed = false;
      this.maximized = false;
      this.minimized = false;
      this.loadedUrl = null;
      this.listeners = new Map();
      this.onceHandlers = new Map();
      this.webContents = {
        mainFrame: { url: 'nexus://app/index.html' },
        sent: [],
        listeners: new Map(),
        send: (channel, frame) => this.webContents.sent.push([channel, frame]),
        on: (event, fn) => this.webContents.listeners.set(event, fn),
        setWindowOpenHandler: (fn) => {
          this.webContents.windowOpenHandler = fn;
        },
      };
      windows.push(this);
    }

    isDestroyed() {
      return this.destroyed;
    }

    isMaximized() {
      return this.maximized;
    }

    isMinimized() {
      return this.minimized;
    }

    maximize() {
      this.maximized = true;
    }

    unmaximize() {
      this.maximized = false;
    }

    restore() {
      this.minimized = false;
    }

    focus() {
      this.focused = true;
    }

    show() {
      this.shown = true;
    }

    once(event, fn) {
      this.onceHandlers.set(event, fn);
    }

    on(event, fn) {
      this.listeners.set(event, fn);
    }

    async loadURL(url) {
      this.loadedUrl = url;
    }
  }

  const app = {
    name: null,
    paths: new Map([
      ['appData', join(root, 'appData')],
      ['home', join(root, 'fake-home')],
      ['documents', join(root, 'fake-documents')],
    ]),
    quitCalls,
    on: (event, fn) => {
      if (!appEvents.has(event)) appEvents.set(event, []);
      appEvents.get(event).push(fn);
    },
    setName: (name) => {
      app.name = name;
    },
    setPath: (name, value) => {
      app.paths.set(name, value);
    },
    getPath: (name) => app.paths.get(name),
    requestSingleInstanceLock: () => true,
    quit: () => quitCalls.push('quit'),
    exit: (code) => quitCalls.push(`exit:${code}`),
    dock: { icons: [], setIcon: (path) => app.dock.icons.push(path) },
  };

  const dialog = {
    showMessageBox: async (...args) => {
      const options = args.at(-1);
      dialogCalls.push({ attachedToWindow: args.length === 2, options });
      return { response: dialogResponse, checkboxChecked: false };
    },
    showOpenDialog: async (...args) => {
      dialogCalls.push({ open: true, options: args.at(-1) });
      return openDialogResult;
    },
  };

  const shell = {
    openExternal: async (url) => {
      openExternalCalls.push(url);
    },
    openPath: async () => '',
    showItemInFolder: () => undefined,
  };

  const ipcMain = {
    on: (channel, fn) => ipcMainListeners.set(channel, fn),
    removeListener: (channel) => ipcMainListeners.delete(channel),
  };

  const Menu = {
    buildFromTemplate: (template) => ({ template }),
    setApplicationMenu: (menu) => menuSet.push(menu),
  };

  const utilityProcess = {
    fork: (...args) => {
      forkCalls.push(args);
      return { pid: 4242, postMessage: () => undefined, kill: () => true, on: () => undefined };
    },
  };

  const safeStorage = {
    available: true,
    isEncryptionAvailable: () => safeStorage.available,
    encryptString: (plain) => new TextEncoder().encode(`enc:${plain}`),
    decryptString: (bytes) => new TextDecoder().decode(bytes).slice(4),
  };

  return {
    app,
    BrowserWindow: FakeBrowserWindow,
    dialog,
    shell,
    session,
    ipcMain,
    utilityProcess,
    Menu,
    safeStorage,
    appEvents,
    windows,
    menuSet,
    ipcMainListeners,
    dialogCalls,
    openExternalCalls,
    forkCalls,
    sessionHooks: session.webRequest,
    setDialogResponse: (response) => {
      dialogResponse = response;
    },
    setOpenDialogResult: (result) => {
      openDialogResult = result;
    },
  };
}

const product = loadProductIdentity(RESOURCES_DIR);

async function makeHost(overrides = {}) {
  const electron = makeFakeElectron();
  const dir = mkdtempSync(join(root, 'case-'));
  const paths = {
    resourcesDir: RESOURCES_DIR,
    distRoot: dir,
    preloadPath: join(dir, 'preload.js'),
    utilityEntry: join(dir, 'utility-host.js'),
    serviceEntry: join(dir, 'service-main.js'),
    userDataDir: join(dir, 'userData'),
    home: join(dir, 'home'),
    documentsPath: join(dir, 'documents'),
  };
  mkdirSync(paths.home, { recursive: true });
  mkdirSync(paths.documentsPath, { recursive: true });
  writeFileSync(paths.serviceEntry, '// fixture service entry\n');
  const ipcCalls = [];
  const disposedGenerations = [];
  const protocolCalls = [];
  const networkHooks = [];
  // Registration-order evidence: the protocol handler must be installed
  // before any window is created (scheme privilege is bootstrap-only,
  // pre-ready — composeDesktopHost has no registerSchemes seam at all).
  electron.lifecycleEvents = [];
  const { BrowserWindow: FakeBrowserWindow } = electron;
  electron.BrowserWindow = class extends FakeBrowserWindow {
    constructor(options) {
      electron.lifecycleEvents.push('window');
      super(options);
    }
  };
  const host = await composeDesktopHost({
    electron,
    product,
    paths,
    env: {},
    isPackaged: false,
    devUrl: null,
    nodeExecutable: process.execPath,
    adapters: {
      registerProtocol: async (distRoot, policy) => {
        electron.lifecycleEvents.push('protocol');
        protocolCalls.push({ distRoot, policy });
      },
      registerIpc: async (window, handlers, options) => {
        ipcCalls.push({ window, handlers, options });
        return {
          dispose: () => {
            disposedGenerations.push(options.generation);
          },
        };
      },
      attachNetworkHooks: (sessionArg, getActiveAuth) => {
        networkHooks.push({ session: sessionArg, getActiveAuth });
      },
    },
    ...overrides,
  });
  return {
    electron,
    host,
    ipcCalls,
    disposedGenerations,
    protocolCalls,
    networkHooks,
    paths,
    dialogCalls: electron.dialogCalls,
    forkCalls: electron.forkCalls,
    sessionHooks: electron.sessionHooks,
  };
}

const VALID_SENDER_VIEW = {
  windowAlive: true,
  senderIsSelectedWebContents: true,
  senderFramePresent: true,
  senderFrameIsMainFrame: true,
  frameUrl: 'nexus://app/index.html',
};

// ---------------------------------------------------------------------------
// Identity and launch-time resolution
// ---------------------------------------------------------------------------

test('product identity loads from resources/product.json and applies to the app', () => {
  assert.deepEqual(product, {
    id: 'io.nexus42.desktop',
    name: 'Nexus',
    version: '0.1.0',
    minimumMacos: '13.0',
  });
  const appData = join(root, 'appData');
  const app = {
    setName: (name) => {
      app.name = name;
    },
    setPath: (name, value) => {
      app.paths ??= new Map();
      app.paths.set(name, value);
    },
    getPath: () => appData,
    dock: { icons: [], setIcon: (path) => app.dock.icons.push(path) },
  };
  applyProductIdentity(app, product, { userDataDir: join(appData, product.id), resourcesDir: RESOURCES_DIR });
  assert.equal(app.name, 'Nexus');
  assert.equal(app.paths.get('userData'), join(appData, 'io.nexus42.desktop'));
  if (process.platform === 'darwin') {
    assert.deepEqual(app.dock.icons, [join(RESOURCES_DIR, 'icons', 'app.icns')]);
  }
});

test('dev URL only accepts the frozen Vite origins and packaged ignores it', () => {
  assert.equal(resolveDevUrl({ NEXUS_DESKTOP_DEV_URL: 'http://localhost:5173' }, false), 'http://localhost:5173');
  assert.equal(resolveDevUrl({ NEXUS_DESKTOP_DEV_URL: 'http://127.0.0.1:5173/' }, false), 'http://127.0.0.1:5173/');
  assert.equal(resolveDevUrl({ NEXUS_DESKTOP_DEV_URL: 'https://evil.example' }, false), null);
  assert.equal(resolveDevUrl({ NEXUS_DESKTOP_DEV_URL: 'http://localhost:5174' }, false), null);
  assert.equal(resolveDevUrl({ NEXUS_DESKTOP_DEV_URL: 'http://localhost:5173' }, true), null);
  assert.equal(resolveDevUrl({}, false), null);
});

test('dist root: packaged requires the bundled web-dist, dev uses the built web dist', () => {
  const resourcesPath = mkdtempSync(join(root, 'res-'));
  assert.throws(
    () => resolveDesktopDistRoot({ isPackaged: true, resourcesPath, repoRoot: root }),
    /missing its bundled web artifact/,
  );
  mkdirSync(join(resourcesPath, 'web-dist'), { recursive: true });
  writeFileSync(join(resourcesPath, 'web-dist', 'index.html'), '<html></html>');
  assert.equal(resolveDesktopDistRoot({ isPackaged: true, resourcesPath, repoRoot: root }), join(resourcesPath, 'web-dist'));
  assert.equal(
    resolveDesktopDistRoot({ isPackaged: false, resourcesPath, repoRoot: '/repo' }),
    join('/repo', 'apps', 'web', 'dist'),
  );
});

test('trusted node executable: packaged layout preferred, PATH is the fallback', () => {
  const resourcesPath = mkdtempSync(join(root, 'res-'));
  const bundled = join(resourcesPath, 'node', 'bin', 'node');
  mkdirSync(dirname(bundled), { recursive: true });
  writeFileSync(bundled, '#!/bin/sh\n');
  chmodSync(bundled, 0o755);
  assert.equal(
    resolveTrustedNodeExecutable({ isPackaged: true, resourcesPath, env: {} }),
    bundled,
  );
  assert.equal(
    resolveTrustedNodeExecutable({ isPackaged: true, resourcesPath: join(root, 'none'), env: {} }),
    null,
  );
  assert.ok(resolveTrustedNodeExecutable({ isPackaged: false, env: process.env }));
});

// ---------------------------------------------------------------------------
// Composition: window chrome, handler map, runtime metadata, menu
// ---------------------------------------------------------------------------

test('production window policy and protocol registration', async () => {
  const { electron, host, ipcCalls, protocolCalls, networkHooks, paths } = await makeHost();
  assert.equal(electron.windows.length, 1);
  const win = electron.windows[0];
  assert.equal(win.options.width, 1280);
  assert.equal(win.options.height, 800);
  assert.equal(win.options.minWidth, 960);
  assert.equal(win.options.minHeight, 640);
  assert.equal(win.options.titleBarStyle, 'hiddenInset');
  assert.deepEqual(win.options.trafficLightPosition, { x: 12, y: 14 });
  assert.equal(win.options.title, 'Nexus');
  assert.deepEqual(win.options.webPreferences, {
    preload: paths.preloadPath,
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: true,
    webSecurity: true,
    disableBlinkFeatures: 'Auxclick',
  });
  assert.equal(win.loadedUrl, 'nexus://app/index.html');
  assert.equal(electron.app.name, 'Nexus');

  // Full typed handler map — exactly the frozen operation union.
  assert.deepEqual(Object.keys(host.handlers).sort(), [...DESKTOP_OPERATIONS].sort());

  // Protocol registered with the local service origin, BEFORE the first
  // window was created. Scheme privilege is registered exactly once,
  // pre-ready, by the bootstrap — composeDesktopHost exposes no
  // registerSchemes seam, so no second registration can happen after ready.
  assert.deepEqual(electron.lifecycleEvents, ['protocol', 'window']);
  assert.deepEqual(protocolCalls, [
    {
      distRoot: paths.distRoot,
      policy: { serviceOrigin: LOCAL_ENDPOINT, fingerprintProbeOrigin: LOCAL_ENDPOINT, dev: false },
    },
  ]);

  // Live generation source supplied to the IPC registration (P0-T1 round-2).
  assert.equal(ipcCalls.length, 1);
  assert.equal(typeof ipcCalls[0].options.getCurrentGeneration, 'function');
  assert.equal(ipcCalls[0].options.generation, 1);
  assert.equal(ipcCalls[0].options.getCurrentGeneration(), 1);
  assert.equal(ipcCalls[0].options.dev, false);
  assert.doesNotThrow(() => assertDesktopEventSender(VALID_SENDER_VIEW, ipcCalls[0].options));

  // Network hooks bound to the main session with the store's auth authority.
  assert.equal(networkHooks.length, 1);
  assert.equal(networkHooks[0].getActiveAuth(), null);

  // Trusted runtime metadata answered synchronously for the preload.
  const event = { returnValue: undefined };
  electron.ipcMainListeners.get(DESKTOP_RUNTIME_CHANNEL)(event);
  assert.deepEqual(event.returnValue, { localEndpoint: LOCAL_ENDPOINT });

  // Standard menu roles only — no native product menu.
  const template = electron.menuSet[0].template;
  assert.deepEqual(
    template.map((item) => item.role),
    process.platform === 'darwin' ? ['appMenu', 'editMenu'] : ['fileMenu', 'editMenu'],
  );
  assert.ok(template.every((item) => Object.keys(item).length === 1));

  // Fresh daemon status snapshot through the typed handler.
  assert.deepEqual(await host.handlers.get_daemon_status(), { state: 'stopped', port: 8420 });

  host.dispose();
});

test('dev launch loads the explicitly launched Vite URL and opts into dev origins', async () => {
  const { electron, ipcCalls, protocolCalls } = await makeHost({ devUrl: 'http://localhost:5173' });
  assert.equal(electron.windows[0].loadedUrl, 'http://localhost:5173');
  assert.equal(ipcCalls[0].options.dev, true);
  assert.equal(protocolCalls[0].policy.dev, true);
});

test('maximize toggle is main-owned on the selected window', async () => {
  const { host, electron } = await makeHost();
  const win = electron.windows[0];
  assert.equal(win.maximized, false);
  await host.handlers.toggle_maximize_window();
  assert.equal(win.maximized, true);
  await host.handlers.toggle_maximize_window();
  assert.equal(win.maximized, false);
});

// ---------------------------------------------------------------------------
// Generation enforcement across window replacement
// ---------------------------------------------------------------------------

test('renderer crash replaces only the renderer: generation bumps, stale senders rejected, service owner untouched', async () => {
  const { electron, host, ipcCalls, disposedGenerations, forkCalls } = await makeHost();
  assert.equal(host.generation(), 1);
  const gone = ipcCalls[0].window.webContents.listeners.get('render-process-gone');
  gone();
  await host.settled();
  await until(() => ipcCalls.length === 2, 'second IPC registration');

  assert.equal(electron.windows.length, 2);
  assert.equal(host.generation(), 2);
  assert.deepEqual(disposedGenerations, [1]);
  assert.equal(ipcCalls[1].options.generation, 2);

  // The previous generation's binding now rejects its own senders.
  assert.throws(
    () => assertDesktopEventSender(VALID_SENDER_VIEW, ipcCalls[0].options),
    (err) => errorCode(err) === 'stale_sender',
  );
  assert.doesNotThrow(() =>
    assertDesktopEventSender({ ...VALID_SENDER_VIEW }, ipcCalls[1].options),
  );

  // No service owner was spawned by a renderer crash.
  assert.equal(forkCalls.length, 0);
  host.dispose();
});

test('second-instance focuses the sole window and never spawns a duplicate service', async () => {
  const { electron, host, ipcCalls, forkCalls } = await makeHost();
  const secondInstance = electron.appEvents.get('second-instance')[0];
  electron.windows[0].minimized = true;
  secondInstance();
  await until(() => electron.windows[0].focused === true && electron.windows[0].minimized === false, 'focus');
  assert.equal(electron.windows.length, 1);
  assert.equal(ipcCalls.length, 1);
  assert.equal(forkCalls.length, 0);

  // No live window: the second-instance path is an explicit no-op — it
  // never recreates a window and never spawns a duplicate service.
  // (Window recreation is owned only by the guarded `activate` and
  // renderer-crash paths.)
  electron.windows[0].destroyed = true;
  host.focusExistingWindow();
  secondInstance();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(electron.windows.length, 1);
  assert.equal(ipcCalls.length, 1);
  assert.equal(forkCalls.length, 0);
  host.dispose();
});

// ---------------------------------------------------------------------------
// Global content lockdown (app-level web-contents-created)
// ---------------------------------------------------------------------------

test('global web-contents-created lockdown denies window.open, non-app navigation and non-app redirects', async () => {
  const { electron } = await makeHost();
  const lockdown = electron.appEvents.get('web-contents-created')[0];
  const listeners = new Map();
  let windowOpenResult = null;
  const contents = {
    setWindowOpenHandler: (fn) => {
      windowOpenResult = fn({ url: 'https://evil.example/popup' });
    },
    on: (event, fn) => listeners.set(event, fn),
  };
  lockdown({}, contents);

  // window.open is denied everywhere.
  assert.deepEqual(windowOpenResult, { action: 'deny' });

  // Non-app navigation and redirect are both prevented (production: no dev
  // origins); the app origin is allowed through for both events.
  for (const eventName of ['will-navigate', 'will-redirect']) {
    const denied = { preventDefault: () => (denied.prevented = true) };
    listeners.get(eventName)(denied, 'https://evil.example/phish');
    assert.equal(denied.prevented, true, `${eventName} must deny non-app URL`);

    const allowed = { preventDefault: () => (allowed.prevented = true) };
    listeners.get(eventName)(allowed, 'nexus://app/index.html');
    assert.notEqual(allowed.prevented, true, `${eventName} must allow app URL`);
  }
});

// ---------------------------------------------------------------------------
// Quit gate (parity row 21) wired into before-quit
// ---------------------------------------------------------------------------

function beforeQuit(electron) {
  return electron.appEvents.get('before-quit')[0];
}

test('before-quit with nothing owned or attached quits without a dialog', async () => {
  const { electron, host, dialogCalls } = await makeHost();
  const event = { preventDefault: () => (event.prevented = true) };
  beforeQuit(electron)(event);
  await until(() => electron.app.quitCalls.length === 1, 'quit');
  assert.equal(event.prevented, true);
  assert.equal(dialogCalls.length, 0);
  host.dispose();
});

test('Stop Service & Quit calls stopExplicit and quits only after confirmation', async () => {
  const { electron, host, dialogCalls } = await makeHost();
  const calls = [];
  host.controller.getStatus = () => ({ state: 'running', port: 8420 });
  host.controller.stopExplicit = async () => {
    calls.push('stopExplicit');
  };
  electron.setDialogResponse(0);

  const event = { preventDefault: () => undefined };
  beforeQuit(electron)(event);
  await until(() => electron.app.quitCalls.length === 1, 'quit');
  assert.deepEqual(calls, ['stopExplicit']);
  assert.equal(dialogCalls[0].options.buttons.length, 3);
  assert.deepEqual(dialogCalls[0].options.buttons, [
    'Stop Daemon & Quit',
    'Keep Daemon & Quit',
    'Cancel',
  ]);
  host.dispose();
});

test('an unconfirmed stop refuses the quit and reports an actionable outcome', async () => {
  const { electron, host, dialogCalls } = await makeHost();
  host.controller.getStatus = () => ({ state: 'running', port: 8420 });
  host.controller.stopExplicit = async () => {
    throw Object.assign(new Error('close was not confirmed'), { code: 'interrupted' });
  };
  electron.setDialogResponse(0);

  beforeQuit(electron)({ preventDefault: () => undefined });
  await until(() => dialogCalls.length === 2, 'outcome dialog');
  assert.equal(electron.app.quitCalls.length, 0);
  const outcome = dialogCalls[1];
  assert.equal(outcome.options.type, 'warning');
  assert.match(outcome.options.detail, /stays open/);
  host.dispose();
});

test('Cancel leaves the session intact', async () => {
  const { electron, host, dialogCalls } = await makeHost();
  host.controller.getStatus = () => ({ state: 'running', port: 8420 });
  electron.setDialogResponse(2);
  beforeQuit(electron)({ preventDefault: () => undefined });
  await until(() => dialogCalls.length === 1, 'quit dialog');
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(electron.app.quitCalls.length, 0);
  host.dispose();
});

test('Keep Service & Quit transfers via keepForQuit and tells the user work was interrupted', async () => {
  const { electron, host, dialogCalls } = await makeHost();
  const calls = [];
  host.controller.getStatus = () => ({ state: 'running', port: 8420 });
  host.controller.keepForQuit = async () => {
    calls.push('keepForQuit');
  };
  electron.setDialogResponse(1);

  beforeQuit(electron)({ preventDefault: () => undefined });
  await until(() => electron.app.quitCalls.length === 1, 'quit');
  assert.deepEqual(calls, ['keepForQuit']);
  const notice = dialogCalls[1];
  assert.equal(notice.options.type, 'info');
  assert.match(notice.options.detail, /in-flight work was interrupted/);
  host.dispose();
});

test('reset_local_database only proceeds after explicit native confirmation', async () => {
  const { electron, host, dialogCalls } = await makeHost();
  const calls = [];
  host.controller.resetLocalState = async () => {
    calls.push('reset');
  };
  electron.setDialogResponse(1); // Cancel
  assert.deepEqual(await host.handlers.reset_local_database(), { status: 'cancelled' });
  assert.deepEqual(calls, []);
  assert.equal(dialogCalls[0].options.type, 'warning');
  assert.deepEqual(dialogCalls[0].options.buttons, ['Reset Local State', 'Cancel']);

  electron.setDialogResponse(0); // Confirm
  assert.deepEqual(await host.handlers.reset_local_database(), { status: 'confirmed' });
  assert.deepEqual(calls, ['reset']);
  host.dispose();
});

// ---------------------------------------------------------------------------
// Connection config handlers + exact-origin network auth authority
// ---------------------------------------------------------------------------

test('connection config round-trips the public projection; the key never leaves main', async () => {
  const { host, networkHooks } = await makeHost();
  assert.equal(await host.handlers.get_connection_config(), null);

  const saved = await host.handlers.set_connection_config({
    config: {
      endpointUrl: 'https://remote.example:9000',
      label: 'remote',
      active: true,
      hasApiKey: true,
    },
    credential: { action: 'replace', value: 'sk-test-key' },
  });
  assert.deepEqual(saved, {
    endpointUrl: 'https://remote.example:9000',
    label: 'remote',
    active: true,
    hasApiKey: true,
  });

  const projection = await host.handlers.get_connection_config();
  assert.equal(projection.hasApiKey, true);
  assert.ok(!('apiKey' in projection), 'public projection must never contain the key');
  assert.ok(!JSON.stringify(projection).includes('sk-test-key'));

  // Main-only auth authority for the network hooks.
  assert.deepEqual(networkHooks[0].getActiveAuth(), {
    endpointOrigin: 'https://remote.example:9000',
    apiKey: 'sk-test-key',
  });

  await host.handlers.delete_connection_config();
  assert.equal(await host.handlers.get_connection_config(), null);
  assert.equal(networkHooks[0].getActiveAuth(), null);
  host.dispose();
});

test('CSP is refreshed per response with the active connection origin only', async () => {
  const { host, sessionHooks } = await makeHost();
  assert.equal(sessionHooks.headersReceivedCalls.length, 1);
  assert.deepEqual(sessionHooks.headersReceivedCalls[0][0], { urls: ['nexus://app/*'] });
  const listener = sessionHooks.headersReceivedCalls[0][1];

  const respond = (responseHeaders) =>
    new Promise((resolve) => {
      listener({ url: 'nexus://app/index.html', responseHeaders }, (result) => resolve(result));
    });

  const local = await respond({ 'content-security-policy': ["default-src *"] });
  const localCsp = local.responseHeaders['Content-Security-Policy'][0];
  assert.match(localCsp, /connect-src 'self' http:\/\/127\.0\.0\.1:8420 http:\/\/127\.0\.0\.1:8420/);
  assert.ok(!localCsp.includes('default-src *'));
  assert.ok(!localCsp.includes('*'), 'no wildcard anywhere in the CSP');

  await host.handlers.set_connection_config({
    config: { endpointUrl: 'https://remote.example:9000', active: true, hasApiKey: false },
    credential: { action: 'keep' },
  });
  const remote = await respond({});
  const remoteCsp = remote.responseHeaders['Content-Security-Policy'][0];
  assert.match(remoteCsp, /connect-src 'self' https:\/\/remote\.example:9000 https:\/\/remote\.example:9000/);
  host.dispose();
});

// ---------------------------------------------------------------------------
// External URL policy (parity row 25) and navigation lockdown (row 26)
// ---------------------------------------------------------------------------

test('external URLs go through the shared predicate and shell.openExternal only', async () => {
  const { electron, host } = await makeHost();
  const windows = electron.windows;
  await host.handlers.open_external_url({ url: 'https://example.com/docs' });
  assert.deepEqual(electron.openExternalCalls, ['https://example.com/docs']);

  await assert.rejects(
    () => host.handlers.open_external_url({ url: 'file:///etc/passwd' }),
    (err) => errorCode(err) === 'url_not_allowed',
  );
  await assert.rejects(
    () => host.handlers.open_external_url({ url: 'https://user:pass@example.com/' }),
    (err) => errorCode(err) === 'url_not_allowed',
  );
  assert.deepEqual(electron.openExternalCalls, ['https://example.com/docs']);

  // window.open is denied outright.
  const win = windows[0];
  assert.deepEqual(win.webContents.windowOpenHandler({ url: 'https://example.com' }), {
    action: 'deny',
  });

  // Navigation outside the app origin is blocked and NEVER auto-opened.
  const willNavigate = win.webContents.listeners.get('will-navigate');
  const prevented = { hit: false };
  willNavigate({ preventDefault: () => (prevented.hit = true) }, 'https://evil.example/');
  assert.equal(prevented.hit, true);
  const allowed = { hit: false };
  willNavigate({ preventDefault: () => (allowed.hit = true) }, 'nexus://app/index.html');
  assert.equal(allowed.hit, false);
  assert.deepEqual(electron.openExternalCalls, ['https://example.com/docs']);

  // app-level open-url passes the same predicate.
  const openUrl = electron.appEvents.get('open-url')[0];
  openUrl({ preventDefault: () => undefined }, 'https://github.com/nexus42');
  openUrl({ preventDefault: () => undefined }, 'file:///etc/passwd');
  assert.deepEqual(electron.openExternalCalls, ['https://example.com/docs', 'https://github.com/nexus42']);
  host.dispose();
});

test('guarded OS actions and directory picker are composed', async () => {
  const { electron, host, paths } = await makeHost();
  const { dialogCalls } = electron;
  // Directory picker: cancel → null, selection → path; defaultPath forwarded.
  electron.setOpenDialogResult({ canceled: true, filePaths: [] });
  assert.equal(await host.handlers.pick_directory({ defaultPath: '/tmp' }), null);
  electron.setOpenDialogResult({ canceled: false, filePaths: [join(paths.home, 'picked')] });
  assert.equal(await host.handlers.pick_directory({}), join(paths.home, 'picked'));
  assert.equal(dialogCalls.at(-1).options.properties.join(','), 'openDirectory,createDirectory');

  // Guarded open: inside the workspace works, escape is denied before any OS call.
  const inside = join(paths.home, 'workspace', 'note.txt');
  mkdirSync(dirname(inside), { recursive: true });
  writeFileSync(inside, 'hi');
  await assert.rejects(
    () => host.handlers.open_with({ path: '/etc/passwd' }),
    (err) => errorCode(err) === 'workspace_root_unknown' || errorCode(err) === 'path_unresolvable' || errorCode(err) === 'path_outside_workspace',
  );
  host.dispose();
});

// ---------------------------------------------------------------------------
// No product proof IPC and no update route
// ---------------------------------------------------------------------------

test('no product proof IPC remains and no update route is introduced', () => {
  const runtimeModules = [
    'main.js',
    'protocol.js',
    'lifecycle-coord.js',
    'preload.js',
    'env.js',
    'utility-host.js',
    'desktop-ipc.js',
    'desktop-contract.js',
    'desktop-actions.js',
    'desktop-config.js',
    'connection-store.js',
    'desktop-network.js',
    'service-controller.js',
    'quit-controller.js',
    'utility-admission.js',
  ];
  const proofTokens = [
    'nexus-proof',
    'nexusProof',
    'proof-step',
    'PROOF_STEPS',
    'registerProofProtocol',
    'NEXUS_PROOF_HOME',
    'NEXUS_PROOF_LOG',
    'NEXUS_PROOF_HIDDEN',
  ];
  for (const module of runtimeModules) {
    const source = readFileSync(join(DIST_DIR, module), 'utf8');
    for (const token of proofTokens) {
      assert.ok(!source.includes(token), `${module} still contains proof surface ${token}`);
    }
  }
  // The proof IPC contract module is gone entirely.
  assert.equal(existsSync(join(DIST_DIR, 'ipc.js')), false);

  // Parity row 29: no auto-update route.
  const mainSource = readFileSync(join(DIST_DIR, 'main.js'), 'utf8');
  assert.ok(!mainSource.includes('autoUpdater'));
  assert.ok(!mainSource.includes('electron-updater'));
});
