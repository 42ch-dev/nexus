import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  SurfaceSourceBadge,
  SurfaceSourceBadges,
  classifySurfaceImport,
  getSurfaceSourceLabel,
} from '../surface-source-badge';

describe('classifySurfaceImport', () => {
  it('classifies @42ch/nexus-ui as promoted', () => {
    expect(classifySurfaceImport('@42ch/nexus-ui')).toBe('promoted');
    expect(classifySurfaceImport('@42ch/nexus-ui/button')).toBe('promoted');
  });

  it('classifies @web-ui as transitional', () => {
    expect(classifySurfaceImport('@web-ui/dialog')).toBe('transitional');
  });

  it('classifies other @web-* roots as extract', () => {
    expect(classifySurfaceImport('@web-layout/shell-sidebar-chrome')).toBe(
      'extract',
    );
    expect(classifySurfaceImport('@web-canvas/nle-timeline-chrome')).toBe(
      'extract',
    );
    expect(classifySurfaceImport('@web-setup/agent-picker')).toBe('extract');
    expect(classifySurfaceImport('@web-shell/selection-submenu')).toBe(
      'extract',
    );
  });
});

describe('getSurfaceSourceLabel', () => {
  it('includes import path by default', () => {
    expect(getSurfaceSourceLabel('@web-canvas/layer-breadcrumb')).toBe(
      'App presentational extract (@web-canvas/layer-breadcrumb)',
    );
    expect(getSurfaceSourceLabel('@42ch/nexus-ui')).toBe(
      'Promoted primitive (@42ch/nexus-ui)',
    );
  });

  it('can omit the import path', () => {
    expect(
      getSurfaceSourceLabel('@web-ui/dialog', { includePath: false }),
    ).toBe('Transitional primitive');
  });
});

describe('SurfaceSourceBadge', () => {
  it('renders extract tier with test id and import path', () => {
    render(
      <SurfaceSourceBadge importPath="@web-layout/creator-shell-content" />,
    );

    const badge = screen.getByTestId('surface-source-badge-extract');
    expect(badge).toHaveAttribute(
      'data-import-path',
      '@web-layout/creator-shell-content',
    );
    expect(badge).toHaveTextContent('App presentational extract');
    expect(badge).toHaveTextContent('@web-layout/creator-shell-content');
  });

  it('renders promoted tier', () => {
    render(<SurfaceSourceBadge importPath="@42ch/nexus-ui" />);
    expect(screen.getByTestId('surface-source-badge-promoted')).toBeInTheDocument();
  });

  it('renders transitional tier', () => {
    render(<SurfaceSourceBadge importPath="@web-ui/dialog" />);
    expect(screen.getByTestId('surface-source-badge-transitional')).toBeInTheDocument();
  });
});

describe('SurfaceSourceBadges', () => {
  it('deduplicates import paths', () => {
    render(
      <SurfaceSourceBadges
        importPaths={[
          '@web-canvas/node-chrome-shell',
          '@web-canvas/node-chrome-shell',
          '@42ch/nexus-ui',
        ]}
      />,
    );

    const row = screen.getByTestId('surface-source-badges');
    expect(within(row).getAllByTestId('surface-source-badge-extract')).toHaveLength(
      1,
    );
    expect(screen.getByTestId('surface-source-badge-promoted')).toBeInTheDocument();
  });

  it('returns null when no paths are provided', () => {
    const { container } = render(<SurfaceSourceBadges importPaths={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});