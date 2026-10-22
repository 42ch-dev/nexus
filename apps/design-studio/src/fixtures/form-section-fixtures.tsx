import {
  FormSection,
  Input,
  Label,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<FormSection>` (V1.149 P1 promotion).
 *
 * The promoted field-group layout behind form dialogs and settings
 * surfaces: native fieldset/legend group semantics, caller-owned fields and
 * copy, and an optional inline error slot (role=alert + error-surface
 * tokens) rendered above the fields.
 */
export function FormSectionFixtures() {
  return (
    <div data-testid="form-section-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="grid gap-6 md:grid-cols-2">
          <FormSection
            title="Connection"
            description="How the app reaches the daemon."
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="form-section-host">Host</Label>
              <Input id="form-section-host" data-testid="form-section-host-input" defaultValue="127.0.0.1" />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="form-section-port">Port</Label>
              <Input id="form-section-port" data-testid="form-section-port-input" defaultValue="7420" />
            </div>
          </FormSection>
          <FormSection
            title="Workspace"
            description="A group with a whole-section error."
            error="Save failed: the workspace path is not writable."
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="form-section-path">Workspace path</Label>
              <Input
                id="form-section-path"
                data-testid="form-section-path-input"
                defaultValue="/var/not-writable"
                aria-invalid="true"
              />
            </div>
          </FormSection>
        </div>
        <p className="text-copy-13 text-gray-700 mt-6">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          FormSection — native fieldset/legend group chrome behind form
          dialogs and settings surfaces; the optional error slot renders the
          shared role=alert error surface above the fields.
        </p>
      </div>
    </div>
  );
}
