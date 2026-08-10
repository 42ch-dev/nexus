/**
 * `CommandPalette` — global shell command palette overlay (⌘K / Ctrl+K).
 *
 * FB-CP-000/002/004, V1.111 P0 T3. Coverage per task brief: a11y role
 * assertions (dialog / combobox / listbox / option), keyboard interactions
 * (Arrow / Enter / Escape), filter wiring, open/close + focus restoration,
 * and the `available()` render-time predicate.
 *
 * V1.112 P1 T2: display fields are translation keys resolved at render time;
 * tests wrap the palette in a LocaleProvider so `useTranslation` resolves.
 */
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearCommands,
  registerCommand,
  type Command,
} from '@/lib/canvas/command-registry';
import { i18n } from '@/lib/i18n/config';
import { LocaleProvider } from '@/components/locale-provider';

import {
  CommandPalette,
  _resetPaletteForTests,
  closePalette,
  openPalette,
} from './command-palette';

/**
 * Render the palette then open it inside `act`. Opening after render (rather
 * than before) keeps the dialog's mount + effects inside a single act scope,
 * avoiding spurious "state update not wrapped in act" warnings from the
 * activeIndex-reset effect that fires on mount.
 */
function renderPalette() {
  const result = render(
    <LocaleProvider>
      <CommandPalette />
    </LocaleProvider>,
  );
  act(() => openPalette());
  return result;
}

/** Factory for a minimal command; overrides only what each test cares about. */
function makeCommand(
  overrides: Partial<Command> & Pick<Command, 'id'>,
): Command {
  return {
    labelKey: overrides.id,
    groupKey: 'Test',
    handler: vi.fn(),
    ...overrides,
  };
}

const ALPHA: Command = makeCommand({
  id: 'test.alpha',
  labelKey: 'Add Chapter',
  groupKey: 'Outline',
  keywordKeys: ['new chapter'],
});
const BETA: Command = makeCommand({
  id: 'test.beta',
  labelKey: 'Add Scene',
  groupKey: 'Outline',
});
const GAMMA: Command = makeCommand({
  id: 'test.gamma',
  labelKey: 'Go to Strategy',
  groupKey: 'Navigate',
});

beforeEach(() => {
  clearCommands();
  _resetPaletteForTests();
  registerCommand(ALPHA);
  registerCommand(BETA);
  registerCommand(GAMMA);
});

afterEach(() => {
  // Unmount before clearing the registry: clearCommands() emits to listeners,
  // which would re-render a still-mounted palette outside an act scope.
  cleanup();
  clearCommands();
  _resetPaletteForTests();
});

describe('CommandPalette — a11y roles', () => {
  it('renders nothing while closed', () => {
    render(
      <LocaleProvider>
        <CommandPalette />
      </LocaleProvider>,
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('exposes a labelled dialog with a combobox, listbox, and options when open', () => {
    renderPalette();

    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('aria-modal', 'true');

    const combobox = screen.getByRole('combobox');
    expect(combobox).toHaveAttribute('aria-autocomplete', 'list');
    expect(combobox).toHaveAttribute('aria-expanded', 'true');
    expect(combobox.getAttribute('aria-controls')).toBeTruthy();

    const listbox = screen.getByRole('listbox');
    expect(listbox.getAttribute('id')).toBe(combobox.getAttribute('aria-controls'));

    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(3);
    expect(options[0]).toHaveTextContent('Add Chapter');
  });

  it('labels the dialog via an sr-only heading', () => {
    renderPalette();
    const dialog = screen.getByRole('dialog');
    const labelledBy = dialog.getAttribute('aria-labelledby');
    expect(labelledBy).toBeTruthy();
    expect(document.getElementById(labelledBy!)).toHaveTextContent('Command palette');
  });
});

describe('CommandPalette — open / close + focus', () => {
  it('focuses the search input on open', () => {
    renderPalette();
    expect(screen.getByRole('combobox')).toHaveFocus();
  });

  it('restores focus to the previously focused element on Escape', () => {
    const trigger = document.createElement('button');
    trigger.textContent = 'Trigger';
    document.body.appendChild(trigger);
    trigger.focus();
    expect(trigger).toHaveFocus();

    renderPalette();

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    trigger.remove();
  });

  it('closes on backdrop click but not on panel click', () => {
    const { container } = renderPalette();
    const dialog = screen.getByRole('dialog');
    const panel = dialog.firstElementChild as HTMLElement;

    // Clicking inside the panel must NOT close (event target !== dialog).
    fireEvent.click(panel);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Clicking the backdrop (the dialog element itself) closes.
    fireEvent.click(dialog);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(container).toBeEmptyDOMElement();
  });

  it('closePalette dismisses an open palette', () => {
    renderPalette();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    act(() => closePalette());
    // Re-render to flush the now-closed store state.
    render(
      <LocaleProvider>
        <CommandPalette />
      </LocaleProvider>,
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('CommandPalette — keyboard navigation', () => {
  it('ArrowDown / ArrowUp move the active option (with wrap-around)', () => {
    renderPalette();
    const input = screen.getByRole('combobox');
    const options = screen.getAllByRole('option');

    // First option is active by default.
    expect(input.getAttribute('aria-activedescendant')).toBe(options[0].id);
    expect(options[0]).toHaveAttribute('aria-selected', 'true');

    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input.getAttribute('aria-activedescendant')).toBe(options[1].id);
    expect(options[1]).toHaveAttribute('aria-selected', 'true');

    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input.getAttribute('aria-activedescendant')).toBe(options[2].id);

    // Wrap from last → first.
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input.getAttribute('aria-activedescendant')).toBe(options[0].id);

    // ArrowUp from first wraps to last.
    fireEvent.keyDown(input, { key: 'ArrowUp' });
    expect(input.getAttribute('aria-activedescendant')).toBe(options[2].id);
  });

  it('Enter invokes the active command handler and closes', () => {
    const handler = vi.fn();
    registerCommand(makeCommand({ id: 'test.alpha', labelKey: 'Add Chapter', handler }));
    renderPalette();
    const input = screen.getByRole('combobox');

    fireEvent.keyDown(input, { key: 'Enter' });
    expect(handler).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('Enter on a non-first option invokes that option', () => {
    const handler = vi.fn();
    registerCommand(
      makeCommand({ id: 'test.beta', labelKey: 'Add Scene', handler }),
    );
    renderPalette();
    const input = screen.getByRole('combobox');

    fireEvent.keyDown(input, { key: 'ArrowDown' }); // → second option
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('clicking an option invokes its handler and closes', async () => {
    const user = userEvent.setup();
    const handler = vi.fn();
    registerCommand(
      makeCommand({ id: 'test.gamma', labelKey: 'Go to Strategy', handler }),
    );
    renderPalette();

    await user.click(screen.getByRole('option', { name: /go to strategy/i }));
    expect(handler).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('Enter does nothing when there are no matches', () => {
    renderPalette();
    const input = screen.getByRole('combobox');

    fireEvent.change(input, { target: { value: 'zzz-no-match' } });
    // Combobox reports the popup collapsed when there are no options.
    expect(input).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText(/no matching commands/i)).toBeInTheDocument();
  });
});

describe('CommandPalette — filter wiring', () => {
  it('shows all commands on an empty query', () => {
    renderPalette();
    expect(screen.getAllByRole('option')).toHaveLength(3);
  });

  it('narrows options as the user types (label substring, case-insensitive)', () => {
    renderPalette();
    const input = screen.getByRole('combobox');

    fireEvent.change(input, { target: { value: 'add' } });
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      'Add Chapter',
      'Add Scene',
    ]);

    fireEvent.change(input, { target: { value: 'chapter' } });
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      'Add Chapter',
    ]);
  });

  it('matches keywords too', () => {
    renderPalette();
    const input = screen.getByRole('combobox');
    // ALPHA has keyword 'new chapter'.
    fireEvent.change(input, { target: { value: 'new chapter' } });
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      'Add Chapter',
    ]);
  });

  it('clearing the query restores the full list and resets the active option', () => {
    renderPalette();
    const input = screen.getByRole('combobox');

    fireEvent.change(input, { target: { value: 'strategy' } });
    const one = screen.getAllByRole('option');
    expect(one).toHaveLength(1);

    fireEvent.change(input, { target: { value: '' } });
    expect(screen.getAllByRole('option')).toHaveLength(3);
    expect(input.getAttribute('aria-activedescendant')).toBe(
      screen.getAllByRole('option')[0].id,
    );
  });
});

describe('CommandPalette — available() predicate', () => {
  it('hides commands whose available() returns false', () => {
    clearCommands();
    registerCommand(
      makeCommand({ id: 'test.off', labelKey: 'Disabled', available: () => false }),
    );
    registerCommand(
      makeCommand({ id: 'test.on', labelKey: 'Enabled', available: () => true }),
    );
    renderPalette();

    expect(screen.queryByRole('option', { name: /disabled/i })).not.toBeInTheDocument();
    expect(screen.getByRole('option', { name: /enabled/i })).toBeInTheDocument();
  });

  it('clamps the active index when availability shrinks the list', () => {
    clearCommands();
    let off = false;
    registerCommand(
      makeCommand({ id: 'test.off', labelKey: 'Disabled', available: () => !off }),
    );
    registerCommand(makeCommand({ id: 'test.on', labelKey: 'Enabled' }));

    const { rerender } = renderPalette();
    const input = screen.getByRole('combobox');
    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(2);

    // Move active to the second option, then make it unavailable.
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    off = true;
    rerender(
      <LocaleProvider>
        <CommandPalette />
      </LocaleProvider>,
    );

    const remaining = screen.getAllByRole('option');
    expect(remaining).toHaveLength(1);
    expect(remaining[0]).toHaveAttribute('aria-selected', 'true');
  });
});

describe('CommandPalette — locale switching', () => {
  it('updates command labels without remounting when the locale changes', () => {
    // Use real keys from the commands catalog so switching to zh-CN changes
    // the rendered text. The palette stays open and is not remounted.
    clearCommands();
    registerCommand({
      id: 'test.locale',
      labelKey: 'go.strategy.label',
      groupKey: 'group.navigate',
      handler: vi.fn(),
    });

    renderPalette();
    expect(screen.getByRole('option')).toHaveTextContent('Go to Harness');

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    // Palette is still open, option text is now Chinese, component was not remounted.
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('option')).toHaveTextContent('前往 Harness');

    // Restore English for subsequent tests.
    act(() => {
      i18n.changeLanguage('en');
    });
  });
});

describe('CommandPalette — v0.4 elevation + state recipe (V1.121 P2 T1)', () => {
  it('panel floats at shadow-elevation-4 on the scrim overlay', () => {
    renderPalette();

    const dialog = screen.getByRole('dialog');
    // Overlay dims via the scrim token (no text on scrim — §Elevation scrim rule).
    expect(dialog).toHaveClass('bg-scrim');
    // The opaque panel above the scrim carries the modal-class elevation.
    const panel = dialog.firstElementChild as HTMLElement;
    expect(panel).toHaveClass('shadow-elevation-4');
    expect(panel).toHaveClass('bg-background-100');
  });

  it('option state transitions are token-driven (duration-state) and reduced-motion safe', () => {
    renderPalette();

    const option = screen.getAllByRole('option')[0];
    expect(option.className).toMatch(/\bduration-state\b/);
    expect(option.className).toMatch(/\bease-standard\b/);
    expect(option.className).toMatch(/\bmotion-reduce:transition-none\b/);
    // Selected state = gray-alpha-100 wash (aria-selected preserved, V1.111 untouched).
    expect(option).toHaveAttribute('aria-selected', 'true');
    expect(option).toHaveClass('bg-gray-alpha-100');
  });
});
