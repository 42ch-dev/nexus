/**
 * note: App-side TS mirror of the P0 preset-profile DTO (V1.171 P1 — AR-27).
 *
 * Mirrors `crates/nexus-contracts/src/local/orchestration/http.rs`
 * `PresetProfileResponse` + nested structs 1:1 (camelCase on the wire). The
 * profile endpoint is hand-coded local tier — NOT codegen'd, NOT in
 * `schemas/`, NOT in the `@42ch/nexus-contracts` npm package — so this file
 * is the single typed surface for
 * `GET /v1/daemon/orchestration/presets/:id/profile`.
 *
 * Presence semantics mirror the Rust `skip_serializing_if`: optional fields
 * serialize absent, so the TS mirrors use `?:` and the UI must not assume
 * them. The catalog (P1 T1) reads `id` + `lanes` only; the full shape is
 * declared now because AR-27 locks the 1:1 mirror and T2 (profile drill-down)
 * consumes states / roles / capabilities / signals.
 */

/** Trigger-lane classification for a preset profile (AR-21 / PL-3 vocabulary). */
export interface PresetProfileLanes {
  /** note: Cron (Work roles): preset id is one of the works-cron role presets
   * (brainstorm / write / review per `RolesSchedule`). */
  cron: boolean;
  /** note: Wall-clock poller: daemon schedule admission on a wall-clock tick. */
  wallClock: boolean;
  /** note: Session start: embedded and system presets only — user presets report
   * `false` (the session-start API loads embedded presets, W-003/F-002). */
  session: boolean;
  /** note: Direct run: schedule start with an explicit run payload. */
  direct: boolean;
}

/** A single state in the outer state machine. */
export interface PresetProfileState {
  /** note: Unique state identifier within this preset. */
  id: string;
  /** note: Optional human-readable description. */
  description?: string;
  /** note: Enter action kinds (`capability` / `inner_graph` / `host_tool`). */
  enter?: PresetProfileEnterAction[];
  /** note: Exit condition kind (`llm_judge` / `rule` / `graph_complete` /
   * `manual` / `timer`); absent for terminal states. */
  exitWhen?: PresetProfileExitWhen;
  /** note: Next transition form (`linear` / `goNogo` / `labeled` /
   * `conditional` / `branches`); absent for terminal states. */
  next?: PresetProfileNext;
  /** note: Whether this state is terminal (no outgoing transitions). */
  terminal: boolean;
}

/** A single enter action on a state. */
export interface PresetProfileEnterAction {
  /** note: Action kind: `capability`, `inner_graph`, or `host_tool`. */
  kind: string;
  /** note: Referenced name: capability name, inner graph name, or tool name. */
  name: string;
}

/** Exit condition for a state. */
export interface PresetProfileExitWhen {
  /** note: Exit condition kind: `llm_judge` / `rule` / `graph_complete` /
   * `manual` / `timer`. */
  kind: string;
  /** note: Judge prompt template path (`llm_judge`). */
  templateFile?: string;
  /** note: Judge capability name (`llm_judge`). */
  judgeCapability?: string;
  /** note: Minimum interval between re-evaluations (`llm_judge`). */
  minInterval?: string;
  /** note: ISO-8601 duration to wait (`timer`). */
  duration?: string;
}

/** Next transition form for a state. */
export interface PresetProfileNext {
  /** note: Next form: `linear` / `goNogo` / `labeled` / `conditional` /
   * `branches`. */
  kind: string;
  /** note: Linear target state id (`linear`). */
  target?: string;
  /** note: GO target state id (`goNogo`). */
  go?: string;
  /** note: NOGO target state id (`goNogo`). */
  nogo?: string;
  /** note: Labeled edges (`labeled`). */
  labeled?: PresetProfileLabeledNext[];
  /** note: Conditional rules (`conditional` legacy form). */
  rules?: PresetProfileConditionalRule[];
  /** note: Expression branches (`branches` form). */
  branches?: PresetProfileConditionalRule[];
  /** note: Default target state id (`conditional` / `branches`). */
  default?: string;
}

/** A labeled next edge (`labeled` form). */
export interface PresetProfileLabeledNext {
  /** note: Label the judge returns to select this edge. */
  label: string;
  /** note: Target state id. */
  target: string;
}

/** A conditional rule (expression → target edge). */
export interface PresetProfileConditionalRule {
  /** note: Expression evaluated against context. */
  when: string;
  /** note: Target state id if the expression evaluates to true. */
  target: string;
}

/** A role definition for multi-agent presets. */
export interface PresetProfileRole {
  /** note: Unique role ID within this preset. */
  id: string;
  /** note: Human-readable description. */
  description: string;
  /** note: Path to the system prompt template (relative to the bundle root). */
  systemPromptFile: string;
  /** note: Recommended skill slugs (ordered; first = primary). */
  recommendedSkills?: string[];
}

/** A declared signal binding (declared, not delivered). */
export interface PresetProfileSignal {
  /** note: Declared signal name. */
  name: string;
  /** note: Action kind on receive: `pause` / `force_transition`. */
  action: string;
  /** note: Target state id (`force_transition`). */
  target?: string;
}

/** Response body for `GET /v1/daemon/orchestration/presets/{id}/profile` (AR-20..23). */
export interface PresetProfileResponse {
  /** note: Preset identifier from the loaded manifest (`LoadedPreset.id`). */
  id: string;
  /** note: Preset schema version. */
  version: number;
  /** note: blake3 hex hash of the source YAML (identity across restarts). */
  sourceHash: string;
  /** note: Trigger-lane classification derived from the manifest + works-cron
   * role membership (AR-21). */
  lanes: PresetProfileLanes;
  /** note: Ordered outer state-machine states. */
  states: PresetProfileState[];
  /** note: Role definitions (empty = single-agent mode). */
  roles?: PresetProfileRole[];
  /** note: Capabilities this preset requires. */
  requiredCapabilities?: string[];
  /** note: Declared signal bindings (declared, not delivered). */
  signals?: PresetProfileSignal[];
}
