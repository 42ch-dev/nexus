/**
 * Fixture coverage registry — SSOT for the public-export → fixture mapping.
 *
 * Every runtime value export of `@42ch/nexus-ui` (packages/nexus-ui/src/index.ts)
 * must either appear in `FIXTURE_COVERAGE` (keyed by export name, valued by
 * the `data-testid` prefix of its Studio fixture) or in `FIXTURE_ALLOWLIST`.
 * `fixture-coverage.test.ts` fails when a new public export ships without a
 * fixture entry, when an entry is stale (no longer exported), or when a
 * registered testid is absent from the Studio source.
 *
 * Values are testid prefixes: a value ending in `-` matches any testid that
 * starts with it (template-literal testids); other values match exactly.
 */

/** Public export name → Studio `data-testid` (or prefix) demonstrating it. */
export const FIXTURE_COVERAGE: Readonly<Record<string, string>> = {
  // Brand components (specimens live on the /brand page).
  NexusLogo: 'logo-card-',
  NexusMark: 'logo-card-',
  NexusLogoVariant: 'logo-card-',

  // Foundation primitives (Components page).
  Button: 'button-variant-matrix',
  Badge: 'badge-fixtures',
  Card: 'card-fixtures',
  CardHeader: 'card-fixtures',
  CardTitle: 'card-fixtures',
  CardDescription: 'card-fixtures',
  CardContent: 'card-fixtures',
  Input: 'input-fixture-',
  Label: 'label-fixtures',
  Textarea: 'textarea-fixture-',
  Select: 'select-fixtures',
  Tabs: 'tabs-controlled-fixture',
  TabsList: 'tabs-controlled-fixture',
  TabsTrigger: 'tabs-controlled-fixture',
  TabsContent: 'tabs-controlled-fixture',

  // Toast family.
  ToastProvider: 'toast-matrix',
  Toaster: 'toast-matrix',
  useToast: 'toast-matrix',

  TransportErrorBlock: 'transport-error-block-fixtures',

  // Dialog / Sheet families (V1.148).
  Dialog: 'dialog-open-trigger',
  DialogTrigger: 'dialog-open-trigger',
  DialogClose: 'dialog-open-trigger',
  DialogContent: 'dialog-open-trigger',
  Sheet: 'sheet-open-trigger',
  SheetTrigger: 'sheet-open-trigger',
  SheetClose: 'sheet-open-trigger',
  SheetContent: 'sheet-open-trigger',

  // Table / States / EmptyCreateCard families (V1.148).
  Table: 'table-fixtures',
  TableHeader: 'table-fixtures',
  TableBody: 'table-fixtures',
  TableRow: 'table-fixtures',
  TableHead: 'table-fixtures',
  TableCell: 'table-fixtures',
  Spinner: 'states-fixtures',
  LoadingState: 'states-fixtures',
  EmptyState: 'states-fixtures',
  UnavailableState: 'states-fixtures',
  ErrorState: 'states-fixtures',
  EmptyCreateCard: 'empty-create-card-fixtures',

  // L2 pattern components (V1.149).
  ListStateContainer: 'list-state-container-fixtures',
  ConfirmDialog: 'confirm-dialog-fixtures',
  FormDialog: 'form-dialog-fixtures',
  FormSection: 'form-section-fixtures',

  // L2 pattern components (V1.150).
  ListPageLayout: 'list-page-layout-fixtures',
  SettingsSection: 'settings-section-fixtures',
  SettingRow: 'settings-section-fixtures',
  SettingsLayout: 'settings-layout-fixtures',
  ListDetailShell: 'list-detail-shell-fixtures',
  DetailCard: 'detail-parts-fixtures',
  FieldRow: 'detail-parts-fixtures',

  // Compute Run Studio family (V1.147 P1).
  RunStatusBadge: 'compute-run-studio-fixtures',
  EntityPickerField: 'compute-run-studio-fixtures',
  RunFormFields: 'compute-run-studio-fixtures',
  ProposalSections: 'compute-run-studio-fixtures',
  RunsTable: 'compute-run-studio-fixtures',

  // Compute-on-Timeline family (V1.147 P2).
  ComputeResultNodeChrome: 'timeline-compute-brief-matrix',
  ComputeInspectorSections: 'timeline-compute-brief-matrix',
};

/**
 * Public exports without component fixtures, documented per plan Task 2:
 * brand/token constants and the class-merge utility are data/utilities, not
 * demoable components — they are exercised by the Tokens/Brand galleries
 * and the token assertion suites instead of a fixture cell.
 */
export const FIXTURE_ALLOWLIST: readonly string[] = [
  'cn',
  'VARIANT_FILENAMES',
  'brandColors',
  'logoClearSpaceRatio',
  'logoCompactMarkHeightPx',
  'logoMarkAspectRatio',
  'logoMarkViewBoxHeight',
  'logoMarkViewBoxWidth',
  'logoMinSizePx',
  'logoShellHeightPx',
  'logoSquareVariants',
  'logoVariantPalettes',
  'logoVariants',
];
