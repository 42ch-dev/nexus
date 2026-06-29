/**
 * World KB canvas helper utilities (V1.74 A10 split).
 *
 * Small, stateless functions extracted from the orchestrator so the canvas
 * facade stays focused on composition. `patchFromForm` is exported here and
 * re-exported from `world-kb-canvas.tsx` for backward-compatible test imports.
 */
import type { Node } from '@xyflow/react';

import type { EntityEditForm } from './entity-inspector';
import type { EntityField } from './world-kb-canvas-types';
import type { WorldKbNodeData } from './types';

/** Extract node data for the alt view (filters to entity/candidate nodes only). */
export function nodesToData(nodes: Node[]): WorldKbNodeData[] {
  return nodes
    .filter((n) => n.type === 'worldkb-entity')
    .map((n) => n.data as unknown as WorldKbNodeData)
    .filter(Boolean);
}

/**
 * Build a patch payload from the captured conflict form + dirty fields.
 * Exported for unit testing the reapply payload shape.
 */
export function patchFromForm(form: EntityEditForm, dirty: EntityField[]) {
  const patch: {
    title?: string;
    body?: Record<string, unknown>;
    aliases?: string[];
    block_type?: EntityEditForm['block_type'];
  } = {};
  // Preserve the trimmed value verbatim — including the empty string. The
  // primary handleSubmit path sends `form.title.trim()` directly, so an
  // explicitly-cleared title surfaces a meaningful 422 from the server.
  // Coercing `''` to `undefined` here dropped `title` from the JSON payload,
  // leaving an empty patch → 400 InvalidInput (V1.73 greploop issue 4).
  if (dirty.includes('title')) patch.title = form.title.trim();
  if (dirty.includes('block_type')) patch.block_type = form.block_type;
  if (dirty.includes('aliases')) {
    patch.aliases = form.aliasesText
      .split(',')
      .map((a: string) => a.trim())
      .filter(Boolean);
  }
  if (dirty.includes('body')) {
    patch.body = form.bodyText.trim() ? safeJson(form.bodyText) : undefined;
  }
  return patch;
}

export function safeJson(text: string): Record<string, unknown> | undefined {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

export function formatRelative(ts: number): string {
  const diff = Date.now() - ts;
  const mins = Math.round(diff / 60_000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  return hrs < 24 ? `${hrs}h ago` : `${Math.round(hrs / 24)}d ago`;
}
