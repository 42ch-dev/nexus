import logoColor from '@42ch/nexus-ui/assets/logos/logo-color.svg';
import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import { NexusLogo as NexusLogoComponent, logoMinSizePx } from '@42ch/nexus-ui';

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
 * Shell placement is **mark only** (no wordmark): `primary` on light, `color`
 * on dark. Resolves SVG assets via Vite and preserves zero-prop call sites in
 * `sidebar.tsx` / `header.tsx`. Wide aspect — size by height (`h-* w-auto`).
 */
export function NexusLogo({ label = 'Nexus', className }: NexusLogoProps) {
  const { resolvedTheme } = useTheme();
  const variant = resolvedTheme === 'dark' ? 'color' : 'primary';
  const src = variant === 'color' ? logoColor : logoPrimary;

  return (
    <NexusLogoComponent
      variant={variant}
      src={src}
      label={label}
      size={logoMinSizePx}
      className={cn('h-6 w-auto shrink-0', className)}
    />
  );
}
