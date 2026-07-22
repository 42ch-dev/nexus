import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import {
  logoMarkAspectRatio,
  logoMarkViewBoxHeight,
  logoMarkViewBoxWidth,
  logoVariantPalettes,
} from '../tokens';
import { NexusLogoVariant } from './nexus-logo-variant';

describe('NexusLogoVariant', () => {
  it('renders timeline geometry without asset imports', () => {
    const { container } = render(<NexusLogoVariant theme="elegant" />);

    const svg = container.querySelector('svg');
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute(
      'viewBox',
      `0 0 ${logoMarkViewBoxWidth} ${logoMarkViewBoxHeight}`,
    );
    expect(svg?.querySelector('image')).toBeNull();
    expect(container.querySelectorAll('circle')).toHaveLength(5);
  });

  it('applies default palette stops for each theme', () => {
    for (const theme of Object.keys(logoVariantPalettes) as Array<
      keyof typeof logoVariantPalettes
    >) {
      const { container, unmount } = render(
        <NexusLogoVariant theme={theme} />,
      );
      const stops = container.querySelectorAll('linearGradient stop');
      expect(stops).toHaveLength(2);
      expect(stops[0]).toHaveAttribute(
        'stop-color',
        logoVariantPalettes[theme].start,
      );
      expect(stops[1]).toHaveAttribute(
        'stop-color',
        logoVariantPalettes[theme].end,
      );
      unmount();
    }
  });

  it('honors explicit palette overrides', () => {
    const { container } = render(
      <NexusLogoVariant
        theme="scifi"
        palette={{ start: '#111111', end: '#eeeeee' }}
      />,
    );

    const stops = container.querySelectorAll('linearGradient stop');
    expect(stops[0]).toHaveAttribute('stop-color', '#111111');
    expect(stops[1]).toHaveAttribute('stop-color', '#eeeeee');
  });

  it('sizes by height with wide aspect', () => {
    const { container } = render(<NexusLogoVariant size={40} />);

    const svg = container.querySelector('svg');
    expect(svg).toHaveAttribute('height', '40');
    expect(svg).toHaveAttribute('width', String(40 * logoMarkAspectRatio));
  });
});
