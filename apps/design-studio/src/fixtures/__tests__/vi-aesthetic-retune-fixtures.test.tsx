import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  VI_COMPACT_MARK_HEIGHT_PX,
  ViAgentPickerAcceptanceFixtures,
  ViBrandAcceptanceFixtures,
  ViButtonAcceptanceFixtures,
  ViTransportErrorAcceptanceFixtures,
} from '@/fixtures/vi-aesthetic-retune-fixtures';

describe('ViBrandAcceptanceFixtures', () => {
  it('renders VI-003, VI-004, and VI-005 sections with light+dark pairs', () => {
    render(<ViBrandAcceptanceFixtures />);

    expect(screen.getByTestId('vi-brand-acceptance-fixtures')).toBeInTheDocument();

    for (const id of ['vi-003', 'vi-004', 'vi-005'] as const) {
      expect(screen.getByTestId(`vi-section-${id}`)).toBeInTheDocument();
      expect(screen.getByTestId(`vi-ledger-${id}`)).toBeInTheDocument();
    }

    expect(screen.getByTestId('vi-003-compact-mark-light')).toBeInTheDocument();
    expect(screen.getByTestId('vi-003-compact-mark-dark')).toBeInTheDocument();
    expect(screen.getByTestId('vi-004-app-icon-current')).toBeInTheDocument();
    expect(screen.getByTestId('vi-004-app-icon-target')).toBeInTheDocument();
    expect(screen.getByTestId('vi-005-square-plate')).toBeInTheDocument();
  });

  it('uses compact mark height at −30% from shell SSOT', () => {
    expect(VI_COMPACT_MARK_HEIGHT_PX).toBe(14);
  });
});

describe('ViButtonAcceptanceFixtures', () => {
  it('renders current and target primary buttons in light+dark', () => {
    render(<ViButtonAcceptanceFixtures />);

    expect(screen.getByTestId('vi-section-vi-002')).toBeInTheDocument();
    expect(screen.getByTestId('vi-002-primary-button-light')).toBeInTheDocument();
    expect(screen.getByTestId('vi-002-primary-button-dark')).toBeInTheDocument();
    expect(screen.getByTestId('vi-002-current-primary-light')).toBeInTheDocument();
    expect(screen.getByTestId('vi-002-target-primary-button')).toBeInTheDocument();
    expect(screen.getByTestId('vi-002-target-dark-primary-button')).toBeInTheDocument();
  });
});

describe('ViTransportErrorAcceptanceFixtures', () => {
  it('renders TransportError with target Retry mocks in light+dark', () => {
    render(<ViTransportErrorAcceptanceFixtures />);

    expect(screen.getByTestId('vi-002-transport-error-light')).toBeInTheDocument();
    expect(screen.getByTestId('vi-002-transport-error-dark')).toBeInTheDocument();
    expect(screen.getAllByTestId('transport-error-block').length).toBeGreaterThanOrEqual(2);
  });
});

describe('ViAgentPickerAcceptanceFixtures', () => {
  it('renders current AgentPicker and target card mocks in light+dark', () => {
    render(<ViAgentPickerAcceptanceFixtures />);

    expect(screen.getByTestId('vi-section-vi-001')).toBeInTheDocument();
    expect(screen.getByTestId('vi-001-agent-picker-light')).toBeInTheDocument();
    expect(screen.getByTestId('vi-001-agent-picker-dark')).toBeInTheDocument();
    expect(screen.getByTestId('vi-001-target-card-light')).toBeInTheDocument();
    expect(screen.getByTestId('vi-001-target-card-dark')).toBeInTheDocument();
    expect(screen.getAllByTestId('agent-picker').length).toBeGreaterThanOrEqual(2);
  });
});
