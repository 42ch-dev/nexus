/**
 * Side checklist tests (plan v1219-world-module T3) — product-final copy
 * (heading `接下来可以`, steps ①②③, `进度 {n} / 3`) and state styling:
 * done steps struck through, the first pending step highlighted, progress
 * derived from the done count.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { cleanup, screen } from '@testing-library/react';
import { i18n } from '@/lib/i18n/config';

import { SideChecklist } from '@/components/world/side-checklist';
import { renderInApp } from '@/test/test-providers';

beforeEach(() => {
  cleanup();
  window.localStorage.setItem('nexus-web-locale', 'zh-CN');
  return i18n.changeLanguage('zh-CN');
});

describe('SideChecklist', () => {
  it('renders the heading, three steps, and 进度 0 / 3 before any Brief', () => {
    renderInApp(<SideChecklist briefDone={false} />);

    expect(screen.getByTestId('world-side-checklist')).toHaveTextContent('接下来可以');
    expect(screen.getByTestId('world-checklist-step-1')).toHaveTextContent('① 写完 Brief');
    expect(screen.getByTestId('world-checklist-step-2')).toHaveTextContent(
      '② 创建第一个作品并绑定世界',
    );
    expect(screen.getByTestId('world-checklist-step-3')).toHaveTextContent(
      '③ 让 AI 起草第一条 Narrative',
    );
    expect(screen.getByTestId('world-checklist-progress')).toHaveTextContent('进度 0 / 3');
  });

  it('marks step ① done and step ② next after the Brief saves (进度 1 / 3)', () => {
    renderInApp(<SideChecklist briefDone={true} />);

    expect(screen.getByTestId('world-checklist-step-1')).toHaveClass('line-through');
    expect(screen.getByTestId('world-checklist-step-2')).toHaveClass('font-medium');
    expect(screen.getByTestId('world-checklist-step-2')).not.toHaveClass('line-through');
    expect(screen.getByTestId('world-checklist-step-3')).not.toHaveClass('font-medium');
    expect(screen.getByTestId('world-checklist-progress')).toHaveTextContent('进度 1 / 3');
  });
});
