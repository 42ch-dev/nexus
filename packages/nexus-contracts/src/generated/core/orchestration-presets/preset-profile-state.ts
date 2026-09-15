/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One state of the outer state machine; `exitWhen`/`next` are absent for terminal states.
 */
export interface PresetProfileState {
  id: string;
  description?: string;
  enter?: PresetProfileEnterAction[];
  exitWhen?: PresetProfileExitWhen;
  next?: PresetProfileNext;
  terminal: boolean;
}
/**
 * One enter action on a state.
 */
export interface PresetProfileEnterAction {
  kind: string;
  name: string;
}
/**
 * Exit condition kind (`llm_judge` / `rule` / `graph_complete` / `manual` / `timer`).
 */
export interface PresetProfileExitWhen {
  kind: string;
  templateFile?: string;
  judgeCapability?: string;
  minInterval?: string;
  duration?: string;
}
/**
 * Next transition form (`linear` / `goNogo` / `labeled` / `conditional` / `branches`).
 */
export interface PresetProfileNext {
  kind: string;
  target?: string;
  go?: string;
  nogo?: string;
  labeled?: PresetProfileLabeledNext[];
  rules?: PresetProfileConditionalRule[];
  branches?: PresetProfileConditionalRule[];
  default?: string;
}
/**
 * A labeled next edge (`labeled` form).
 */
export interface PresetProfileLabeledNext {
  label: string;
  target: string;
}
/**
 * A conditional rule (expression -> target edge).
 */
export interface PresetProfileConditionalRule {
  when: string;
  target: string;
}
