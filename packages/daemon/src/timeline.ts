import {
  TOOL_OUTPUT_LIMIT,
  type AgentStep,
  type TaskItem,
  type TimelineBody,
  type TimelineItem,
  type ToolCall,
} from "@pocket/protocol";
import { toolDetail } from "./tool-detail.js";
import type { ProviderEvent } from "./providers/types.js";

export type Timeline = {
  readonly items: ReadonlyArray<TimelineItem>;
  readonly epoch: number;
  readonly seq: number;
  readonly toolIndex: ReadonlyMap<string, number>;
  readonly openTextIndex: number | null;
  /** Calls already shown as their own card, so their result must not reopen a tool row. */
  readonly folded: ReadonlySet<string>;
};

export type Applied = readonly [Timeline, TimelineItem | null];

export const empty: Timeline = {
  items: [],
  epoch: 0,
  seq: 0,
  toolIndex: new Map(),
  openTextIndex: null,
  folded: new Set(),
};

function clamp(output: string): string {
  return output.length <= TOOL_OUTPUT_LIMIT ? output : `${output.slice(0, TOOL_OUTPUT_LIMIT)}\n… truncated`;
}

function append(timeline: Timeline, body: TimelineBody, id: string, ts: number): readonly [Timeline, TimelineItem] {
  const seq = timeline.seq + 1;
  const item = { ...body, id, seq, ts } as TimelineItem;
  return [{ ...timeline, items: [...timeline.items, item], seq }, item];
}

function replace(timeline: Timeline, index: number, item: TimelineItem): Timeline {
  const items = timeline.items.slice();
  items[index] = item;
  return { ...timeline, items };
}

function str(input: Record<string, unknown>, key: string): string | undefined {
  const value = input[key];
  return typeof value === "string" ? value : undefined;
}

function record(raw: unknown): Record<string, unknown> {
  return (typeof raw === "object" && raw ? raw : {}) as Record<string, unknown>;
}

function taskItems(raw: unknown): TaskItem[] | null {
  const todos = record(raw).todos;
  if (!Array.isArray(todos)) return null;
  return todos.map((todo) => {
    const entry = record(todo);
    const status = entry.status;
    return {
      text: str(entry, "content") ?? "",
      status: status === "in_progress" || status === "completed" ? status : "pending",
    };
  });
}

function isDelegation(name: string): boolean {
  return name === "Task" || name === "Agent";
}

/** "code-reviewer" reads as "Code reviewer" in the run header. */
function runName(raw: unknown): string {
  const input = record(raw);
  const type = str(input, "subagent_type");
  if (type) {
    const words = type.replace(/[-_]+/g, " ").trim();
    return words.charAt(0).toUpperCase() + words.slice(1);
  }
  return str(input, "description") ?? "Subagent";
}

function withRun(call: ToolCall, steps: readonly AgentStep[]): ToolCall {
  return { ...call, agentRun: { name: call.agentRun?.name ?? "Subagent", steps } };
}

/** Folds an event produced inside a delegated run onto the Task call that spawned it. */
function nest(timeline: Timeline, event: ProviderEvent, parentToolUseId: string): Applied {
  const index = timeline.toolIndex.get(parentToolUseId);
  const item = index === undefined ? undefined : timeline.items[index];
  if (index === undefined || !item || item.kind !== "tool") return [timeline, null];

  const steps = item.call.agentRun?.steps ?? [];
  const last = steps[steps.length - 1];

  if (event.kind === "assistant_text" || event.kind === "thinking") {
    const kind = event.kind === "thinking" ? ("reasoning" as const) : ("message" as const);
    const next =
      last && last.kind === kind
        ? [...steps.slice(0, -1), { ...last, text: last.text + event.text }]
        : [...steps, { id: `${parentToolUseId}:${steps.length}`, kind, text: event.text }];
    const updated: TimelineItem = { ...item, call: withRun(item.call, next) };
    return [replace(timeline, index, updated), updated];
  }

  if (event.kind === "tool_start") {
    const step: AgentStep = {
      id: event.toolUseId,
      kind: "tool",
      text: event.name,
      detail: toolDetail(event.name, event.input),
      status: "running",
    };
    const updated: TimelineItem = { ...item, call: withRun(item.call, [...steps, step]) };
    return [replace(timeline, index, updated), updated];
  }

  if (event.kind === "tool_end") {
    const at = steps.findIndex((step) => step.id === event.toolUseId);
    if (at === -1) return [timeline, null];
    const next = steps.slice();
    next[at] = { ...next[at]!, status: event.ok ? "ok" : "error" };
    const updated: TimelineItem = { ...item, call: withRun(item.call, next) };
    return [replace(timeline, index, updated), updated];
  }

  return [timeline, null];
}

/** A new run must not merge into text or tools left open by the previous one. */
export function startEpoch(timeline: Timeline): Timeline {
  return { ...timeline, epoch: timeline.epoch + 1, toolIndex: new Map(), openTextIndex: null, folded: new Set() };
}

export function addUserText(timeline: Timeline, text: string, id: string, ts: number): readonly [Timeline, TimelineItem] {
  return append({ ...timeline, openTextIndex: null }, { kind: "user", text }, id, ts);
}

export function apply(timeline: Timeline, event: ProviderEvent, id: string, ts: number): Applied {
  if ("parentToolUseId" in event && event.parentToolUseId) return nest(timeline, event, event.parentToolUseId);

  switch (event.kind) {
    case "session":
    case "task_start":
    case "task_progress":
    case "task_end":
    case "task_list":
      return [timeline, null];

    case "assistant_text":
    case "thinking": {
      const kind = event.kind === "thinking" ? ("thinking" as const) : ("assistant" as const);
      const index = timeline.openTextIndex;
      const open = index === null ? undefined : timeline.items[index];

      if (index !== null && open && open.kind === kind) {
        const merged: TimelineItem = { ...open, text: open.text + event.text };
        return [replace(timeline, index, merged), merged];
      }

      const [next, item] = append({ ...timeline, openTextIndex: null }, { kind, text: event.text }, id, ts);
      return [{ ...next, openTextIndex: next.items.length - 1 }, item];
    }

    case "tool_start": {
      const items = taskItems(event.input);
      if (event.name === "TodoWrite" && items) {
        const [next, item] = append({ ...timeline, openTextIndex: null }, { kind: "tasks", items }, id, ts);
        return [{ ...next, folded: new Set(next.folded).add(event.toolUseId) }, item];
      }

      const plan = event.name === "ExitPlanMode" ? str(record(event.input), "plan") : undefined;
      if (plan !== undefined) {
        const [next, item] = append({ ...timeline, openTextIndex: null }, { kind: "plan", text: plan }, id, ts);
        return [{ ...next, folded: new Set(next.folded).add(event.toolUseId) }, item];
      }

      const call: ToolCall = {
        toolUseId: event.toolUseId,
        name: event.name,
        detail: toolDetail(event.name, event.input),
        status: "running",
        ...(isDelegation(event.name) ? { agentRun: { name: runName(event.input), steps: [] } } : {}),
      };
      const [next, item] = append({ ...timeline, openTextIndex: null }, { kind: "tool", call }, id, ts);
      const toolIndex = new Map(next.toolIndex).set(event.toolUseId, next.items.length - 1);
      return [{ ...next, toolIndex }, item];
    }

    case "tool_end": {
      if (timeline.folded.has(event.toolUseId)) return [timeline, null];

      const index = timeline.toolIndex.get(event.toolUseId);
      const item = index === undefined ? undefined : timeline.items[index];
      if (index === undefined || !item || item.kind !== "tool") return [timeline, null];

      const updated: TimelineItem = {
        ...item,
        call: {
          ...item.call,
          status: event.ok ? "ok" : "error",
          durationMs: ts - item.ts,
          ...(event.output !== undefined ? { output: clamp(event.output) } : {}),
        },
      };
      return [replace(timeline, index, updated), updated];
    }

    case "result": {
      const body: TimelineBody = {
        kind: "result",
        ok: event.ok,
        durationMs: event.durationMs,
        ...(event.costUsd !== undefined ? { costUsd: event.costUsd } : {}),
        ...(event.error !== undefined ? { error: event.error } : {}),
        ...(event.usage !== undefined ? { usage: event.usage } : {}),
        ...(event.turns !== undefined ? { turns: event.turns } : {}),
      };
      return append({ ...timeline, openTextIndex: null }, body, id, ts);
    }

    case "compacted":
      return append({ ...timeline, openTextIndex: null }, { kind: "compact", trigger: event.trigger }, id, ts);

    case "error":
      return append({ ...timeline, openTextIndex: null }, { kind: "result", ok: false, durationMs: 0, error: event.message }, id, ts);
  }
}

export function page(
  timeline: Timeline,
  sinceSeq: number,
  limit: number,
): { readonly items: ReadonlyArray<TimelineItem>; readonly hasOlder: boolean } {
  const newer = timeline.items.filter((item) => item.seq > sinceSeq);
  const items = newer.slice(-limit);
  return { items, hasOlder: items.length < newer.length || sinceSeq > 0 };
}
