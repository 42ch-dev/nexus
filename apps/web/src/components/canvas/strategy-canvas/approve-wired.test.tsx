/**
 * Approve wiring (v239-p3-canvas-compose-ui T6): what the approve response
 * renders — the frozen snapshot (approval id, exact pins, simulation
 * identity, candidate revision) and the no-publish statement — and the named
 * refusal copy for the cases that refuse. The flag-off surface passes none of
 * this, keeping the shipped disabled disclosure (T7).
 */
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { NexusClientError } from '@/lib/nexus';
import type { ApprovedStrategySnapshot } from '@42ch/nexus-contracts';

import { renderInApp } from '@/test/test-providers';
import { ApproveDisclosure } from './approve-disclosure';
import { approveRefusalMessage } from './hooks/use-approve-strategy';
import { candidateDocument, simulationTrajectory } from '@/test/simulation-fixture';

function snapshot(): ApprovedStrategySnapshot {
  return {
    approval_id: 'apr_123',
    strategy_id: 'user/demo',
    candidate_revision: 3,
    document: candidateDocument({ revision: 3 }),
    content_hash: 'hash-3',
    trajectory: simulationTrajectory({ ir_content_hash: 'hash-3' }),
    pins: [{ id: 'prompt-step', version: '1.0.0', content_hash: 'abc123' }],
    approver: 'creator-1',
    approved_at: '2026-11-13T00:00:00Z',
  };
}

function renderDisclosure(props: Partial<Parameters<typeof ApproveDisclosure>[0]> = {}) {
  return renderInApp(
    <ApproveDisclosure
      open
      onOpenChange={() => {}}
      status="ready"
      trajectory={simulationTrajectory()}
      error={null}
      onApprove={() => {}}
      {...props}
    />,
  );
}

describe('approveRefusalMessage', () => {
  it('names the slot and pattern for slot_unbound', () => {
    const error = new NexusClientError(422, 'invalid_input', 'refused', {
      code: 'approval_refused',
      reason: 'slot_unbound',
      slot: 'topic',
      package_id: 'prompt-step',
      package_version: '1.0.0',
    });
    expect(approveRefusalMessage(error)).toBe(
      'Bind slot topic on prompt-step before approval.',
    );
  });

  it('names both hashes for pin_changed', () => {
    const error = new NexusClientError(422, 'invalid_input', 'refused', {
      code: 'approval_refused',
      reason: 'pin_changed',
      package_id: 'prompt-step',
      package_version: '1.0.0',
      recorded_content_hash: 'aaa',
      scanned_content_hash: 'bbb',
    });
    expect(approveRefusalMessage(error)).toBe(
      'Pattern prompt-step@1.0.0 changed: recorded aaa, current bbb. Recompose and simulate again before approval.',
    );
  });

  it('falls back to the wire message for other refusals', () => {
    const error = new NexusClientError(422, 'invalid_input', 'Simulation is stale.', {
      reason: 'simulation_stale',
    });
    expect(approveRefusalMessage(error)).toBe('Simulation is stale.');
  });
});

describe('ApproveDisclosure — wired approve (compose path)', () => {
  it('renders the frozen snapshot: approval id, exact pins, simulation identity, no-publish', () => {
    renderDisclosure({ approval: { kind: 'approved', snapshot: snapshot() } });
    expect(screen.getByTestId('approve-approved')).toBeInTheDocument();
    expect(screen.getByTestId('approve-approved')).toHaveTextContent('apr_123');
    expect(screen.getByTestId('approve-pin')).toHaveTextContent('prompt-step@1.0.0');
    expect(screen.getByTestId('approve-pin')).toHaveTextContent('abc123');
    expect(screen.getByTestId('approve-simulation-identity')).toHaveTextContent('hash-3');
    expect(screen.getByTestId('approve-approved')).toHaveTextContent('does not publish, apply');
  });

  it('renders a named refusal with its recovery copy', () => {
    renderDisclosure({
      approval: { kind: 'refused', message: 'Bind slot topic on prompt-step before approval.' },
    });
    expect(screen.getByTestId('approve-refused')).toHaveTextContent(
      'Bind slot topic on prompt-step before approval.',
    );
  });

  it('disables Approve with the gate reason while a required slot is unbound', () => {
    renderDisclosure({ gateReason: 'Bind slot topic on prompt-step before approval.' });
    expect(screen.getByTestId('approve-gate')).toHaveTextContent(
      'Bind slot topic on prompt-step before approval.',
    );
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
  });

  it('keeps the reason-bearing disabled disclosure when no handler exists (flag off)', () => {
    renderDisclosure({ onApprove: undefined, approval: null });
    expect(screen.getByTestId('approve-unavailable')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
    expect(screen.queryByTestId('approve-approved')).not.toBeInTheDocument();
  });
});
