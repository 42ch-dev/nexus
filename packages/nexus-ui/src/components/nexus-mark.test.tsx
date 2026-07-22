import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import {
  logoMarkAspectRatio,
  logoMarkViewBoxHeight,
  logoMarkViewBoxWidth,
} from '../tokens';
import { NexusMark } from './nexus-mark';

describe('NexusMark', () => {
  it('renders a wide timeline svg with viewBox and default title', () => {
    const { container } = render(<NexusMark />);

    const svg = container.querySelector('svg');
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute(
      'viewBox',
      `0 0 ${logoMarkViewBoxWidth} ${logoMarkViewBoxHeight}`,
    );
    expect(svg).toHaveAttribute('role', 'img');
    expect(svg?.querySelector('title')?.textContent).toBe('Nexus');
  });

  it('sizes by height and keeps wide aspect (w-auto friendly)', () => {
    const { container } = render(<NexusMark size={48} />);

    const svg = container.querySelector('svg');
    expect(svg).toHaveAttribute('height', '48');
    expect(svg).toHaveAttribute('width', String(48 * logoMarkAspectRatio));
    expect(svg).toHaveStyle({ width: 'auto', height: '48px' });
  });

  it('uses currentColor for rings and solid center (no baked gradient)', () => {
    const { container } = render(
      <div style={{ color: '#ff0000' }}>
        <NexusMark />
      </div>,
    );

    const strokeGroup = container.querySelector('svg > g');
    expect(strokeGroup).toHaveAttribute('stroke', 'currentColor');
    expect(strokeGroup?.querySelectorAll('circle')).toHaveLength(4);

    const solid = container.querySelector('svg > circle');
    expect(solid).toHaveAttribute('fill', 'currentColor');
    expect(solid).toHaveAttribute('cx', '142');
    expect(solid).toHaveAttribute('r', '14');
  });

  it('honors a custom accessible label', () => {
    const { container } = render(<NexusMark label="Nexus mark" />);

    expect(container.querySelector('title')?.textContent).toBe('Nexus mark');
  });
});
