/**
 * `command-registry` — action registry + types for the shared command palette
 * (FB-CP-001, FB-CP-003, V1.111 P0 T1).
 *
 * Coverage mirrors the task brief: register / lookup / unregister / filter /
 * rank, plus the `useCommands()` + `useRegisterCommand()` React bindings.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import {
  clearCommands,
  filterCommands,
  getCommands,
  registerCommand,
  unregisterCommand,
  useCommands,
  useRegisterCommand,
  type Command,
} from './command-registry';

/** Factory for a minimal command; overrides only what each test cares about. */
function makeCommand(
  overrides: Partial<Command> & Pick<Command, 'id'>,
): Command {
  return {
    labelKey: overrides.id,
    groupKey: 'test.group',
    handler: vi.fn(),
    ...overrides,
  };
}

/** Resolve a Command's key fields into the strings `filterCommands` ranks on. */
function resolve(command: Command, label?: string, keywords?: string[]) {
  return {
    ...command,
    label: label ?? command.labelKey,
    group: command.groupKey,
    keywords: keywords ?? command.keywordKeys ?? [],
  };
}

const ADD_CHAPTER: Command = makeCommand({
  id: 'outline.add-chapter',
  labelKey: 'Add Chapter',
  groupKey: 'Outline',
  keywordKeys: ['new chapter', 'insert'],
});
const ADD_SCENE: Command = makeCommand({
  id: 'outline.add-scene',
  labelKey: 'Add Scene',
  groupKey: 'Outline',
  keywordKeys: ['beat'],
});
const RUN_STRATEGY: Command = makeCommand({
  id: 'strategy.run',
  labelKey: 'Run Strategy',
  groupKey: 'Strategy',
  keywordKeys: ['execute', 'start'],
});

describe('command store — register / lookup / unregister', () => {
  beforeEach(() => {
    clearCommands();
  });

  it('starts empty', () => {
    expect(getCommands()).toEqual([]);
  });

  it('registers a command and exposes it in the snapshot', () => {
    registerCommand(ADD_CHAPTER);
    expect(getCommands()).toEqual([ADD_CHAPTER]);
  });

  it('preserves registration order across multiple registers', () => {
    registerCommand(ADD_CHAPTER);
    registerCommand(ADD_SCENE);
    registerCommand(RUN_STRATEGY);
    expect(getCommands().map((c) => c.id)).toEqual([
      'outline.add-chapter',
      'outline.add-scene',
      'strategy.run',
    ]);
  });

  it('warns in dev mode when registering a duplicate id', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    registerCommand(ADD_CHAPTER);
    const duplicate = makeCommand({
      id: 'outline.add-chapter',
      labelKey: 'Add Chapter (duplicate)',
    });
    registerCommand(duplicate);
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0][0]).toContain('outline.add-chapter');
    expect(warn.mock.calls[0][0]).toContain('Add Chapter');
    warn.mockRestore();
  });

  it('replaces a command when re-registered with the same id (idempotent)', () => {
    registerCommand(ADD_CHAPTER);
    const updated: Command = makeCommand({
      id: 'outline.add-chapter',
      labelKey: 'Add Chapter (renamed)',
    });
    registerCommand(updated);
    expect(getCommands()).toEqual([updated]);
    expect(getCommands()).toHaveLength(1);
  });

  it('removes a command on unregister', () => {
    registerCommand(ADD_CHAPTER);
    registerCommand(ADD_SCENE);
    unregisterCommand('outline.add-chapter');
    expect(getCommands()).toEqual([ADD_SCENE]);
  });

  it('unregister is a no-op for an unknown id (no snapshot change, no throw)', () => {
    registerCommand(ADD_CHAPTER);
    const before = getCommands();
    // Unknown unregister should not throw or change the snapshot reference.
    expect(() => unregisterCommand('does.not.exist')).not.toThrow();
    expect(getCommands()).toBe(before);
    expect(getCommands()).toEqual([ADD_CHAPTER]);
  });

  it('clearCommands empties the store', () => {
    registerCommand(ADD_CHAPTER);
    registerCommand(ADD_SCENE);
    clearCommands();
    expect(getCommands()).toEqual([]);
  });

  it('clearCommands is a no-op when already empty (no spurious emit)', () => {
    expect(() => clearCommands()).not.toThrow();
    expect(getCommands()).toEqual([]);
  });
});

describe('useCommands — useSyncExternalStore subscription', () => {
  beforeEach(() => {
    clearCommands();
  });

  it('reflects the initial empty store on first render', () => {
    const { result } = renderHook(() => useCommands());
    expect(result.current).toEqual([]);
  });

  it('re-renders with a new snapshot when a command is registered', () => {
    const { result } = renderHook(() => useCommands());
    expect(result.current).toEqual([]);

    act(() => {
      registerCommand(ADD_CHAPTER);
    });
    expect(result.current).toEqual([ADD_CHAPTER]);

    act(() => {
      registerCommand(ADD_SCENE);
    });
    expect(result.current).toEqual([ADD_CHAPTER, ADD_SCENE]);
  });

  it('re-renders when a command is unregistered', () => {
    registerCommand(ADD_CHAPTER);
    registerCommand(ADD_SCENE);
    const { result } = renderHook(() => useCommands());
    expect(result.current).toEqual([ADD_CHAPTER, ADD_SCENE]);

    act(() => {
      unregisterCommand('outline.add-chapter');
    });
    expect(result.current).toEqual([ADD_SCENE]);
  });

  it('returns a stable snapshot reference between mutations', () => {
    registerCommand(ADD_CHAPTER);
    const { result, rerender } = renderHook(() => useCommands());
    const first = result.current;
    rerender();
    expect(result.current).toBe(first);
  });

  it('unsubscribes on unmount (later mutations do not throw)', () => {
    const { unmount } = renderHook(() => useCommands());
    unmount();
    expect(() => {
      registerCommand(ADD_CHAPTER);
      unregisterCommand('outline.add-chapter');
    }).not.toThrow();
  });
});

describe('useRegisterCommand — lifetime binding', () => {
  beforeEach(() => {
    clearCommands();
  });

  it('registers on mount and unregisters on unmount', () => {
    const { unmount } = renderHook(() => useRegisterCommand(ADD_CHAPTER));
    expect(getCommands()).toEqual([ADD_CHAPTER]);

    unmount();
    expect(getCommands()).toEqual([]);
  });

  it('registers multiple independent commands from separate hooks', () => {
    const a = renderHook(() => useRegisterCommand(ADD_CHAPTER));
    const b = renderHook(() => useRegisterCommand(RUN_STRATEGY));
    expect(getCommands().map((c) => c.id)).toEqual([
      'outline.add-chapter',
      'strategy.run',
    ]);

    a.unmount();
    expect(getCommands().map((c) => c.id)).toEqual(['strategy.run']);

    b.unmount();
    expect(getCommands()).toEqual([]);
  });

  it('survives React StrictMode double-mount (register → cleanup → register)', () => {
    // StrictMode double-invokes effects in dev: mount, unmount, mount. The
    // id-keyed Map makes this a net register.
    const { unmount, rerender } = renderHook(() => useRegisterCommand(ADD_CHAPTER));
    rerender();
    expect(getCommands()).toEqual([ADD_CHAPTER]);
    unmount();
    expect(getCommands()).toEqual([]);
  });

  it('does not churn the registry when re-rendered with an equivalent-id command (captures once on mount)', () => {
    // The caller passes an inline literal each render with the same id but a
    // different handler. The hook must register once (mount-only semantics),
    // not on every render — otherwise the store thrashes.
    const handlerA = vi.fn();
    const handlerB = vi.fn();
    const { rerender } = renderHook(
      ({ handler }) => useRegisterCommand({ ...ADD_CHAPTER, handler }),
      { initialProps: { handler: handlerA } },
    );
    expect(getCommands()).toEqual([{ ...ADD_CHAPTER, handler: handlerA }]);

    rerender({ handler: handlerB });
    // Still the mount-captured handler — field changes after mount are ignored
    // by design (documented simplify note in command-registry.ts).
    expect(getCommands()).toEqual([{ ...ADD_CHAPTER, handler: handlerA }]);
  });
});

describe('filterCommands — query matching', () => {
  const ALL = [ADD_CHAPTER, ADD_SCENE, RUN_STRATEGY].map((c) => resolve(c));

  it('returns all commands in input order for an empty query', () => {
    expect(filterCommands(ALL, '')).toEqual(ALL);
  });

  it('returns all commands in input order for a whitespace-only query', () => {
    expect(filterCommands(ALL, '   ')).toEqual(ALL);
  });

  it('matches case-insensitively against the label', () => {
    expect(filterCommands(ALL, 'add').map((c) => c.id)).toEqual([
      'outline.add-chapter',
      'outline.add-scene',
    ]);
    expect(filterCommands(ALL, 'ADD').map((c) => c.id)).toEqual([
      'outline.add-chapter',
      'outline.add-scene',
    ]);
  });

  it('matches against keywords when the label does not match', () => {
    expect(filterCommands(ALL, 'beat').map((c) => c.id)).toEqual([
      'outline.add-scene',
    ]);
    expect(filterCommands(ALL, 'execute').map((c) => c.id)).toEqual([
      'strategy.run',
    ]);
  });

  it('drops commands that match neither label nor keywords', () => {
    expect(filterCommands(ALL, 'nonexistent')).toEqual([]);
  });

  it('does not apply the available() gate (palette composes that separately)', () => {
    const hidden = resolve(
      makeCommand({
        id: 'x.hidden',
        labelKey: 'Hidden',
        available: () => false,
      }),
    );
    // filterCommands is query-only; available() is the consumer's concern.
    expect(filterCommands([hidden], 'hidden')).toEqual([hidden]);
    expect(filterCommands([hidden], '')).toEqual([hidden]);
  });
});

describe('filterCommands — ranking', () => {
  it('surfaces an exact label match ahead of substring matches', () => {
    const exact = resolve(makeCommand({ id: 'add', labelKey: 'Add' }));
    const chapter = resolve(makeCommand({ id: 'add-chapter', labelKey: 'Add Chapter' }));
    const additive = resolve(makeCommand({ id: 'additive', labelKey: 'Additive' }));
    const out = filterCommands([chapter, exact, additive], 'add');
    // exact label → first, then the two startswith/contains, in input order.
    expect(out[0]).toBe(exact);
    expect(out).toHaveLength(3);
  });

  it('ranks label-startswith ahead of label-contains', () => {
    const starts = resolve(makeCommand({ id: 's', labelKey: 'Add Scene' }));
    const contains = resolve(makeCommand({ id: 'c', labelKey: 'Read Addins' }));
    const out = filterCommands([contains, starts], 'add');
    expect(out.map((c) => c.id)).toEqual(['s', 'c']);
  });

  it('ranks label-contains ahead of keyword-only matches', () => {
    const labelMatch = resolve(makeCommand({ id: 'lm', labelKey: 'Run Header' }));
    const keywordOnly = resolve(
      makeCommand({
        id: 'ko',
        labelKey: 'Strategy',
        keywordKeys: ['overrun'],
      }),
    );
    const out = filterCommands([keywordOnly, labelMatch], 'run');
    expect(out.map((c) => c.id)).toEqual(['lm', 'ko']);
  });

  it('preserves input order within the same rank tier (stable)', () => {
    const a = resolve(makeCommand({ id: 'a', labelKey: 'Go Alpha' }));
    const b = resolve(makeCommand({ id: 'b', labelKey: 'Go Beta' }));
    const c = resolve(makeCommand({ id: 'c', labelKey: 'Go Gamma' }));
    const out = filterCommands([a, b, c], 'go');
    expect(out.map((c2) => c2.id)).toEqual(['a', 'b', 'c']);
  });

  it('full tier ordering: exact → startswith → contains → keyword', () => {
    const exact = resolve(makeCommand({ id: 'ex', labelKey: 'Node' }));
    const starts = resolve(makeCommand({ id: 'st', labelKey: 'Node Start' }));
    const contains = resolve(makeCommand({ id: 'co', labelKey: 'On Node Pad' }));
    const keyword = resolve(
      makeCommand({
        id: 'kw',
        labelKey: 'Graph',
        keywordKeys: ['node-like'],
      }),
    );
    // Deliberately shuffled input to prove ordering is rank-driven, not input.
    const out = filterCommands([keyword, contains, starts, exact], 'node');
    expect(out.map((c) => c.id)).toEqual(['ex', 'st', 'co', 'kw']);
  });

  it('does not mutate the input array', () => {
    const input = [resolve(ADD_CHAPTER), resolve(ADD_SCENE)];
    const snapshot = [...input];
    filterCommands(input, 'add');
    expect(input).toEqual(snapshot);
  });
});
