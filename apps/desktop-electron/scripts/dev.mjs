#!/usr/bin/env node
/**
 * Unsigned Electron development driver.
 *
 * Both modes use the same prepared native payload, service build and host build.
 * Dist mode serves apps/web/dist through the nexus://app scheme; web mode owns
 * a Vite process and passes its loopback origin to the host. No native build,
 * Cargo/Tauri command, signing tool, or release path belongs here.
 */
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const APP_ROOT = resolve(__dirname, '..');
const REPO_ROOT = resolve(APP_ROOT, '..', '..');
const VITE_ORIGIN = 'http://127.0.0.1:5173';
const CHILD_STOP_TIMEOUT_MS = 5_000;
const STARTUP_TIMEOUT_MS = 30_000;

const children = new Set();
let shuttingDown = false;
let shutdownPromise = null;

function usage() {
  process.stderr.write('Usage: node scripts/dev.mjs --mode <dist|web>\n');
}

function parseMode(argv) {
  if (argv.length === 1 && argv[0] === '--help') {
    usage();
    process.exit(0);
  }
  if (argv.length !== 2 || argv[0] !== '--mode' || !['dist', 'web'].includes(argv[1])) {
    usage();
    throw new Error('expected exactly one supported --mode: dist or web');
  }
  return argv[1];
}

function commandLabel(command, args) {
  return [command, ...args].join(' ');
}

function signalProcessGroup(child, signal) {
  if (!child.pid) return;
  try {
    // Detached children own their process group, so this also reaches Electron
    // helpers, the service utility and Vite's worker descendants.
    process.kill(-child.pid, signal);
  } catch (error) {
    if (error?.code !== 'ESRCH') {
      try {
        child.kill(signal);
      } catch {
        // The child exited between the group and direct signal attempts.
      }
    }
  }
}

function spawnOwned(command, args, options = {}) {
  const child = spawn(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    env: options.env ?? process.env,
    stdio: options.stdio ?? 'inherit',
    detached: true,
  });
  children.add(child);
  child.once('exit', () => children.delete(child));
  child.once('error', () => children.delete(child));
  return child;
}

function runChecked(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const label = commandLabel(command, args);
    const child = spawnOwned(command, args, options);
    const onError = (error) => {
      children.delete(child);
      reject(new Error(`${label} failed to start: ${error.message}`));
    };
    child.once('error', onError);
    child.once('exit', (code, signal) => {
      children.delete(child);
      if (code === 0) {
        resolvePromise();
        return;
      }
      reject(new Error(`${label} exited with ${code ?? `signal ${signal ?? 'unknown'}`}`));
    });
  });
}

async function validateNativePayload() {
  const require = createRequire(import.meta.url);
  const envModule = require.resolve(join(APP_ROOT, 'dist', 'env.js'));
  const nativeCheck = await import(`file://${envModule}`);
  nativeCheck.assertNativePayloadPresent();
  process.stderr.write('[desktop-dev] native payload: present and compatible manifest found\n');
}

async function prepare(mode) {
  // Build the TypeScript dependency closure explicitly; pnpm does not build
  // workspace dependencies merely because the service imports their packages.
  for (const packageName of [
    '@42ch/nexus-contracts',
    '@42ch/nexus-native',
    '@42ch/nexus-provider-acp',
    '@42ch/nexus-service',
  ]) {
    await runChecked('pnpm', ['--filter', packageName, 'run', 'build']);
  }
  await runChecked('pnpm', ['--dir', APP_ROOT, 'run', 'build']);
  await validateNativePayload();
  if (mode === 'dist') {
    await runChecked('pnpm', ['--filter', 'web', 'run', 'build']);
  }
}

async function waitForVite(child) {
  const deadline = Date.now() + STARTUP_TIMEOUT_MS;
  let lastError = 'not responding';
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error(`Vite exited before readiness (${child.exitCode ?? child.signalCode})`);
    }
    try {
      const response = await fetch(`${VITE_ORIGIN}/`);
      if (response.ok) {
        process.stderr.write(`[desktop-dev] Vite ready at ${VITE_ORIGIN}\n`);
        return;
      }
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 150));
  }
  throw new Error(`Vite did not become ready within ${STARTUP_TIMEOUT_MS}ms (${lastError})`);
}

async function launch(mode) {
  let vite = null;
  const env = { ...process.env };
  if (mode === 'web') {
    vite = spawnOwned('pnpm', ['--dir', join(REPO_ROOT, 'apps', 'web'), 'run', 'dev', '--', '--host', '127.0.0.1'], {
      env,
    });
    await waitForVite(vite);
    env.NEXUS_DESKTOP_DEV_URL = VITE_ORIGIN;
  } else {
    delete env.NEXUS_DESKTOP_DEV_URL;
  }

  process.stderr.write(`[desktop-dev] mode=${mode}\n`);
  const electron = spawnOwned('pnpm', ['--dir', APP_ROOT, 'exec', 'electron', '.'], { env });
  const exitCode = await new Promise((resolvePromise, reject) => {
    electron.once('error', reject);
    electron.once('exit', (code, signal) => resolvePromise(code ?? (signal ? 128 : 1)));
  });
  if (vite && !shuttingDown) {
    signalProcessGroup(vite, 'SIGTERM');
  }
  if (exitCode !== 0 && !shuttingDown) {
    throw new Error(`Electron exited with ${exitCode}`);
  }
}

async function stopChildren() {
  const owned = [...children];
  if (owned.length === 0) return;
  for (const child of owned) signalProcessGroup(child, 'SIGINT');
  const deadline = Date.now() + CHILD_STOP_TIMEOUT_MS;
  while (children.size > 0 && Date.now() < deadline) {
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));
  }
  for (const child of children) signalProcessGroup(child, 'SIGKILL');
}

function installSignalHandlers() {
  const handleSignal = (signal) => {
    if (!shutdownPromise) {
      shuttingDown = true;
      shutdownPromise = stopChildren().finally(() => {
        process.exit(signal === 'SIGINT' ? 130 : 143);
      });
    }
  };
  process.on('SIGINT', () => handleSignal('SIGINT'));
  process.on('SIGTERM', () => handleSignal('SIGTERM'));
}

async function main() {
  const mode = parseMode(process.argv.slice(2));
  installSignalHandlers();
  await prepare(mode);
  await launch(mode);
}

main().catch(async (error) => {
  process.stderr.write(`[desktop-dev] ${error instanceof Error ? error.message : String(error)}\n`);
  await stopChildren();
  process.exitCode = 1;
});
