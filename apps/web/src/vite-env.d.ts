/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Daemon URL for dev proxy fallback resolution (see vite.config.ts). */
  readonly VITE_DAEMON_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
