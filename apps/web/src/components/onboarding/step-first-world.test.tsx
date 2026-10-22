import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StepFirstWorld } from './step-first-world';
import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';

// Promise.withResolvers is unavailable at the current tsconfig lib — local
// equivalent until the lib target reaches es2024.
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

    expect(screen.getByText('第 3 步 / 共 3 步')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '创建你的第一个世界' })).toBeInTheDocument();
    expect(
      screen.getByText('世界 = 你的故事宇宙。设定、时间线、角色都住在里面'),
    ).toBeInTheDocument();

    expect(screen.getByTestId('world-template-blank')).toHaveTextContent('空白世界');
    expect(screen.getByTestId('world-template-blank')).toHaveTextContent('从零开始');
    expect(screen.getByTestId('world-template-fantasy')).toHaveTextContent('奇幻冒险');
    expect(screen.getByTestId('world-template-fantasy')).toHaveTextContent('带示例时间线与角色');
    expect(screen.getByTestId('world-template-scifi')).toHaveTextContent('科幻远征');
    expect(screen.getByTestId('world-template-fantasy')).toHaveAttribute('aria-checked', 'true');

    const field = screen.getByLabelText('世界名称');
    expect(field).toHaveAttribute('placeholder', '例如：灰烬王座');
    expect(field).toHaveValue('灰烬王座');
    expect(screen.getByRole('button', { name: '创建并进入' })).toBeEnabled();

    // The advanced area reads the reported root only after it is opened.
    expect(screen.queryByTestId('first-world-root')).not.toBeInTheDocument();
  });

  it('preselects each template name while keeping the field editable', async () => {
    const user = userEvent.setup();
    renderStep();

    await user.click(screen.getByTestId('world-template-blank'));
    expect(screen.getByTestId('world-template-blank')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByLabelText('世界名称')).toHaveValue('');
    expect(screen.getByRole('button', { name: '创建并进入' })).toBeDisabled();

    await user.click(screen.getByTestId('world-template-scifi'));
    expect(screen.getByLabelText('世界名称')).toHaveValue('远星号');

    await user.clear(screen.getByLabelText('世界名称'));
    await user.type(screen.getByLabelText('世界名称'), '  我的世界  ');
    expect(screen.getByLabelText('世界名称')).toHaveValue('  我的世界  ');
    expect(screen.getByRole('button', { name: '创建并进入' })).toBeEnabled();
  });

  it('submits the trimmed name and locks the button while the finish runs', async () => {
    const user = userEvent.setup();
    const pending = withResolvers<void>();
    const onFinish = vi.fn(() => pending.promise);
    renderStep({ onFinish });

    await user.clear(screen.getByLabelText('世界名称'));
    await user.type(screen.getByLabelText('世界名称'), '  灰烬王座  ');
    await user.click(screen.getByRole('button', { name: '创建并进入' }));

    expect(onFinish).toHaveBeenCalledWith('灰烬王座');
    await waitFor(() => expect(screen.getByRole('button', { name: '创建并进入' })).toBeDisabled());

    pending.resolve();
    await waitFor(() => expect(screen.getByRole('button', { name: '创建并进入' })).toBeEnabled());
  });

  it('keeps the draft and reports the reason when creation fails', async () => {
    const user = userEvent.setup();
    const onFinish = vi.fn().mockRejectedValue(new Error('daemon refused'));
    renderStep({ onFinish });

    await user.click(screen.getByTestId('world-template-scifi'));
    await user.click(screen.getByRole('button', { name: '创建并进入' }));

    expect(
      await screen.findByText('创建世界失败：daemon refused。请重试或更换名称。'),
    ).toBeInTheDocument();
    // Still in the step with the draft (template + name).
    expect(screen.getByRole('heading', { name: '创建你的第一个世界' })).toBeInTheDocument();
    expect(screen.getByTestId('world-template-scifi')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByLabelText('世界名称')).toHaveValue('远星号');
    expect(screen.getByRole('button', { name: '创建并进入' })).toBeEnabled();
  });

  it('claims no absolute path until the service reports a root', async () => {
    const user = userEvent.setup();
    renderStep({ client: makeClient(undefined) });

    expect(screen.getByText(/文件会保存在默认工作区/)).toBeInTheDocument();
    expect(screen.queryByText('/home/creator/Nexus')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '高级：更改位置' }));
    expect(screen.getByTestId('first-world-root')).toHaveTextContent('默认工作区');
  });

  it('shows the reported creative_root in the advanced area', async () => {
    const user = userEvent.setup();
    renderStep();

    await waitFor(() =>
      expect(screen.getByRole('button', { name: '高级：更改位置' })).toBeInTheDocument(),
    );
    await user.click(screen.getByRole('button', { name: '高级：更改位置' }));

    expect(screen.getByTestId('first-world-root')).toHaveTextContent('/home/creator/Nexus');
  });

  it('returns to the previous step', async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();
    renderStep({ onBack });

    await user.click(screen.getByRole('button', { name: '返回' }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});
