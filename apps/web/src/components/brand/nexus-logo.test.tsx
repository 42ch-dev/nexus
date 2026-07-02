import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { useTheme } from '@/components/theme-provider';

vi.mock('@/components/theme-provider', () => ({
  useTheme: vi.fn(),
}));

const mockedUseTheme = vi.mocked(useTheme);

describe('NexusLogo', () => {
  beforeEach(() => {
    mockedUseTheme.mockReturnValue({
      theme: 'light',
      setTheme: vi.fn(),
      toggleTheme: vi.fn(),
    });
  });

  it('renders the deep-blue mark in light theme', () => {
    mockedUseTheme.mockReturnValue({
      theme: 'light',
      setTheme: vi.fn(),
      toggleTheme: vi.fn(),
    });
    render(<NexusLogo />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo.getAttribute('src')).toContain('%231E3A5F');
  });

  it('renders the cyan mark in dark theme', () => {
    mockedUseTheme.mockReturnValue({
      theme: 'dark',
      setTheme: vi.fn(),
      toggleTheme: vi.fn(),
    });
    render(<NexusLogo />);
    const logo = screen.getByRole('img', { name: 'Nexus' });
    expect(logo.getAttribute('src')).toContain('%2325D1E0');
  });

  it('honors a custom accessible label', () => {
    render(<NexusLogo label="Nexus local workspace" />);
    expect(screen.getByRole('img', { name: 'Nexus local workspace' })).toBeInTheDocument();
  });
});
