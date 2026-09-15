/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed v1 service discovery record published to <user_home>/.nexus42/run/service.json and emitted as the single NEXUS_SERVICE_READY stdout line. Carries no API key or bearer secret; identity/epoch is compared against authenticated runtime status before attach or stop.
 */
export interface CoreServiceDiscovery {
  schema_version: 1;
  /**
   * Random per-start instance identity. Attach and stop must match it against the authenticated runtime; a stale record never authorizes signaling a PID.
   */
  instance_id: string;
  /**
   * Owning process id, diagnostic only. Never a stop authorization.
   */
  pid: number;
  /**
   * Canonical raw user home; home-layout appends .nexus42 exactly once. Mismatched home never attaches.
   */
  user_home: string;
  /**
   * Selected creator id; null only for the uninitialized shell.
   */
  creator_id: string | null;
  /**
   * Selected workspace slug; null only for the uninitialized shell.
   */
  workspace_slug: string | null;
  /**
   * Conditional monotonic engine epoch; null until the core is initialized.
   */
  engine_epoch: number | null;
  /**
   * Tagged endpoint: http URL or unix absolute socket path. The closed union rejects ambiguous URL/socket combinations.
   */
  endpoint: HttpServiceEndpoint | UnixSocketServiceEndpoint;
  /**
   * TLS certificate fingerprint for https endpoints; null when TLS is unused.
   */
  tls_fingerprint: string | null;
  /**
   * uninitialized is an explicit shell, not a promise of domain readiness.
   */
  readiness: "uninitialized" | "ready";
  protocol_version: 1;
}
export interface HttpServiceEndpoint {
  /**
   * HTTP transport discriminant.
   */
  transport: "http";
  /**
   * Base URL served by the service.
   */
  url: string;
}
export interface UnixSocketServiceEndpoint {
  /**
   * Unix-socket transport discriminant.
   */
  transport: "unix";
  /**
   * Absolute Unix domain socket path.
   */
  path: string;
}
