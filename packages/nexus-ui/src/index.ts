
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
