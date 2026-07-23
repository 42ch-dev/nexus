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

const INLINE_CREATE_LABELS = {
  tabs: { world: 'World', work: 'Work' },
  world: {
    titleLabel: 'World title',
    titlePlaceholder: 'Enter a world name',
    submit: 'Create World',
    disabledTitle: 'Desktop only',
  },
  work: {
    titleLabel: 'Work title',
    titlePlaceholder: 'Enter a work name',
    goalLabel: 'Long-term goal',
    goalPlaceholder: 'What should this work achieve?',
    ideaLabel: 'Initial idea',
    ideaPlaceholder: 'Start from a scene or concept…',
    profileLabel: 'Work profile (optional)',
    profileOptions: [{ value: 'novel', label: 'Novel' }],
    submit: 'Create Work',
  },
} as const;

describe('CreatorShellContent', () => {
  it('renders create-inline with world form and tab bar testids', () => {
    render(
      <CreatorShellContent
        mode="create-inline"
        canCreateWorld={false}
        labels={INLINE_CREATE_LABELS}
        onWorldSubmit={() => {}}
        onWorkSubmit={() => {}}
        data-testid="sidebar-create-panel"
      />,
    );

    const root = screen.getByTestId('sidebar-create-panel');
    expect(root).toHaveAttribute('data-mode', 'create-inline');
    expect(screen.getByTestId('sidebar-create-tab-bar')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-tab-world')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-tab-work')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-form-world')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-submit-world')).toBeDisabled();
    expect(screen.queryByTestId('creator-create-world')).not.toBeInTheDocument();
  });

  it('fires inline submit callbacks from create-inline forms', () => {
    const onWorldSubmit = vi.fn();
    const onWorkSubmit = vi.fn();

    render(
      <CreatorShellContent
        mode="create-inline"
        canCreateWorld
        labels={INLINE_CREATE_LABELS}
        onWorldSubmit={onWorldSubmit}
        onWorkSubmit={onWorkSubmit}
      />,
    );

    fireEvent.change(screen.getByLabelText('World title'), { target: { value: 'Ashen Gate' } });
    fireEvent.click(screen.getByTestId('sidebar-create-submit-world'));
    expect(onWorldSubmit).toHaveBeenCalledWith('Ashen Gate');

    fireEvent.click(screen.getByTestId('sidebar-create-tab-work'));
    fireEvent.change(screen.getByLabelText('Work title'), { target: { value: 'Novel' } });
    fireEvent.change(screen.getByLabelText('Long-term goal'), { target: { value: 'Finish draft' } });
    fireEvent.change(screen.getByLabelText('Initial idea'), { target: { value: 'A long road' } });
    fireEvent.click(screen.getByTestId('sidebar-create-submit-work'));
    expect(onWorkSubmit).toHaveBeenCalledWith({
      title: 'Novel',
      longTermGoal: 'Finish draft',
      initialIdea: 'A long road',
    });
  });

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
