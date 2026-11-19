import { expect, it } from 'vitest';
import studioManifest from '../../../design-studio/package.json';
import webManifest from '../../package.json';

it.each([webManifest, studioManifest])(
  '$name leaves shared workspace dependency builds to pnpm',
  ({ scripts }) => {
    expect(scripts).not.toHaveProperty('prebuild');
  },
);
