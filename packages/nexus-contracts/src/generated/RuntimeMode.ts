/**
 * RuntimeMode
 *
 * Creator runtime mode controlling platform dependency behavior. See ADR-015, ADR-017. local_only: no platform HTTP dependency. local_first: optional platform structured services, no platform LLM. cloud_enhanced: full platform capabilities.
 *
 * @schema_version 1
 * @source runtime-mode.schema.json
 */

/** Creator runtime mode controlling platform dependency behavior. See ADR-015, ADR-017. local_only: no platform HTTP dependency. local_first: optional platform structured services, no platform LLM. cloud_enhanced: full platform capabilities. */
export type RuntimeMode = 'local_only' | 'local_first' | 'cloud_enhanced';
