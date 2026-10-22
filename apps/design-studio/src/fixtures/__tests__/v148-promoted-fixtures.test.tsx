import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { DialogFixtures } from '@/fixtures/dialog-fixtures';
import { EmptyCreateCardFixtures } from '@/fixtures/empty-create-card-fixtures';
import { SheetFixtures } from '@/fixtures/sheet-fixtures';
import { StatesFixtures } from '@/fixtures/states-fixtures';
import { TableFixtures } from '@/fixtures/table-fixtures';

/* ---- V1.148 P1 promoted families — Dialog / Sheet / Table / States / EmptyCreateCard */

describe('DialogFixtures', () => {
  it('opens the promoted Dialog with title, description, and live actions', () => {
    render(<DialogFixtures />);

    fireEvent.click(screen.getByRole('button', { name: 'Open dialog' }));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Example dialog')).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        'This dialog demonstrates the title, description, and action pattern.',
      ),
    ).toBeInTheDocument();
    // Caller-owned closeLabel fallback is the package default.
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Close dialog' }),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('SheetFixtures', () => {
  it('opens the end-rail Sheet and closes via the SheetClose control', () => {
    render(<SheetFixtures />);

    fireEvent.click(screen.getByRole('button', { name: 'Open sheet' }));
    const sheet = screen.getByRole('dialog');
    expect(within(sheet).getByText('Example sheet')).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('StatesFixtures', () => {
  it('mounts the Spinner / Loading / Empty / Unavailable / Error matrix', () => {
    render(<StatesFixtures />);

    expect(screen.getByTestId('states-spinner')).toBeInTheDocument();
    expect(screen.getByTestId('states-loading')).toHaveTextContent(
      'Loading data…',
    );
    const emptyHeadline = screen
      .getByTestId('states-empty')
      .querySelector('.font-display');
    expect(emptyHeadline).not.toBeNull();
    expect(emptyHeadline!).toHaveTextContent('No works yet');
    expect(screen.getByTestId('states-unavailable')).toBeInTheDocument();
    expect(
      within(screen.getByTestId('states-error')).getByRole('alert'),
    ).toBeInTheDocument();
  });

  it('ErrorState and UnavailableState retry actions fire live callbacks', () => {
    render(<StatesFixtures />);

    fireEvent.click(
      within(screen.getByTestId('states-error')).getByRole('button', {
        name: 'Try again',
      }),
    );
    expect(screen.getByTestId('states-error-retry-count')).toHaveTextContent(
      'Retry requested 1 time.',
    );

    fireEvent.click(
      within(screen.getByTestId('states-unavailable')).getByRole('button', {
        name: 'Try again',
      }),
    );
    expect(
      screen.getByTestId('states-unavailable-retry-count'),
    ).toHaveTextContent('Retry requested 1 time.');
  });
});

describe('TableFixtures', () => {
  it('renders the overflow-wrapped table with all rows', () => {
    render(<TableFixtures />);

    const wrapper = screen.getByTestId('table-overflow-wrapper');
    const table = within(wrapper).getByRole('table');
    expect(within(table).getAllByRole('row')).toHaveLength(5); // header + 4 rows
    expect(within(table).getByText('work-01')).toBeInTheDocument();
    expect(
      within(table).getByText(/deliberately long work title/),
    ).toBeInTheDocument();
  });
});

describe('EmptyCreateCardFixtures', () => {
  it('renders enabled + disabled cards; enabled click is live, disabled is inert', () => {
    render(<EmptyCreateCardFixtures />);

    const enabled = screen.getByTestId('empty-create-card-enabled');
    expect(enabled).toBeEnabled();
    const disabled = screen.getByTestId('empty-create-card-disabled');
    expect(disabled).toBeDisabled();
    expect(disabled).toHaveAttribute('title', 'Desktop app only');

    fireEvent.click(enabled);
    expect(screen.getByTestId('empty-create-card-click-count')).toHaveTextContent(
      'Enabled card clicked 1 time',
    );
    fireEvent.click(disabled);
    expect(screen.getByTestId('empty-create-card-click-count')).toHaveTextContent(
      'Enabled card clicked 1 time',
    );
  });
});
