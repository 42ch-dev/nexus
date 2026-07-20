import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { CreatorShellFixtures } from '@/fixtures/creator-shell-fixtures';

function renderFixtures() {
  return render(
    <MemoryRouter>
      <CreatorShellFixtures />
    </MemoryRouter>,
  );
}

describe('CreatorShellFixtures', () => {
  it('renders interactive toggle and static fixture frames', () => {
    renderFixtures();

    expect(screen.getByTestId('creator-shell-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('creator-shell-fixture-interactive')).toBeInTheDocument();
    expect(screen.getByTestId('creator-shell-fixture-create-fallback')).toBeInTheDocument();
    expect(screen.getByTestId('creator-shell-fixture-create-world')).toBeInTheDocument();
    expect(screen.getByTestId('creator-shell-fixture-controller-world')).toBeInTheDocument();
    expect(screen.getByTestId('creator-shell-fixture-controller-work')).toBeInTheDocument();
  });

  it('shows Worlds-first nav groups in shell frames', () => {
    renderFixtures();
    const frames = screen.getAllByTestId('creator-shell-frame');
    expect(frames.length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Worlds').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('All Works').length).toBeGreaterThanOrEqual(1);
  });

  it('toggles interactive fixture between Create and Controller modes', () => {
    renderFixtures();
    const interactive = screen.getByTestId('creator-shell-fixture-interactive');
    const content = () => within(interactive).getByTestId('creator-shell-interactive-content');

    expect(content()).toHaveAttribute('data-mode', 'create');
    expect(within(content()).getByTestId('creator-create-work')).toBeInTheDocument();

    fireEvent.click(within(interactive).getByTestId('creator-shell-toggle-world'));
    expect(content()).toHaveAttribute('data-mode', 'controller');
    expect(within(content()).getByTestId('creator-controller-back')).toBeInTheDocument();

    fireEvent.click(within(content()).getByTestId('creator-controller-back'));
    expect(content()).toHaveAttribute('data-mode', 'create');
  });

  it('static create-fallback frame disables World CTA and enables Work CTA', () => {
    renderFixtures();
    const frame = screen.getByTestId('creator-shell-fixture-create-fallback');
    const content = within(frame).getByTestId('creator-shell-create-fallback');
    expect(within(content).getByTestId('creator-create-world')).toBeDisabled();
    expect(within(content).getByTestId('creator-create-work')).not.toBeDisabled();
  });

  it('controller frames show placeholder without business widgets', () => {
    renderFixtures();
    const worldFrame = screen.getByTestId('creator-shell-fixture-controller-world');
    const worldContent = within(worldFrame).getByTestId('creator-shell-controller-world');
    expect(within(worldContent).getByText('Controller Panel — coming soon')).toBeInTheDocument();
    expect(within(worldContent).queryByRole('button', { name: /Delete/i })).not.toBeInTheDocument();
  });
});
