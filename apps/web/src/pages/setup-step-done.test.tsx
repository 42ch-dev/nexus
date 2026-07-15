import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupStepDone } from '@/pages/setup-step-done';
import { renderInApp } from '@/test/test-providers';

describe('SetupStepDone', () => {
  it('renders the completion message and the Finish button', async () => {
    renderInApp(<SetupStepDone onFinish={vi.fn()} />, {
      initialRouterEntries: ['/setup'],
    });

    expect(screen.getByText("You're ready 🎉")).toBeInTheDocument();
    expect(
      screen.getByText('Open Nexus to start writing. You can change settings anytime.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open' })).toBeInTheDocument();
  });

  it('calls onFinish when the Finish button is clicked', async () => {
    const user = userEvent.setup();
    const onFinish = vi.fn();

    renderInApp(<SetupStepDone onFinish={onFinish} />, {
      initialRouterEntries: ['/setup'],
    });

    const finishButton = screen.getByRole('button', { name: 'Open' });
    await user.click(finishButton);

    await waitFor(() => expect(onFinish).toHaveBeenCalled());
  });

  it('calls onBack when Back is clicked', async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();

    renderInApp(<SetupStepDone onFinish={vi.fn()} onBack={onBack} />, {
      initialRouterEntries: ['/setup'],
    });

    const backButton = screen.getByRole('button', { name: 'Back' });
    expect(backButton).not.toHaveTextContent('Back');
    await user.click(backButton);
    expect(onBack).toHaveBeenCalled();
  });

  it('shows a loading state when isFinishing is true', async () => {
    renderInApp(<SetupStepDone onFinish={vi.fn()} isFinishing />, {
      initialRouterEntries: ['/setup'],
    });

    const finishButton = await waitFor(() =>
      screen.getByRole('button', { name: 'Finishing…' }),
    );
    expect(finishButton).toBeDisabled();
  });

  it('renders the Finish button as a wide prominent bottom CTA', async () => {
    renderInApp(<SetupStepDone onFinish={vi.fn()} />, {
      initialRouterEntries: ['/setup'],
    });

    const finishButton = await waitFor(() =>
      screen.getByRole('button', { name: 'Open' }),
    );
    expect(finishButton).toHaveClass('w-full', 'max-w-setup-wizard-surface-cta-primary-max-width');
  });
});
