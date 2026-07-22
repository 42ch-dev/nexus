/**
 * Theme-aware shell mark for Design Studio fixtures.
 * Mirrors apps/web `components/brand/nexus-logo.tsx` placement rules:
 * whiteBg on light, color on dark; mark only (no brand plate); wide aspect
 * (`h-* w-auto`).
 *
 * Reads the document `.dark` class (kept in sync by Studio ThemeProvider).
 * Works in live Studio and in unit fixtures that do not mount ThemeProvider.
 */

import logoColor from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoWhiteBg from '@42ch/nexus-ui/assets/logos/logo-white-bg.svg';
import { NexusLogo, logoShellHeightPx } from '@42ch/nexus-ui';
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
      variant={isDark ? 'color' : 'whiteBg'}
      src={isDark ? logoColor : logoWhiteBg}
      label="Nexus"
      size={logoShellHeightPx}
      className={className ?? 'h-5 w-auto max-w-full shrink-0'}
    />
  );
}
