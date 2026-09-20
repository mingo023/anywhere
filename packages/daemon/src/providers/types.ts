import type { Decision, PermissionMode, TurnUsage } from "@pocket/protocol";
import type { Effect, Scope, Stream } from "effect";
import type { ProviderFailure } from "../errors.js";

/** Set when the event came from inside a delegated run, naming the Task call that spawned it. */
type Nested = { readonly parentToolUseId?: string };

export type ProviderEvent =
  | { readonly kind: "session"; readonly providerSessionId: string; readonly model: string }
  | ({ readonly kind: "assistant_text"; readonly text: string } & Nested)
  | ({ readonly kind: "thinking"; readonly text: string } & Nested)
  | ({ readonly kind: "tool_start"; readonly toolUseId: string; readonly name: string; readonly input: unknown } & Nested)
  | ({ readonly kind: "tool_end"; readonly toolUseId: string; readonly ok: boolean; readonly output?: string } & Nested)
  | {
      readonly kind: "result";
      readonly ok: boolean;
      readonly durationMs: number;
      readonly costUsd?: number;
      readonly error?: string;
      readonly usage?: TurnUsage;
      readonly turns?: number;
    }
  | { readonly kind: "compacted"; readonly trigger: "manual" | "auto" }
  | {
      readonly kind: "task_start";
      readonly taskId: string;
      readonly taskType: string;
      readonly description: string;
      readonly toolUseId?: string;
      readonly subagentType?: string;
    }
  | {
      readonly kind: "task_progress";
      readonly taskId: string;
      readonly tools: number;
      readonly tokens: number;
      readonly lastTool?: string;
      readonly summary?: string;
    }
  | { readonly kind: "task_end"; readonly taskId: string; readonly status: "ok" | "error" | "stopped"; readonly summary?: string }
  /** The live set after any membership change; entries missing from it are no longer running. */
  | { readonly kind: "task_list"; readonly taskIds: ReadonlyArray<string> }
  | { readonly kind: "error"; readonly message: string };

export type StartOptions = {
  readonly cwd: string;
  readonly env: Record<string, string>;
  readonly model?: string | undefined;
  readonly permissionMode?: PermissionMode | undefined;
  readonly resume?: string | undefined;
  readonly askPermission: (toolName: string, input: unknown) => Effect.Effect<Decision>;
};

export interface ProviderSession {
  readonly events: Stream.Stream<ProviderEvent>;
  readonly prompt: (text: string) => Effect.Effect<void>;
  readonly interrupt: Effect.Effect<void>;
  readonly stopTask: (taskId: string) => Effect.Effect<void>;
}

export interface Provider {
  readonly id: string;
  readonly start: (options: StartOptions) => Effect.Effect<ProviderSession, ProviderFailure, Scope.Scope>;
}
