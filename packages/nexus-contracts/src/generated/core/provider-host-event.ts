/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Externally tagged provider host event union promoted from handwritten HostEvent serde.
 */
export type ProviderHostEvent =
  | {
      SessionCreated: {
        session_id: string;
        provider_id: string;
      };
    }
  | {
      OpStarted: {
        session_id: string;
        op_id: string;
      };
    }
  | {
      ThoughtDelta: {
        session_id: string;
        op_id: string;
        text: string;
      };
    }
  | {
      MessageDelta: {
        session_id: string;
        op_id: string;
        text: string;
      };
    }
  | {
      ToolCall: {
        session_id: string;
        op_id: string;
        tool_call_id: string;
        tool_name: string;
      };
    }
  | {
      ToolCallUpdate: {
        session_id: string;
        op_id: string;
        tool_call_id: string;
        content: string;
      };
    }
  | {
      PlanUpdate: {
        session_id: string;
        op_id: string;
        content: string;
      };
    }
  | {
      Status: {
        session_id: string | null;
        level: "info" | "warning" | "error";
        message: string;
      };
    }
  | {
      OpFinished: {
        session_id: string;
        op_id: string;
        reason: "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" | "cancelled";
      };
    }
  | {
      OpFailed: {
        session_id: string;
        op_id: string;
        error_category: string;
        error_message: string;
      };
    }
  | {
      SessionStopped: {
        session_id: string;
        reason: "graceful_shutdown" | "provider_exit" | "error" | "cancelled";
      };
    };
