/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed v1 service discovery record published to <user_home>/.nexus42/run/service.json and emitted as the single NEXUS_SERVICE_READY stdout line. Carries no API key or bearer secret; identity/epoch is compared against authenticated runtime status before attach or stop. Shell-vs-ready is structural: a ready record carries non-null creator_id, workspace_slug and engine_epoch; an uninitialized shell carries all three as null.
 */
export type CoreServiceDiscovery = ReadyServiceDiscovery | UninitializedServiceDiscovery;
export type SchemaVersion = 1;
/**
 * Random per-start instance identity. Attach and stop must match it against the authenticated runtime; a stale record never authorizes signaling a PID.
 */
export type InstanceId = string;
/**
 * Owning process id, diagnostic only. Never a stop authorization.
 */
export type Pid = number;
/**
 * Canonical raw user home; home-layout appends .nexus42 exactly once. Mismatched home never attaches.
 */
export type UserHome = string;
/**
 * Tagged endpoint: http URL or unix absolute socket path. The closed union rejects ambiguous URL/socket combinations.
 */
export type Endpoint = HttpServiceEndpoint | UnixSocketServiceEndpoint;
/**
 * TLS certificate fingerprint for https endpoints; null when TLS is unused.
 */
export type TlsFingerprint = string | null;
export type ProtocolVersion = 1;

/**
 * Ready record: the core is initialized, so selected creator/workspace identity and the engine epoch are all present.
 */
export interface ReadyServiceDiscovery {
  schema_version: SchemaVersion;
  instance_id: InstanceId;
  pid: Pid;
  user_home: UserHome;
  /**
   * Selected creator id; present on every ready record, null only on the uninitialized shell.
   */
  creator_id: string;
  /**
   * Selected workspace slug; present on every ready record, null only on the uninitialized shell.
   */
  workspace_slug: string;
  /**
   * Monotonic engine epoch; present on every ready record, null until the core is initialized.
   */
  engine_epoch: number;
  endpoint: Endpoint;
  tls_fingerprint: TlsFingerprint;
  /**
   * Ready: domain surface is served. An explicit shell uses the uninitialized variant instead.
   */
  readiness: "ready";
  protocol_version: ProtocolVersion;
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
/**
 * Uninitialized shell: explicit, not a promise of domain readiness; creator/workspace/epoch identity is null.
 */
export interface UninitializedServiceDiscovery {
  schema_version: SchemaVersion;
  instance_id: InstanceId;
  pid: Pid;
  user_home: UserHome;
  /**
   * Null: an uninitialized shell has no selected creator.
   */
  creator_id: null;
  /**
   * Null: an uninitialized shell has no selected workspace.
   */
  workspace_slug: null;
  /**
   * Null: the engine epoch does not exist until the core is initialized.
   */
  engine_epoch: null;
  endpoint: Endpoint;
  tls_fingerprint: TlsFingerprint;
  /**
   * Uninitialized is an explicit shell, not a promise of domain readiness.
   */
  readiness: "uninitialized";
  protocol_version: ProtocolVersion;
}
