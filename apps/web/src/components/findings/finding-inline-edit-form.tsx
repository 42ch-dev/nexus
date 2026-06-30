/**
 * Finding inline-edit form — extracted from `finding-detail-panel.tsx` so the
 * panel stays under the 250-line module-size discipline (V1.78 qc1 S-002,
 * mirroring the V1.74 A10 split pattern).
 *
 * Owns the title / description / severity / kind / rule_suggestion form and
 * the minimal-diff PATCH builder. The parent retains mutation orchestration
 * and form state so this module stays a pure presentational slice.
 */
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import type { FindingDetailResponse, UpdateFindingRequest } from '@42ch/nexus-contracts';

/** DAO `VALID_SEVERITIES` (`crates/nexus-local-db/src/findings.rs:104`). */
export const SEVERITY_OPTIONS = ['info', 'minor', 'major', 'blocker'] as const;

export interface InlineForm {
  title: string;
  description: string;
  severity: string;
  kind: string;
  ruleSuggestion: string;
}

export function formFromFinding(f: FindingDetailResponse): InlineForm {
  return {
    title: f.title,
    description: f.description,
    severity: f.severity,
    kind: f.kind,
    ruleSuggestion: f.rule_suggestion ?? '',
  };
}

/**
 * Compute the minimal PATCH diff between the canonical finding and the edited
 * form state. Only changed fields are included so the wire stays small and
 * unchanged fields are not risked.
 *
 * `rule_suggestion` is a plain string on the wire
 * (`update-finding-request.schema.json` — `"type": "string"`, not nullable);
 * clearing the field sends an empty string.
 */
export function buildPatch(
  finding: FindingDetailResponse,
  form: InlineForm,
): UpdateFindingRequest | null {
  const patch: UpdateFindingRequest = {};
  if (form.title !== finding.title) patch.title = form.title;
  if (form.description !== finding.description) patch.description = form.description;
  if (form.severity !== finding.severity) patch.severity = form.severity;
  if (form.kind !== finding.kind) patch.kind = form.kind;
  if (form.ruleSuggestion !== (finding.rule_suggestion ?? '')) {
    patch.rule_suggestion = form.ruleSuggestion;
  }
  return Object.keys(patch).length > 0 ? patch : null;
}

interface FindingInlineEditFormProps {
  form: InlineForm;
  setForm: React.Dispatch<React.SetStateAction<InlineForm>>;
  patch: UpdateFindingRequest | null;
  pending: boolean;
  onSave: () => void;
  onReset: () => void;
}

export function FindingInlineEditForm({
  form,
  setForm,
  patch,
  pending,
  onSave,
  onReset,
}: FindingInlineEditFormProps) {
  const isDirty = patch !== null;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="finding-title">Title</Label>
        <input
          id="finding-title"
          type="text"
          value={form.title}
          onChange={(e) => setForm((f) => ({ ...f, title: e.target.value }))}
          disabled={pending}
          className="h-10 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
        />
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="finding-description">Description</Label>
        <textarea
          id="finding-description"
          value={form.description}
          onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
          disabled={pending}
          rows={4}
          className="min-h-[96px] w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
        />
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="finding-severity">Severity</Label>
          <Select
            id="finding-severity"
            value={form.severity}
            onChange={(e) => setForm((f) => ({ ...f, severity: e.target.value }))}
            disabled={pending}
          >
            {SEVERITY_OPTIONS.map((opt) => (
              <option key={opt} value={opt}>
                {opt.charAt(0).toUpperCase() + opt.slice(1)}
              </option>
            ))}
          </Select>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="finding-kind">Kind</Label>
          <input
            id="finding-kind"
            type="text"
            value={form.kind}
            onChange={(e) => setForm((f) => ({ ...f, kind: e.target.value }))}
            disabled={pending}
            className="h-10 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
          />
        </div>
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="finding-rule-suggestion">Rule Suggestion</Label>
        <input
          id="finding-rule-suggestion"
          type="text"
          value={form.ruleSuggestion}
          onChange={(e) => setForm((f) => ({ ...f, ruleSuggestion: e.target.value }))}
          disabled={pending}
          placeholder="Clear to remove the rule suggestion"
          className="h-10 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
        />
      </div>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="primary"
          size="small"
          onClick={onSave}
          disabled={!isDirty || pending}
        >
          Save Changes
        </Button>
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={onReset}
          disabled={!isDirty || pending}
        >
          Reset
        </Button>
        {isDirty && <span className="text-copy-13 text-gray-700">Unsaved changes</span>}
      </div>
    </section>
  );
}
