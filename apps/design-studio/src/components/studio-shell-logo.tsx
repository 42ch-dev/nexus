/**
 * Theme-aware shell mark for Design Studio fixtures.
 * Mirrors apps/web `components/brand/nexus-logo.tsx` placement rules:
 * primary on light, color on dark; mark only; wide aspect (`h-* w-auto`).
 *
 * Reads the document `.dark` class (kept in sync by Studio ThemeProvider).
 * Works in live Studio and in unit fixtures that do not mount ThemeProvider.
 */

import logoColor from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import { NexusLogo, logoMinSizePx } from '@42ch/nexus-ui';
import { useSyncExternalStore } from 'react';

function subscribeHtmlDark(onStoreChange: () => void) {
  const root = document.documentElement;
  const observer = new MutationObserver(onStoreChange);
  observer.observe(root, { attributes: true, attributeFilter: ['class'] });
  return () => observer.disconnect();
}

function getHtmlDarkSnapshot(): boolean {
  return document.documentElement.classList.contains('dark');
}

function useDocumentDark(): boolean {
  return useSyncExternalStore(subscribeHtmlDark, getHtmlDarkSnapshot, () => false);
}

export function StudioShellLogo({ className }: { className?: string } = {}) {
  const isDark = useDocumentDark();

  return (
    <NexusLogo
      variant={isDark ? 'color' : 'primary'}
      src={isDark ? logoColor : logoPrimary}
      label="Nexus"
      size={logoMinSizePx}
      className={className ?? 'h-6 w-auto shrink-0'}
    />
  );
}
