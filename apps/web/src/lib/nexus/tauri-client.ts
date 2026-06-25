/**
 * `TauriClient` — V1.65 desktop shell NexusClient implementation (STUB).
 *
 * Spec: web-ui.md §5 + §9 roadmap. In V1.64 the desktop shell does not ship;
 * this class freezes the boundary so P2 screens are transport-agnostic. When
 * `apps/desktop` lands in V1.65, each method will delegate to Tauri's
 * `invoke('plugin:nexus|<op>', { ... })` (or a sidecar IPC contract TBD) and
 * the SPA code stays unchanged — only the active client impl swaps.
 *
 * In the browser build every method throws `not_implemented_in_browser_build`
 * so any accidental selection surfaces immediately rather than silently
 * no-op'ing.
 */
import { NexusClientError } from './errors';
import type { DaemonHealth, NexusClient } from './types';

const NOT_IMPLEMENTED = (): never => {
  throw new NexusClientError(
    0,
    'not_implemented_in_browser_build',
    'TauriClient is not available in the browser build. The desktop shell ships in V1.65 (see apps/web/README.md §Roadmap).',
  );
};

export class TauriClient implements NexusClient {
  health(): Promise<DaemonHealth> {
    return NOT_IMPLEMENTED();
  }
  listWorks(): never {
    return NOT_IMPLEMENTED();
  }
  getWork(): never {
    return NOT_IMPLEMENTED();
  }
  createWork(): never {
    return NOT_IMPLEMENTED();
  }
  patchWork(): never {
    return NOT_IMPLEMENTED();
  }
  listSessions(): never {
    return NOT_IMPLEMENTED();
  }
  getSession(): never {
    return NOT_IMPLEMENTED();
  }
  listSchedules(): never {
    return NOT_IMPLEMENTED();
  }
  inspectSchedule(): never {
    return NOT_IMPLEMENTED();
  }
  listCapabilities(): never {
    return NOT_IMPLEMENTED();
  }
  listFindings(): never {
    return NOT_IMPLEMENTED();
  }
  listPresets(): never {
    return NOT_IMPLEMENTED();
  }
  scaffoldPreset(): never {
    return NOT_IMPLEMENTED();
  }
  validatePreset(): never {
    return NOT_IMPLEMENTED();
  }
  reloadPreset(): never {
    return NOT_IMPLEMENTED();
  }
  listChapters(): never {
    return NOT_IMPLEMENTED();
  }
  getChapter(): never {
    return NOT_IMPLEMENTED();
  }
  getChapterOutline(): never {
    return NOT_IMPLEMENTED();
  }
  putChapterOutline(): never {
    return NOT_IMPLEMENTED();
  }
  patchChapter(): never {
    return NOT_IMPLEMENTED();
  }
  getChapterBody(): never {
    return NOT_IMPLEMENTED();
  }
}
