/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Daemon URL for dev proxy fallback resolution (see vite.config.ts). */
  readonly VITE_DAEMON_URL?: string;
  /**
   * P4-T3 development-only proof route gate. Vite statically replaces this
   * `import.meta.env` member, so an unset/other value is a compile-time `false`
   * in production and the proof page + its lazy chunk are tree-shaken out.
   */
  readonly VITE_RFT_NATIVE_PROOF?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
