/**
 * FB-SE-004 — Keyboard edge-creation dialog (§4.4 pointer alternative).
 *
 * A Radix-based dialog that lets keyboard-only authors create a transition
 * without dragging: choose source → choose target → choose edge kind, then
 * commit with **Create Transition**. The commit routes through the same
 * `strategy.patch_transition` with `op: "create"` as the spatial path
 * (FB-SE-002). Radix Dialog handles focus trap, Escape, and return-focus
 * automatically (§4.4 a11y). Voice & Content labels are locked by the
 * primary spec.
 */
import { useEffect, useState } from 'react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input, Label, Select } from '@/components/ui';
import type { PresetState } from '@/lib/canvas/preset-yaml';

export interface KeyboardCreateArgs {
  sourceStateId: string;
  targetStateId: string;
  transitionKind: 'next' | 'branch' | 'default';
  condition?: string;
  label?: string;
}

export interface EdgeCreateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  states: PresetState[];
  onCommit: (args: KeyboardCreateArgs) => void;
  isCommitting: boolean;
}

const EDGE_KIND_OPTIONS = [
  { value: 'next', label: 'Linear (next)' },
  { value: 'branch', label: 'Branch (conditional)' },
  { value: 'default', label: 'Default' },
] as const;

export function EdgeCreateDialog({
  open,
  onOpenChange,
  states,
  onCommit,
  isCommitting,
}: EdgeCreateDialogProps) {
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [kind, setKind] = useState<'next' | 'branch' | 'default'>('next');
  const [condition, setCondition] = useState('');
  const [labelText, setLabelText] = useState('');

  // Reset the form each time the dialog opens.
  useEffect(() => {
    if (open) {
      setSourceId('');
      setTargetId('');
      setKind('next');
      setCondition('');
      setLabelText('');
    }
  }, [open]);

  // If the source changes and the target is no longer valid, clear target.
  const targetOptions = states.filter((s) => s.id !== sourceId);
  if (targetId && targetId === sourceId) {
    setTargetId('');
  }

  const canCommit = sourceId !== '' && targetId !== '' && !isCommitting;

  function handleCommit() {
    if (!canCommit) return;
    const args: KeyboardCreateArgs = {
      sourceStateId: sourceId,
      targetStateId: targetId,
      transitionKind: kind,
    };
    if (condition.trim()) args.condition = condition.trim();
    if (labelText.trim()) args.label = labelText.trim();
    onCommit(args);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Create Transition" description="Add a transition between two states.">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            handleCommit();
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-source">Choose source</Label>
            <Select
              id="edge-create-source"
              value={sourceId}
              onChange={(e) => setSourceId(e.target.value)}
              autoFocus
            >
              <option value="" disabled>
                Select a source state…
              </option>
              {states.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.id}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-target">Choose target</Label>
            <Select
              id="edge-create-target"
              value={targetId}
              onChange={(e) => setTargetId(e.target.value)}
              disabled={sourceId === ''}
            >
              <option value="" disabled>
                {sourceId === '' ? 'Choose a source first…' : 'Select a target state…'}
              </option>
              {targetOptions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.id}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-kind">Choose edge kind</Label>
            <Select id="edge-create-kind" value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
              {EDGE_KIND_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-condition">Condition</Label>
            <Input
              id="edge-create-condition"
              value={condition}
              onChange={(e) => setCondition(e.target.value)}
              placeholder="e.g. word_count > 1000"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-label">Label</Label>
            <Input
              id="edge-create-label"
              value={labelText}
              onChange={(e) => setLabelText(e.target.value)}
              placeholder="Optional transition label"
            />
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => onOpenChange(false)}
              disabled={isCommitting}
            >
              Cancel
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!canCommit}>
              {isCommitting ? 'Creating…' : 'Create Transition'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
