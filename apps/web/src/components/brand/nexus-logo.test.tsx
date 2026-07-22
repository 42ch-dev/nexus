import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { NexusLogo } from '@/components/brand/nexus-logo';

vi.mock('@42ch/nexus-ui/assets/logos/logo-primary.svg', () => ({
  default: '/mock/logo-primary.svg',
}));

describe('NexusLogo', () => {
  it('renders the primary plate lockup', () => {
    render(<NexusLogo />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo.getAttribute('src')).toContain('logo-primary.svg');
    expect(logo).toHaveClass('h-5', 'w-auto', 'max-w-full', 'shrink-0');
    expect(logo).toHaveAttribute('height', '20');
  });

  it('honors a custom accessible label', () => {
    render(<NexusLogo label="Nexus local workspace" />);
    expect(screen.getByRole('img', { name: 'Nexus local workspace' })).toBeInTheDocument();
  });

  it('allows className to override height while keeping w-auto', () => {
    render(<NexusLogo className="h-7" />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo).toHaveClass('h-7', 'w-auto');
  });
});
