#!/usr/bin/env node
/**
 * Bounded direct-exec launch probe for one published unsigned Nexus.app.
 *
 * Probe semantics (plan Design decision (b)): it execs
 * `<app>/Contents/MacOS/Nexus` directly — NEVER `open -W`, which returned 0
 * while no process lived (W2 evidence) — with stdout/stderr captured, for up
 * to `--timeout-ms` (default 20000). FAIL if the process exits for any reason
 * within the window (a healthy desktop app does not self-exit; success is not
 * pattern-matched on exit codes) or if stderr contains
 * `[desktop] bootstrap failed`. PASS only if the process is still alive at the
 * window's end with no marker. On EVERY exit path (early exit, marker failure,
 * window end, spawn error) the process group is terminated SIGTERM→SIGKILL
 * (best-effort, including when the leader has already exited) and the captured
 * stdout/stderr pipes are destroyed, so a descendant holding the pipes cannot
 * keep the bounded probe alive indefinitely.
 *
 * The probed binary is `<app>/Contents/MacOS/<CFBundleExecutable>` (fallback
 * `Nexus`), read from Info.plist exactly as `verify-package.mjs` derives it, so
 * the two verifiers cannot silently diverge.
 *
 * Captured stderr is retained only as a bounded tail (`STDERR_BUFFER_BYTES`,
 * 64 KiB) so a chatty run cannot grow memory without bound; the bootstrap
 * marker is latched as it streams in, so a marker older than the retained tail
 * is not lost.
 *
 * Coverage boundary (plan Design decision (c)): a PASS proves the produced
 * artifact completes bootstrap and stays alive for the window in the invoking
 * macOS session. It does NOT prove the consumer double-click path through
 * Gatekeeper quarantine (locally produced artifacts carry no quarantine
 * xattr), nor window-render or service-level behavior after bootstrap. There
 * is no pass-by-inability: on any non-darwin platform (or when the app cannot
 * be exec'd at all) this probe FAILS CLOSED — never a skipped or fake pass.
 *
 * Output: one JSON evidence record (command, timeoutMs, liveness, stderr
 * tail, verdict) on stdout; exit 0 pass / 1 fail.
 */
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, lstatSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const BOOTSTRAP_FAILED_MARKER = '[desktop] bootstrap failed';
const STDERR_TAIL_BYTES = 4096;
const STDERR_BUFFER_BYTES = 64 * 1024;
const KILL_GRACE_MS = 3000;
const DEFAULT_TIMEOUT_MS = 20000;

class ProbeError extends Error {}

function fail(message) {
  throw new ProbeError(message);
}

export function parseArgs(argv) {
  const args = { app: null, timeoutMs: DEFAULT_TIMEOUT_MS, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      if (args.help) fail('duplicate --help');
      args.help = true;
    } else if (arg === '--app') {
      if (args.app !== null) fail('duplicate --app');
      const value = argv[++index];
      if (!value || value.startsWith('-')) fail('--app requires a value');
      args.app = value;
    } else if (arg === '--timeout-ms') {
      const value = argv[++index];
      const timeoutMs = Number(value);
      if (!value || value.startsWith('-') || !Number.isInteger(timeoutMs) || timeoutMs <= 0) {
        fail('--timeout-ms requires a positive integer value');
      }
      args.timeoutMs = timeoutMs;
    } else if (arg.startsWith('--')) {
      fail(`unsupported option ${arg}; accepted options are --app and --timeout-ms`);
    } else {
      fail(`unexpected argument ${arg}`);
    }
  }
  if (!args.help && args.app === null) fail('--app is required');
  return args;
}

export function usage() {
  console.error('Usage: node apps/desktop-electron/scripts/verify-launch.mjs --app artifacts/desktop/<version>/darwin-<arch>/Nexus.app [--timeout-ms 20000]');
  console.error('Direct-execs the app binary and requires it to stay alive for the window with no bootstrap-failed marker. Fails closed off-darwin.');
}

function tail(text) {
  return text.length <= STDERR_TAIL_BYTES ? text : text.slice(-STDERR_TAIL_BYTES);
}

function killGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch {
    // Best-effort orphan cleanup: the group may already be gone.
  }
}

function requireExecutable(path, label) {
  if (!existsSync(path)) fail(`${label} missing: ${path}`);
  const stat = lstatSync(path);
  if (!stat.isFile()) fail(`${label} is not a regular file: ${path}`);
  if (stat.size === 0) fail(`${label} is empty: ${path}`);
}

/**
 * Resolve the bundle executable from Info.plist `CFBundleExecutable`, using the
 * same rule as `verify-package.mjs` (fallback `Nexus`) so the two verifiers
 * cannot silently diverge on the probed binary. A missing or unreadable plist
 * falls back to the contract name; the executable must still exist, which
 * `requireExecutable` enforces fail-closed.
 */
function resolveBundleExecutable(appPath) {
  const plistPath = join(appPath, 'Contents', 'Info.plist');
  if (!existsSync(plistPath)) return 'Nexus';
  try {
    const result = spawnSync('plutil', ['-convert', 'json', '-o', '-', '--', plistPath], { encoding: 'utf8' });
    if (result.status !== 0) return 'Nexus';
    const plist = JSON.parse(result.stdout);
    return (plist.CFBundleExecutable ?? '') ? plist.CFBundleExecutable : 'Nexus';
  } catch {
    return 'Nexus';
  }
}

/**
 * Run the bounded launch probe. `platform` is injectable so headless tests can
 * exercise the fail-closed path; the CLI always passes process.platform.
 * Resolves with the JSON evidence record; never rejects on a probe outcome.
 */
export function verifyLaunch({ appPath, timeoutMs, platform = process.platform }) {
  const command = join(appPath, 'Contents', 'MacOS', resolveBundleExecutable(appPath));
  const base = { command, timeoutMs, platform, liveness: false, stderrTail: '', verdict: 'FAIL' };

  if (platform !== 'darwin') {
    return Promise.resolve({
      ...base,
      reason: `unsupported platform: ${platform}; the direct-exec launch probe supports darwin only and fails closed rather than skipping`,
    });
  }
  if (!existsSync(appPath) || !lstatSync(appPath).isDirectory()) fail(`app bundle missing: ${appPath}`);
  requireExecutable(command, 'app executable');

  return new Promise((resolvePromise) => {
    let child;
    try {
      // detached: own process group so the whole group (and any keep-on-quit
      // handoff children that stayed in it) can be killed at window end.
      child = spawn(command, [], { detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (error) {
      resolvePromise({ ...base, reason: `spawn failed: ${error.message}` });
      return;
    }
    child.unref();

    let stderr = '';
    let markerSeen = false;
    let settled = false;

    // Terminate the process group and release the captured pipes on every exit
    // path. The leader may already have exited, so the group is signalled
    // best-effort; destroying the streams stops a descendant that inherited the
    // pipes from keeping this probe pending after the promise resolves.
    const cleanup = () => {
      killGroup(child.pid, 'SIGTERM');
      const force = setTimeout(() => killGroup(child.pid, 'SIGKILL'), KILL_GRACE_MS);
      force.unref();
      child.stdout?.destroy();
      child.stderr?.destroy();
    };

    const finish = (record) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      cleanup();
      resolvePromise(record);
    };

    child.stdout.resume();
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString('utf8');
      if (stderr.length > STDERR_BUFFER_BYTES) stderr = stderr.slice(-STDERR_BUFFER_BYTES);
      if (!markerSeen && stderr.includes(BOOTSTRAP_FAILED_MARKER)) markerSeen = true;
    });

    child.on('error', (error) => {
      finish({ ...base, reason: `spawn failed: ${error.message}`, stderrTail: tail(stderr) });
    });

    child.on('exit', (code, signal) => {
      finish({
        ...base,
        exitCode: code,
        exitSignal: signal,
        stderrTail: tail(stderr),
        reason: markerSeen
          ? `stderr contains "${BOOTSTRAP_FAILED_MARKER}"`
          : `process exited within the ${timeoutMs}ms window (exit code ${code}, signal ${signal}); a healthy desktop app stays alive`,
      });
    });

    const timer = setTimeout(() => {
      if (markerSeen) {
        finish({
          ...base,
          liveness: true,
          stderrTail: tail(stderr),
          reason: `stderr contains "${BOOTSTRAP_FAILED_MARKER}"`,
        });
      } else {
        finish({ ...base, liveness: true, stderrTail: tail(stderr), verdict: 'PASS', reason: `alive at window end with no bootstrap-failed marker` });
      }
    }, timeoutMs);
  });
}

const invokedDirectly = process.argv[1] !== undefined
  && import.meta.url === pathToFileURL(resolve(process.argv[1])).href;

if (invokedDirectly) {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) {
      usage();
    } else {
      const record = await verifyLaunch({ appPath: resolve(args.app), timeoutMs: args.timeoutMs });
      console.log(JSON.stringify(record, null, 2));
      process.exitCode = record.verdict === 'PASS' ? 0 : 1;
    }
  } catch (error) {
    console.error(error.message);
    usage();
    process.exitCode = 1;
  }
}
