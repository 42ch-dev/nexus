import { Link } from 'react-router-dom';

import { ScreenPlaceholder } from '@/components/screen-placeholder';
import { Button } from '@/components/ui/button';

/**
 * Control Room + Setup screen placeholders (plan P1 shell; content in P2).
 * Each names its Local API data source so routes are self-documenting while
 * empty (web-ui.md §6). Copy follows DESIGN.md §Voice & Content.
 */

export function WorksPage() {
  return (
    <ScreenPlaceholder
      title="Works"
      description="See every Work, its status, and how far along it is."
      dataSource="GET /v1/local/works"
    />
  );
}

export function WorkDetailPage() {
  return (
    <ScreenPlaceholder
      title="Work detail"
      description="Intake status, current stage, and linked schedules for one Work."
      dataSource="GET /v1/local/works/{work_id}"
      actions={
        <Button asChild variant="secondary" size="small">
          <Link to="/works">Back to Works</Link>
        </Button>
      }
    />
  );
}

export function SessionsPage() {
  return (
    <ScreenPlaceholder
      title="Orchestration sessions"
      description="Watch running, completed, and failed sessions."
      dataSource="GET /v1/local/orchestration/sessions"
    />
  );
}

export function SchedulePage() {
  return (
    <ScreenPlaceholder
      title="Schedule"
      description="Cron roles per Work with next-fire in UTC and local time."
      dataSource="GET /v1/local/orchestration/schedules"
    />
  );
}

export function CapabilitiesPage() {
  return (
    <ScreenPlaceholder
      title="Capabilities"
      description="Browse the nexus.* capabilities the runtime exposes."
      dataSource="GET /v1/local/orchestration/capabilities"
    />
  );
}

export function FindingsPage() {
  return (
    <ScreenPlaceholder
      title="Findings"
      description="Findings raised against a Work, with severity filtering."
      dataSource="GET /v1/local/works/{work_id}/findings  (lands with F-P2 / plan P0)"
    />
  );
}

export function PresetsPage() {
  return (
    <ScreenPlaceholder
      title="Presets"
      description="List, scaffold, validate, and reload presets."
      dataSource="GET /v1/local/presets  ·  validate: POST /v1/local/presets:validate"
    />
  );
}

export function NotFoundPage() {
  return (
    <ScreenPlaceholder
      title="Page not found"
      description="That route is not part of the Control Room + Setup surface."
      dataSource="—"
      actions={
        <Button asChild variant="secondary" size="small">
          <Link to="/works">Go to Works</Link>
        </Button>
      }
    />
  );
}
