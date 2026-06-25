import { useEffect, useState, type FormEvent } from 'react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { usePatchWork } from '@/api/queries';
import type { WorkDetailResponse } from '@42ch/nexus-contracts';

/**
 * Patch Work dialog — PATCH /v1/local/works/{work_id}.
 *
 * Surfaces the status/stage fields authors change most (plan T7). The contract
 * uses free-strings (no enum), so inputs are free-text with the current value
 * pre-filled. Only non-empty deltas are sent.
 */
const STAGE_HINT =
  'Statuses are free-text in the runtime (e.g. intake, active, completed, paused).';

export function PatchWorkDialog({
  work,
  open,
  onOpenChange,
}: {
  work: WorkDetailResponse;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const patch = usePatchWork();
  const [status, setStatus] = useState(work.status);
  const [intakeStatus, setIntakeStatus] = useState(work.intake_status);
  const [currentStage, setCurrentStage] = useState(work.current_stage);
  const [stageStatus, setStageStatus] = useState(work.stage_status);

  useEffect(() => {
    if (open) {
      setStatus(work.status);
      setIntakeStatus(work.intake_status);
      setCurrentStage(work.current_stage);
      setStageStatus(work.stage_status);
    }
  }, [open, work]);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const request: Record<string, string> = {};
    if (status.trim() && status !== work.status) request.status = status.trim();
    if (intakeStatus.trim() && intakeStatus !== work.intake_status)
      request.intake_status = intakeStatus.trim();
    if (currentStage.trim() && currentStage !== work.current_stage)
      request.current_stage = currentStage.trim();
    if (stageStatus.trim() && stageStatus !== work.stage_status)
      request.stage_status = stageStatus.trim();
    if (Object.keys(request).length === 0) {
      onOpenChange(false);
      return;
    }
    await patch.mutateAsync({ workId: work.work_id, request });
    onOpenChange(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Update Work" description={work.title}>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="patch-status">Status</Label>
            <Input
              id="patch-status"
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              placeholder={work.status}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="patch-intake">Intake status</Label>
            <Input
              id="patch-intake"
              value={intakeStatus}
              onChange={(e) => setIntakeStatus(e.target.value)}
              placeholder={work.intake_status}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="patch-stage">Current stage</Label>
            <Input
              id="patch-stage"
              value={currentStage}
              onChange={(e) => setCurrentStage(e.target.value)}
              placeholder={work.current_stage}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="patch-stage-status">Stage status</Label>
            <Input
              id="patch-stage-status"
              value={stageStatus}
              onChange={(e) => setStageStatus(e.target.value)}
              placeholder={work.stage_status}
            />
          </div>
          <p className="text-copy-13 text-gray-700">{STAGE_HINT}</p>
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={patch.isPending}>
              {patch.isPending ? 'Updating Work…' : 'Update Work'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
