import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { NexusLogo, VARIANT_FILENAMES } from './nexus-logo';

describe('NexusLogo', () => {
  it('renders an img with default alt, size, and forwarded src', () => {
    render(<NexusLogo variant="primary" src="/mock/logo-primary.svg" />);

    const img = screen.getByRole('img', { name: 'Nexus' });
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute('src', '/mock/logo-primary.svg');
    expect(img).toHaveAttribute('height', '32');
    expect(img).toHaveAttribute('decoding', 'async');
  });

  it('honors custom label, className, and size', () => {
    render(
      <NexusLogo
        variant="white"
        src="/mock/logo-white.svg"
        label="Nexus local"
        className="brand-logo"
        size={48}
      />,
    );

    const img = screen.getByRole('img', { name: 'Nexus local' });
    expect(img).toHaveClass('brand-logo');
    expect(img).toHaveAttribute('height', '48');
    expect(img).toHaveAttribute('src', '/mock/logo-white.svg');
  });

  it('maps every variant to its canonical filename', () => {
    expect(VARIANT_FILENAMES).toEqual({
      primary: 'logo-primary.svg',
      color: 'logo-color.svg',
      white: 'logo-white.svg',
      mono: 'logo-mono.svg',
    });
  });
});
