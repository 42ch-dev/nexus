import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { ListStateContainer } from './list-state-container';

/**
 * V1.149 P1 — L2 pattern component (DESIGN.md components.list-state-container).
 *
 * Pins: the four-branch status switch (loading/error/empty/ready), retry
 * callback wiring into the promoted ErrorState, caller-owned empty copy vs
 * fallback copy, and composition of the promoted States primitives (theme
 * parity is inherited from Spinner/ErrorState token recipes + dark hooks).
 */
describe('ListStateContainer', () => {
  it('ready branch renders caller children and no state block', () => {
    render(
      <ListStateContainer status="ready">
        <ul aria-label="agents">
          <li>codex</li>
        </ul>
      </ListStateContainer>,
    );

    expect(screen.getByRole('list', { name: 'agents' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByText('Loading…')).not.toBeInTheDocument();
  });

  it('loading branch renders the promoted LoadingState with caller copy', () => {
    const { container } = render(
      <ListStateContainer status="loading" loadingLabel="Scanning for agents…">
        <ul />
      </ListStateContainer>,
    );

    expect(screen.getByText('Scanning for agents…')).toBeInTheDocument();
    const spinner = container.querySelector('svg');
    expect(spinner).not.toBeNull();
    // Theme parity inherited from the promoted Spinner recipe.
    expect(spinner!.getAttribute('class')).toMatch(/\btext-blue-1000\b/);
    expect(spinner!.getAttribute('class')).toMatch(/\bdark:text-blue-700\b/);
  });

  it('error branch renders the promoted ErrorState and wires retry', () => {
    const retry = vi.fn();
    render(
      <ListStateContainer status="error" retry={retry} errorTitle="Could not load agents">
        <ul />
      </ListStateContainer>,
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Could not load agents');
    expect(alert.className).toMatch(/\bbg-error-surface\b/);
    expect(alert.className).toMatch(/\bborder-error-surface-border\b/);

    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it('error branch hides retry when the caller passes none', () => {
    render(
      <ListStateContainer status="error">
        <ul />
      </ListStateContainer>,
    );

    expect(screen.getByRole('alert')).toHaveTextContent('Could not load this view');
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument();
  });

  it('empty branch renders caller-owned title, hint, and action', () => {
    render(
      <ListStateContainer
        status="empty"
        emptyTitle="No agents yet"
        emptyHint="Install an agent to start the local loop."
        emptyAction={<button>Create agent</button>}
      >
        <ul />
      </ListStateContainer>,
    );

    expect(screen.getByText('No agents yet')).toBeInTheDocument();
    expect(screen.getByText('Install an agent to start the local loop.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create agent' })).toBeInTheDocument();
    // Empty-state content voice inherited from the promoted EmptyState.
    expect(screen.getByText('No agents yet').className).toMatch(/\bfont-display\b/);
  });

  it('empty branch falls back to English copy when the caller omits it', () => {
    render(
      <ListStateContainer status="empty">
        <ul />
      </ListStateContainer>,
    );

    expect(screen.getByText('Nothing here yet')).toBeInTheDocument();
  });

  it('empty vs no-selection context is distinguished by caller copy', () => {
    const { unmount } = render(
      <ListStateContainer status="empty" emptyTitle="No agents yet">
        <ul />
      </ListStateContainer>,
    );
    expect(screen.getByText('No agents yet')).toBeInTheDocument();
    unmount();

    render(
      <ListStateContainer status="empty" emptyTitle="Select a work to see its chapters">
        <ul />
      </ListStateContainer>,
    );
    expect(screen.getByText('Select a work to see its chapters')).toBeInTheDocument();
  });

  it('merges an optional className onto the container', () => {
    const { container } = render(
      <ListStateContainer status="ready" className="rounded-card border border-gray-alpha-400">
        <ul />
      </ListStateContainer>,
    );

    expect((container.firstChild as HTMLElement).className).toMatch(/\brounded-card\b/);
  });
});
