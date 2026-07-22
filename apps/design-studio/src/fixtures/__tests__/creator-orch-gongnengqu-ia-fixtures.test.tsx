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

  it('shows Create-only left hub (not Menu nav) with Worlds/Works lists on the right', () => {
    renderFixtures();

    const creatorFrame = screen.getByTestId('gongnengqu-ia-creator-hub-light');
    const createLeft = within(creatorFrame).getByTestId('gongnengqu-ia-creator-hub-light-create-left');
    const createContent = within(createLeft).getByTestId('gongnengqu-ia-creator-hub-light-create-content');

    expect(createContent).toHaveAttribute('data-mode', 'create');
    expect(within(createContent).getByTestId('creator-create-world')).toBeInTheDocument();
    expect(within(createContent).getByTestId('creator-create-work')).toBeInTheDocument();

    expect(querySidebarWorldsMenu(creatorFrame)).not.toBeInTheDocument();
    expect(within(creatorFrame).queryByText('All Works')).not.toBeInTheDocument();

    const lists = within(creatorFrame).getByTestId('gongnengqu-ia-creator-hub-light-entity-lists');
    expect(within(lists).getByTestId('gongnengqu-ia-creator-hub-light-entity-lists-worlds')).toBeInTheDocument();
    expect(within(lists).getByTestId('gongnengqu-ia-creator-hub-light-entity-lists-works')).toBeInTheDocument();
    expect(within(lists).getByText('奇幻大陆')).toBeInTheDocument();
    expect(within(lists).getByText('漫漫长路')).toBeInTheDocument();
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

  it('interactive fixture enters entity mode from a right-side list row', () => {
    renderFixtures();

    const interactive = screen.getByTestId('gongnengqu-ia-interactive');
    const lists = within(interactive).getByTestId('gongnengqu-ia-interactive-entity-lists');

    fireEvent.click(
      within(lists).getByTestId('gongnengqu-ia-interactive-entity-lists-worlds-row-world-fantasy'),
    );

    const controller = within(interactive).getByTestId('gongnengqu-ia-interactive-controller-content');
    expect(controller).toHaveAttribute('data-mode', 'controller');
    expect(within(controller).getByText(/已选世界：奇幻大陆/)).toBeInTheDocument();

    fireEvent.click(within(controller).getByTestId('creator-controller-back'));
    expect(
      within(interactive).getByTestId('gongnengqu-ia-interactive-entity-lists'),
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
