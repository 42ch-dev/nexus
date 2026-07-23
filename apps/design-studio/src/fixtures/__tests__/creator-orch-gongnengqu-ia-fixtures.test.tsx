import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ThemeProvider } from '@/components/theme-provider';
import { CreatorOrchGongnengquIaFixtures } from '@/fixtures/creator-orch-gongnengqu-ia-fixtures';

function mockMatchMedia(prefersDark: boolean) {
  const media = {
    matches: prefersDark,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  vi.spyOn(window, 'matchMedia').mockReturnValue(media as unknown as MediaQueryList);
}

function renderFixtures() {
  return render(
    <ThemeProvider>
      <MemoryRouter>
        <CreatorOrchGongnengquIaFixtures />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

function querySidebarWorldsMenu(container: HTMLElement) {
  return within(container).queryByRole('button', { name: /^世界$/ });
}

describe('CreatorOrchGongnengquIaFixtures', () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });

  it('mounts creator hub and orchestrator fixture frames', () => {
    renderFixtures();

    expect(screen.getByTestId('creator-orch-gongnengqu-ia-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-fixture-creator-hub')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-fixture-orchestrator')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-fixture-interactive')).toBeInTheDocument();
  });

  it('renders a single theme-aware frame per fixture (no light/dark matrix)', () => {
    renderFixtures();

    expect(screen.getAllByTestId('gongnengqu-ia-theme-caption')).toHaveLength(3);
    expect(screen.getByTestId('gongnengqu-ia-creator-hub')).toBeInTheDocument();
    expect(screen.getByTestId('gongnengqu-ia-orchestrator')).toBeInTheDocument();
    expect(screen.queryByTestId('gongnengqu-ia-creator-hub-themes-light')).not.toBeInTheDocument();
    expect(screen.queryByTestId('gongnengqu-ia-creator-hub-themes-dark')).not.toBeInTheDocument();
  });

  it('shows sidebar inline create panel with browse-only hub content on the right', () => {
    renderFixtures();

    const creatorFrame = screen.getByTestId('gongnengqu-ia-creator-hub');
    const sidebar = within(creatorFrame).getByTestId('gongnengqu-ia-creator-hub-sidebar');
    const createPanel = within(sidebar).getByTestId('sidebar-create-panel');

    expect(within(sidebar).getByTestId('shell-sidebar-panel')).toBeInTheDocument();
    expect(createPanel).toHaveAttribute('data-mode', 'create-inline');
    expect(within(createPanel).getByTestId('sidebar-create-tab-bar')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-tab-world')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-tab-work')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-form-world')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-submit-world')).toBeInTheDocument();
    expect(within(createPanel).queryByTestId('creator-create-world')).not.toBeInTheDocument();
    expect(within(createPanel).queryByTestId('creator-create-work')).not.toBeInTheDocument();

    expect(querySidebarWorldsMenu(creatorFrame)).not.toBeInTheDocument();
    expect(within(creatorFrame).queryByText('All Works')).not.toBeInTheDocument();
    expect(
      within(creatorFrame).queryByTestId(/workspace-pane-inline-form/),
    ).not.toBeInTheDocument();

    const browse = within(creatorFrame).getByTestId('gongnengqu-ia-creator-hub-hub-browse');
    expect(within(browse).getByTestId('gongnengqu-ia-creator-hub-hub-browse-tab-bar')).toBeInTheDocument();
    expect(
      within(browse).getByTestId('gongnengqu-ia-creator-hub-hub-browse-card-list-pane-world-card-world-fantasy'),
    ).toBeInTheDocument();
    expect(
      within(browse).queryByTestId('gongnengqu-ia-creator-hub-hub-browse-card-list-pane-work-card-work-novel'),
    ).not.toBeInTheDocument();
  });

  it('switches create-zone tabs between world and work inline forms', () => {
    renderFixtures();

    const createPanel = within(
      screen.getByTestId('gongnengqu-ia-creator-hub-sidebar'),
    ).getByTestId('sidebar-create-panel');

    expect(within(createPanel).getByTestId('sidebar-create-form-world')).toBeInTheDocument();
    expect(within(createPanel).queryByTestId('sidebar-create-form-work')).not.toBeInTheDocument();

    fireEvent.click(within(createPanel).getByTestId('sidebar-create-tab-work'));

    expect(within(createPanel).getByTestId('sidebar-create-form-work')).toBeInTheDocument();
    expect(within(createPanel).getByTestId('sidebar-create-submit-work')).toBeInTheDocument();
    expect(within(createPanel).queryByTestId('sidebar-create-form-world')).not.toBeInTheDocument();
  });

  it('shows 工作区 footer in both 创作 and 编排 presentations', () => {
    renderFixtures();

    const creatorFrame = screen.getByTestId('gongnengqu-ia-creator-hub');
    const orchestratorFrame = screen.getByTestId('gongnengqu-ia-orchestrator');

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
