import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import type { LogoVariantName } from '../tokens';
import { NexusLogo, type Variant, VARIANT_FILENAMES } from './nexus-logo';

// Compile-time guard: the re-exported Variant alias must be identical to the
// canonical LogoVariantName from tokens.ts.
type AssertVariantAlias = [Variant] extends [LogoVariantName]
  ? [LogoVariantName] extends [Variant]
    ? true
    : never
  : never;
const _variantAliasCheck: AssertVariantAlias = true;

describe('NexusLogo', () => {
  it('renders an img with default alt, size, and forwarded src', () => {
    render(<NexusLogo variant="primary" src="/mock/logo-primary.svg" />);

    const img = screen.getByRole('img', { name: 'Nexus' });
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute('src', '/mock/logo-primary.svg');
    expect(img).toHaveAttribute('height', '32');
    expect(img).toHaveAttribute('decoding', 'async');
    expect(img).toHaveStyle({ width: 'auto', height: '32px' });
  });

  it('honors custom label, className, size, and draggable', () => {
    render(
      <NexusLogo
        variant="white"
        src="/mock/logo-white.svg"
        label="Nexus local"
        className="brand-logo"
        size={48}
        draggable={false}
      />,
    );

    const img = screen.getByRole('img', { name: 'Nexus local' });
    expect(img).toHaveClass('brand-logo');
    expect(img).toHaveAttribute('height', '48');
    expect(img).toHaveAttribute('src', '/mock/logo-white.svg');
    expect(img).toHaveAttribute('draggable', 'false');
  });

  it('supports the text wordmark variant', () => {
    render(<NexusLogo variant="text" src="/mock/logo-text.svg" size={28} />);

    const img = screen.getByRole('img', { name: 'Nexus' });
    expect(img).toHaveAttribute('src', '/mock/logo-text.svg');
    expect(img).toHaveAttribute('height', '28');
  });

  it('maps every variant to its canonical filename', () => {
    expect(VARIANT_FILENAMES).toEqual({
      primary: 'logo-primary.svg',
      whiteBg: 'logo-white-bg.svg',
      white: 'logo-white.svg',
      mono: 'logo-mono.svg',
      text: 'logo-text.svg',
    });
  });
});
