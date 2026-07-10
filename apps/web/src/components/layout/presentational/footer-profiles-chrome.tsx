import { Plus } from 'lucide-react';
import { forwardRef, type KeyboardEvent } from 'react';

import { cn } from '@/lib/utils';

export interface FooterProfile {
  id: string;
  displayName: string;
  active?: boolean;
}

export interface FooterProfilesChromeProps {
  /** Section label above the toolbar (e.g. "Profiles"). */
  sectionLabel: string;
  /** Accessible name for the add-profile button. */
  addButtonLabel: string;
  profiles: FooterProfile[];
  /** Focused item index in the roving-tabindex sequence. */
  focusIndex: number;
  /** Optional active-profile display name shown below the toolbar. */
  activeDisplayName?: string;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onFocus: (index: number) => void;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onItemRef: (index: number, el: HTMLButtonElement | null) => void;
  onAddRef: (el: HTMLButtonElement | null) => void;
}

/**
 * Presentational profile-switcher chrome — Slack/Chrome-style avatar row.
 *
 * The host owns the data, active state, dialog orchestration, and keyboard
 * focus management. This component renders the markup and attaches the
 * supplied callbacks/refs so the wrapper can implement roving tabindex.
 */
export function FooterProfilesChrome({
  sectionLabel,
  addButtonLabel,
  profiles,
  focusIndex,
  activeDisplayName,
  onSelect,
  onAdd,
  onFocus,
  onKeyDown,
  onItemRef,
  onAddRef,
}: FooterProfilesChromeProps) {
  return (
    <div className="flex flex-col gap-2">
      <span className="px-3 text-label-12 font-medium uppercase tracking-wide text-gray-700">
        {sectionLabel}
      </span>
      <div
        role="toolbar"
        aria-label={sectionLabel}
        className="flex items-center gap-2 px-3"
        onKeyDown={onKeyDown}
      >
        {profiles.map((profile, index) => (
          <ProfileAvatar
            key={profile.id}
            profile={profile}
            tabIndex={focusIndex === index ? 0 : -1}
            onFocus={() => onFocus(index)}
            onSelect={() => onSelect(profile.id)}
            ref={(el) => {
              onItemRef(index, el);
            }}
          />
        ))}
        <button
          ref={(el) => {
            onAddRef(el);
          }}
          type="button"
          tabIndex={focusIndex === profiles.length ? 0 : -1}
          onFocus={() => onFocus(profiles.length)}
          onClick={onAdd}
          aria-label={addButtonLabel}
          className={cn(
            'flex h-8 w-8 items-center justify-center rounded-full border border-dashed transition-colors',
            'border-footer-profile-add-button-border bg-footer-profile-add-button-bg text-footer-profile-add-button-text',
            'hover:bg-footer-profile-add-button-hover-bg hover:border-footer-profile-add-button-hover-border hover:text-footer-profile-add-button-hover-text',
          )}
        >
          <Plus className="h-4 w-4" aria-hidden />
        </button>
      </div>
      {activeDisplayName ? (
        <div className="flex items-center gap-2 px-3">
          <span className="text-label-14 text-gray-1000 truncate">
            {activeDisplayName}
          </span>
        </div>
      ) : null}
    </div>
  );
}

interface ProfileAvatarProps {
  profile: FooterProfile;
  tabIndex: number;
  onFocus: () => void;
  onSelect: () => void;
}

const ProfileAvatar = forwardRef<HTMLButtonElement, ProfileAvatarProps>(
  ({ profile, tabIndex, onFocus, onSelect }, ref) => {
    const initials = profile.displayName.slice(0, 1).toUpperCase();
    return (
      <button
        ref={ref}
        type="button"
        tabIndex={tabIndex}
        onClick={onSelect}
        onFocus={onFocus}
        aria-pressed={profile.active}
        title={profile.displayName}
        className={cn(
          'flex h-8 w-8 items-center justify-center rounded-full text-button-14 font-button transition-colors',
          profile.active
            ? 'bg-footer-profile-avatar-bg-active text-footer-profile-avatar-text-active'
            : 'bg-footer-profile-avatar-bg text-footer-profile-avatar-text hover:bg-footer-profile-avatar-bg-hover',
        )}
      >
        {initials}
      </button>
    );
  },
);
ProfileAvatar.displayName = 'ProfileAvatar';
