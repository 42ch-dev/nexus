import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import type { EntranceId } from '@/components/layout/entrance-registry';
import { StepIdentity } from './step-identity';

function Harness({ onNext }: { onNext: () => void }) {
  const [value, setValue] = useState<EntranceId | null>(null);
  return <StepIdentity value={value} onChange={setValue} onNext={onNext} />;
}

describe('StepIdentity', () => {
  it('renders the product-final identity copy', () => {
    render(<StepIdentity value={null} onChange={() => {}} onNext={() => {}} />);

    expect(screen.getByText('NEXUS · 初次设置')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '你的 AI 创作工作室' })).toBeInTheDocument();
    expect(
      screen.getByText('世界有记忆，故事不自相矛盾。先告诉我——你想用 Nexus 做什么？'),
    ).toBeInTheDocument();
    expect(screen.getByText('我来写故事')).toBeInTheDocument();
    expect(
      screen.getByText('长篇小说 · 跑团战役 · 世界观构建 / AI 记得你的设定，帮你写出初稿'),
    ).toBeInTheDocument();
    expect(screen.getByText('推荐路径')).toBeInTheDocument();
    expect(screen.getByText('我来开发工具')).toBeInTheDocument();
    expect(
      screen.getByText('编排引擎 · 预设 · WASM 模块 / 完整的操作台与策略画布'),
    ).toBeInTheDocument();
  });

  it('keeps 下一步 disabled until a card is chosen, then reports the entrance', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    render(<Harness onNext={onNext} />);

    const next = screen.getByRole('button', { name: '下一步' });
    expect(next).toBeDisabled();

    const developer = screen.getByTestId('identity-option-developer');
    expect(developer).toHaveAttribute('aria-checked', 'false');
    await user.click(developer);

    expect(developer).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('identity-option-content-creator')).toHaveAttribute(
      'aria-checked',
      'false',
    );
    expect(next).toBeEnabled();

    await user.click(next);
    expect(onNext).toHaveBeenCalledTimes(1);
  });
});
