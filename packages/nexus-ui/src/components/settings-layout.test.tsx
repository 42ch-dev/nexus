import { fireEvent, render, screen } from '@testing-library/react';
import { Globe, Palette, Plug } from 'lucide-react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { SettingsLayout } from './settings-layout';

const SECTIONS = [
  { id: 'appearance', label: 'Appearance', icon: Palette },
  { id: 'connection', label: 'Connection', icon: Plug },
  { id: 'workspace', label: 'Workspace', icon: Globe },
] as const;

/**
 * V1.150 P2 — L2 pattern component (DESIGN.md components.settings-layout).
 *
 * Pins caller-visible behavior: caller-owned section labels + navLabel copy,
 * nav selection reaches the caller via a real click, active section marked
 * with aria-current, hiddenIds filter nav entries, optional page chrome copy
 * appears only when passed, and the rail/outlet token recipe matches the
 * settings frame (light + dark parity).
 */
describe('SettingsLayout', () => {
  it('renders caller section labels, navLabel, and outlet content', () => {
    render(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={() => {}}
        navLabel="Settings sections"
      >
        <div data-testid="outlet">appearance body</div>
      </SettingsLayout>,
    );

    expect(screen.getByRole('navigation', { name: 'Settings sections' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Appearance' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Connection' })).toBeInTheDocument();
    expect(screen.getByTestId('outlet')).toBeInTheDocument();
  });

  it('marks the active section and reports selection via a real click', () => {
    const onNavigate = vi.fn();
    render(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={onNavigate}
        navLabel="Settings sections"
      >
        body
      </SettingsLayout>,
    );

    expect(screen.getByRole('button', { name: 'Appearance' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByRole('button', { name: 'Connection' })).not.toHaveAttribute('aria-current');

    fireEvent.click(screen.getByRole('button', { name: 'Connection' }));
    expect(onNavigate).toHaveBeenCalledTimes(1);
    expect(onNavigate).toHaveBeenCalledWith('connection');
  });

  it('filters nav entries listed in hiddenIds', () => {
    render(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={() => {}}
        hiddenIds={['workspace']}
        navLabel="Settings sections"
      >
        body
      </SettingsLayout>,
    );

    expect(screen.getByRole('button', { name: 'Appearance' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Workspace' })).toBeNull();
  });

  it('renders page chrome copy only when title is passed', () => {
    const { rerender } = render(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={() => {}}
        navLabel="Settings sections"
      >
        body
      </SettingsLayout>,
    );
    expect(screen.queryByRole('heading', { name: 'Settings' })).toBeNull();
    expect(screen.queryByText('Manage workspace preferences.')).toBeNull();

    rerender(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={() => {}}
        navLabel="Settings sections"
        title="Settings"
        helper="Manage workspace preferences."
      >
        body
      </SettingsLayout>,
    );
    expect(screen.getByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByText('Manage workspace preferences.')).toBeInTheDocument();
  });

  it('keeps the frame token recipe (light/dark parity)', () => {
    render(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={() => {}}
        navLabel="Settings sections"
      >
        body
      </SettingsLayout>,
    );

    const nav = screen.getByRole('navigation', { name: 'Settings sections' });
    expect(nav.className).toMatch(/\bborder-gray-alpha-200\b/);
    expect(screen.getByRole('button', { name: 'Appearance' }).className).toMatch(
      /\bbg-gray-alpha-100\b/,
    );
    expect(screen.getByRole('button', { name: 'Connection' }).className).toMatch(
      /\btext-gray-700\b/,
    );
  });

  it('merges a caller className onto the shell', () => {
    const { container } = render(
      <SettingsLayout
        sections={SECTIONS}
        activeId="appearance"
        onNavigate={() => {}}
        navLabel="Settings sections"
        className="settings-page"
      >
        body
      </SettingsLayout>,
    );

    expect(container.firstElementChild!.className).toMatch(/\bsettings-page\b/);
  });
});
