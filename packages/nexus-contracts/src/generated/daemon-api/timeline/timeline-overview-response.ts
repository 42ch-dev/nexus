/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Cursor-paginated overview of visible Worlds with per-World era/event counts and last activity timestamp. Response for GET /v1/daemon/timeline/overview.
 */
export interface TimelineOverviewResponse {
  /**
   * @maxItems 20
   */
  worlds:
    | []
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ]
    | [
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        },
        {
          world_id: string;
          title?: string | null;
          era_count: number;
          event_count: number;
          last_event_at?: string | null;
          [k: string]: unknown | undefined;
        }
      ];
  cursor?: string | null;
  total_worlds: number;
}
