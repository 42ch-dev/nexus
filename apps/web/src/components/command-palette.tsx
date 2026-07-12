/**
 * `CommandPalette` — the global shell command palette overlay (⌘K / Ctrl+K).
 *
 * FB-CP-000/002/004, V1.111 P0 T3. Mounted once in `RootLayout`; opens via
 * {@link openPalette} (wired to `useHotkey('mod+k', …)` in `root-layout.tsx`)
 * and closes on Escape / option activation / backdrop click.
 *
 * Open-state ownership (architect lock plan `## Architecture locks` item 4):
 * the palette owns its open/close via a tiny module-level store — same pattern
 * as `lib/canvas/command-registry.ts` (module store + `useSyncExternalStore`).
 * This keeps `RootLayout` free of palette state: the hotkey just calls
 * `openPalette()`, and `<CommandPalette/>` reads `useCommandPalette()` to
 * decide whether to render. A provider/context was rejected for the registry
 * for the same reason (re-renders the whole tree); the palette open-flag is
 * even simpler and benefits from the same decoupling.
 *
 * Contents come from the canvas action registry: `useCommands()` snapshot
 * filtered by the `available()` predicate at render (T1 contract — the store
 * cannot track dynamic availability), then ranked by `filterCommands`.
 *
 * a11y (HARD, WAI-ARIA combobox + listbox pattern):
 *   - outer `role="dialog"` `aria-modal="true"` labelled by the heading;
 *   - the input is the combobox (`role="combobox"`, `aria-autocomplete="list"`,
 *     `aria-expanded`, `aria-controls`, `aria-activedescendant`) — focus stays
 *     on the input while the active option is virtualized through
 *     `aria-activedescendant`;
 *   - the list is `role="listbox"` with `role="option"` children;
 *   - Escape closes; focus is restored to the element that had focus on open.
 *
 * simplify: the listbox is flat — the registry's `group` field is not rendered
 * as ARIA group headings this iteration (flat ranked order from
 * `filterCommands` is what T3 specifies; grouped listbox markup is a follow-up
 * if authors ask for it). Upgrade path: wrap options in nested
 * `role="group"[aria-label]` per `group`, preserving rank within each group.
 */
import { useEffect, useId, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { filterCommands, useCommands, type Command } from '@/lib/canvas/command-registry';
import { cn } from '@/lib/utils';

// --- open-state store (module-level, mirrors command-registry pattern) ----

let paletteOpen = false;
const paletteListeners = new Set<() => void>();

function emitPalette(): void {
  for (const listener of paletteListeners) listener();
}
function subscribePalette(listener: () => void): () => void {
  paletteListeners.add(listener);
  return () => {
    paletteListeners.delete(listener);
  };
}
function getPaletteOpen(): boolean {
  return paletteOpen;
}

/** Open the command palette (called from the ⌘K hotkey in `RootLayout`). */
export function openPalette(): void {
  if (paletteOpen) return;
  paletteOpen = true;
  emitPalette();
}

/** Close the command palette (Escape / activation / backdrop click). */
export function closePalette(): void {
  if (!paletteOpen) return;
  paletteOpen = false;
  emitPalette();
}

/** Subscribe to palette open state. Re-renders on open/close only. */
export function useCommandPalette(): boolean {
  return useSyncExternalStore(subscribePalette, getPaletteOpen, getPaletteOpen);
}

/** Test-only: force the store back to closed (isolates test cases). */
export function _resetPaletteForTests(): void {
  paletteOpen = false;
  paletteListeners.clear();
}

// --- component ------------------------------------------------------------

const NO_RESULTS_ID = 'command-palette-no-results';

/**
 * The palette overlay. Render once in `RootLayout`; renders nothing while
 * closed. Delegates the open dialog to {@link CommandPaletteDialog} so the
 * dialog's mount/unmount lifecycle cleanly drives focus capture + restoration.
 */
export function CommandPalette(): React.ReactElement | null {
  const open = useCommandPalette();
  return open ? <CommandPaletteDialog /> : null;
}

/** Command with display strings resolved from translation keys. */
type ResolvedCommand = Command & {
  readonly label: string;
  readonly group: string;
  readonly keywords: string[];
};

function CommandPaletteDialog(): React.ReactElement {
  const { t } = useTranslation('commands');
  const commands = useCommands();
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);

  const inputRef = useRef<HTMLInputElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  const titleId = useId();
  const listboxId = useId();
  const optionBaseId = useId();

  // Resolve translation keys at render time so labels update instantly on
  // locale switches without re-registering commands. The registry stays stable
  // (keyed by id); useTranslation subscribes this component to i18n changes.
  const resolvedCommands = useMemo<ResolvedCommand[]>(
    () =>
      commands.map((c) => ({
        ...c,
        label: t(c.labelKey, { ns: c.labelNs ?? 'commands' }),
        group: t(c.groupKey, { ns: c.groupNs ?? 'commands' }),
        keywords:
          c.keywordKeys?.map((k) => t(k, { ns: c.keywordNs ?? 'commands' })) ??
          [],
      })),
    [commands, t],
  );

  // Available() is a render-time gate (T1 contract) — evaluated on EVERY render
  // (NOT memoized) so a predicate that flips between renders is honoured. The
  // registry is small (canvas-action-scoped this iteration), so an un-memoized
  // scan is cheap; ranking is also linear in the visible set.
  const availableCommands = resolvedCommands.filter(
    (c) => c.available?.() ?? true,
  );
  const visible = filterCommands(availableCommands, query);

  // Clamp active index into range whenever the filtered set shrinks/grows so
  // `aria-activedescendant` never points at a stale or out-of-range option.
  const safeActiveIndex = visible.length === 0 ? 0 : Math.min(activeIndex, visible.length - 1);
  const activeOptionId =
    visible.length > 0 ? `${optionBaseId}-${safeActiveIndex}` : undefined;

  // Open lifecycle: capture prior focus, focus the input, bind Escape + Tab.
  // Cleanup (runs on unmount when the palette closes) restores focus.
  useEffect(() => {
    previousFocus.current = document.activeElement as HTMLElement | null;
    inputRef.current?.focus();

    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closePalette();
        return;
      }
      // Focus trap: the input is the only real tab stop (options are driven
      // via aria-activedescendant, not DOM focus). Keep Tab inside the dialog.
      if (event.key === 'Tab') {
        event.preventDefault();
        inputRef.current?.focus();
      }
    }

    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      previousFocus.current?.focus();
    };
  }, []);

  function onQueryChange(event: React.ChangeEvent<HTMLInputElement>): void {
    // Reset selection to the top on every keystroke so the first match is the
    // implicit "Enter" target (matches the combobox APG default). Colocating
    // the reset with its cause avoids a mount-time effect that fires a
    // redundant setState outside React's initial act scope in tests.
    setQuery(event.target.value);
    setActiveIndex(0);
  }

  function moveActive(step: number): void {
    if (visible.length === 0) return;
    setActiveIndex((current) => {
      const next = (current + step + visible.length) % visible.length;
      return next < 0 ? next + visible.length : next;
    });
  }

  function invoke(command: ResolvedCommand): void {
    closePalette();
    command.handler();
  }

  function onInputKeyDown(event: ReactKeyboardEvent<HTMLInputElement>): void {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        moveActive(1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        moveActive(-1);
        break;
      case 'Enter': {
        event.preventDefault();
        const target = visible[safeActiveIndex];
        if (target) invoke(target);
        break;
      }
      default:
        break;
    }
  }

  function onBackdropClick(event: React.MouseEvent<HTMLDivElement>): void {
    if (event.target === event.currentTarget) closePalette();
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-scrim p-4 pt-[12vh]"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      onClick={onBackdropClick}
    >
      <div className="flex w-full max-w-dialog flex-col overflow-hidden rounded-popover border border-gray-alpha-400 bg-background-100 shadow-popover">
        <h2 id={titleId} className="sr-only">
          {t('palette.title')}
        </h2>
        {/* Combobox: APG "editable combobox with listbox popup". The input owns
            focus and carries the combobox role; the active option is virtualized
            via aria-activedescendant (options never receive DOM focus). */}
        <input
          ref={inputRef}
          type="text"
          role="combobox"
          aria-haspopup="listbox"
          value={query}
          onChange={onQueryChange}
          onKeyDown={onInputKeyDown}
          aria-autocomplete="list"
          aria-expanded={visible.length > 0 ? 'true' : 'false'}
          aria-controls={listboxId}
          aria-activedescendant={activeOptionId}
          aria-label={t('palette.search.label')}
          placeholder={t('palette.search.placeholder')}
          className="w-full border-b border-gray-alpha-400 bg-transparent px-4 py-3 text-copy-14 text-gray-1000 outline-none placeholder:text-gray-700"
        />

        {visible.length === 0 ? (
          <p id={NO_RESULTS_ID} className="px-4 py-6 text-center text-copy-14 text-gray-900">
            {t('palette.noResults')}
          </p>
        ) : (
          <ul
            id={listboxId}
            role="listbox"
            aria-label={t('palette.listbox.label')}
            className="max-h-[320px] overflow-y-auto p-1"
          >
            {visible.map((command, index) => (
              <CommandOption
                key={command.id}
                id={`${optionBaseId}-${index}`}
                command={command}
                selected={index === safeActiveIndex}
                onSelect={() => invoke(command)}
                onMouseEnter={() => setActiveIndex(index)}
              />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function CommandOption({
  id,
  command,
  selected,
  onSelect,
  onMouseEnter,
}: {
  id: string;
  command: ResolvedCommand;
  selected: boolean;
  onSelect: () => void;
  onMouseEnter: () => void;
}): React.ReactElement {
  const Icon = command.icon;
  return (
    <li
      id={id}
      role="option"
      aria-selected={selected}
      className={cn(
        'flex h-9 cursor-pointer items-center gap-2 rounded-control px-3 text-copy-14 text-gray-1000',
        selected ? 'bg-gray-alpha-100' : '',
      )}
      onClick={onSelect}
      onMouseEnter={onMouseEnter}
    >
      {Icon ? (
        <Icon className="h-4 w-4 shrink-0 text-gray-900" aria-hidden />
      ) : null}
      <span className="truncate">{command.label}</span>
    </li>
  );
}
