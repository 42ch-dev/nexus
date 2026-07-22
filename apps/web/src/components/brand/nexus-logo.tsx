import logoColor from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoWhiteBg from '@42ch/nexus-ui/assets/logos/logo-white-bg.svg';
import { NexusLogo as NexusLogoComponent, logoShellHeightPx } from '@42ch/nexus-ui';

import { useTheme } from '@/components/theme-provider';
import { cn } from '@/lib/utils';

export interface NexusLogoProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
}

/**
 * Theme-aware Nexus timeline mark — thin wrapper around `@42ch/nexus-ui`.
 *
 * Shell placement is **mark only** (no wordmark, no brand plate): `whiteBg`
 * on light, `color` on dark. `logo-primary.svg` is the deep-blue plate lockup
 * and is not used in chrome. Resolves SVG assets via Vite and preserves
 * zero-prop call sites in `sidebar.tsx` / `header.tsx`. Wide aspect —
 * size by height (`h-* w-auto`).
 */
export function NexusLogo({ label = 'Nexus', className }: NexusLogoProps) {
  const { resolvedTheme } = useTheme();
  const variant = resolvedTheme === 'dark' ? 'color' : 'whiteBg';
  const src = variant === 'color' ? logoColor : logoWhiteBg;

  return (
    <NexusLogoComponent
      variant={variant}
      src={src}
      label={label}
      size={logoShellHeightPx}
      className={cn('h-5 w-auto max-w-full shrink-0', className)}
    />
  );
}
