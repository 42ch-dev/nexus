/**
 * World KB canvas orchestrator — pure-helper tests (V1.73 P0).
 *
 * `patchFromForm` rebuilds the patch payload for the conflict-modal "Reapply"
 * path. It must produce the SAME shape the primary inspector submit path
 * produces, otherwise the reapply silently misbehaves (V1.73 greploop issue 4).
 */
import { describe, expect, it } from 'vitest';

import { patchFromForm, type EntityField } from '../world-kb-canvas';
import type { EntityEditForm } from '../entity-inspector';

function form(overrides: Partial<EntityEditForm> = {}): EntityEditForm {
  return {
    title: 'Aria',
    bodyText: '',
    aliasesText: '',
    block_type: 'character',
    ...overrides,
  };
}

describe('patchFromForm (reapply payload shape)', () => {
  it('preserves a non-empty trimmed title', () => {
    const patch = patchFromForm(form({ title: '  Aria Stormwind  ' }), [
      'title',
    ] as EntityField[]);
    expect(patch.title).toBe('Aria Stormwind');
  });

  // Regression for V1.73 greploop issue 4: coercing the trimmed empty string to
  // `undefined` dropped `title` from the JSON payload, so a reapply after the
  // user cleared the title sent an empty patch → 400 InvalidInput. The primary
  // submit path sends the empty string, surfacing a meaningful 422.
  it('keeps an explicitly-cleared title as the empty string (not undefined)', () => {
    const patch = patchFromForm(form({ title: '   ' }), ['title'] as EntityField[]);

    // `title` must be present as an empty string...
    expect(patch.title).toBe('');
    expect('title' in patch).toBe(true);
    // ...and survive JSON serialization so it reaches the server.
    expect(JSON.parse(JSON.stringify(patch)).title).toBe('');
  });

  it('omits title entirely when it is not in the dirty set', () => {
    const patch = patchFromForm(form({ title: 'unchanged' }), [
      'body',
    ] as EntityField[]);
    expect(patch.title).toBeUndefined();
    expect('title' in patch).toBe(false);
  });
});
