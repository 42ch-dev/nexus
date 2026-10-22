import { Plus } from 'lucide-react';
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';

import { EmptyCreateCard } from './empty-create-card';

/**
 * V1.148 P1 — promoted EmptyCreateCard primitive.
 *
 * Pins: caller-owned copy and icon, the dashed-card create affordance recipe
 * (V1.125 P2 AC-V1125-5), disabled gating (non-interactive + optional native
 * tooltip), and the light/dark icon hook (brand-deep-blue / blue-700).
 */
describe('EmptyCreateCard', () => {
  it('renders caller-owned copy with the dashed card recipe', () => {
    const { container } = render(
      <EmptyCreateCard
        icon={Plus}
        title="Create your first World"
        description="A World holds your narrative universe."
        onClick={() => undefined}
      />,
    );

    const card = screen.getByRole('button', { name: /Create your first World/ });
    expect(card.className).toMatch(/\bborder-dashed\b/);
    expect(card.className).toMatch(/\brounded-card\b/);
    expect(card.className).toMatch(/\bborder-gray-alpha-400\b/);
    expect(container.querySelector('svg')).not.toBeNull();
    expect(screen.getByText('A World holds your narrative universe.')).toBeInTheDocument();
  });

  it('icon uses deep ink on light and cobalt on dark', () => {
    render(
      <EmptyCreateCard
        icon={Plus}
        title="Create your first World"
        description="desc"
        onClick={() => undefined}
      />,
    );
    const icon = document.querySelector('button svg')!;
    expect(icon.getAttribute('class')).toMatch(/\btext-brand-deep-blue\b/);
    expect(icon.getAttribute('class')).toMatch(/\bdark:text-blue-700\b/);
  });

  it('fires the caller callback on click', () => {
    const onClick = vi.fn();
    render(
      <EmptyCreateCard icon={Plus} title="Create" description="desc" onClick={onClick} />,
    );
    fireEvent.click(screen.getByRole('button', { name: /Create/ }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('disabled state is non-interactive and carries the native tooltip', () => {
    const onClick = vi.fn();
    render(
      <EmptyCreateCard
        icon={Plus}
        title="Create"
        description="desc"
        onClick={onClick}
        disabled
        titleAttr="Desktop only"
        data-testid="empty-create"
      />,
    );
    const card = screen.getByTestId('empty-create');
    expect(card).toBeDisabled();
    expect(card).toHaveAttribute('title', 'Desktop only');
    fireEvent.click(card);
    expect(onClick).not.toHaveBeenCalled();
  });
});
