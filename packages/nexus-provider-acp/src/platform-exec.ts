/**
 * Platform command seam for the Windows-owned-process lane.
 *
 * Windows process identity and tree termination need OS queries (`Get-CimInstance`
 * for CreationDate/parent, `taskkill /T /F` for the owned tree) that cannot run on
 * the Darwin/Linux test hosts. The platform and the command runner are therefore
 * injectable, so the Windows code path is exercised for real in tests rather than
 * asserted away.
 */

import { execFileSync, execFile } from 'node:child_process';

/** Synchronous command runner (identity queries run on sync paths). */
export type SyncCommandRunner = (file: string, args: readonly string[]) => string;

/** Asynchronous command runner (termination runs on the async reap path). */
export type AsyncCommandRunner = (file: string, args: readonly string[]) => Promise<void>;

const defaultSyncRunner: SyncCommandRunner = (file, args) =>
  execFileSync(file, [...args], { encoding: 'utf8' });

const defaultAsyncRunner: AsyncCommandRunner = (file, args) => {
  const { promise, resolve, reject } = Promise.withResolvers<void>();
  execFile(file, [...args], (error) => (error ? reject(error) : resolve()));
  return promise;
};

let syncRunner: SyncCommandRunner = defaultSyncRunner;
let asyncRunner: AsyncCommandRunner = defaultAsyncRunner;
let platformOverride: string | null = null;

/** Effective platform: the real one unless a test override is installed. */
export function platformOf(): string {
  return platformOverride ?? process.platform;
}

export function isWindows(): boolean {
  return platformOf() === 'win32';
}

export function runSync(file: string, args: readonly string[]): string {
  return syncRunner(file, args);
}

export function runAsync(file: string, args: readonly string[]): Promise<void> {
  return asyncRunner(file, args);
}

/** @internal tests: force a platform for the OS-specific paths. */
export function setPlatformOverride(platform: string | null): void {
  platformOverride = platform;
}

/** @internal tests: inject the command runners. */
export function setCommandRunners(
  sync: SyncCommandRunner | null,
  async: AsyncCommandRunner | null,
): void {
  syncRunner = sync ?? defaultSyncRunner;
  asyncRunner = async ?? defaultAsyncRunner;
}

/**
 * PowerShell invocation that returns one process's CreationDate and parent PID.
 *
 * `ConvertTo-Json` keeps the parse minimal and deterministic; a missing process
 * yields `null`, which callers treat as "identity unavailable" (never as a match).
 */
export function win32IdentityCommand(pid: number): readonly string[] {
  return [
    '-NoProfile',
    '-NonInteractive',
    '-Command',
    `$p = Get-CimInstance Win32_Process -Filter "ProcessId=${pid}"; ` +
      `if ($p) { $p | Select-Object CreationDate,ParentProcessId | ConvertTo-Json -Compress }`,
  ];
}

/** `taskkill` invocation that terminates the owned process tree, force-kill. */
export function win32TreeKillCommand(pid: number): readonly string[] {
  return ['/PID', String(pid), '/T', '/F'];
}

export const WIN32_POWERSHELL = 'powershell.exe';
export const WIN32_TASKKILL = 'taskkill.exe';
