/**
 * Workspace slug derivation (plan v1220 P3-T3).
 *
 * The single rule behind the create panel's auto-derived root preview. The
 * slug is client-proposed and core-validated: trim → lowercase → every maximal
 * run of non-`[a-z0-9]` becomes `-` → leading/trailing `-` trimmed. A name that
 * leaves nothing (pure CJK / symbol-only) falls back to
 * `ws-<8 lowercase hex of FNV-1a-32 over the trimmed name>`, so the slug stays
 * ASCII, deterministic and stable for the same name while the original display
 * name rides the request's `display_name`.
 *
 * Collisions are never auto-suffixed: a taken slug is a 409 inline error.
 */

const FNV_OFFSET_BASIS = 0x811c9dc5;
const FNV_PRIME = 0x01000193;

/** FNV-1a-32 over the UTF-8 bytes of `value`, unsigned. */
function fnv1a32(value: string): number {
  let hash = FNV_OFFSET_BASIS;
  for (const byte of new TextEncoder().encode(value)) {
    hash = Math.imul(hash ^ byte, FNV_PRIME) >>> 0;
  }
  return hash >>> 0;
}

/** Derive the ASCII workspace slug for a display name. */
export function deriveWorkspaceSlug(displayName: string): string {
  const trimmed = displayName.trim();
  const slug = trimmed
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  if (slug.length > 0) return slug;
  return `ws-${fnv1a32(trimmed).toString(16).padStart(8, '0')}`;
}
