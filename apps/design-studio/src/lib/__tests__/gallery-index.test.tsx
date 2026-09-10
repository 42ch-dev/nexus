import { act } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { focusGalleryHeading, getGalleryLabel } from '@/lib/gallery-index';

describe('focusGalleryHeading', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('retries until the target heading mounts and focuses it with tabIndex=-1', async () => {
    const cancel = focusGalleryHeading('delayed-surfaces-heading');

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });

    expect(document.getElementById('delayed-surfaces-heading')).toBeNull();

    const heading = document.createElement('h3');
    heading.id = 'delayed-surfaces-heading';
    heading.textContent = 'Canvas — Three mirrored surfaces + shared chrome';
    document.body.appendChild(heading);

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100));
    });

    expect(heading).toHaveAttribute('tabindex', '-1');
    expect(document.activeElement).toBe(heading);

    cancel();
  });

  it('stops retrying when cancelled before the target appears', async () => {
    const cancel = focusGalleryHeading('never-mounted');

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    cancel();

    const heading = document.createElement('h3');
    heading.id = 'never-mounted';
    document.body.appendChild(heading);

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100));
    });

    expect(document.activeElement).not.toBe(heading);
  });
});

describe('getGalleryLabel', () => {
  it('returns frozen route labels for pair-frame titles', () => {
    expect(getGalleryLabel('/surfaces/canvas')).toBe('Canvas');
    expect(getGalleryLabel('/unknown')).toBe('Gallery');
  });
});
