/**
 * Findings lifecycle adjacency — client-side mirror of the DAO 6-state machine.
 *
 * Pins the transition table that the UI uses to disable illegal transitions
 * (defense-in-depth; the server is the authority — HTTP 422
 * `INVALID_TRANSITION`). Spec: `findings-lifecycle.md` §2.2. Keep in sync with
 * `crates/nexus-local-db/src/findings.rs:172` (`is_valid_transition()`).
 */
import { describe, expect, it } from 'vitest';

import {
  FINDING_STATUSES,
  TERMINAL_FINDING_STATUSES,
  isTerminalStatus,
  isValidTransition,
  nextStatuses,
} from '@/lib/findings-lifecycle';

describe('findings lifecycle — 6-state status machine', () => {
  it('exposes exactly the 6 DAO statuses', () => {
    expect(FINDING_STATUSES).toEqual([
      'open',
      'triaged',
      'in_review',
      'resolved',
      'wont_fix',
      'duplicate',
    ]);
  });

  it('open may advance to every other status', () => {
    expect(nextStatuses('open')).toEqual([
      'triaged',
      'in_review',
      'resolved',
      'wont_fix',
      'duplicate',
    ]);
  });

  it('triaged may advance to in_review and the three terminals', () => {
    expect(nextStatuses('triaged')).toEqual([
      'in_review',
      'resolved',
      'wont_fix',
      'duplicate',
    ]);
  });

  it('in_review may only resolve, waive, or mark duplicate', () => {
    expect(nextStatuses('in_review')).toEqual(['resolved', 'wont_fix', 'duplicate']);
  });

  it.each(['resolved', 'wont_fix', 'duplicate'] as const)('%s is terminal (no outbound transitions)', (status) => {
    expect(nextStatuses(status)).toEqual([]);
    expect(isTerminalStatus(status)).toBe(true);
    expect(TERMINAL_FINDING_STATUSES.has(status)).toBe(true);
  });

  it('rejects self-transitions (status: "<current>" is INVALID_TRANSITION)', () => {
    for (const s of FINDING_STATUSES) {
      expect(isValidTransition(s, s)).toBe(false);
    }
  });

  it('rejects backward / non-adjacent transitions', () => {
    // resolved/wont_fix/duplicate cannot reach anything (terminal).
    expect(isValidTransition('resolved', 'open')).toBe(false);
    expect(isValidTransition('wont_fix', 'triaged')).toBe(false);
    expect(isValidTransition('duplicate', 'resolved')).toBe(false);
    // triaged cannot go back to open.
    expect(isValidTransition('triaged', 'open')).toBe(false);
    // in_review cannot go back to open or triaged.
    expect(isValidTransition('in_review', 'open')).toBe(false);
    expect(isValidTransition('in_review', 'triaged')).toBe(false);
  });

  it('accepts every adjacency row from the DAO table', () => {
    expect(isValidTransition('open', 'triaged')).toBe(true);
    expect(isValidTransition('open', 'in_review')).toBe(true);
    expect(isValidTransition('open', 'resolved')).toBe(true);
    expect(isValidTransition('triaged', 'in_review')).toBe(true);
    expect(isValidTransition('triaged', 'resolved')).toBe(true);
    expect(isValidTransition('in_review', 'resolved')).toBe(true);
    expect(isValidTransition('in_review', 'wont_fix')).toBe(true);
  });

  it('golden: client adjacency table matches the DAO is_valid_transition source', () => {
    // Hand-transcribed from crates/nexus-local-db/src/findings.rs:172-189.
    // Self-transitions and unknown endpoints are rejected; terminal statuses
    // have no outbound edges.
    const daoTransition = (from: string, to: string): boolean => {
      if (from === to) return false;
      switch (from) {
        case 'open':
          return ['triaged', 'in_review', 'resolved', 'wont_fix', 'duplicate'].includes(to);
        case 'triaged':
          return ['in_review', 'resolved', 'wont_fix', 'duplicate'].includes(to);
        case 'in_review':
          return ['resolved', 'wont_fix', 'duplicate'].includes(to);
        case 'resolved':
        case 'wont_fix':
        case 'duplicate':
          return false;
        default:
          return false;
      }
    };
    for (const from of FINDING_STATUSES) {
      for (const to of FINDING_STATUSES) {
        expect(isValidTransition(from, to)).toBe(daoTransition(from, to));
      }
    }
  });

  it('handles undefined / unknown statuses defensively', () => {
    expect(isValidTransition(undefined, 'open')).toBe(false);
    expect(isValidTransition('open', undefined)).toBe(false);
    expect(isValidTransition('unknown', 'open')).toBe(false);
    expect(nextStatuses(undefined)).toEqual([]);
    expect(isTerminalStatus(undefined)).toBe(false);
    expect(isTerminalStatus('unknown')).toBe(false);
  });
});
