import { forwardRef, useEffect, useRef, useState } from 'react';
import { Plus } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useCreators, useCreateCreator } from '@/api/queries';
import { useActiveCreatorId, useSetActiveCreatorId } from '@/lib/active-creator-context';
import { cn } from '@/lib/utils';
import type { CreatorInfo } from '@42ch/nexus-contracts';

/**
 * Sidebar footer profile switcher — Slack/Chrome-style avatar row.
 *
 * Renders one avatar per local creator plus a "+" affordance to create a new
 * creator. Click/keyboard switches the active creator id stored in client
 * context (persisted to localStorage / Tauri store). Single-creator case:
 * clicking the lone avatar is a no-op.
 *
 * Keyboard contract (web-ui.md §29.5): ArrowLeft/ArrowRight move focus between
 * avatars; Home/End jump to first/last; Escape moves focus out of the toolbar.
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

  return (
    <div className="flex flex-col gap-2">
      <span className="px-3 text-label-12 font-medium uppercase tracking-wide text-gray-700">
        Profiles
      </span>
      <div
        role="toolbar"
        aria-label="Profiles"
        className="flex items-center gap-2 px-3"
        onKeyDown={handleKeyDown}
      >
        {items.map((creator, index) => (
          <CreatorAvatar
            key={creator.creator_id}
            ref={(el) => {
              itemRefs.current[index] = el;
            }}
            creator={creator}
            active={creator.creator_id === activeCreatorId}
            tabIndex={focusIndex === index ? 0 : -1}
            onFocus={() => setFocusIndex(index)}
            onSelect={() => {
              if (items.length > 1) setActiveCreatorId(creator.creator_id);
            }}
          />
        ))}
        <button
          ref={(el) => {
            itemRefs.current[items.length] = el;
          }}
          type="button"
          tabIndex={focusIndex === items.length ? 0 : -1}
          onFocus={() => setFocusIndex(items.length)}
          onClick={() => setCreateOpen(true)}
          aria-label="Add creator"
          className={cn(
            'flex h-8 w-8 items-center justify-center rounded-full border border-dashed transition-colors',
            'border-footer-profile-add-button-border bg-footer-profile-add-button-bg text-footer-profile-add-button-text',
            'hover:bg-footer-profile-add-button-hover-bg hover:border-footer-profile-add-button-hover-border hover:text-footer-profile-add-button-hover-text',
          )}
        >
          <Plus className="h-4 w-4" aria-hidden />
        </button>
      </div>

      <CreateCreatorDialog open={createOpen} onOpenChange={setCreateOpen} />
    </div>
  );
}

interface CreatorAvatarProps {
  creator: CreatorInfo;
  active: boolean;
  tabIndex: number;
  onFocus: () => void;
  onSelect: () => void;
}

const CreatorAvatar = forwardRef<HTMLButtonElement, CreatorAvatarProps>(
  ({ creator, active, tabIndex, onFocus, onSelect }, ref) => {
    const initials = creator.display_name.slice(0, 1).toUpperCase();
    return (
      <button
        ref={ref}
        type="button"
        tabIndex={tabIndex}
        onClick={onSelect}
        onFocus={onFocus}
        aria-pressed={active}
        title={creator.display_name}
        className={cn(
          'flex h-8 w-8 items-center justify-center rounded-full text-button-14 font-button transition-colors',
          active
            ? 'bg-footer-profile-avatar-bg-active text-footer-profile-avatar-text-active'
            : 'bg-footer-profile-avatar-bg text-footer-profile-avatar-text hover:bg-footer-profile-avatar-bg-hover',
        )}
      >
        {initials}
      </button>
    );
  },
);
CreatorAvatar.displayName = 'CreatorAvatar';

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
