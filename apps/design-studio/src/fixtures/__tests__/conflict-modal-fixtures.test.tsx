/**
 * Conflict-modal Studio fixtures — smoke + boundary tests (V1.124 P2 T3b).
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ConflictModalFixtures } from '@/fixtures/conflict-modal-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../conflict-modal-fixtures.tsx',
);

function mockMatchMedia(prefersDark: boolean) {
  const media = {
    matches: prefersDark,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  vi.spyOn(window, 'matchMedia').mockReturnValue(
    media as unknown as MediaQueryList,
  );
}

beforeEach(() => {
  document.documentElement.classList.remove('dark');
});

afterEach(() => {
  vi.restoreAllMocks();
  document.documentElement.classList.remove('dark');
});

describe('conflict-modal-fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
  });

  it('imports the shared chrome extract with transitional annotation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(
      /@web-canvas\/conflict-modal-chrome.*transitional/,
    );
  });
});

describe('ConflictModalFixtures render', () => {
  it('renders resolve, overlap, and open-preview frames', () => {
    mockMatchMedia(false);
    render(<ConflictModalFixtures />);

    expect(screen.getByTestId('conflict-modal-fixtures')).toBeInTheDocument();
    expect(
      screen.getByTestId('conflict-modal-fixture-resolve'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('conflict-modal-fixture-overlap'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('conflict-modal-fixture-open-preview'),
    ).toBeInTheDocument();
  });

  it('shows open preview dialog with product actions', () => {
    mockMatchMedia(false);
    render(<ConflictModalFixtures />);

    const preview = screen.getByTestId('conflict-modal-fixture-open-preview');
    expect(
      within(preview).getByRole('dialog', {
        name: 'This state changed while you were editing',
      }),
    ).toBeInTheDocument();
    expect(within(preview).getByText('Use current')).toBeInTheDocument();
    expect(within(preview).getByText('Keep editing')).toBeInTheDocument();
    expect(within(preview).getByText('Reapply')).toBeInTheDocument();
    expect(within(preview).getByText('Server version')).toBeInTheDocument();
    expect(within(preview).getByText('Your edit')).toBeInTheDocument();
  });

  it('disables Reapply when server/local fields overlap (open preview)', () => {
    mockMatchMedia(false);
    render(<ConflictModalFixtures />);

    const preview = screen.getByTestId('conflict-modal-fixture-open-preview');
    const reapply = within(preview).getByRole('button', { name: /Reapply/i });
    expect(reapply).toBeDisabled();
  });

  it('opens resolve-path modal on click', () => {
    mockMatchMedia(false);
    render(<ConflictModalFixtures />);

    const resolve = screen.getByTestId('conflict-modal-fixture-resolve');
    fireEvent.click(
      within(resolve).getByRole('button', { name: 'Open conflict modal' }),
    );
    expect(
      within(resolve).getByRole('dialog', {
        name: 'This state changed while you were editing',
      }),
    ).toBeInTheDocument();
    const reapply = within(resolve).getByRole('button', { name: /Reapply/i });
    expect(reapply).not.toBeDisabled();
  });

  it('renders under .dark without throw', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<ConflictModalFixtures />)).not.toThrow();
    expect(screen.getByTestId('conflict-modal-fixtures')).toBeInTheDocument();
  });
});
