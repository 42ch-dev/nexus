import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { CreatorOrchGongnengquIaFixtures } from '@/fixtures/creator-orch-gongnengqu-ia-fixtures';

function renderFixtures() {
  return render(
    <MemoryRouter>
      <CreatorOrchGongnengquIaFixtures />
    </MemoryRouter>,
  );
}

function querySidebarWorldsMenu(container: HTMLElement) {
  return within(container).queryByRole('button', { name: /^世界$/ });
}

describe('CreatorOrchGongnengquIaFixtures', () => {
  it('mounts creator hub and orchestrator fixture frames', () => {
    renderFixtures();

    expect(screen.getByTestId('creator-orch-gongnengqu-ia-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-fixture-creator-hub')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-fixture-orchestrator')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-fixture-interactive')).toBeInTheDocument();
  });

  it('renders light and dark theme pairs for creator hub and orchestrator', () => {
    renderFixtures();

    expect(screen.getByTestId('gongnengqu-ia-creator-hub-themes-light')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-creator-hub-themes-dark')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-orchestrator-themes-light')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-orchestrator-themes-dark')).toBeInTheDocument();
  });

  it('shows sidebar create panel with browse-only hub content on the right', () => {
    renderFixtures();

    const creatorFrame = screen.getByTestId('gongnengqu-ia-creator-hub-light');
    const sidebar = within(creatorFrame).getByTestId('gongnengqu-ia-creator-hub-light-sidebar');

    expect(within(sidebar).getByTestId('shell-sidebar-panel')).toBeInTheDocument();
    expect(within(sidebar).getByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(within(sidebar).getByTestId('creator-create-world')).toBeInTheDocument();
    expect(within(sidebar).getByTestId('creator-create-work')).toBeInTheDocument();

    expect(querySidebarWorldsMenu(creatorFrame)).not.toBeInTheDocument();
    expect(within(creatorFrame).queryByText('All Works')).not.toBeInTheDocument();
    expect(
      within(creatorFrame).queryByTestId(/workspace-pane-inline-form/),
    ).not.toBeInTheDocument();

    const browse = within(creatorFrame).getByTestId('gongnengqu-ia-creator-hub-light-hub-browse');
    expect(within(browse).getByTestId('gongnengqu-ia-creator-hub-light-hub-browse-tab-bar')).toBeInTheDocument();
    expect(
      within(browse).getByTestId('gongnengqu-ia-creator-hub-light-hub-browse-card-list-pane-world-card-world-fantasy'),
    ).toBeInTheDocument();
    expect(
      within(browse).queryByTestId('gongnengqu-ia-creator-hub-light-hub-browse-card-list-pane-work-card-work-novel'),
    ).not.toBeInTheDocument();
  });

  it('shows 工作区 footer in both 创作 and 编排 presentations', () => {
    renderFixtures();

    const creatorFrame = screen.getByTestId('gongnengqu-ia-creator-hub-light');
    const orchestratorFrame = screen.getByTestId('gongnengqu-ia-orchestrator-light');

    expect(
      within(creatorFrame).getByRole('toolbar', { name: '工作区' }),
    ).toBeInTheDocument();
    expect(
      within(orchestratorFrame).getByRole('toolbar', { name: '工作区' }),
    ).toBeInTheDocument();
  });

  it('interactive fixture enters entity mode from a browse card', () => {
    renderFixtures();

    const interactive = screen.getByTestId('gongnengqu-ia-interactive');
    const browse = within(interactive).getByTestId('gongnengqu-ia-interactive-hub-browse');

    fireEvent.click(
      within(browse).getByTestId('gongnengqu-ia-interactive-hub-browse-card-list-pane-world-card-world-fantasy'),
    );

    const controller = within(interactive).getByTestId('gongnengqu-ia-interactive-controller-content');
    expect(controller).toHaveAttribute('data-mode', 'controller');
    expect(within(controller).getByText(/已选世界：奇幻大陆/)).toBeInTheDocument();

    fireEvent.click(within(controller).getByTestId('creator-controller-back'));
    expect(
      within(interactive).getByTestId('gongnengqu-ia-interactive-hub-browse'),
    ).toBeInTheDocument();
  });

  it('interactive fixture keeps 工作区 footer when switching to 编排', () => {
    renderFixtures();

    const interactive = screen.getByTestId('gongnengqu-ia-interactive');
    expect(within(interactive).getByRole('toolbar', { name: '工作区' })).toBeInTheDocument();

    fireEvent.click(within(interactive).getByRole('tab', { name: '编排' }));
    expect(within(interactive).getByTestId('gongnengqu-ia-interactive-orchestrator-content')).toBeInTheDocument();
    expect(within(interactive).getByRole('toolbar', { name: '工作区' })).toBeInTheDocument();
    expect(within(interactive).getAllByText('Memory').length).toBeGreaterThanOrEqual(1);
  });
});
