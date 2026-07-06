import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';

import { Button } from './button';

describe('Button', () => {
  it('matches the primary variant snapshot in light mode', () => {
    const { container } = render(<Button variant="primary">Continue</Button>);
    expect(container.firstChild).toMatchSnapshot();
  });

  it('matches the primary variant snapshot in dark mode', () => {
    document.documentElement.classList.add('dark');
    const { container } = render(<Button variant="primary">Continue</Button>);
    expect(container.firstChild).toMatchSnapshot();
    document.documentElement.classList.remove('dark');
  });
});
