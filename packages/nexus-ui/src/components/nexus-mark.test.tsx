import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { NexusMark } from './nexus-mark';

describe('NexusMark', () => {
  it('renders an inline svg with viewBox and default title', () => {
    const { container } = render(<NexusMark />);

    const svg = container.querySelector('svg');
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute('viewBox', '0 0 100 100');
    expect(svg).toHaveAttribute('role', 'img');
    expect(svg?.querySelector('title')?.textContent).toBe('Nexus');
  });

  it('respects the size prop', () => {
    const { container } = render(<NexusMark size={48} />);

    const svg = container.querySelector('svg');
    expect(svg).toHaveAttribute('width', '48');
    expect(svg).toHaveAttribute('height', '48');
  });

  it('uses currentColor for stroke and fill groups', () => {
    const { container } = render(<div style={{ color: '#ff0000' }}>
        <NexusMark />
      </div>,
    );

    const groups = container.querySelectorAll('svg > g');
    expect(groups).toHaveLength(2);
    expect(groups[0]).toHaveAttribute('stroke', 'currentColor');
    expect(groups[1]).toHaveAttribute('fill', 'currentColor');
  });

  it('honors a custom accessible label', () => {
    const { container } = render(<NexusMark label="Nexus mark" />);

    expect(container.querySelector('title')?.textContent).toBe('Nexus mark');
  });
});
