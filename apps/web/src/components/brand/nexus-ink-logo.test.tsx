import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { NexusInkLogo } from './nexus-ink-logo';

describe('NexusInkLogo', () => {
  it('renders a non-draggable bright mark for ink surfaces', () => {
    render(<NexusInkLogo />);

    expect(screen.getByRole('img', { name: 'Nexus' })).toHaveAttribute('draggable', 'false');
  });
});
