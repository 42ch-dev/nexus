import { useEffect, useState, type FormEvent } from 'react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label, Select, Textarea } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useToast } from '@/lib/use-toast';
import { useCreateWork } from '@/api/queries';

/**
 * Work-profile options for the selector. The `value` is the wire identifier
 * sent to the daemon and MUST match the backend canonical set — the daemon
 * HTTP handler stores `work_profile` verbatim (no normalization; see
 * `crates/nexus-daemon-runtime/src/api/handlers/works.rs` create_work). The
 * authoritative set is the DB CHECK constraint in
 * `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql`
 * and the Rust helpers in `crates/nexus-local-db/src/works.rs` —
 * `game_bible` uses an underscore. Exported for the wire-contract test.
 * Extraction to a SSOT module is tracked as R-V167P1-QC1-S2.
 */
export const WORK_PROFILES = [
  { value: 'novel', label: 'Novel' },
  { value: 'essay', label: 'Essay' },
  { value: 'game_bible', label: 'Game Bible' },
  { value: 'script', label: 'Script' },
] as const;

/**
 * Create Work dialog — POST /v1/local/works.
 *
 * The contract `CreateWorkRequest` requires title + long_term_goal +
 * initial_idea and accepts an optional `work_profile` (V1.67 G1; the wire
 * field already existed — the daemon assigned profiles internally before).
 * The selector defaults to `novel` for display, but `work_profile` is only
 * sent when the author explicitly chooses a profile — an untouched form
 * omits the field (daemon stores NULL), preserving the V1.66 wire shape
 * (qc1 W1). DESIGN.md §Voice & Content: Verb + Noun action ("Create Work");
 * loading state uses present participle.
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
  const [workProfile, setWorkProfile] = useState<string>(WORK_PROFILES[0].value);
  // W1: track whether the author explicitly touched the selector. Untouched
  // forms omit `work_profile` so the daemon stores NULL (V1.66 semantics).
  const [workProfileTouched, setWorkProfileTouched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset the form whenever the dialog opens.
  useEffect(() => {
    if (open) {
      setTitle('');
      setLongTermGoal('');
      setInitialIdea('');
      setWorkProfile(WORK_PROFILES[0].value);
      setWorkProfileTouched(false);
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
        ...(workProfileTouched ? { work_profile: workProfile } : {}),
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
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-profile">Work profile</Label>
            <Select
              id="work-profile"
              value={workProfile}
              onChange={(e) => {
                setWorkProfile(e.target.value);
                setWorkProfileTouched(true);
              }}
            >
              {WORK_PROFILES.map((profile) => (
                <option key={profile.value} value={profile.value}>
                  {profile.label}
                </option>
              ))}
            </Select>
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
