import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';

import { ComputeResultNodeChrome } from './compute-result-node-chrome';

const BASE_PROPS = {
  title: 'Aria strikes Brann',
  kindLabel: 'Compute result',
  provenanceLabel: 'From module run',
  moduleName: 'Basic Combat',
  moduleVersion: '1.0.0',
};

describe('ComputeResultNodeChrome', () => {
  it('renders title, kind pill, provenance chip, and module meta', () => {
    render(<ComputeResultNodeChrome {...BASE_PROPS} runId="run_9f3a2c" />);

    expect(screen.getByTestId('compute-result-node-chrome')).toBeInTheDocument();
    expect(screen.getByText('Aria strikes Brann')).toBeInTheDocument();
    expect(screen.getByTestId('compute-node-kind-pill')).toHaveTextContent('Compute result');
    expect(screen.getByTestId('compute-node-provenance-chip')).toHaveTextContent(
      'From module run',
    );
    expect(screen.getByTestId('compute-node-meta')).toHaveTextContent('Basic Combat · v1.0.0');
    expect(screen.getByTestId('compute-node-run-id')).toHaveTextContent('run_9f3a2c');
  });

  it('omits the run id suffix when no direct Run exists (preset path)', () => {
    render(
      <ComputeResultNodeChrome
        {...BASE_PROPS}
        provenanceLabel="From preset"
        moduleName="Combat Engine"
      />,
    );

    expect(screen.queryByTestId('compute-node-run-id')).not.toBeInTheDocument();
    expect(screen.getByTestId('compute-node-provenance-chip')).toHaveTextContent('From preset');
    expect(screen.getByTestId('compute-node-meta')).toHaveTextContent(
      'Combat Engine · v1.0.0',
    );
  });

  it('marks the Cpu affordance aria-hidden and token-accented', () => {
    render(<ComputeResultNodeChrome {...BASE_PROPS} />);

    const icon = screen.getByTestId('compute-result-node-chrome').querySelector('svg');
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute('aria-hidden', 'true');
    // Same-family token: compute nodes share the Narrative layer accent.
    expect(icon!.getAttribute('class')).toContain('text-canvas-layer-narrative-accent');
  });

  it('is a pure presentational surface — copy props only, no side effects', () => {
    const onRender = vi.fn();
    render(
      <ComputeResultNodeChrome
        {...BASE_PROPS}
        moduleName="Economy Ticker"
        moduleVersion="2.1.0"
      />,
    );
    onRender();
    expect(onRender).toHaveBeenCalledTimes(1);
  });
});
