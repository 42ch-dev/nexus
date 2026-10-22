import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ConfirmDialogFixtures } from '@/fixtures/confirm-dialog-fixtures';
import { DetailPartsFixtures } from '@/fixtures/detail-parts-fixtures';
import { FormDialogFixtures } from '@/fixtures/form-dialog-fixtures';
import { FormSectionFixtures } from '@/fixtures/form-section-fixtures';
import { ListDetailShellFixtures } from '@/fixtures/list-detail-shell-fixtures';
import { ListPageLayoutFixtures } from '@/fixtures/list-page-layout-fixtures';
import { ListStateContainerFixtures } from '@/fixtures/list-state-container-fixtures';
import { SettingsLayoutFixtures } from '@/fixtures/settings-layout-fixtures';
import { SettingsSectionFixtures } from '@/fixtures/settings-section-fixtures';

/* ---- V1.149 P1 + V1.150 P1-P3 promoted L2 pattern families */

describe('ListStateContainerFixtures', () => {
  it('mounts all four branches', () => {
    render(<ListStateContainerFixtures />);
    expect(screen.getByTestId('list-state-loading')).toBeInTheDocument();
    expect(screen.getByTestId('list-state-error')).toBeInTheDocument();
    expect(screen.getByTestId('list-state-empty')).toBeInTheDocument();
    expect(screen.getByTestId('list-state-ready')).toHaveTextContent('Workspace Alpha');
  });

  it('error retry and empty create callbacks are live', () => {
    render(<ListStateContainerFixtures />);
    fireEvent.click(screen.getByTestId('list-state-empty-create'));
    expect(screen.getByTestId('list-state-create-count')).toHaveTextContent('1');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(screen.getByTestId('list-state-retry-count')).toHaveTextContent('1');
  });
});

describe('ConfirmDialogFixtures', () => {
  it('opens the destructive confirm dialog; confirm and cancel are live', () => {
    render(<ConfirmDialogFixtures />);
    fireEvent.click(screen.getByTestId('confirm-dialog-open'));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Delete workspace?')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete workspace' }));
    expect(screen.getByTestId('confirm-dialog-confirmed-count')).toHaveTextContent('1');
    fireEvent.click(screen.getByTestId('confirm-dialog-open'));
    fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }),
    );
    expect(screen.getByTestId('confirm-dialog-cancelled-count')).toHaveTextContent('1');
  });

  it('pending disables actions and guards dismissal', () => {
    render(<ConfirmDialogFixtures />);
    fireEvent.click(screen.getByTestId('confirm-dialog-pending-toggle'));
    fireEvent.click(screen.getByTestId('confirm-dialog-open'));
    const dialog = screen.getByRole('dialog');
    const confirm = within(dialog).getByRole('button', { name: /Delete workspace/ });
    const cancel = within(dialog).getByRole('button', { name: 'Cancel' });
    expect(confirm).toBeDisabled();
    expect(cancel).toBeDisabled();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Close dialog' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
});

describe('FormDialogFixtures', () => {
  it('submits through the native form; cancel closes', () => {
    render(<FormDialogFixtures />);
    fireEvent.click(screen.getByTestId('form-dialog-open'));
    const dialog = screen.getByRole('dialog');
    fireEvent.change(screen.getByTestId('form-dialog-name-input'), {
      target: { value: 'Harbor light' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Create project' }));
    expect(screen.getByTestId('form-dialog-submit-count')).toHaveTextContent('1');
    fireEvent.click(screen.getByTestId('form-dialog-open'));
    fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('renders the inline error slot when the toggle is on', () => {
    render(<FormDialogFixtures />);
    fireEvent.click(screen.getByTestId('form-dialog-error-toggle'));
    fireEvent.click(screen.getByTestId('form-dialog-open'));
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Creation failed: the name is already taken.',
    );
  });
});

describe('FormSectionFixtures', () => {
  it('mounts both fieldset groups; the error slot renders role=alert', () => {
    render(<FormSectionFixtures />);
    expect(screen.getByTestId('form-section-host-input')).toBeInTheDocument();
    expect(screen.getByTestId('form-section-path-input')).toHaveAttribute(
      'aria-invalid',
      'true',
    );
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Save failed: the workspace path is not writable.',
    );
  });
});

describe('ListPageLayoutFixtures', () => {
  it('renders header, filters, results table, and live footer actions', () => {
    render(<ListPageLayoutFixtures />);
    expect(screen.getByRole('heading', { name: 'Findings' })).toBeInTheDocument();
    expect(screen.getByTestId('list-page-filter-search')).toBeInTheDocument();
    expect(screen.getByTestId('list-page-table')).toHaveTextContent('Harbor light');
    fireEvent.click(screen.getByTestId('list-page-refresh'));
    fireEvent.click(screen.getByTestId('list-page-load-more'));
    expect(screen.getByText(/Refreshed 1×/)).toBeInTheDocument();
  });
});

describe('ListDetailShellFixtures', () => {
  it('shows the caller-owned detail panel for the selection; clear shows EmptyState', () => {
    render(<ListDetailShellFixtures />);
    expect(screen.getByTestId('list-detail-panel')).toHaveTextContent('Finding Alpha');
    fireEvent.click(screen.getByTestId('list-detail-item-beta'));
    expect(screen.getByTestId('list-detail-panel')).toHaveTextContent('Finding Beta');
    fireEvent.click(screen.getByTestId('list-detail-clear'));
    expect(screen.queryByTestId('list-detail-panel')).not.toBeInTheDocument();
    expect(screen.getByText('No item selected')).toBeInTheDocument();
  });
});

describe('DetailPartsFixtures', () => {
  it('renders populated rows and hides null/undefined/empty rows', () => {
    render(<DetailPartsFixtures />);
    expect(screen.getByText('Succeeded')).toBeInTheDocument();
    expect(screen.getByText('run-9f31c2')).toBeInTheDocument();
    expect(screen.queryByText('Report')).not.toBeInTheDocument();
    expect(screen.queryByText('Params')).not.toBeInTheDocument();
    expect(screen.queryByText('Note')).not.toBeInTheDocument();
    expect(screen.getByTestId('detail-parts-null-note')).toBeInTheDocument();
  });
});

describe('SettingsSectionFixtures', () => {
  it('mounts the settings section with label/control/hint rows wired by htmlFor', () => {
    render(<SettingsSectionFixtures />);
    expect(screen.getByRole('group', { name: 'Appearance' })).toBeInTheDocument();
    expect(screen.getByTestId('settings-row-theme')).toBeInTheDocument();
    expect(screen.getByLabelText('Theme')).toBe(screen.getByTestId('settings-row-theme'));
    expect(screen.getByText('Shown to other members of the workspace.')).toBeInTheDocument();
  });
});

describe('SettingsLayoutFixtures', () => {
  it('renders the nav rail and navigates live via onNavigate', () => {
    render(<SettingsLayoutFixtures />);
    const nav = screen.getByRole('navigation', { name: 'Settings sections' });
    expect(within(nav).getByTestId('settings-section-nav-appearance')).toHaveAttribute(
      'aria-current',
      'page',
    );
    fireEvent.click(within(nav).getByTestId('settings-section-nav-connection'));
    expect(screen.getByTestId('settings-layout-last-nav')).toHaveTextContent('connection');
    expect(
      within(nav).getByTestId('settings-section-nav-connection'),
    ).toHaveAttribute('aria-current', 'page');
  });
});
