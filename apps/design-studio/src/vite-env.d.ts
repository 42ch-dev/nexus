/// <reference types="vite/client" />
// see https://vitejs.dev/guide/env-and-mode

declare module '@web-locales/en/*.json' {
  const value: Record<string, unknown>;
  export default value;
}

