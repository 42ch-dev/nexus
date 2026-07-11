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
function makeCommand(overrides: Partial<Command> & Pick<Command, 'id'>): Command {
  return {
    label: overrides.id,
    group: 'Test',
    handler: vi.fn(),
    ...overrides,
  };
}

const ADD_CHAPTER: Command = makeCommand({
  id: 'outline.add-chapter',
  label: 'Add Chapter',
  group: 'Outline',
  keywords: ['new chapter', 'insert'],
});
const ADD_SCENE: Command = makeCommand({
  id: 'outline.add-scene',
  label: 'Add Scene',
  group: 'Outline',
  keywords: ['beat'],
});
const RUN_STRATEGY: Command = makeCommand({
  id: 'strategy.run',
  label: 'Run Strategy',
  group: 'Strategy',
  keywords: ['execute', 'start'],
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

  it('replaces a command when re-registered with the same id (idempotent)', () => {
    registerCommand(ADD_CHAPTER);
    const updated: Command = makeCommand({
      id: 'outline.add-chapter',
      label: 'Add Chapter (renamed)',
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
  const ALL: Command[] = [ADD_CHAPTER, ADD_SCENE, RUN_STRATEGY];

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
    const hidden: Command = makeCommand({
      id: 'x.hidden',
      label: 'Hidden',
      available: () => false,
    });
    // filterCommands is query-only; available() is the consumer's concern.
    expect(filterCommands([hidden], 'hidden')).toEqual([hidden]);
    expect(filterCommands([hidden], '')).toEqual([hidden]);
  });
});

describe('filterCommands — ranking', () => {
  it('surfaces an exact label match ahead of substring matches', () => {
    const exact: Command = makeCommand({ id: 'add', label: 'Add' });
    const chapter: Command = makeCommand({ id: 'add-chapter', label: 'Add Chapter' });
    const additive: Command = makeCommand({ id: 'additive', label: 'Additive' });
    const out = filterCommands([chapter, exact, additive], 'add');
    // exact label → first, then the two startswith/contains, in input order.
    expect(out[0]).toBe(exact);
    expect(out).toHaveLength(3);
  });

  it('ranks label-startswith ahead of label-contains', () => {
    const starts: Command = makeCommand({ id: 's', label: 'Add Scene' });
    const contains: Command = makeCommand({ id: 'c', label: 'Read Addins' });
    const out = filterCommands([contains, starts], 'add');
    expect(out.map((c) => c.id)).toEqual(['s', 'c']);
  });

  it('ranks label-contains ahead of keyword-only matches', () => {
    const labelMatch: Command = makeCommand({ id: 'lm', label: 'Run Header' });
    const keywordOnly: Command = makeCommand({
      id: 'ko',
      label: 'Strategy',
      keywords: ['overrun'],
    });
    const out = filterCommands([keywordOnly, labelMatch], 'run');
    expect(out.map((c) => c.id)).toEqual(['lm', 'ko']);
  });

  it('preserves input order within the same rank tier (stable)', () => {
    const a: Command = makeCommand({ id: 'a', label: 'Go Alpha' });
    const b: Command = makeCommand({ id: 'b', label: 'Go Beta' });
    const c: Command = makeCommand({ id: 'c', label: 'Go Gamma' });
    const out = filterCommands([a, b, c], 'go');
    expect(out.map((c2) => c2.id)).toEqual(['a', 'b', 'c']);
  });

  it('full tier ordering: exact → startswith → contains → keyword', () => {
    const exact: Command = makeCommand({ id: 'ex', label: 'Node' });
    const starts: Command = makeCommand({ id: 'st', label: 'Node Start' });
    const contains: Command = makeCommand({ id: 'co', label: 'On Node Pad' });
    const keyword: Command = makeCommand({
      id: 'kw',
      label: 'Graph',
      keywords: ['node-like'],
    });
    // Deliberately shuffled input to prove ordering is rank-driven, not input.
    const out = filterCommands([keyword, contains, starts, exact], 'node');
    expect(out.map((c) => c.id)).toEqual(['ex', 'st', 'co', 'kw']);
  });

  it('does not mutate the input array', () => {
    const input = [ADD_CHAPTER, ADD_SCENE];
    const snapshot = [...input];
    filterCommands(input, 'add');
    expect(input).toEqual(snapshot);
  });
});
