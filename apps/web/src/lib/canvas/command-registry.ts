/**
 * Canvas command registry — extensible action store backing the shared command
 * palette (FB-CP-001, FB-CP-003, V1.111 P0 T1).
 *
 * Surfaces register commands via `useRegisterCommand(action)` in an effect; the
 * palette subscribes via `useCommands()` and filters with `filterCommands`.
 * The store is module-level + `useSyncExternalStore` so a register in one
 * subtree does NOT re-render the whole tree (a provider/context was rejected
 * for exactly this reason — see plan `## Architecture locks` item 1).
 *
 * Extensibility contract (FB-CP-003): P1/P2/future surfaces add commands
 * without editing the palette component — they just call `useRegisterCommand`.
 *
 * Scope note: lives under `lib/canvas/` because the actions registered this
 * iteration are canvas-action-scoped. If non-canvas actions are added later,
 * promote to `apps/web/src/lib/command-registry.ts` (architect lock #1).
 */
import { useEffect, useSyncExternalStore } from 'react';
import type { LucideIcon } from 'lucide-react';

/** A single palette action. All fields are read once at registration time. */
export interface Command {
  /** Stable, unique id, namespaced by surface (e.g. `outline.add-chapter`). */
  readonly id: string;
  /** Display label (Title Case per DESIGN.md §Voice & Content). */
  readonly label: string;
  /** Logical grouping shown as a heading in the palette (e.g. `Outline`). */
  readonly group: string;
  /** Extra search terms; matched after the label. */
  readonly keywords?: readonly string[];
  /** Optional leading icon. */
  readonly icon?: LucideIcon;
  /** Fired when the user activates the command (Enter/click). */
  readonly handler: () => void;
  /**
   * Dynamic availability gate, evaluated by the palette on each render (NOT
   * tracked by the store — `available()` is render-time dynamic and the store
   * only emits on register/unregister). Commands whose `available` returns
   * `false` are hidden regardless of the query. Omit to always show.
   */
  readonly available?: () => boolean;
}

// --- module-level store ---------------------------------------------------

// simplify: id-keyed Map (last-write-wins; unregister is id-keyed). Correct for
// namespaced ids where no two surfaces register the same id (the palette use
// case). Upgrade path if id collisions ever become real: ref-count
// registrations so overlapping mounts do not prematurely evict each other.
const commandMap = new Map<string, Command>();
const listeners = new Set<() => void>();

// Cached snapshot — rebuilt only on mutation so `useSyncExternalStore` sees a
// stable reference between changes (returning a fresh array each call would
// trigger an infinite re-render loop).
let commandSnapshot: readonly Command[] = [];

function rebuildSnapshot(): void {
  commandSnapshot = Array.from(commandMap.values());
}

function emit(): void {
  for (const listener of listeners) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): readonly Command[] {
  return commandSnapshot;
}

/**
 * Register (or replace) a command. Idempotent on `id` — re-registering the
 * same id overwrites the prior entry, which makes the store safe under React
 * StrictMode double-invoked effects.
 */
export function registerCommand(command: Command): void {
  commandMap.set(command.id, command);
  rebuildSnapshot();
  emit();
}

/** Remove a command by id. No-op (no emit) if the id is not registered. */
export function unregisterCommand(id: string): void {
  if (commandMap.delete(id)) {
    rebuildSnapshot();
    emit();
  }
}

/** Read the current registered commands (snapshot). For non-React consumers/tests. */
export function getCommands(): readonly Command[] {
  return commandSnapshot;
}

/** Drop all commands. Used by tests to isolate cases; also a hard-reset hook. */
export function clearCommands(): void {
  if (commandMap.size === 0) {
    return;
  }
  commandMap.clear();
  rebuildSnapshot();
  emit();
}

// --- React bindings -------------------------------------------------------

/**
 * Subscribe to the command store. Re-renders the caller on register/unregister
 * only; returns a stable array reference between mutations.
 */
export function useCommands(): readonly Command[] {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/**
 * Register a command for the lifetime of the calling component. Registers on
 * mount, unregisters on unmount. Re-registering the same `id` overwrites
 * (idempotent — safe under React StrictMode double-mount).
 *
 * The command is captured once on mount (keyed by `id`); changing other fields
 * after mount has no effect until the component re-mounts. This is correct for
 * surface commands whose handler is stable for the mount lifetime (the palette
 * use case). simplify: if a surface ever needs a live-updating handler/label,
 * keep the latest command in a ref and wrap `handler`/`available` to read
 * through it, rather than churning the registry on every render.
 */
export function useRegisterCommand(command: Command): void {
  useEffect(() => {
    registerCommand(command);
    return () => unregisterCommand(command.id);
    // Mount/unmount only, keyed by id. Field changes after mount are ignored
    // by design (see docblock); depending on the whole `command` object would
    // re-register on every render if the caller passes an inline literal.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [command.id]);
}

// --- query ----------------------------------------------------------------

/** Relevance tiers for `filterCommands` (lower tier surfaces first). */
const RANK_EXACT_LABEL = 0;
const RANK_LABEL_STARTS_WITH = 1;
const RANK_LABEL_CONTAINS = 2;
const RANK_KEYWORD_CONTAINS = 3;

/**
 * Filter and rank commands by a query. Case-insensitive substring match across
 * `label` and `keywords`. Rank order (lower = surfaces first):
 *   0. exact label match (case-insensitive)
 *   1. label starts with query
 *   2. label contains query
 *   3. any keyword contains query
 * Within a tier, input order is preserved (stable). Non-matching commands are
 * dropped.
 *
 * Empty/whitespace query → all commands in input order (the palette's
 * "just opened" state — no filtering, no ranking).
 *
 * `available()` is NOT applied here; it is a render-time gate the palette
 * composes (`commands.filter(c => c.available?.() ?? true)`) since the store
 * cannot know when a dynamic predicate flips.
 */
export function filterCommands(
  commands: readonly Command[],
  query: string,
): Command[] {
  const q = query.trim().toLowerCase();
  if (q === '') {
    return [...commands];
  }

  const tiers: Command[][] = [[], [], [], []];
  for (const command of commands) {
    const label = command.label.toLowerCase();
    let tier: number;
    if (label === q) {
      tier = RANK_EXACT_LABEL;
    } else if (label.startsWith(q)) {
      tier = RANK_LABEL_STARTS_WITH;
    } else if (label.includes(q)) {
      tier = RANK_LABEL_CONTAINS;
    } else if (command.keywords?.some((k) => k.toLowerCase().includes(q))) {
      tier = RANK_KEYWORD_CONTAINS;
    } else {
      continue;
    }
    tiers[tier].push(command);
  }

  return [...tiers[RANK_EXACT_LABEL], ...tiers[RANK_LABEL_STARTS_WITH], ...tiers[RANK_LABEL_CONTAINS], ...tiers[RANK_KEYWORD_CONTAINS]];
}
