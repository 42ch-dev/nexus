import { useEffect, useState, type FormEvent } from 'react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label, Textarea } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useToast } from '@/lib/use-toast';
import { useCreateWork } from '@/api/queries';

/**
 * Create Work dialog — POST /v1/local/works.
 *
 * The contract `CreateWorkRequest` requires title + long_term_goal +
 * initial_idea. Note: the Local API does not accept `work_profile` at creation
 * (the daemon assigns profiles internally), so the UI offers the foundational
 * fields only. Tracked as a residual for a future profile-aware create.
 *
 * DESIGN.md §Voice & Content: Verb + Noun action ("Create Work"); loading
 * state uses present participle.
 */
export function CreateWorkDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated?: (workId: string) => void;
}) {
  const create = useCreateWork();
  const { toast } = useToast();
  const [title, setTitle] = useState('');
  const [longTermGoal, setLongTermGoal] = useState('');
  const [initialIdea, setInitialIdea] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Reset the form whenever the dialog opens.
  useEffect(() => {
    if (open) {
      setTitle('');
      setLongTermGoal('');
      setInitialIdea('');
      setError(null);
    }
  }, [open]);

  const valid = title.trim().length > 0 && longTermGoal.trim().length > 0 && initialIdea.trim().length > 0;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!valid) {
      setError('Title, long-term goal, and initial idea are required.');
      return;
    }
    try {
      const res = await create.mutateAsync({
        title: title.trim(),
        long_term_goal: longTermGoal.trim(),
        initial_idea: initialIdea.trim(),
      });
      toast({ variant: 'success', title: 'Work created', description: res.work_id });
      onOpenChange(false);
      onCreated?.(res.work_id);
    } catch {
      // Error toast already fired by the mutation's onError callback.
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Create Work"
        description="Start a new creative Work in the local runtime."
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-title">Title</Label>
            <Input
              id="work-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="The Work's name"
              invalid={Boolean(error) && title.trim().length === 0}
              autoFocus
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-goal">Long-term goal</Label>
            <Textarea
              id="work-goal"
              value={longTermGoal}
              onChange={(e) => setLongTermGoal(e.target.value)}
              placeholder="Where this Work is heading"
              invalid={Boolean(error) && longTermGoal.trim().length === 0}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-idea">Initial idea</Label>
            <Textarea
              id="work-idea"
              value={initialIdea}
              onChange={(e) => setInitialIdea(e.target.value)}
              placeholder="The seed the runtime will build on"
              invalid={Boolean(error) && initialIdea.trim().length === 0}
            />
          </div>
          {error && <p className="text-copy-13 text-red-700">{error}</p>}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!valid || create.isPending}>
              {create.isPending ? 'Creating Work…' : 'Create Work'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
