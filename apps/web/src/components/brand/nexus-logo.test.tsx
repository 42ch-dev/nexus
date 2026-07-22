import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { useTheme } from '@/components/theme-provider';

vi.mock('@42ch/nexus-ui/assets/logos/logo-color.svg', () => ({
  default: '/mock/logo-color.svg',
}));
vi.mock('@42ch/nexus-ui/assets/logos/logo-primary.svg', () => ({
  default: '/mock/logo-primary.svg',
}));

vi.mock('@/components/theme-provider', () => ({
  useTheme: vi.fn(),
}));

const mockedUseTheme = vi.mocked(useTheme);

describe('NexusLogo', () => {
  beforeEach(() => {
    mockedUseTheme.mockReturnValue({
      theme: 'light',
      resolvedTheme: 'light',
      setTheme: vi.fn(),
      toggleTheme: vi.fn(),
    });
  });

  it('renders the primary timeline mark in light theme', () => {
    mockedUseTheme.mockReturnValue({
      theme: 'light',
      resolvedTheme: 'light',
      setTheme: vi.fn(),
      toggleTheme: vi.fn(),
    });
    render(<NexusLogo />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo.getAttribute('src')).toContain('logo-primary.svg');
    expect(logo).toHaveClass('h-5', 'w-auto', 'max-w-full', 'shrink-0');
    expect(logo).toHaveAttribute('height', '20');
  });

  it('renders the color timeline mark in dark theme', () => {
    mockedUseTheme.mockReturnValue({
      theme: 'dark',
      resolvedTheme: 'dark',
      setTheme: vi.fn(),
      toggleTheme: vi.fn(),
    });
    render(<NexusLogo />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo.getAttribute('src')).toContain('logo-color.svg');
    expect(logo).toHaveClass('w-auto');
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
