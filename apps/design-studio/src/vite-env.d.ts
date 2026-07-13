/// <reference types="vite/client" />

/**
 * Type declarations for the Studio Vite alias that imports web's English
 * locale JSON catalogs. The alias is defined in vite.config.ts and mirrored
 * in vitest.config.ts / tsconfig.json paths.
 */
declare module '@web-locales/en/*.json' {
  const value: Record<string, unknown>;
  export default value;
}

