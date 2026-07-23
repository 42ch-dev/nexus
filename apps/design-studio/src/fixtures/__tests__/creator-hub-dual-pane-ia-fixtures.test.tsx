import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CreatorHubDualPaneIaFixtures } from '@/fixtures/creator-hub-dual-pane-ia-fixtures';

function renderFixtures() {
  return render(<CreatorHubDualPaneIaFixtures />);
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

  it('shows inline create and empty-state copy on world empty light frame', () => {
    renderFixtures();

    const frame = screen.getByTestId('creator-hub-dual-pane-ia-world-empty-light');
    expect(
      within(frame).getByTestId('creator-hub-dual-pane-ia-world-empty-light-workspace-pane-inline-form'),
    ).toBeInTheDocument();
    expect(
      within(frame).getByTestId('creator-hub-dual-pane-ia-world-empty-light-card-list-pane-empty-i18n-key'),
    ).toHaveTextContent('hub.empty.worlds');
  });

  it('shows compact create on populated world frame', () => {
    renderFixtures();

    const frame = screen.getByTestId('creator-hub-dual-pane-ia-world-populated-light');
    expect(
      within(frame).getByTestId('creator-hub-dual-pane-ia-world-populated-light-workspace-pane-compact-create'),
    ).toBeInTheDocument();
    expect(
      within(frame).getByTestId('creator-hub-dual-pane-ia-world-populated-light-card-list-pane-world-card-world-fantasy'),
    ).toBeInTheDocument();
  });

  it('renders 8-variant acceptance matrix cells', () => {
    renderFixtures();

    const matrix = screen.getByTestId('creator-hub-dual-pane-ia-variant-matrix');
    expect(within(matrix).getAllByTestId(/creator-hub-dual-pane-ia-matrix-.*-(light|dark)$/)).toHaveLength(8);
  });
});
