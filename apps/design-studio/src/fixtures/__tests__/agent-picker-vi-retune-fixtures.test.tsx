import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AgentPickerViRetuneFixtures } from '@/fixtures/agent-picker-vi-retune-fixtures';

describe('AgentPickerViRetuneFixtures', () => {
  it('mounts V1.134 retune fixture sections', () => {
    render(<AgentPickerViRetuneFixtures />);

    expect(screen.getByTestId('agent-picker-vi-retune-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('vi-retune-fixture-dot-matrix')).toBeInTheDocument();
    expect(screen.getByTestId('vi-retune-fixture-ready-selected')).toBeInTheDocument();
    expect(screen.getByTestId('vi-retune-fixture-ready-unselected')).toBeInTheDocument();
    expect(screen.getByTestId('vi-retune-fixture-interactive')).toBeInTheDocument();
    expect(screen.getByTestId('vi-retune-fixture-statuses')).toBeInTheDocument();
  });

  it('renders light+dark theme pairs for dot matrix and ready grids', () => {
    render(<AgentPickerViRetuneFixtures />);

    for (const base of ['vi-retune-dot-matrix', 'vi-retune-ready-selected', 'vi-retune-ready-unselected'] as const) {
      expect(screen.getByTestId(`${base}-light`)).toBeInTheDocument();
      expect(screen.getByTestId(`${base}-dark`)).toBeInTheDocument();
    }
  });

  it('shows lit, hollow, and muted StatusDot states in the matrix', () => {
    render(<AgentPickerViRetuneFixtures />);

    const matrix = screen.getByTestId('vi-retune-dot-matrix-light');
    expect(within(matrix).getByTestId('vi-target-dot-matrix')).toBeInTheDocument();
    expect(within(matrix).queryAllByTestId('agent-status-dot')).toHaveLength(3);
    expect(matrix.querySelector('[data-dot="lit"]')).not.toBeNull();
    expect(matrix.querySelector('[data-dot="hollow"]')).not.toBeNull();
    expect(matrix.querySelector('[data-dot="muted"]')).not.toBeNull();
  });

  it('renders target ready grid with selection ring and status dots', () => {
    render(<AgentPickerViRetuneFixtures />);

    const selectedFrame = screen.getByTestId('vi-retune-ready-selected-light');
    const grid = within(selectedFrame).getByTestId('vi-target-agent-picker-ready');
    const selectedCard = within(grid).getByTestId('vi-target-agent-card-claude-native');

    expect(selectedCard).toHaveClass('border-2', 'border-blue-700');
    expect(selectedCard).not.toHaveClass('bg-blue-700/8');
    expect(within(selectedCard).getByTestId('agent-status-dot')).toHaveAttribute('data-dot', 'lit');
  });

  it('toggles dot state in the interactive fixture', () => {
    render(<AgentPickerViRetuneFixtures />);

    const interactive = screen.getByTestId('vi-retune-fixture-interactive');
    const grid = within(interactive).getByTestId('vi-target-agent-picker-ready');
    const claudeCard = within(grid).getByTestId('vi-target-agent-card-claude-native');

    expect(within(claudeCard).getByTestId('agent-status-dot')).toHaveAttribute('data-dot', 'lit');

    fireEvent.click(within(interactive).getByTestId('vi-target-select-none'));

    expect(within(claudeCard).getByTestId('agent-status-dot')).toHaveAttribute('data-dot', 'hollow');
  });

  it('renders loading, empty, and error via shared AgentPicker', () => {
    render(<AgentPickerViRetuneFixtures />);

    expect(screen.getAllByTestId('agent-picker').length).toBeGreaterThanOrEqual(6);
    expect(screen.getAllByTestId('vi-target-agent-picker-ready').length).toBeGreaterThanOrEqual(4);
  });
});
