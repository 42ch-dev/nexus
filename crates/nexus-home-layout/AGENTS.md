# nexus-home-layout — Path Layout Helpers

Defines the canonical `~/.nexus42/` directory structure per ADR-014.

## Key Rules

- All crates touching the filesystem must use these helpers — do not hardcode paths.
- `~/.nexus42/` is the only user-local root. Do not use `~/.config/nexus42/` or XDG dirs.

## Pre-release Note

Since pre-1.0, the path layout may change without migration. A re-init or wipe is acceptable when paths change.

**Path history:** `~/.nexus42/device-id` was canonicalized in V1.148 P4 (F-1: all device-id call sites passed the nexus home instead of the raw home, so pre-fix installs wrote a nested `~/.nexus42/.nexus42/device-id`). Post-fix, the canonical file is created fresh on first read — pre-fix installs get a one-time identity churn (X-Device-ID, Connect `host_id`). Accepted without migration per the pre-1.0 policy above; `get_or_create_device_id` takes the **raw home** and joins `.nexus42` itself — never pre-join the nexus root.
