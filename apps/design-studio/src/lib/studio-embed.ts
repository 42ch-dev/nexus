/**
 * Studio iframe embed helpers — resolves forced light/dark from query string.
 */

export type EmbeddedTheme = 'light' | 'dark';

const EMBED_PARAM = 'studio-embed';

/** Message posted by embedded gallery documents after route commit. */
export type StudioEmbedReadyMessage = {
  type: 'nexus-studio-embed-ready';
  theme: EmbeddedTheme;
  path: string;
};

export function isStudioEmbedReadyMessage(value: unknown): value is StudioEmbedReadyMessage {
  if (!value || typeof value !== 'object') return false;
  const msg = value as Partial<StudioEmbedReadyMessage>;
  return (
    msg.type === 'nexus-studio-embed-ready' &&
    (msg.theme === 'light' || msg.theme === 'dark') &&
    typeof msg.path === 'string'
  );
}

/**
 * Returns a forced embed theme only when framed and `studio-embed` is exactly
 * `light` or `dark`. Top-level Studio ignores invalid or absent values.
 */
export function resolveEmbeddedTheme(search: string, isFramed: boolean): EmbeddedTheme | null {
  if (!isFramed) return null;
  const value = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search).get(
    EMBED_PARAM,
  );
  if (value === 'light' || value === 'dark') return value;
  return null;
}

/** Build an allowlisted same-origin embed URL for comparison frames. */
export function buildStudioEmbedSrc(path: string, hash: string, theme: EmbeddedTheme): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  const params = new URLSearchParams();
  params.set(EMBED_PARAM, theme);
  const fragment = hash.startsWith('#') ? hash : hash ? `#${hash}` : '';
  return `${normalizedPath}?${params.toString()}${fragment}`;
}
