import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';

import { ComputeInspectorSections } from './compute-inspector-sections';

const COPY = {
  moduleTitle: 'Module',
  reportTitle: 'Report',
  affectedTitle: 'Affected knowledge',
  runTitle: 'Run',
  paramsTitle: 'Parameters',
  openRunLabel: 'Open Run',
};

describe('ComputeInspectorSections', () => {
  it('renders module, report, params, affected, and run sections with provenance', () => {
    render(
      <ComputeInspectorSections
        moduleName="Basic Combat"
        moduleVersion="1.0.0"
        reportDigest="Brann takes 6 damage and staggers back."
        paramsDigest="attacker_id: char-aria · defender_id: char-brann"
        affectedEntries={[
          { id: 'char-aria', title: 'Aria' },
          { id: 'char-brann', title: 'Brann' },
        ]}
        runId="run_9f3a2c"
        provenanceLabel="From module run"
        copy={COPY}
        onOpenRun={() => undefined}
      />,
    );

    expect(screen.getByTestId('compute-inspector-section-module')).toHaveTextContent(
      'Basic Combat',
    );
    expect(screen.getByTestId('compute-inspector-module-version')).toHaveTextContent('v1.0.0');
    expect(screen.getByTestId('compute-inspector-report-digest')).toHaveTextContent(
      'Brann takes 6 damage',
    );
    expect(screen.getByTestId('compute-inspector-params-digest')).toHaveTextContent(
      'attacker_id: char-aria',
    );
    expect(screen.getByTestId('compute-inspector-affected-char-aria')).toHaveTextContent('Aria');
    expect(screen.getByTestId('compute-inspector-affected-char-brann')).toHaveTextContent(
      'Brann',
    );
    expect(screen.getByTestId('compute-inspector-run-id')).toHaveTextContent('run_9f3a2c');
    expect(screen.getByTestId('compute-inspector-provenance')).toHaveTextContent(
      'From module run',
    );
    expect(screen.getByTestId('compute-inspector-open-run')).toHaveTextContent('Open Run');
  });

  it('fires onOpenRun from the Open Run affordance', () => {
    const onOpenRun = vi.fn();
    render(
      <ComputeInspectorSections
        moduleName="Basic Combat"
        moduleVersion="1.0.0"
        runId="run_9f3a2c"
        provenanceLabel="From module run"
        copy={COPY}
        onOpenRun={onOpenRun}
      />,
    );

    fireEvent.click(screen.getByTestId('compute-inspector-open-run'));
    expect(onOpenRun).toHaveBeenCalledTimes(1);
  });

  it('hides report/params/affected/run sections when the event is sparse', () => {
    render(
      <ComputeInspectorSections
        moduleName="Combat Engine"
        moduleVersion="3.0.0"
        provenanceLabel="From preset"
        copy={COPY}
      />,
    );

    expect(screen.getByTestId('compute-inspector-section-module')).toBeInTheDocument();
    expect(screen.queryByTestId('compute-inspector-section-report')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compute-inspector-section-params')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compute-inspector-section-affected')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compute-inspector-section-run')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compute-inspector-open-run')).not.toBeInTheDocument();
  });

  it('always renders the provenance chip — preset nodes stay distinguishable', () => {
    render(
      <ComputeInspectorSections
        moduleName="Combat Engine"
        moduleVersion="3.0.0"
        provenanceLabel="From preset"
        copy={COPY}
      />,
    );

    // The chip lives in the Module card in every state, so the inspector can
    // tell which path produced the node even when no direct Run exists.
    expect(screen.getByTestId('compute-inspector-provenance')).toHaveTextContent('From preset');
    expect(screen.queryByTestId('compute-inspector-section-run')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compute-inspector-open-run')).not.toBeInTheDocument();
  });
});
