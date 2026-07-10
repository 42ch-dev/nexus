import { useEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useCreators, useCreateCreator } from '@/api/queries';
import { useActiveCreatorId, useSetActiveCreatorId } from '@/lib/active-creator-context';
import { FooterProfilesChrome } from './presentational/footer-profiles-chrome';

/**
 * Sidebar footer profile switcher — Slack/Chrome-style avatar row.
 *
 * Thin wrapper around {@link FooterProfilesChrome}: owns the creator query,
 * active-creator context, and the create-creator dialog. The chrome owns the
 * presentational markup and data-testid SSOT.
 */
export function FooterProfiles() {
  const creators = useCreators();
  const activeCreatorId = useActiveCreatorId();
  const setActiveCreatorId = useSetActiveCreatorId();
  const [createOpen, setCreateOpen] = useState(false);
  const [focusIndex, setFocusIndex] = useState(0);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const items = creators.data?.items ?? [];
  const total = items.length + 1; // last slot is the "Add creator" button

  useEffect(() => {
    setFocusIndex((prev) => Math.min(prev, Math.max(total - 1, 0)));
  }, [total]);

  function focusAt(index: number) {
    const next = Math.max(0, Math.min(total - 1, index));
    itemRefs.current[next]?.focus();
    setFocusIndex(next);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    switch (event.key) {
      case 'ArrowRight':
        event.preventDefault();
        focusAt(focusIndex + 1);
        break;
      case 'ArrowLeft':
        event.preventDefault();
        focusAt(focusIndex - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusAt(0);
        break;
      case 'End':
        event.preventDefault();
        focusAt(total - 1);
        break;
      case 'Escape':
        event.preventDefault();
        itemRefs.current[focusIndex]?.blur();
        break;
      default:
        break;
    }
  }

  const profiles = items.map((creator) => ({
    id: creator.creator_id,
    displayName: creator.display_name,
    active: creator.creator_id === activeCreatorId,
  }));

  return (
    <>
      <FooterProfilesChrome
        sectionLabel="Profiles"
        addButtonLabel="Add creator"
        profiles={profiles}
        focusIndex={focusIndex}
        onSelect={(id) => {
          if (items.length > 1) setActiveCreatorId(id);
        }}
        onAdd={() => setCreateOpen(true)}
        onFocus={setFocusIndex}
        onKeyDown={handleKeyDown}
        onItemRef={(index, el) => {
          itemRefs.current[index] = el;
        }}
        onAddRef={(el) => {
          itemRefs.current[items.length] = el;
        }}
      />

      <CreateCreatorDialog open={createOpen} onOpenChange={setCreateOpen} />
    </>
  );
}

function CreateCreatorDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const create = useCreateCreator();
  const [displayName, setDisplayName] = useState('');

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!displayName.trim()) return;
    create.mutate(
      { display_name: displayName.trim() },
      {
        onSuccess: () => {
          setDisplayName('');
          onOpenChange(false);
        },
      },
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-md"
        title="Add Creator"
        description="Create a new local creator profile."
      >
        <form onSubmit={submit}>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="creator-name">Display name</Label>
              <Input
                id="creator-name"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                placeholder="e.g. Default Creator"
                autoFocus
              />
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="tertiary"
              onClick={() => onOpenChange(false)}
              disabled={create.isPending}
            >
              Cancel
            </Button>
            <Button type="submit" variant="primary" disabled={!displayName.trim() || create.isPending}>
              {create.isPending ? 'Creating…' : 'Create'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
