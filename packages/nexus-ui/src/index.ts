/**
 * @42ch/nexus-ui — brand assets, tokens, theme helpers, React brand components,
 * and promoted pure presentational primitives.
 * V1.87 ships: `<NexusLogo>` and `<NexusMark>`.
 * V1.99 P0 ships: `<Button>`, `<Badge>`, `<Card>` (+ sub-primitives).
 * V1.100 P2 ships: `<Input>`, `<Label>`, `<Textarea>`.
 * V1.101 P2 ships: `<Select>`.
 * V1.106 P0 ships: `<Toast>` (+ ToastProvider / Toaster / useToast).
 * V1.129 P1 ships: `<TransportErrorBlock>`.
 * V1.137 P2 ships: `<Tabs>` (+ TabsList / TabsTrigger / TabsContent).
 * V1.147 P1 ships: Compute Run Studio primitives (`<RunFormFields>`,
 * `<EntityPickerField>`, `<ProposalSections>`, `<RunStatusBadge>`, `<RunsTable>`).
 * V1.147 P2 ships: compute-on-Timeline primitives (`<ComputeResultNodeChrome>`,
 * `<ComputeInspectorSections>`).
 * VI logo upgrade ships: timeline `<NexusMark>`, `logoVariants.text`,
 * Studio-only `<NexusLogoVariant>` specimens.
 * V1.148 P1 ships: `<Dialog>` (+ trigger/close/content), `<Sheet>` (+ trigger/close/content),
 * `<Table>` primitives, `<States>` (Spinner/LoadingState/EmptyState/UnavailableState/ErrorState),
 * `<EmptyCreateCard>`.
 * V1.149 P1 ships: L2 patterns `<ListStateContainer>`, `<ConfirmDialog>`,
 * `<FormDialog>` (+ `<FormSection>`).
 */

// Class-merge utility — the V1.100 SSOT for cn / extendTailwindMerge.
// Consumers (apps/web, apps/design-studio) import or re-export from here;
// deep imports from @42ch/nexus-ui/src/* are forbidden.
export { cn } from './lib/cn';

export {
  brandColors,
  logoClearSpaceRatio,
  logoCompactMarkHeightPx,
  logoMarkAspectRatio,
  logoMarkViewBoxHeight,
  logoMarkViewBoxWidth,
  logoMinSizePx,
  logoShellHeightPx,
  logoSquareVariants,
  logoVariantPalettes,
  logoVariants,
  type BrandColorName,
  type LogoSquareVariantName,
  type LogoVariantName,
  type LogoVariantPalette,
  type LogoVariantTheme,
} from './tokens';

export { NexusLogo, VARIANT_FILENAMES, type Variant, type NexusLogoProps } from './components/nexus-logo';
export { NexusMark, type NexusMarkProps } from './components/nexus-mark';
export {
  NexusLogoVariant,
  type NexusLogoVariantProps,
} from './components/nexus-logo-variant';

// V1.99 P0 — promoted presentational primitives
export { Button, type ButtonProps } from './components/button';
export { Badge, type BadgeProps } from './components/badge';
export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  // V1.121 P1 — v0.4 additive opt-ins (interactive elevation recipe; title voice)
  type CardProps,
  type CardTitleProps,
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

// V1.129 P1 — promoted transport-error presentational primitive
// (Studio fixture → app wiring across gates + toasts + settings).
export {
  TransportErrorBlock,
  type TransportErrorBlockProps,
  type TransportErrorKind,
} from './components/transport-error-block';

// V1.137 P2 — promoted Tabs presentational primitive (React context compound).
export {
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
  type TabsProps,
  type TabsTriggerProps,
  type TabsContentProps,
} from './components/tabs';

// V1.148 P1 — promoted Dialog presentational primitive (Radix dialog; caller-owned copy).
export {
  Dialog,
  DialogTrigger,
  DialogClose,
  DialogContent,
  type DialogContentProps,
} from './components/dialog';

// V1.148 P1 — promoted Sheet presentational primitive (end-rail drawer; caller-owned copy).
export {
  Sheet,
  SheetTrigger,
  SheetClose,
  SheetContent,
  type SheetContentProps,
} from './components/sheet';

// V1.148 P1 — promoted Table presentational primitives (native table recipe).
export {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from './components/table';

// V1.148 P1 — promoted States presentational primitives (Spinner/Loading/Empty/Unavailable/Error).
export {
  Spinner,
  LoadingState,
  EmptyState,
  UnavailableState,
  ErrorState,
} from './components/states';

// V1.148 P1 — promoted EmptyCreateCard presentational primitive (empty-list create affordance).
export { EmptyCreateCard } from './components/empty-create-card';

// V1.149 P1 — promoted L2 pattern components (compose promoted Dialog/States/Button;
// caller-owned copy; no query/router/i18n deps).
export {
  ListStateContainer,
  type ListStateContainerProps,
} from './components/list-state-container';

// V1.147 P1 — promoted Compute Run Studio presentational primitives.
// Pure presentational: data + caller-owned copy + callbacks in; no daemon,
// no wire-contract imports (structural mirrors only), no routing.
export {
  RunStatusBadge,
  type RunStatus,
  type RunStatusBadgeProps,
} from './components/run-status-badge';
export {
  EntityPickerField,
  type EntityPickerEntry,
  type EntityPickerFieldProps,
} from './components/entity-picker-field';
export {
  RunFormFields,
  type InvocationSchema,
  type InvocationSchemaProperty,
  type RunFormCopy,
  type RunFormFieldsProps,
} from './components/run-form-fields';
export {
  ProposalSections,
  type BattleReportProposal,
  type ComputeProposals,
  type ProposalSectionsCopy,
  type ProposalSectionsProps,
  type StateDeltaProposal,
  type TimelineEventProposal,
} from './components/proposal-sections';
export {
  RunsTable,
  type RunsTableCopy,
  type RunsTableProps,
  type RunTableRow,
} from './components/runs-table';
// V1.147 P2 — compute-on-Timeline primitives (Studio fixture:
// `timeline-compute.tsx`, Components → Compute Timeline).
export {
  ComputeResultNodeChrome,
  type ComputeResultNodeChromeProps,
} from './components/compute-result-node-chrome';
export {
  ComputeInspectorSections,
  type ComputeAffectedEntry,
  type ComputeInspectorSectionsCopy,
  type ComputeInspectorSectionsProps,
} from './components/compute-inspector-sections';
