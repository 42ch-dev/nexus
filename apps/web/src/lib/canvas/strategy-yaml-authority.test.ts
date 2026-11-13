/**
 * The Strategy canvas structural-write guard.
 *
 * The legacy YAML surface has many structural writers (header create, palette
 * command, spatial connect, keyboard create, edge inspector, draft replay).
 * They all dispatch through {@link createStrategyYamlAuthority}; this table is
 * the regression that keeps it that way: a new op added to
 * {@link STRUCTURAL_WRITE_OPS} has no dispatcher here until it is wired, and
 * the parameterised cases fail until the guard refuses it.
 */
import { describe, expect, it, vi } from 'vitest';

import {
  STRUCTURAL_WRITE_OPS,
  StructuralWriteRefusedError,
  createStrategyYamlAuthority,
  type StrategyYamlAuthority,
  type StructuralWriteOp,
} from './strategy-yaml-authority';

function makeWriter() {
  return {
    strategyPatchState: vi.fn().mockResolvedValue({ new_revision: 1 }),
    strategyPatchTransition: vi.fn().mockResolvedValue({ new_revision: 1 }),
  };
}

/** One dispatcher per enumerated op — `Record` makes a missing op a type error. */
const dispatch: Record<StructuralWriteOp, (authority: StrategyYamlAuthority) => Promise<unknown>> = {
  'state.patch': (authority) =>
    authority.patchState('preset-1', 'draft', {
      strategy_id: 'preset-1',
      state_id: 'draft',
      base_revision: 1,
      set: { label: 'Draft' },
    }),
  'transition.create': (authority) =>
    authority.patchTransition('preset-1', {
      strategy_id: 'preset-1',
      base_revision: 1,
      source_state_id: 'draft',
      new_target: 'done',
      op: 'create',
    }),
  'transition.update': (authority) =>
    authority.patchTransition('preset-1', {
      strategy_id: 'preset-1',
      base_revision: 1,
      source_state_id: 'draft',
      old_target: 'done',
      new_target: 'revise',
      op: 'update',
    }),
};

describe('createStrategyYamlAuthority', () => {
  it.each(STRUCTURAL_WRITE_OPS)('refuses %s while the candidate owns the document', async (op) => {
    const writer = makeWriter();
    const authority = createStrategyYamlAuthority(writer, () => true);

    const error = await dispatch[op](authority).catch((thrown: unknown) => thrown);

    expect(error).toBeInstanceOf(StructuralWriteRefusedError);
    expect(error).toMatchObject({ code: 'structural_write_refused', op });
    expect(writer.strategyPatchState).not.toHaveBeenCalled();
    expect(writer.strategyPatchTransition).not.toHaveBeenCalled();
  });

  it.each(STRUCTURAL_WRITE_OPS)('dispatches %s unchanged with no candidate document', async (op) => {
    const writer = makeWriter();
    const authority = createStrategyYamlAuthority(writer, () => false);

    await expect(dispatch[op](authority)).resolves.toEqual({ new_revision: 1 });
    expect(
      writer.strategyPatchState.mock.calls.length + writer.strategyPatchTransition.mock.calls.length,
    ).toBe(1);
  });

  it('reads the authority flag at dispatch time, so a replay crossing the flip is refused', async () => {
    let disabled = false;
    const writer = makeWriter();
    const authority = createStrategyYamlAuthority(writer, () => disabled);

    disabled = true;

    await expect(dispatch['transition.create'](authority)).rejects.toBeInstanceOf(
      StructuralWriteRefusedError,
    );
    expect(writer.strategyPatchTransition).not.toHaveBeenCalled();
  });
});
