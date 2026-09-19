/**
 * Desktop IPC registration (v1.192 P0-T1) — main-process side of the frozen
 * IPC and trust boundary.
 *
 * `registerDesktopIpc(window, handlers)` admits invoke envelopes on
 * `nexus:desktop:invoke` ONLY from the live selected top-frame of the app
 * origin (no `sender.getURL()` fallback), bounded by the frozen admission
 * limits, and dispatches to the typed handler map. Everything security-
 * relevant is exported as a pure function so `tests/desktop-security.test.mjs`
 * can exercise it without launching Electron.
 *
 * The `electron` value import is dynamic (exception to the static-import
 * rule): this module's pure helpers must stay importable in plain node where
 * the `electron` package's entry point is a binary-path string, not the API.
 */

import type { BrowserWindow, IpcMainInvokeEvent } from 'electron';
import {
  DEFAULT_REQUEST_TIMEOUT_MS,
  DESKTOP_INVOKE_CHANNEL,
  DESKTOP_STATUS_CHANNEL,
  MAX_QUEUED_BYTES,
  MAX_QUEUED_CALLS,
  MAX_REQUEST_BYTES,
  MAX_ACTIVE_CALLS,
  assertDesktopStatusFrame,
  desktopErr,
  desktopError,
  desktopOk,
  errorCode,
  errorMessage,
  isDesktopAppOrigin,
  jsonBytes,
  parseDesktopRequest,
  type DaemonStatus,
  type DesktopOperation,
  type DesktopOperationPayload,
  type DesktopOperationResult,
  type DesktopOriginOptions,
  type DesktopResponse,
} from './desktop-contract.js';

export type DesktopHandlerContext = { operation: DesktopOperation };

export type DesktopHandlers = {
  [O in DesktopOperation]: (
    payload: DesktopOperationPayload[O],
    ctx: DesktopHandlerContext,
  ) => Promise<DesktopOperationResult[O]>;
};

export interface RegisterDesktopIpcOptions extends DesktopOriginOptions {
  /** Current window generation; events from an older generation are stale. */
  generation?: number;
  requestTimeoutMs?: number;
}

/**
 * Structural view of an invoke event's sender, extracted by the registration
 * closure in main. `frameUrl` is `event.senderFrame?.url`; null means there is
 * no frame object and the request MUST be rejected (never fall back to
 * `sender.getURL()`).
 */
export interface DesktopSenderView {
  windowAlive: boolean;
  senderIsSelectedWebContents: boolean;
  senderFramePresent: boolean;
  senderFrameIsMainFrame: boolean;
  frameUrl: string | null;
  generation: number;
}

/**
 * Frozen sender check: live selected window → selected webContents → real
 * frame object → top main frame → current generation → exact app origin.
 * Throws coded errors (invalid_sender / stale_sender / invalid_origin).
 */
export function assertDesktopSender(
  view: DesktopSenderView,
  currentGeneration: number,
  options?: DesktopOriginOptions,
): void {
  if (!view.windowAlive || !view.senderIsSelectedWebContents) {
    throw desktopError('invalid_sender', 'ipc sender is not the selected live desktop window');
  }
  if (!view.senderFramePresent || !view.senderFrameIsMainFrame) {
    throw desktopError('invalid_sender', 'ipc sender is not the top-level app frame');
  }
  if (view.generation !== currentGeneration) {
    throw desktopError('stale_sender', 'ipc sender belongs to a previous window generation');
  }
  if (typeof view.frameUrl !== 'string' || !isDesktopAppOrigin(view.frameUrl, options)) {
    throw desktopError('invalid_origin', 'ipc sender frame is not the app origin');
  }
}

interface QueueEntry {
  start: () => void;
}

/**
 * Admission ledger: at most MAX_ACTIVE_CALLS in flight; overflow queues up to
 * MAX_QUEUED_CALLS calls subject to a MAX_QUEUED_BYTES aggregate cap; further
 * overflow is rejected `busy`. Resolves with a release function once a slot
 * has actually been acquired. One ledger per registration.
 */
export class DesktopAdmission {
  private active = 0;
  private readonly queue: QueueEntry[] = [];
  private queuedBytes = 0;

  /** Returns a release function once admitted, or null when the caller must be rejected `busy`. */
  async admit(payloadBytes: number): Promise<(() => void) | null> {
    if (this.active < MAX_ACTIVE_CALLS) {
      this.active += 1;
      return () => this.release();
    }
    if (
      this.queue.length >= MAX_QUEUED_CALLS ||
      this.queuedBytes + payloadBytes > MAX_QUEUED_BYTES
    ) {
      return null;
    }
    this.queuedBytes += payloadBytes;
    await new Promise<void>((resolve) => {
      this.queue.push({ start: resolve });
    });
    this.queuedBytes -= payloadBytes;
    this.active += 1;
    return () => this.release();
  }

  private release(): void {
    this.active -= 1;
    this.queue.shift()?.start();
  }
}

export interface RegisterDesktopIpcResult {
  /** Removes the invoke handler (window teardown / generation replacement). */
  dispose: () => void;
}

function requestIdOf(raw: unknown): string {
  if (raw && typeof raw === 'object' && 'request_id' in raw && typeof raw.request_id === 'string') {
    return raw.request_id;
  }
  return 'unknown';
}

/**
 * Register the typed desktop invoke channel on `ipcMain` for one selected
 * window. The electron value import is dynamic so this module's pure helpers
 * stay importable in plain node tests (see file header).
 */
export async function registerDesktopIpc(
  window: BrowserWindow,
  handlers: DesktopHandlers,
  options: RegisterDesktopIpcOptions = {},
): Promise<RegisterDesktopIpcResult> {
  const { ipcMain } = await import('electron');
  const generation = options.generation ?? 0;
  const timeoutMs = options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
  const admission = new DesktopAdmission();

  const invokeHandler = async (event: IpcMainInvokeEvent, raw: unknown): Promise<DesktopResponse> => {
    const webContents = window.webContents;
    let request;
    try {
      assertDesktopSender(
        {
          windowAlive: !window.isDestroyed(),
          senderIsSelectedWebContents: event.sender === webContents,
          senderFramePresent: event.senderFrame != null,
          senderFrameIsMainFrame: event.senderFrame === webContents.mainFrame,
          frameUrl: event.senderFrame?.url ?? null,
          generation,
        },
        generation,
        options,
      );
      request = parseDesktopRequest(raw);
    } catch (err) {
      return desktopErr(requestIdOf(raw), errorCode(err), errorMessage(err));
    }

    const release = await admission.admit(Math.min(jsonBytes(raw ?? null), MAX_REQUEST_BYTES));
    if (!release) {
      return desktopErr(request.request_id, 'busy', 'desktop admission cap exceeded');
    }
    try {
      const handlerFn = handlers[request.operation] as (
        payload: never,
        ctx: DesktopHandlerContext,
      ) => Promise<unknown>;
      const result = await withDeadline(
        handlerFn(request.payload as never, { operation: request.operation }),
        timeoutMs,
        request.request_id,
      );
      return desktopOk(request.request_id, result);
    } catch (err) {
      return desktopErr(request.request_id, errorCode(err), errorMessage(err));
    } finally {
      release();
    }
  };

  ipcMain.handle(DESKTOP_INVOKE_CHANNEL, invokeHandler);
  return {
    dispose: () => {
      ipcMain.removeHandler(DESKTOP_INVOKE_CHANNEL);
    },
  };
}

async function withDeadline<T>(promise: Promise<T>, timeoutMs: number, requestId: string): Promise<T> {
  let rejectDeadline: ((err: Error) => void) | undefined;
  const timer = setTimeout(() => {
    rejectDeadline?.(desktopError('timeout', `desktop request ${requestId} timed out after ${timeoutMs}ms`));
  }, timeoutMs);
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_resolve, reject) => {
        rejectDeadline = reject;
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Send a bounded status frame to the selected window's renderer. Validates the
 * frozen status bounds (4 KiB frame / 2 KiB detail tail) before sending.
 */
export async function sendDesktopStatus(window: BrowserWindow, status: DaemonStatus): Promise<void> {
  const validated = assertDesktopStatusFrame(status);
  if (window.isDestroyed()) return;
  window.webContents.send(DESKTOP_STATUS_CHANNEL, validated);
}
