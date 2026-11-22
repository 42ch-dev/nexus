/**
 * note: The document the Canvas renders, presented to its editing surface
 * (`PresetState`/`ParsedPreset`) without any YAML.
 */
import { describe, expect, it } from 'vitest';

import { candidateDocument } from '@/test/simulation-fixture';

import { parsedPresetFromIr } from './ir-preset';

describe('parsedPresetFromIr', () => {
  it('presents document nodes as states with their transitions', () => {
    const parsed = parsedPresetFromIr(candidateDocument());

    expect(parsed.manifest.preset.id).toBe('user/demo');
    expect(parsed.manifest.preset.initial).toBe('draft');
    expect(parsed.revision).toBe(1);
    expect(parsed.problems).toEqual([]);
    expect(parsed.manifest.states.map((state) => state.id)).toEqual([
      'draft',
      'routing',
      'pause',
      'done',
    ]);
    // note: One outgoing edge is a linear target.
    expect(parsed.manifest.states[0].next).toBe('routing');
    // note: Several become the conditional form, with the `default` edge as fallback.
    expect(parsed.manifest.states[1].next).toEqual({
      kind: 'conditional',
      rules: [{ when: undefined, to: 'pause' }],
      default: 'done',
    });
    expect(parsed.manifest.states[3].terminal).toBe(true);
  });

  it('keeps the condition on a state with exactly one guarded outgoing edge', () => {
    const parsed = parsedPresetFromIr(
      candidateDocument({
        edges: [
          { id: 'draft->done', from: 'draft', to: 'done', kind: 'conditional', when: 'quality >= 3' },
        ],
      }),
    );

    // note: The linear form would drop the guard, so the projection keeps the conditional form.
    expect(parsed.manifest.states[0].next).toEqual({
      kind: 'conditional',
      rules: [{ when: 'quality >= 3', to: 'done' }],
    });
  });

  it('leaves inner steps, pattern slots and nest groups out of the state list', () => {
    const parsed = parsedPresetFromIr(
      candidateDocument({
        nodes: [
          ...candidateDocument().nodes,
          { id: 'step', kind: 'inner_step', effect_class: 'pure', step_kind: 'acp_prompt', template: 'tmpl' },
          { id: 'slot', kind: 'pattern_slot', effect_class: 'external' },
          { id: 'group', kind: 'nest_group', effect_class: 'pure', child_workflow_id: 'child' },
        ],
      }),
    );

    expect(parsed.manifest.states.map((state) => state.id)).toEqual([
      'draft',
      'routing',
      'pause',
      'done',
    ]);
  });
});
