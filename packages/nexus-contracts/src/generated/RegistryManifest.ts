/**
 * ACP Registry Manifest
 *
 * Schema for the ACP Registry manifest response from the CDN. The registry lists available ACP agents with their distribution information.
 *
 * @schema_version 1
 * @source registry-manifest.schema.json
 */
import type { SchemaVersion } from './CommonTypes';

export interface RegistryManifest {
  version: string;
  agents: string[];
  extensions?: unknown[];
}
