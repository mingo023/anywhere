import {
  query,
  type NonNullableUsage,
  type Options,
  type SDKMessage,
  type SDKUserMessage,
} from "@anthropic-ai/claude-agent-sdk";
import type { Decision, TurnUsage } from "@pocket/protocol";
import { Effect, Mailbox, Runtime, Stream } from "effect";
import { ProviderFailure } from "../errors.js";
import type { Provider, ProviderEvent, ProviderSession, StartOptions } from "./types.js";

type ContentBlock = { type: string; [key: string]: unknown };

function textOf(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((block) => (typeof block === "object" && block && "text" in block ? String((block as ContentBlock).text) : ""))
    .join("");
}

function usageOf(usage: NonNullableUsage): TurnUsage {
  const cacheReadTokens = usage.cache_read_input_tokens;
  return {
    inputTokens: usage.input_tokens,
    outputTokens: usage.output_tokens,
    ...(cacheReadTokens ? { cacheReadTokens } : {}),
  };
}

const TASK_STATUS: Record<string, "ok" | "error" | "stopped"> = {
  completed: "ok",
  failed: "error",
  killed: "stopped",
  stopped: "stopped",
};

function* mapMessage(message: SDKMessage): Generator<ProviderEvent> {
  if (message.type === "system" && message.subtype === "init") {
    yield { kind: "session", providerSessionId: message.session_id, model: message.model };
    return;
  }

  if (message.type === "system" && message.subtype === "compact_boundary") {
    yield { kind: "compacted", trigger: message.compact_metadata.trigger };
    return;
  }

  if (message.type === "system" && message.subtype === "task_started") {
    if (message.ambient || message.skip_transcript) return;
    yield {
      kind: "task_start",
      taskId: message.task_id,
      taskType: message.task_type ?? "",
      description: message.description,
      ...(message.tool_use_id ? { toolUseId: message.tool_use_id } : {}),
      ...(message.subagent_type ? { subagentType: message.subagent_type } : {}),
    };
    return;
  }

  if (message.type === "system" && message.subtype === "task_progress") {
    yield {
      kind: "task_progress",
      taskId: message.task_id,
      tools: message.usage.tool_uses,
      tokens: message.usage.total_tokens,
      ...(message.last_tool_name ? { lastTool: message.last_tool_name } : {}),
      ...(message.summary ? { summary: message.summary } : {}),
    };
    return;
  }

  if (message.type === "system" && message.subtype === "task_updated") {
    const status = TASK_STATUS[message.patch.status ?? ""];
    if (status) yield { kind: "task_end", taskId: message.task_id, status };
    return;
  }

  if (message.type === "system" && message.subtype === "task_notification") {
    if (message.ambient) return;
    yield {
      kind: "task_end",
      taskId: message.task_id,
      status: TASK_STATUS[message.status] ?? "ok",
      ...(message.summary ? { summary: message.summary } : {}),
    };
    return;
  }

  if (message.type === "system" && message.subtype === "background_tasks_changed") {
    yield { kind: "task_list", taskIds: message.tasks.filter((task) => !task.ambient).map((task) => task.task_id) };
    return;
  }

  if (message.type === "assistant") {
    const nested = message.parent_tool_use_id ? { parentToolUseId: message.parent_tool_use_id } : {};
    for (const block of message.message.content as unknown as ContentBlock[]) {
      if (block.type === "text" && block.text) yield { kind: "assistant_text", text: String(block.text), ...nested };
      else if (block.type === "thinking" && block.thinking)
        yield { kind: "thinking", text: String(block.thinking), ...nested };
      else if (block.type === "tool_use")
        yield { kind: "tool_start", toolUseId: String(block.id), name: String(block.name), input: block.input, ...nested };
    }
    return;
  }

  if (message.type === "user") {
    const content = message.message.content;
    if (!Array.isArray(content)) return;
    const nested = message.parent_tool_use_id ? { parentToolUseId: message.parent_tool_use_id } : {};
    for (const block of content as unknown as ContentBlock[]) {
      if (block.type !== "tool_result") continue;
      yield {
        kind: "tool_end",
        toolUseId: String(block.tool_use_id),
        ok: block.is_error !== true,
        output: textOf(block.content),
        ...nested,
      };
    }
    return;
  }

  if (message.type === "result") {
    yield {
      kind: "result",
      ok: !message.is_error,
      durationMs: message.duration_ms,
      ...(message.total_cost_usd !== undefined ? { costUsd: message.total_cost_usd } : {}),
      ...(message.subtype === "success" ? {} : { error: message.subtype }),
      ...(message.usage ? { usage: usageOf(message.usage) } : {}),
      ...(message.num_turns !== undefined ? { turns: message.num_turns } : {}),
    };
  }
}

const denyOnAbort = (signal: AbortSignal): Effect.Effect<Decision> =>
  Effect.async<Decision>((resume) => {
    if (signal.aborted) return resume(Effect.succeed("deny"));
    const onAbort = () => resume(Effect.succeed("deny"));
    signal.addEventListener("abort", onAbort, { once: true });
    return Effect.sync(() => signal.removeEventListener("abort", onAbort));
  });

export const claudeProvider: Provider = {
  id: "claude",

  start: (options: StartOptions) =>
    Effect.gen(function* () {
      const runtime = yield* Effect.runtime<never>();
      const events = yield* Mailbox.make<ProviderEvent>();
      const outbox = yield* Mailbox.make<SDKUserMessage>();
      const abort = new AbortController();

      const sdkOptions: Options = {
        cwd: options.cwd,
        env: { ...process.env, ...options.env },
        abortController: abort,
        permissionMode: options.permissionMode ?? "default",
        tools: { type: "preset", preset: "claude_code" },
        forwardSubagentText: true,
        /** Without it the CLI fails closed and an interrupt kills every background task too. */
        perTaskStopAffordance: true,
        settingSources: ["user", "project", "local"],
        canUseTool: async (toolName, toolInput, { signal }) => {
          const decision = await Runtime.runPromise(runtime)(
            Effect.raceFirst(options.askPermission(toolName, toolInput), denyOnAbort(signal)),
          );
          return decision === "allow"
            ? { behavior: "allow", updatedInput: toolInput }
            : { behavior: "deny", message: "Denied by user" };
        },
      };
      if (options.model) sdkOptions.model = options.model;
      if (options.resume) sdkOptions.resume = options.resume;

      const stream = query({
        prompt: Stream.toAsyncIterable(Mailbox.toStream(outbox)),
        options: sdkOptions,
      });

      yield* Effect.addFinalizer(() => Effect.sync(() => abort.abort()).pipe(Effect.ignore));

      yield* Stream.fromAsyncIterable(
        stream,
        (cause) => new ProviderFailure({ message: cause instanceof Error ? cause.message : String(cause) }),
      ).pipe(
        Stream.flatMap((message) => Stream.fromIterable(mapMessage(message))),
        Stream.runForEach((event) => events.offer(event)),
        Effect.catchAll((error) => events.offer({ kind: "error", message: error.message })),
        Effect.ensuring(events.end),
        Effect.forkScoped,
      );

      const session: ProviderSession = {
        events: Mailbox.toStream(events),
        prompt: (text) =>
          outbox
            .offer({ type: "user", message: { role: "user", content: text }, parent_tool_use_id: null, session_id: "" })
            .pipe(Effect.asVoid),
        interrupt: Effect.tryPromise(() => stream.interrupt()).pipe(Effect.ignore),
        stopTask: (taskId) => Effect.tryPromise(() => stream.stopTask(taskId)).pipe(Effect.ignore),
      };

      return session;
    }),
};
