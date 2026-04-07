import type { SchemaVersion } from './CommonTypes';

/**
 * ACP Registry Manifest
 *
 * Schema for the ACP Registry manifest response from the CDN. The registry lists available ACP agents with their distribution information.
 *
 * @schema_version 1
 * @source registry-manifest.schema.json
 */
/** Schema for the ACP Registry manifest response from the CDN. The registry lists available ACP agents with their distribution information. */
export interface RegistryManifest {
  version: string;
  agents: AgentEntry[];
  extensions?: unknown[];
}
/** AgentEntry */
export interface AgentEntry {
  id: string;
  name: string;
  version: string;
  description?: string;
  repository?: string;
  authors?: string[];
  license?: string;
  icon?: string;
  distribution: Distribution;
}
/** Agent distribution configuration (npx or binary) */
export interface Distribution {
  npx?: NpxDistribution;
  binary?: BinaryDistribution;
}
/** NpxDistribution */
export interface NpxDistribution {
  package: string;
  args?: string[];
  env?: Record<string, unknown>;
}
/** Per-platform binary distribution */
export interface BinaryDistribution {
  'darwin-aarch64'?: PlatformBinary;
  'darwin-x86_64'?: PlatformBinary;
  'linux-aarch64'?: PlatformBinary;
  'linux-x86_64'?: PlatformBinary;
  'windows-aarch64'?: PlatformBinary;
  'windows-x86_64'?: PlatformBinary;
}
/** PlatformBinary */
export interface PlatformBinary {
  archive: string;
  cmd: string;
  args?: string[];
}
