import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { NexusTextLogo } from '@/components/brand/nexus-text-logo';

vi.mock('@42ch/nexus-ui/assets/logos/logo-text.svg', () => ({
  default: '/mock/logo-text.svg',
}));

describe('NexusTextLogo', () => {
  it('renders the logo-text wordmark through NexusLogo variant=text', () => {
    render(<NexusTextLogo />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo.getAttribute('src')).toContain('logo-text.svg');
    expect(logo).toHaveAttribute('height', '24');
    expect(logo).toHaveClass('w-auto', 'max-w-full', 'shrink-0');
  });

  it('honors size and className', () => {
    render(<NexusTextLogo size={26} className="h-[26px] brightness-0 invert" />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo).toHaveAttribute('height', '26');
    expect(logo).toHaveClass('h-[26px]', 'brightness-0', 'invert');
  });
});
