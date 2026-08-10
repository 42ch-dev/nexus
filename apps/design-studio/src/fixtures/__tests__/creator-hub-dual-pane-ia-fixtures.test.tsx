import { render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it } from 'vitest';

import { CreatorHubDualPaneIaFixtures } from '@/fixtures/creator-hub-dual-pane-ia-fixtures';

function renderFixtures() {
  return render(
    <MemoryRouter>
      <CreatorHubDualPaneIaFixtures />
    </MemoryRouter>,
  );
}

describe('CreatorHubDualPaneIaFixtures', () => {
  it('renders all fixture sections', () => {
    renderFixtures();

    expect(screen.getByTestId('creator-hub-dual-pane-ia-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-ia-fixture-world-tab')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-ia-fixture-work-tab')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-ia-fixture-matrix')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-ia-fixture-interactive')).toBeInTheDocument();
  });

  it('renders light and dark theme pairs for world empty state', () => {
    renderFixtures();

    expect(screen.getByTestId('creator-hub-dual-pane-ia-world-empty-themes-light')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-ia-world-empty-themes-dark')).toBeInTheDocument();
  });

  it('shows sidebar inline create and browse-only content on world empty light frame', () => {
    renderFixtures();

    const frame = screen.getByTestId('creator-hub-dual-pane-ia-world-empty-light');
    const sidebar = within(frame).getByTestId('creator-hub-dual-pane-ia-world-empty-light-sidebar');

    expect(within(sidebar).getByTestId('shell-sidebar-panel')).toBeInTheDocument();
    const createPanel = within(sidebar).getByTestId('sidebar-create-panel');
    expect(createPanel).toHaveAttribute('data-mode', 'create-inline');
    expect(within(createPanel).getByTestId('sidebar-create-tab-bar')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-form-world')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-submit-world')).toBeInTheDocument();
    expect(within(createPanel).queryByTestId('creator-create-world')).not.toBeInTheDocument();

    expect(
      within(frame).queryByTestId(/workspace-pane-inline-form/),
    ).not.toBeInTheDocument();
    expect(
      within(frame).getByTestId('creator-hub-dual-pane-ia-world-empty-light-card-list-pane-empty-i18n-key'),
    ).toHaveTextContent('hub.empty.worlds');
  });

  it('shows card list without content-left create on populated world frame', () => {
    renderFixtures();

    const frame = screen.getByTestId('creator-hub-dual-pane-ia-world-populated-light');
    const createPanel = within(frame).getByTestId('sidebar-create-panel');
    expect(createPanel).toHaveAttribute('data-mode', 'create-inline');
    expect(
      within(frame).queryByTestId(/workspace-pane-inline-form|workspace-pane-compact-create/),
    ).not.toBeInTheDocument();
    expect(
      within(frame).getByTestId(
        'creator-hub-dual-pane-ia-world-populated-light-card-list-pane-world-card-world-fantasy',
      ),
    ).toBeInTheDocument();
  });

  it('renders 8-variant acceptance matrix cells', () => {
    renderFixtures();

    const matrix = screen.getByTestId('creator-hub-dual-pane-ia-variant-matrix');
    expect(within(matrix).getAllByTestId(/creator-hub-dual-pane-ia-matrix-.*-(light|dark)$/)).toHaveLength(8);
  });
});
