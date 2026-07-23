/**
 * Shared Studio chrome helpers for hub sidebar-create IA fixtures.
 */
import { useRef, useState, type KeyboardEvent } from 'react';

import {
  FooterProfilesChrome,
  type FooterProfile,
} from '@web-layout/footer-profiles-chrome';

export function FixtureWorkspaceFooter({ testId }: { testId?: string }) {
  const [activeId, setActiveId] = useState('local-creator');
  const [focusIndex, setFocusIndex] = useState(0);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const addRef = useRef<HTMLButtonElement | null>(null);

  const profiles: FooterProfile[] = [
    {
      id: 'local-creator',
      displayName: '本地创作者',
      active: activeId === 'local-creator',
    },
  ];
  const total = profiles.length + 1;

  function focusAt(index: number) {
    const next = Math.max(0, Math.min(total - 1, index));
    const el = next === profiles.length ? addRef.current : itemRefs.current[next];
    el?.focus();
    setFocusIndex(next);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
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
      default:
        break;
    }
  }

  return (
    <div data-testid={testId ?? 'hub-sidebar-fixture-workspace-footer'}>
      <FooterProfilesChrome
        sectionLabel="工作区"
        addButtonLabel="添加创作者"
        profiles={profiles}
        focusIndex={focusIndex}
        onSelect={setActiveId}
        onAdd={() => {}}
        onFocus={setFocusIndex}
        onKeyDown={handleKeyDown}
        onItemRef={(index, el) => {
          itemRefs.current[index] = el;
        }}
        onAddRef={(el) => {
          addRef.current = el;
        }}
      />
    </div>
  );
}
