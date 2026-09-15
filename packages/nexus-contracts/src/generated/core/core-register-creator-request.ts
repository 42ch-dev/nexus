/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Local creator registration request through the home/configuration entry. Display name is optional for persistent local identity; platform_creator_id links an existing platform identity.
 */
export interface CoreRegisterCreatorRequest {
  /**
   * Optional display name for the persistent local creator.
   */
  display_name?: string;
  /**
   * Optional platform creator id to link to the local identity.
   */
  platform_creator_id?: string;
}
