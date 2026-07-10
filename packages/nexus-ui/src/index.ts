/**
 * @42ch/nexus-ui — brand assets, tokens, theme helpers, React brand components,
 * and promoted pure presentational primitives.
 * V1.87 ships: `<NexusLogo>` and `<NexusMark>`.
 * V1.99 P0 ships: `<Button>`, `<Badge>`, `<Card>` (+ sub-primitives).
 * V1.100 P2 ships: `<Input>`, `<Label>`, `<Textarea>`.
 * V1.101 P2 ships: `<Select>`.
 */

// Class-merge utility — the V1.100 SSOT for cn / extendTailwindMerge.
// Consumers (apps/web, apps/design-studio) import or re-export from here;
// deep imports from @42ch/nexus-ui/src/* are forbidden.
export { cn } from './lib/cn';

export {
  brandColors,
  logoClearSpaceRatio,
  logoMinSizePx,
  logoVariants,
  type BrandColorName,
  type LogoVariantName,
} from './tokens';

export { NexusLogo, VARIANT_FILENAMES, type Variant, type NexusLogoProps } from './components/nexus-logo';
export { NexusMark, type NexusMarkProps } from './components/nexus-mark';

// V1.99 P0 — promoted presentational primitives
export { Button, type ButtonProps } from './components/button';
export { Badge, type BadgeProps } from './components/badge';
export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from './components/card';

// V1.100 P2 — promoted form-field presentational primitives
export { Input, type InputProps } from './components/input';
export { Label, type LabelProps } from './components/label';
export { Textarea, type TextareaProps } from './components/textarea';

// V1.101 P2 — promoted native Select presentational primitive
export { Select, type SelectProps } from './components/select';

// V1.106 P0 — promoted Toast presentational primitive (Studio Surfaces fixtures)
export {
  ToastProvider,
  Toaster,
  useToast,
  type Toast,
  type ToastVariant,
} from './components/toast';
