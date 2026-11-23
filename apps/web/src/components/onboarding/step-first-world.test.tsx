import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StepFirstWorld } from './step-first-world';
import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';

function withResolvers<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function makeClient(creativeRoot?: string): NexusClient {
  return {
    getActiveWorkspace: () =>
      Promise.resolve({
        creator_id: 'creator-a',
        workspace_slug: 'default',
        creative_root: creativeRoot,
        operational_dir: '/tmp/nexus',
      }),
  } as unknown as NexusClient;
}

function renderStep(
  {
    client = makeClient('/home/creator/Nexus'),
    onBack = vi.fn(),
    onFinish = vi.fn().mockResolvedValue(undefined),
  }: {
    client?: NexusClient;
    onBack?: () => void;
    onFinish?: (name: string) => Promise<void>;
  } = {},
) {
  return {
    onBack,
    onFinish,
    ...renderInApp(<StepFirstWorld onBack={onBack} onFinish={onFinish} />, {
      client,
      activeCreatorId: 'creator-a',
    }),
  };
}

describe('StepFirstWorld (creator first World)', () => {
  beforeEach(() => window.localStorage.clear());

  it('renders the product copy with the fantasy template preselected', () => {
    renderStep();

    expect(screen.getByText('Step 3 of 3')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Create Your First World' })).toBeInTheDocument();
    expect(
      screen.getByText('A World is your story universe. Lore, timelines and characters all live inside it'),
    ).toBeInTheDocument();

    expect(screen.getByTestId('world-template-blank')).toHaveTextContent('Blank World');
    expect(screen.getByTestId('world-template-blank')).toHaveTextContent('Start from scratch');
    expect(screen.getByTestId('world-template-fantasy')).toHaveTextContent('Fantasy Adventure');
    expect(screen.getByTestId('world-template-fantasy')).toHaveTextContent('Comes with a sample timeline and characters');
    expect(screen.getByTestId('world-template-scifi')).toHaveTextContent('Sci-Fi Expedition');
    expect(screen.getByTestId('world-template-fantasy')).toHaveAttribute('aria-checked', 'true');

    const field = screen.getByLabelText('World name');
    expect(field).toHaveAttribute('placeholder', 'e.g. Throne of Ashes');
    expect(field).toHaveValue('Throne of Ashes');
    expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled();

    expect(screen.queryByTestId('first-world-root')).not.toBeInTheDocument();
  });

  it('preselects each template name while keeping the field editable', async () => {
    const user = userEvent.setup();
    renderStep();

    await user.click(screen.getByTestId('world-template-blank'));
    expect(screen.getByTestId('world-template-blank')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByLabelText('World name')).toHaveValue('');
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();

    await user.click(screen.getByTestId('world-template-scifi'));
    expect(screen.getByLabelText('World name')).toHaveValue('Farstar');

    await user.clear(screen.getByLabelText('World name'));
    await user.type(screen.getByLabelText('World name'), '  我的世界  ');
    expect(screen.getByLabelText('World name')).toHaveValue('  我的世界  ');
    expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled();
  });

  it('submits the trimmed name and locks the button while the finish runs', async () => {
    const user = userEvent.setup();
    const pending = withResolvers<void>();
    const onFinish = vi.fn(() => pending.promise);
    renderStep({ onFinish });

    await user.clear(screen.getByLabelText('World name'));
    await user.type(screen.getByLabelText('World name'), '  Throne of Ashes  ');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    expect(onFinish).toHaveBeenCalledWith('Throne of Ashes');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled());

    pending.resolve();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled());
  });

  it('keeps the draft and reports the reason when creation fails', async () => {
    const user = userEvent.setup();
    const onFinish = vi.fn().mockRejectedValue(new Error('daemon refused'));
    renderStep({ onFinish });

    await user.click(screen.getByTestId('world-template-scifi'));
    await user.click(screen.getByRole('button', { name: 'Create' }));

    expect(
      await screen.findByText('Could not create the World: daemon refused. Try again or choose a different name.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Create Your First World' })).toBeInTheDocument();
    expect(screen.getByTestId('world-template-scifi')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByLabelText('World name')).toHaveValue('Farstar');
    expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled();
  });

  it('claims no absolute path until the service reports a root', async () => {
    const user = userEvent.setup();
    renderStep({ client: makeClient(undefined) });

    expect(screen.getByText(/Files are saved in the default workspace/)).toBeInTheDocument();
    expect(screen.queryByText('/home/creator/Nexus')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Advanced: Change location' }));
    expect(screen.getByTestId('first-world-root')).toHaveTextContent('Default workspace');
  });

  it('shows the reported creative_root in the advanced area', async () => {
    const user = userEvent.setup();
    renderStep();

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Advanced: Change location' })).toBeInTheDocument(),
    );
    await user.click(screen.getByRole('button', { name: 'Advanced: Change location' }));

    expect(screen.getByTestId('first-world-root')).toHaveTextContent('/home/creator/Nexus');
  });

  it('returns to the previous step', async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();
    renderStep({ onBack });

    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});
