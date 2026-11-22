/**
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

export { Button, type ButtonProps } from './components/button';
export { Badge, type BadgeProps } from './components/badge';
export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  type CardProps,
  type CardTitleProps,
} from './components/card';

export { Input, type InputProps } from './components/input';
export { Label, type LabelProps } from './components/label';
export { Textarea, type TextareaProps } from './components/textarea';

export { Select, type SelectProps } from './components/select';

export {
  ToastProvider,
  Toaster,
  useToast,
  type Toast,
  type ToastVariant,
} from './components/toast';

// (Studio fixture → app wiring across gates + toasts + settings).
export {
  TransportErrorBlock,
  type TransportErrorBlockProps,
  type TransportErrorKind,
} from './components/transport-error-block';

export {
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
  type TabsProps,
  type TabsTriggerProps,
  type TabsContentProps,
} from './components/tabs';

export {
  Dialog,
  DialogTrigger,
  DialogClose,
  DialogContent,
  type DialogContentProps,
} from './components/dialog';

export {
  Sheet,
  SheetTrigger,
  SheetClose,
  SheetContent,
  type SheetContentProps,
} from './components/sheet';

export {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from './components/table';

export {
  Spinner,
  LoadingState,
  EmptyState,
  UnavailableState,
  ErrorState,
} from './components/states';

export { EmptyCreateCard } from './components/empty-create-card';

export {
  ListStateContainer,
  type ListStateContainerProps,
} from './components/list-state-container';
export { ConfirmDialog, type ConfirmDialogProps } from './components/confirm-dialog';
export { FormDialog, type FormDialogProps } from './components/form-dialog';
export { FormSection, type FormSectionProps } from './components/form-section';
export { ListPageLayout, type ListPageLayoutProps } from './components/list-page-layout';
export {
  SettingsSection,
  type SettingsSectionProps,
  SettingRow,
  type SettingRowProps,
} from './components/settings-section';
export {
  SettingsLayout,
  type SettingsLayoutProps,
  type SettingsLayoutSection,
} from './components/settings-layout';
// responsive split, caller-owned regions, no open/close state) and the
// read-only detail parts (FieldRow null-hide rows + DetailCard over the
// promoted Card primitives). HubCard stays app-side per plan D5 decision.
export {
  ListDetailShell,
  type ListDetailShellProps,
} from './components/list-detail-shell';
export {
  DetailCard,
  type DetailCardProps,
  FieldRow,
  type FieldRowProps,
} from './components/detail-parts';

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
