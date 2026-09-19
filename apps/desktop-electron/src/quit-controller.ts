/**
 * Quit gate for the desktop host (v1.192 P0-T6).
 *
 * One serialized decision per quit — the plan's "actual quit goes through this
 * single-flight dialog" — over the three frozen choices (parity row 21):
 *
 *   1. **Stop Service & Quit** — the authenticated instance/epoch-bound stop
 *      (`stopExplicit()`, `rust-core-service-boundary.md` §8.1) waits for a real
 *      close. A close that is not confirmed refuses the quit and surfaces the
 *      interrupted state: the app stays open with its recovery path. Ordinary
 *      `stop()` is deliberately not used here — row 8 says an attached
 *      independent service keeps running through it, which is not an explicit
 *      stop.
 *   2. **Keep Service & Quit** — D-21: the utility can never outlive the app, so
 *      Keep is a *transfer*, not a detach. `keepForQuit()` verifies the
 *      installed standalone Node (≥22.22) and the packaged service entry,
 *      confirms the cooperative close, starts that same entry as a detached Node
 *      child on the same home/port and only then releases ownership. In-flight
 *      work is interrupted; the outcome says so instead of promising continuity.
 *   3. **Cancel** — no quit, session intact.
 *
 * An absent Node runtime, a missing service entry or any other handoff failure
 * never turns into a quit: the app-owned service is still running, and the
 * refusal is reported with an actionable reason. Nothing is stopped as a
 * fallback, so a user's in-flight work is never silently dropped.
 *
 * The dialog and the user-visible outcome are main-owned adapters injected here;
 * `createDetachedServiceHandoff` is the P0-T6-owned `DetachedServiceHandoff` the
 * P0-T4 controller injects. Nothing in this module imports Electron.
 */

import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { accessSync, constants, statSync } from 'node:fs';
import { delimiter, join } from 'node:path';
import type { DaemonStatus } from './desktop-contract.js';
import { sanitizeInheritedEnv } from './env.js';
import { utilityError, utilityMessageText, type DetachedServiceHandoff } from './service-controller.js';

// ---------------------------------------------------------------------------
// Quit gate (Stop Service & Quit / Keep Service & Quit / Cancel)
// ---------------------------------------------------------------------------

/** The three frozen choices (parity row 21). */
export type QuitChoice = 'stop' | 'keep' | 'cancel';

/**
 * One quit decision, reported to the main-owned user-visible adapter.
 *
 * `allowed` is exactly what `requestQuit()` returns. Reported outcomes are the
 * ones the user must see: every refusal, and the Keep transfer (whose in-flight
 * work was interrupted). A plain quit, a Cancel and the nothing-to-do fast path
 * report nothing.
 */
export interface QuitOutcome {
  /** Whether the app may quit. */
  allowed: boolean;
  /** The branch that ran; `null` when no choice was produced (dismissal, dialog failure). */
  choice: QuitChoice | null;
  /** Actionable reason for a refusal, or the interruption notice for a Keep. */
  detail?: string;
}

/**
 * D-21: Keep interrupts in-flight work. The user is told that, and is not
 * promised that the work continues — the independent service is a new owner,
 * not a resumed session.
 */
export const KEEP_INTERRUPTED_DETAIL =
  'in-flight work was interrupted; the service continues as an independent process';

/** The subset of the P0-T4 controller this gate drives. */
export interface QuitServiceController {
  /** Authenticated instance/epoch-bound stop; an unconfirmed close rejects. */
  stopExplicit(): Promise<void>;
  /** Confirmed close plus detached handoff; throws when it cannot transfer. */
  keepForQuit(): Promise<void>;
  /** Frozen status snapshot `{state, version?, port, detail?}`. */
  getStatus(): DaemonStatus;
}

export interface DesktopQuitControllerOptions {
  controller: QuitServiceController;
  /**
   * Main-owned dialog for the three frozen choices. A dismissed dialog resolves
   * `null` and counts as Cancel.
   */
  chooseQuitChoice(status: DaemonStatus): Promise<QuitChoice | null>;
  /** Main-owned user-visible outcome; see {@link QuitOutcome} for when it is called. */
  reportQuitOutcome(outcome: QuitOutcome): void;
}

export class DesktopQuitController {
  private readonly controller: QuitServiceController;
  private readonly chooseQuitChoice: (status: DaemonStatus) => Promise<QuitChoice | null>;
  private readonly reportQuitOutcome: (outcome: QuitOutcome) => void;
  /** Single-flight: `before-quit` re-raises while a decision is running. */
  private inFlight: Promise<boolean> | null = null;

  constructor(options: DesktopQuitControllerOptions) {
    this.controller = options.controller;
    this.chooseQuitChoice = options.chooseQuitChoice;
    this.reportQuitOutcome = options.reportQuitOutcome;
  }

  /** Single-flight quit decision; `true` means the app may quit. */
  requestQuit(): Promise<boolean> {
    if (this.inFlight) return this.inFlight;
    const flight = this.decide().finally(() => {
      if (this.inFlight === flight) this.inFlight = null;
    });
    this.inFlight = flight;
    return flight;
  }

  private async decide(): Promise<boolean> {
    const status = this.controller.getStatus();
    // Shipped semantics (`apps/desktop/src-tauri/src/lib.rs` quit prompt): with
    // nothing this app runs or is attached to, there is no service to stop or
    // keep, so the quit is not gated by a dialog.
    if (status.state === 'stopped') return true;

    let choice: QuitChoice | null;
    try {
      choice = await this.chooseQuitChoice(status);
    } catch (error) {
      return this.refuse(null, `the quit dialog failed, so the app stays open: ${utilityMessageText(error)}`);
    }
    if (choice === null || choice === 'cancel') return false;

    if (choice === 'stop') {
      try {
        await this.controller.stopExplicit();
      } catch (error) {
        return this.refuse(
          'stop',
          `the local service close was not confirmed, so the app stays open: ${utilityMessageText(error)}`,
        );
      }
      return true;
    }

    try {
      await this.controller.keepForQuit();
    } catch (error) {
      return this.refuse(
        'keep',
        `the service was not transferred to an independent process, so the app stays open: ${utilityMessageText(error)}`,
      );
    }
    this.report(true, 'keep', KEEP_INTERRUPTED_DETAIL);
    return true;
  }

  private refuse(choice: QuitChoice | null, detail: string): boolean {
    this.report(false, choice, detail);
    return false;
  }

  private report(allowed: boolean, choice: QuitChoice | null, detail: string): void {
    // The decision is already made; a broken reporter must not change it.
    try {
      this.reportQuitOutcome({ allowed, choice, detail });
    } catch {
      // ignored
    }
  }
}

// ---------------------------------------------------------------------------
// Detached handoff (the D-21 Keep transfer target)
// ---------------------------------------------------------------------------

/** Minimum standalone Node for the detached service (`package.json` `engines`). */
export const DETACHED_SERVICE_NODE_VERSION = '22.22.0';

const NODE_VERSION_RE = /^v?(\d+)\.(\d+)\.(\d+)/;
const NODE_PROBE_TIMEOUT_MS = 5_000;

export interface DetachedHandoffOptions {
  /** Absolute packaged standalone service entry (`dist/main.js`), resolved by main. */
  serviceEntry: string;
  /** Trusted standalone Node executable; default: the first `node` on `PATH`. */
  nodeExecutable?: string;
  /** Trusted child environment; default `sanitizeInheritedEnv(process.env)`, as for the utility owner. */
  env?: NodeJS.ProcessEnv;
  /** Deadline for the `<node> --version` probe. */
  probeTimeoutMs?: number;
}

/**
 * Release-line comparison: a version at or above `minimum` qualifies, so a newer
 * major/minor/patch does and a pre-release of a newer line does too.
 */
export function isCompatibleNodeVersion(
  version: string,
  minimum: string = DETACHED_SERVICE_NODE_VERSION,
): boolean {
  const actual = parseNodeVersion(version);
  const required = parseNodeVersion(minimum);
  if (!actual || !required) return false;
  for (let part = 0; part < 3; part += 1) {
    if (actual[part] !== required[part]) return actual[part] > required[part];
  }
  return true;
}

function parseNodeVersion(version: string): [number, number, number] | null {
  const parsed = NODE_VERSION_RE.exec(version.trim());
  if (!parsed) return null;
  return [Number(parsed[1]), Number(parsed[2]), Number(parsed[3])];
}

/**
 * First executable `node` on the trusted `PATH`. An explicit
 * {@link DetachedHandoffOptions.nodeExecutable} is preferred: a GUI-launched app
 * inherits launchd's minimal `PATH`, which does not contain a user-managed Node.
 */
export function findNodeOnPath(env: NodeJS.ProcessEnv = process.env): string | null {
  const path = env.PATH;
  if (!path) return null;
  for (const dir of path.split(delimiter)) {
    if (dir === '') continue;
    // `stat`, not `lstat`: a Node installed by a package manager is legitimately
    // a symlink. The strict no-symlink posture applies to user-home records.
    const candidate = join(dir, 'node');
    try {
      if (!statSync(candidate).isFile()) continue;
      accessSync(candidate, constants.X_OK);
      return candidate;
    } catch {
      continue;
    }
  }
  return null;
}

function probeNodeVersion(
  node: string,
  env: NodeJS.ProcessEnv,
  timeoutMs: number,
): Promise<string> {
  const { promise, resolve, reject } = Promise.withResolvers<string>();
  execFile(node, ['--version'], { env, timeout: timeoutMs, encoding: 'utf8' }, (error, stdout) => {
    if (error) {
      reject(
        utilityError(
          'unavailable',
          `the installed Node runtime could not be probed (${node}): ${utilityMessageText(error)}`,
        ),
      );
      return;
    }
    resolve(String(stdout).trim());
  });
  return promise;
}

function assertServiceEntry(serviceEntry: string): void {
  try {
    // Bundle-resolved path (main-owned), so a symlink is followed rather than
    // refused; only the packaged regular file counts as available.
    if (statSync(serviceEntry).isFile()) return;
  } catch {
    // falls through to the actionable error
  }
  throw utilityError('unavailable', `the packaged service entry is not a readable file: ${serviceEntry}`);
}

function waitForSpawn(child: ChildProcess): Promise<void> {
  const { promise, resolve, reject } = Promise.withResolvers<void>();
  child.once('spawn', () => resolve());
  child.once('error', (error: Error) => {
    reject(utilityError('interrupted', `the detached service child failed to start: ${error.message}`));
  });
  return promise;
}

/**
 * Node/entry adapter for `keepForQuit()`: verify the installed standalone Node
 * (≥22.22) and the packaged service entry, then start that entry as an
 * independent Node child on the same home/port. `prepare()` runs before the
 * controller closes the owned service, so a missing runtime or entry leaves the
 * app-owned service running instead of leaving no owner at all.
 */
export function createDetachedServiceHandoff(options: DetachedHandoffOptions): DetachedServiceHandoff {
  const serviceEntry = options.serviceEntry;
  const env = options.env ?? sanitizeInheritedEnv(process.env);
  const probeTimeoutMs = options.probeTimeoutMs ?? NODE_PROBE_TIMEOUT_MS;
  let resolvedNode: string | null = options.nodeExecutable ?? null;

  function requireNode(): string {
    if (resolvedNode === null) {
      const found = findNodeOnPath(env);
      if (found === null) {
        throw utilityError(
          'unavailable',
          `no standalone Node runtime was found on PATH; install Node ${DETACHED_SERVICE_NODE_VERSION} or newer to keep the service running after quit`,
        );
      }
      resolvedNode = found;
    }
    return resolvedNode;
  }

  return {
    async prepare(): Promise<void> {
      const node = requireNode();
      const version = await probeNodeVersion(node, env, probeTimeoutMs);
      if (!isCompatibleNodeVersion(version)) {
        throw utilityError(
          'unavailable',
          parseNodeVersion(version) === null
            ? `the installed Node runtime did not report a usable version (${version || 'no output'}) at ${node}`
            : `the installed Node runtime (${version}) is older than the required ${DETACHED_SERVICE_NODE_VERSION}`,
        );
      }
      assertServiceEntry(serviceEntry);
    },

    async start({ home, host, port }: { home: string; host: string; port: number }): Promise<void> {
      const node = requireNode();
      assertServiceEntry(serviceEntry);
      let child: ChildProcess;
      try {
        // `detached` + `unref` + `stdio:'ignore'`: the child holds no pipe and no
        // process group of the exiting parent, so it is the one owner left.
        child = spawn(node, [serviceEntry, '--home', home, '--host', host, '--port', String(port)], {
          detached: true,
          stdio: 'ignore',
          env,
        });
      } catch (error) {
        throw utilityError(
          'interrupted',
          `the detached service child could not be started: ${utilityMessageText(error)}`,
        );
      }
      await waitForSpawn(child);
      child.unref();
    },
  };
}
