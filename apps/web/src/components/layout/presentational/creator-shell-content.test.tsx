import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { CreatorShellContent } from './creator-shell-content';

const CREATE_LABELS = {
  createWorldTitle: 'Create World',
  createWorldDescription: 'Start a new World.',
  createWorkTitle: 'Create Work',
  createWorkDescription: 'Create a Work to get started.',
  createWorldDisabledTitle: 'Desktop only',
} as const;

const CONTROLLER_LABELS = {
  title: 'Controller Panel',
  description: 'Controller Panel — coming soon',
  selectedSummary: 'Selected Work: Novel',
  back: 'Back',
} as const;

describe('CreatorShellContent', () => {
  it('renders honest Work fallback when createWorld is absent', () => {
    render(
      <CreatorShellContent
        mode="create"
        canCreateWorld={false}
        labels={CREATE_LABELS}
        onCreateWork={() => {}}
      />,
    );

    expect(screen.getByTestId('creator-shell-content')).toHaveAttribute('data-mode', 'create');
    expect(screen.getByTestId('creator-create-world')).toBeDisabled();
    expect(screen.getByTestId('creator-create-work')).not.toBeDisabled();
  });

  it('renders active World + Work CTAs when createWorld is present', () => {
    render(
      <CreatorShellContent
        mode="create"
        canCreateWorld
        labels={CREATE_LABELS}
        onCreateWorld={() => {}}
        onCreateWork={() => {}}
      />,
    );

    expect(screen.getByTestId('creator-create-world')).not.toBeDisabled();
    expect(screen.getByTestId('creator-create-work')).not.toBeDisabled();
  });

  it('fires create callbacks from card CTAs', () => {
    const onCreateWork = vi.fn();
    const onCreateWorld = vi.fn();

    render(
      <CreatorShellContent
        mode="create"
        canCreateWorld
        labels={CREATE_LABELS}
        onCreateWorld={onCreateWorld}
        onCreateWork={onCreateWork}
      />,
    );

    fireEvent.click(screen.getByTestId('creator-create-world'));
    fireEvent.click(screen.getByTestId('creator-create-work'));
    expect(onCreateWorld).toHaveBeenCalledTimes(1);
    expect(onCreateWork).toHaveBeenCalledTimes(1);
  });

  it('renders controller stub with placeholder + Back only', () => {
    const onBack = vi.fn();
    render(
      <CreatorShellContent
        mode="controller"
        selectedEntity={{ kind: 'work', id: 'w1', label: 'Novel' }}
        labels={CONTROLLER_LABELS}
        onBack={onBack}
      />,
    );

    const root = screen.getByTestId('creator-shell-content');
    expect(root).toHaveAttribute('data-mode', 'controller');
    expect(screen.getByText('Controller Panel — coming soon')).toBeInTheDocument();
    expect(screen.getByTestId('creator-controller-selected')).toHaveTextContent(
      'Selected Work: Novel',
    );
    expect(screen.queryByRole('button', { name: /Delete/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('creator-controller-back'));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});
