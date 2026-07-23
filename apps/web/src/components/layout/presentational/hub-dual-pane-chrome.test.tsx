import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { HubCardListPane } from './hub-card-list-pane';
import { HubDualPaneChrome } from './hub-dual-pane-chrome';
import { HubTabBar } from './hub-tab-bar';
import { HubWorkspacePane } from './hub-workspace-pane';

const LABELS = {
  tabs: { world: 'Worlds', work: 'Works' },
  workspace: {
    createWorldTitle: 'Create World',
    createWorldDescription: 'Start a new World.',
    createWorkTitle: 'Create Work',
    createWorkDescription: 'Start a new Work.',
    createWorldCompact: 'Create new World…',
    createWorkCompact: 'Create new Work…',
    titleLabel: 'Title',
    titlePlaceholder: 'Enter title',
    submitLabel: 'Create',
  },
  cardList: {
    emptyWorlds: 'No Worlds yet — create one from the left',
    emptyWorks: 'No Works yet — create one from the left',
    emptyWorldsKey: 'hub.empty.worlds',
    emptyWorksKey: 'hub.empty.works',
  },
} as const;

describe('HubTabBar', () => {
  it('switches tabs via click', () => {
    const onTabChange = vi.fn();

    render(
      <HubTabBar
        activeTab="world"
        onTabChange={onTabChange}
        labels={LABELS.tabs}
        data-testid="hub-tabs"
      />,
    );

    fireEvent.click(screen.getByTestId('hub-tabs-work'));
    expect(onTabChange).toHaveBeenCalledWith('work');
  });
});

describe('HubWorkspacePane', () => {
  it('renders expanded inline create when createExpanded is true', () => {
    render(
      <HubWorkspacePane
        activeTab="world"
        labels={LABELS.workspace}
        createExpanded
        data-testid="workspace"
      />,
    );

    expect(screen.getByTestId('workspace-inline-form')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-title-input')).toBeInTheDocument();
  });

  it('renders compact create affordance when createExpanded is false', () => {
    render(
      <HubWorkspacePane
        activeTab="work"
        labels={LABELS.workspace}
        createExpanded={false}
        data-testid="workspace"
      />,
    );

    expect(screen.getByTestId('workspace-compact-create')).toHaveTextContent('Create new Work…');
  });
});

describe('HubCardListPane', () => {
  it('shows empty state with i18n key for active tab', () => {
    render(
      <HubCardListPane
        activeTab="world"
        worlds={[]}
        works={[{ id: 'w1', label: 'Novel' }]}
        labels={LABELS.cardList}
        data-testid="cards"
      />,
    );

    expect(screen.getByTestId('cards-empty')).toBeInTheDocument();
    expect(screen.getByTestId('cards-empty-i18n-key')).toHaveTextContent('hub.empty.worlds');
  });

  it('renders cards for the active tab only', () => {
    render(
      <HubCardListPane
        activeTab="work"
        worlds={[{ id: 'world-1', label: 'Fantasy' }]}
        works={[{ id: 'work-1', label: 'Novel' }]}
        labels={LABELS.cardList}
        data-testid="cards"
      />,
    );

    expect(screen.getByTestId('cards-work-card-work-1')).toBeInTheDocument();
    expect(screen.queryByTestId('cards-world-card-world-1')).not.toBeInTheDocument();
  });
});

describe('HubDualPaneChrome', () => {
  it('links tab bar to both panes', () => {
    const onTabChange = vi.fn();

    render(
      <HubDualPaneChrome
        activeTab="world"
        onTabChange={onTabChange}
        worlds={[]}
        works={[]}
        labels={LABELS}
        data-testid="hub"
      />,
    );

    expect(screen.getByTestId('hub-workspace-pane-inline-form')).toBeInTheDocument();
    expect(screen.getByTestId('hub-card-list-pane-empty')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('hub-tab-bar-work'));
    expect(onTabChange).toHaveBeenCalledWith('work');
  });
});
